// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Proteen Das

//! Tables (TABLE, table editor) and multileader styles (MLEADERSTYLE).

use crate::app::App;
use crate::commands::{dim_layer, Command, Cx, Flow, Want};
use crate::theme;
use eframe::egui::{self, RichText};
use icad_core::entity::Shape;
use icad_core::geom::*;
use icad_core::prim::HAlign;
use icad_core::table::*;
use icad_core::Document;

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum TableSource {
    Empty,
    Csv,
    PartsList,
}

/// The TABLE dialog.
pub struct TableDialog {
    pub source: TableSource,
    pub rows: usize,
    pub cols: usize,
    pub col_w: f64,
    pub row_h: f64,
    pub text_h: f64,
    pub title: String,
    pub csv: String,
}

/// A table being edited (double-click / TABLEDIT).
pub struct TableEditor {
    pub id: u64,
    pub table: Table,
}

/// Places a prepared table by its top-left corner.
pub struct PlaceTableCmd {
    pub table: Table,
}

impl Command for PlaceTableCmd {
    fn name(&self) -> &'static str {
        "TABLE"
    }
    fn prompt(&self, _: &Cx) -> String {
        "Specify insertion point (top-left corner):".into()
    }
    fn wants(&self) -> Want {
        Want::Point
    }
    fn point(&mut self, cx: &mut Cx, p: V2) -> Flow {
        cx.checkpoint();
        let mut t = self.table.clone();
        t.pos = p;
        let layer = dim_layer(cx);
        cx.doc().add_on(&layer, Shape::Table(t));
        Flow::Done
    }
    fn preview(&self, _: &Document, c: V2) -> Vec<Shape> {
        let mut t = self.table.clone();
        t.pos = c;
        vec![Shape::Table(t)]
    }
}

impl TableDialog {
    fn new(doc: &Document) -> Self {
        let s = doc.ascale();
        let th = 2.5 * s;
        Self {
            source: TableSource::Empty,
            rows: 4,
            cols: 4,
            col_w: 30.0 * s,
            row_h: th * 2.4,
            text_h: th,
            title: String::new(),
            csv: String::new(),
        }
    }

    fn build(&self, doc: &Document) -> Option<Table> {
        let mut t = match self.source {
            TableSource::Empty => Table::new(V2::ZERO, self.rows, self.cols, self.col_w, self.row_h, self.text_h),
            TableSource::Csv => {
                let rows = parse_csv(&self.csv);
                if rows.is_empty() {
                    return None;
                }
                let mut t = Table::from_rows(V2::ZERO, &rows, self.text_h);
                t.row_heights.iter_mut().for_each(|h| *h = self.row_h);
                t
            }
            TableSource::PartsList => {
                // Item number first, block name as a fallback description.
                let rows = parse_csv(&crate::attributes::attributes_csv(doc));
                if rows.len() <= 1 {
                    return None;
                }
                let head = &rows[0];
                let keep: Vec<usize> = (0..head.len()).filter(|i| !matches!(head[*i].as_str(), "X" | "Y")).collect();
                let rows: Vec<Vec<String>> =
                    rows.iter().map(|r| keep.iter().map(|i| r.get(*i).cloned().unwrap_or_default()).collect()).collect();
                let mut t = Table::from_rows(V2::ZERO, &rows, self.text_h);
                t.row_heights.iter_mut().for_each(|h| *h = self.row_h);
                t
            }
        };
        if !self.title.trim().is_empty() {
            t.insert_row(0);
            let cols = t.cols();
            t.cells[0][0] = Cell { text: self.title.trim().to_string(), align: HAlign::Center, span: cols - 1 };
        }
        Some(t)
    }
}

/// Edit a table's cells and structure. Returns true when changed.
pub fn table_editor(ui: &mut egui::Ui, t: &mut Table) -> bool {
    let mut ch = false;
    ui.horizontal_wrapped(|ui| {
        if ui.button("+ Row").clicked() {
            let n = t.rows();
            t.insert_row(n);
            ch = true;
        }
        if ui.button("+ Column").clicked() {
            let n = t.cols();
            t.insert_col(n);
            ch = true;
        }
        if ui.button("Fit columns").on_hover_text("Size every column to its text").clicked() {
            t.fit_columns();
            ch = true;
        }
        ui.label("Text height");
        ch |= ui.add(egui::DragValue::new(&mut t.text_height).speed(0.1).range(0.01..=1e6)).changed();
    });
    ui.add_space(4.0);
    egui::ScrollArea::both().max_height(380.0).show(ui, |ui| {
        let mut remove_row = None;
        let mut remove_col = None;
        egui::Grid::new("tbl_cells").spacing([4.0, 4.0]).show(ui, |ui| {
            ui.label("");
            for c in 0..t.cols() {
                ui.vertical(|ui| {
                    ui.horizontal(|ui| {
                        ch |= ui.add(egui::DragValue::new(&mut t.col_widths[c]).speed(0.5).range(0.1..=1e6).prefix("w ")).changed();
                        if t.cols() > 1 && ui.small_button("✖").on_hover_text("Remove column").clicked() {
                            remove_col = Some(c);
                        }
                    });
                    let mut align = t.cells.first().map(|r| r[c].align).unwrap_or(HAlign::Center);
                    let before = align;
                    egui::ComboBox::from_id_salt(("tbl_align", c)).width(70.0).selected_text(format!("{align:?}")).show_ui(ui, |ui| {
                        for a in [HAlign::Left, HAlign::Center, HAlign::Right] {
                            ui.selectable_value(&mut align, a, format!("{a:?}"));
                        }
                    });
                    if align != before {
                        for r in &mut t.cells {
                            r[c].align = align;
                        }
                        ch = true;
                    }
                });
            }
            ui.end_row();
            for r in 0..t.rows() {
                ui.horizontal(|ui| {
                    if t.rows() > 1 && ui.small_button("✖").on_hover_text("Remove row").clicked() {
                        remove_row = Some(r);
                    }
                    let merged = t.cells[r][0].span + 1 >= t.cols() && t.cols() > 1;
                    let mut m = merged;
                    if ui.checkbox(&mut m, "").on_hover_text("Merge the whole row (title)").changed() {
                        t.cells[r][0].span = if m { t.cols() - 1 } else { 0 };
                        ch = true;
                    }
                });
                let mut c = 0;
                while c < t.cols() {
                    let span = t.cells[r][c].span;
                    let w = 90.0 * (span + 1) as f32 + 4.0 * span as f32;
                    ch |= ui.add(egui::TextEdit::singleline(&mut t.cells[r][c].text).desired_width(w)).changed();
                    for _ in 0..span {
                        ui.label("");
                    }
                    c += span + 1;
                }
                ui.end_row();
            }
        });
        if let Some(r) = remove_row {
            t.remove_row(r);
            ch = true;
        }
        if let Some(c) = remove_col {
            t.remove_col(c);
            ch = true;
        }
    });
    ch
}

/// Edit a multileader style. Returns true when changed.
fn mstyle_editor(ui: &mut egui::Ui, s: &mut MLeaderStyle) -> bool {
    let mut ch = false;
    egui::Grid::new("mls").num_columns(2).spacing([10.0, 6.0]).show(ui, |ui| {
        ui.label("Arrowhead");
        egui::ComboBox::from_id_salt("mls_arrow").selected_text(s.arrow.name()).show_ui(ui, |ui| {
            for a in ArrowKind::ALL {
                ch |= ui.selectable_value(&mut s.arrow, a, a.name()).changed();
            }
        });
        ui.end_row();
        ui.label("Arrow size (mm)");
        ch |= ui.add(egui::DragValue::new(&mut s.arrow_size).speed(0.1).range(0.0..=50.0)).changed();
        ui.end_row();
        ui.label("Text height (mm)");
        ch |= ui.add(egui::DragValue::new(&mut s.text_height).speed(0.1).range(0.5..=50.0)).changed();
        ui.end_row();
        ui.label("Landing");
        ui.horizontal(|ui| {
            ch |= ui.checkbox(&mut s.landing, "").changed();
            ch |= ui.add_enabled(s.landing, egui::DragValue::new(&mut s.landing_len).speed(0.1).range(0.0..=50.0).suffix(" mm")).changed();
        });
        ui.end_row();
        ui.label("Text frame");
        ui.horizontal(|ui| {
            for (f, l) in [(TextFrame::None, "None"), (TextFrame::Box, "Box"), (TextFrame::Circle, "Balloon")] {
                ch |= ui.radio_value(&mut s.frame, f, l).changed();
            }
        });
        ui.end_row();
    });
    ch
}

impl App {
    pub fn open_table_dialog(&mut self) {
        self.dlg.table = Some(TableDialog::new(&self.tabs[self.active].doc));
    }

    /// Double-click / TABLEDIT on a table.
    pub fn edit_table_of(&mut self, id: u64) -> bool {
        match self.tabs[self.active].doc.get(id).map(|e| &e.shape) {
            Some(Shape::Table(t)) => {
                self.dlg.table_edit = Some(TableEditor { id, table: t.clone() });
                true
            }
            _ => false,
        }
    }

    pub fn table_dialogs(&mut self, ctx: &egui::Context) {
        // New table.
        if let Some(mut d) = self.dlg.table.take() {
            let mut open = true;
            let mut place = false;
            let doc = &self.tabs[self.active].doc;
            egui::Window::new("Insert Table").open(&mut open).collapsible(false).default_width(460.0).show(ctx, |ui| {
                ui.horizontal(|ui| {
                    ui.radio_value(&mut d.source, TableSource::Empty, "Empty");
                    ui.radio_value(&mut d.source, TableSource::Csv, "From CSV / spreadsheet");
                    ui.radio_value(&mut d.source, TableSource::PartsList, "Parts list (block attributes)");
                });
                ui.add_space(4.0);
                egui::Grid::new("tbl_new").num_columns(2).spacing([10.0, 6.0]).show(ui, |ui| {
                    if d.source == TableSource::Empty {
                        ui.label("Rows × columns");
                        ui.horizontal(|ui| {
                            ui.add(egui::DragValue::new(&mut d.rows).range(1..=500));
                            ui.label("×");
                            ui.add(egui::DragValue::new(&mut d.cols).range(1..=50));
                        });
                        ui.end_row();
                        ui.label("Column width");
                        ui.add(egui::DragValue::new(&mut d.col_w).speed(0.5).range(0.1..=1e6));
                        ui.end_row();
                    }
                    ui.label("Row height");
                    ui.add(egui::DragValue::new(&mut d.row_h).speed(0.1).range(0.1..=1e6));
                    ui.end_row();
                    ui.label("Text height");
                    ui.add(egui::DragValue::new(&mut d.text_h).speed(0.1).range(0.01..=1e6));
                    ui.end_row();
                    ui.label("Title row");
                    ui.add(egui::TextEdit::singleline(&mut d.title).hint_text("optional, e.g. PARTS LIST"));
                    ui.end_row();
                });
                match d.source {
                    TableSource::Csv => {
                        ui.horizontal(|ui| {
                            ui.label(RichText::new("Paste comma- or tab-separated rows (from a spreadsheet):").size(12.0));
                            if ui.small_button("Open file…").clicked() {
                                if let Some(p) = rfd::FileDialog::new().add_filter("CSV", &["csv", "txt", "tsv"]).pick_file() {
                                    d.csv = std::fs::read_to_string(p).unwrap_or_default();
                                }
                            }
                        });
                        ui.add(egui::TextEdit::multiline(&mut d.csv).desired_rows(6).desired_width(f32::INFINITY).code_editor());
                    }
                    TableSource::PartsList => {
                        let n = doc.entities.iter().filter(|e| matches!(&e.shape, Shape::Insert(i) if doc.block(&i.name).is_some_and(|b| b.attdefs().next().is_some()))).count();
                        ui.label(
                            RichText::new(format!("{n} block reference(s) with attributes: one row each, one column per attribute tag."))
                                .size(12.0)
                                .color(theme::TEXT_DIM),
                        );
                    }
                    TableSource::Empty => {}
                }
                ui.add_space(6.0);
                if ui.button(RichText::new("Place in drawing").strong()).clicked() {
                    place = true;
                }
            });
            if place {
                match d.build(&self.tabs[self.active].doc) {
                    Some(t) => {
                        self.cmd = Some(Box::new(PlaceTableCmd { table: t }));
                        self.log.push("Command: TABLE".into());
                    }
                    None => {
                        self.log.push("Nothing to put in the table.".into());
                        self.dlg.table = Some(d);
                    }
                }
            } else if open {
                self.dlg.table = Some(d);
            }
        }
        // Edit table.
        if let Some(mut e) = self.dlg.table_edit.take() {
            let mut open = true;
            let mut ok = false;
            let mut cancel = false;
            egui::Window::new("Edit Table").open(&mut open).collapsible(false).default_width(640.0).show(ctx, |ui| {
                table_editor(ui, &mut e.table);
                ui.separator();
                ui.horizontal(|ui| {
                    if ui.button(RichText::new("OK").strong()).clicked() {
                        ok = true;
                    }
                    if ui.button("Cancel").clicked() {
                        cancel = true;
                    }
                });
            });
            if ok {
                let t = self.tab();
                t.checkpoint();
                if let Some(ent) = t.doc.get_mut(e.id) {
                    ent.shape = Shape::Table(e.table);
                }
            } else if open && !cancel {
                self.dlg.table_edit = Some(e);
            }
        }
        // Multileader styles.
        if self.dlg.mleader_styles {
            let mut open = true;
            let t = &mut self.tabs[self.active];
            let before = (t.doc.mleader_styles.clone(), t.doc.mleader_style.clone());
            egui::Window::new("Multileader Styles").open(&mut open).collapsible(false).default_width(420.0).show(ctx, |ui| {
                let doc = &mut t.doc;
                ui.horizontal(|ui| {
                    ui.label("Current style");
                    egui::ComboBox::from_id_salt("mls_cur").selected_text(doc.mleader_style.clone()).show_ui(ui, |ui| {
                        for s in &doc.mleader_styles {
                            ui.selectable_value(&mut doc.mleader_style, s.name.clone(), &s.name);
                        }
                    });
                    if ui.button("New").clicked() {
                        let mut s =
                            doc.mleader_styles.iter().find(|s| s.name == doc.mleader_style).cloned().unwrap_or_else(MLeaderStyle::standard);
                        let mut k = 1;
                        while doc.mleader_styles.iter().any(|x| x.name == format!("Style {k}")) {
                            k += 1;
                        }
                        s.name = format!("Style {k}");
                        doc.mleader_style = s.name.clone();
                        doc.mleader_styles.push(s);
                    }
                    let in_use = doc
                        .entities
                        .iter()
                        .any(|e| matches!(&e.shape, Shape::Leader(l) if l.style.as_deref() == Some(doc.mleader_style.as_str())));
                    if ui
                        .add_enabled(doc.mleader_styles.len() > 1 && !in_use, egui::Button::new("Delete"))
                        .on_disabled_hover_text("Styles in use cannot be deleted")
                        .clicked()
                    {
                        doc.mleader_styles.retain(|s| s.name != doc.mleader_style);
                        doc.mleader_style = doc.mleader_styles[0].name.clone();
                    }
                });
                ui.separator();
                let cur = doc.mleader_style.clone();
                if let Some(s) = doc.mleader_styles.iter_mut().find(|s| s.name == cur) {
                    ui.horizontal(|ui| {
                        ui.label("Name");
                        let mut name = s.name.clone();
                        if ui.text_edit_singleline(&mut name).changed() && !name.trim().is_empty() {
                            s.name = name.clone();
                            doc.mleader_style = name;
                        }
                    });
                    mstyle_editor(ui, s);
                    let pts = [V2::ZERO, V2::new(8.0, 8.0)];
                    let prims = s.prims(&pts, if s.frame == TextFrame::Circle { "12" } else { "NOTE TEXT" }, 1.0, 1.0);
                    crate::canvas::preview_prims(ui, &prims, egui::vec2(360.0, 90.0));
                }
                ui.label(
                    RichText::new("MLEADER draws with the current style; change a leader's style in the Properties palette.")
                        .size(11.0)
                        .color(theme::TEXT_DIM),
                );
            });
            // Renamed styles carry their leaders along.
            let (old_styles, _) = &before;
            for (o, n) in old_styles.iter().zip(&t.doc.mleader_styles) {
                if o.name != n.name {
                    for e in &mut t.doc.entities {
                        if let Shape::Leader(l) = &mut e.shape {
                            if l.style.as_deref() == Some(o.name.as_str()) {
                                l.style = Some(n.name.clone());
                            }
                        }
                    }
                }
            }
            if (t.doc.mleader_styles.clone(), t.doc.mleader_style.clone()) != before {
                t.dirty = true;
            }
            self.dlg.mleader_styles = open;
        }
    }
}
