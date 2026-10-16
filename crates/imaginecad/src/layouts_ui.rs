// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Proteen Das

//! Paper-space layouts in the editor: layout tabs, the paper canvas with
//! viewports (MVIEW), viewport and layout properties, and batch plotting
//! (PUBLISH) to one multi-page PDF or one PDF per sheet.

use crate::app::App;
use crate::canvas;
use crate::commands::{Command, Cx, Flow, Want};
use crate::theme;
use eframe::egui::{self, vec2, Color32, PointerButton, Rect, RichText, Sense, Stroke};
use icad_core::geom::*;
use icad_core::layout::{Layout, PlotStyle, Viewport};
use icad_core::sheet::{scale_label, STANDARD_SCALES};
use icad_core::Document;

/// Commands that make sense on a paper layout.
pub fn allowed_on_paper(cmd: &str) -> bool {
    matches!(cmd, "MVIEW" | "MV" | "ZOOM" | "Z" | "LIST" | "LI" | "DIST" | "DI" | "ID")
}

// ------------------------------------------------------------------ MVIEW

/// MVIEW: a new viewport by two corners on the paper, or Fit to fill the
/// drawable area.
#[derive(Default)]
pub struct MviewCmd {
    first: Option<V2>,
}

fn add_viewport(cx: &mut Cx, a: V2, b: V2) -> Flow {
    let Some(li) = cx.tab.paper else {
        cx.msg("MVIEW works on a layout tab.");
        return Flow::Done;
    };
    let r = BBox::from_pts(a, b);
    if r.width() < 1.0 || r.height() < 1.0 {
        cx.msg("The viewport is too small.");
        return Flow::Continue;
    }
    cx.checkpoint();
    let ext = cx.tab.doc.extents();
    let l = &mut cx.tab.doc.layouts[li];
    let mut vp = Viewport { min: r.min, max: r.max, center: V2::ZERO, scale: 1.0, locked: false };
    vp.fit(&ext);
    let scale = vp.scale;
    l.viewports.push(vp);
    let n = l.viewports.len();
    cx.tab.paper_vp = Some(n - 1);
    cx.msg(format!("Viewport added at {}.", scale_label(scale)));
    Flow::Done
}

impl Command for MviewCmd {
    fn name(&self) -> &'static str {
        "MVIEW"
    }
    fn prompt(&self, _: &Cx) -> String {
        match self.first {
            None => "Specify corner of viewport or [Fit]:".into(),
            Some(_) => "Specify opposite corner:".into(),
        }
    }
    fn wants(&self) -> Want {
        Want::Point
    }
    fn base(&self) -> Option<V2> {
        self.first
    }
    fn point(&mut self, cx: &mut Cx, p: V2) -> Flow {
        match self.first {
            None => {
                self.first = Some(p);
                Flow::Continue
            }
            Some(a) => add_viewport(cx, a, p),
        }
    }
    fn value(&mut self, cx: &mut Cx, s: &str) -> Flow {
        if s.eq_ignore_ascii_case("F") || s.eq_ignore_ascii_case("FIT") {
            let Some(li) = cx.tab.paper else { return Flow::Done };
            let area = cx.tab.doc.layouts[li].sheet.drawable_mm();
            return add_viewport(cx, area.min, area.max);
        }
        cx.msg(format!("Invalid input: {s}"));
        Flow::Continue
    }
    fn preview(&self, _: &Document, c: V2) -> Vec<icad_core::Shape> {
        match self.first {
            Some(a) => vec![icad_core::Shape::Polyline { pts: BBox::from_pts(a, c).corners().to_vec(), closed: true }],
            None => vec![],
        }
    }
}

// ------------------------------------------------------------------ PUBLISH

/// One sheet offered for batch plotting: tab index, layout (None = main sheet).
pub struct PublishRow {
    pub tab: usize,
    pub layout: Option<usize>,
    pub label: String,
    pub on: bool,
}

pub struct PublishDialog {
    pub rows: Vec<PublishRow>,
    pub single_file: bool,
    pub style: Option<PlotStyle>,
}

impl App {
    /// Switch the active drawing to a sheet: None = model, Some(None) = the
    /// main sheet (Layout1), Some(Some(i)) = paper layout i.
    pub fn show_sheet(&mut self, which: Option<Option<usize>>) {
        let t = self.tab();
        match which {
            None => {
                t.layout = false;
                t.paper = None;
            }
            Some(None) => {
                t.paper = None;
                if !t.layout {
                    t.layout = true;
                    self.view_req = Some(crate::commands::ViewReq::Sheet);
                }
            }
            Some(Some(i)) => {
                if t.paper != Some(i) {
                    t.paper = Some(i);
                    t.paper_vp = None;
                    t.paper_fit = true;
                }
            }
        }
    }

    pub fn new_layout(&mut self) {
        let t = self.tab();
        t.checkpoint();
        let mut k = t.doc.layouts.len() + 2;
        while t.doc.layouts.iter().any(|l| l.name == format!("Layout{k}")) {
            k += 1;
        }
        let ext = t.doc.extents();
        let l = Layout::new(format!("Layout{k}"), t.doc.sheet.clone(), &ext);
        t.doc.layouts.push(l);
        let i = t.doc.layouts.len() - 1;
        self.show_sheet(Some(Some(i)));
        self.log.push(format!("Layout{k} created with one viewport. MVIEW adds more; select a viewport to set its scale."));
    }

    /// Status bar tabs: Model, Layout1 (main sheet), the paper layouts, +.
    pub fn layout_tabs(&mut self, ui: &mut egui::Ui) {
        let (layout, paper, names): (bool, Option<usize>, Vec<String>) = {
            let t = &self.tabs[self.active];
            (t.layout, t.paper, t.doc.layouts.iter().map(|l| l.name.clone()).collect())
        };
        if ui.selectable_label(!layout && paper.is_none(), "Model").clicked() {
            self.show_sheet(None);
        }
        if ui.selectable_label(layout && paper.is_none(), "Layout1").on_hover_text("The main sheet (title block over the model)").clicked()
        {
            self.show_sheet(Some(None));
        }
        let mut delete = None;
        for (i, n) in names.iter().enumerate() {
            let r = ui.selectable_label(paper == Some(i), n);
            if r.clicked() {
                self.show_sheet(Some(Some(i)));
            }
            r.context_menu(|ui| {
                if ui.button("Rename…").clicked() {
                    self.dlg.layout_rename = Some((i, n.clone()));
                    ui.close_menu();
                }
                if ui.button("Page Setup…").clicked() {
                    self.show_sheet(Some(Some(i)));
                    self.dlg.page = true;
                    ui.close_menu();
                }
                if ui.button("Plot to PDF…").clicked() {
                    self.show_sheet(Some(Some(i)));
                    self.export("pdf");
                    ui.close_menu();
                }
                if ui.button("Delete").clicked() {
                    delete = Some(i);
                    ui.close_menu();
                }
            });
        }
        if ui.small_button("+").on_hover_text("New layout with a viewport of the model").clicked() {
            self.new_layout();
        }
        if let Some(i) = delete {
            let t = self.tab();
            t.checkpoint();
            t.doc.layouts.remove(i);
            t.paper = None;
            t.layout = false;
        }
    }

    pub fn layout_dialogs(&mut self, ctx: &egui::Context) {
        if let Some((i, mut name)) = self.dlg.layout_rename.take() {
            let mut open = true;
            let mut ok = false;
            egui::Window::new("Rename Layout").open(&mut open).collapsible(false).resizable(false).show(ctx, |ui| {
                let r = ui.text_edit_singleline(&mut name);
                r.request_focus();
                if ui.button("OK").clicked() || (r.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter))) {
                    ok = true;
                }
            });
            let t = self.tab();
            let taken = t.doc.layouts.iter().enumerate().any(|(k, l)| k != i && l.name.eq_ignore_ascii_case(name.trim()));
            if ok && !name.trim().is_empty() && !taken {
                t.checkpoint();
                if let Some(l) = t.doc.layouts.get_mut(i) {
                    l.name = name.trim().to_string();
                }
            } else if open && !ok {
                self.dlg.layout_rename = Some((i, name));
            }
        }
        self.publish_dialog(ctx);
    }

    pub fn open_publish(&mut self) {
        let mut rows = vec![];
        for (ti, t) in self.tabs.iter().enumerate() {
            rows.push(PublishRow { tab: ti, layout: None, label: format!("{} — Layout1", t.name), on: ti == self.active });
            for (li, l) in t.doc.layouts.iter().enumerate() {
                rows.push(PublishRow { tab: ti, layout: Some(li), label: format!("{} — {}", t.name, l.name), on: ti == self.active });
            }
        }
        self.dlg.publish = Some(PublishDialog { rows, single_file: true, style: None });
    }

    fn publish_dialog(&mut self, ctx: &egui::Context) {
        let Some(mut d) = self.dlg.publish.take() else { return };
        let mut open = true;
        let mut go = false;
        egui::Window::new("Publish (Batch Plot)").open(&mut open).collapsible(false).default_width(420.0).show(ctx, |ui| {
            ui.label("Sheets to plot:");
            egui::ScrollArea::vertical().max_height(260.0).show(ui, |ui| {
                for r in &mut d.rows {
                    ui.checkbox(&mut r.on, &r.label);
                }
            });
            ui.horizontal(|ui| {
                if ui.small_button("All").clicked() {
                    d.rows.iter_mut().for_each(|r| r.on = true);
                }
                if ui.small_button("None").clicked() {
                    d.rows.iter_mut().for_each(|r| r.on = false);
                }
            });
            ui.separator();
            ui.radio_value(&mut d.single_file, true, "One multi-page PDF");
            ui.radio_value(&mut d.single_file, false, "One PDF per sheet (choose a folder)");
            ui.horizontal(|ui| {
                ui.label("Plot style");
                egui::ComboBox::from_id_salt("pub_style").selected_text(d.style.map(|s| s.name()).unwrap_or("Each sheet's own")).show_ui(
                    ui,
                    |ui| {
                        ui.selectable_value(&mut d.style, None, "Each sheet's own");
                        for s in PlotStyle::ALL {
                            ui.selectable_value(&mut d.style, Some(s), s.name());
                        }
                    },
                );
            });
            ui.add_space(6.0);
            let n = d.rows.iter().filter(|r| r.on).count();
            if ui.add_enabled(n > 0, egui::Button::new(RichText::new(format!("Publish {n} sheet(s)")).strong())).clicked() {
                go = true;
            }
        });
        if go {
            let pdfa = self.settings.pdfa;
            let sheets: Vec<(String, icad_io::pdf::Page)> = d
                .rows
                .iter()
                .filter(|r| r.on)
                .filter_map(|r| {
                    let opts = self.plot_options(d.style, r.tab);
                    let doc = &self.tabs.get(r.tab)?.doc;
                    let page = match r.layout {
                        None => icad_io::pdf::sheet_page(doc, &opts),
                        Some(li) => icad_io::pdf::layout_page(doc, li, &opts)?,
                    };
                    Some((r.label.replace(" — ", "-").replace(['/', '\\', ':'], "_"), page))
                })
                .collect();
            let result = if d.single_file {
                rfd::FileDialog::new().add_filter("PDF", &["pdf"]).set_file_name("sheets.pdf").save_file().map(|p| {
                    let pages: Vec<icad_io::pdf::Page> = sheets.into_iter().map(|s| s.1).collect();
                    let n = pages.len();
                    std::fs::write(&p, icad_io::pdf::write_pages_with(&pages, "ImagineCAD sheets", pdfa))
                        .map(|_| format!("Published {n} sheet(s) to {}", p.display()))
                })
            } else {
                rfd::FileDialog::new().pick_folder().map(|dir| {
                    let n = sheets.len();
                    for (name, page) in sheets {
                        std::fs::write(dir.join(format!("{name}.pdf")), icad_io::pdf::write_pages_with(&[page], &name, pdfa))?;
                    }
                    Ok(format!("Published {n} PDF file(s) to {}", dir.display()))
                })
            };
            match result {
                Some(Ok(m)) => self.log.push(m),
                Some(Err(e)) => self.error(format!("Publishing failed: {e}")),
                None => self.dlg.publish = Some(d),
            }
            return;
        }
        if open {
            self.dlg.publish = Some(d);
        }
    }

    // -------------------------------------------------------------- paper canvas

    pub fn paper_canvas(&mut self, ui: &mut egui::Ui, li: usize) {
        let (resp, painter) = ui.allocate_painter(ui.available_size(), Sense::click_and_drag());
        let rect = resp.rect;
        let ctx = ui.ctx().clone();
        let lineweights = self.settings.lineweights;
        let t = &mut self.tabs[self.active];
        let Some((w, h)) = t.doc.layouts.get(li).map(|l| l.sheet.paper_mm()) else {
            t.paper = None;
            return;
        };
        let page = BBox::from_pts(V2::ZERO, V2::new(w, h));
        t.paper_view.rect = rect;
        if t.paper_fit || self.view_req.take().is_some() {
            t.paper_view.fit(&page.expand(8.0));
            t.paper_fit = false;
        }
        // Navigation: wheel / pinch zoom, middle or Alt drag to pan.
        let hover = resp.hover_pos();
        if let Some(hp) = hover {
            let (scroll, zoom_d) = ctx.input(|i| (i.smooth_scroll_delta, i.zoom_delta()));
            if scroll.y.abs() > 0.0 {
                t.paper_view.zoom_at(hp, (1.0025f64).powf(scroll.y as f64));
            }
            if (zoom_d - 1.0).abs() > 1e-6 {
                t.paper_view.zoom_at(hp, zoom_d as f64);
            }
        }
        let (middle, alt, delta) = ctx.input(|i| (i.pointer.middle_down(), i.modifiers.alt, i.pointer.delta()));
        let panning = resp.hovered() && (middle || (alt && resp.dragged()));
        if panning {
            t.paper_view.pan_pixels(delta.x, delta.y);
        }
        // Paper, then the layout.
        painter.rect_filled(rect, 0.0, theme::PAPER_SURROUND);
        let pr = Rect::from_two_pos(t.paper_view.to_screen(page.min), t.paper_view.to_screen(page.max));
        painter.rect_filled(pr.translate(vec2(5.0, 5.0)), 0.0, Color32::from_black_alpha(90));
        painter.rect_filled(pr, 0.0, theme::PAPER);
        let items = t.doc.layout_items(li, false);
        canvas::draw_items(&painter, &t.paper_view, &t.doc, &items, lineweights);
        if let Some(vp) = t.paper_vp.and_then(|i| t.doc.layouts[li].viewports.get(i)) {
            let r = Rect::from_two_pos(t.paper_view.to_screen(vp.min), t.paper_view.to_screen(vp.max));
            painter.rect_stroke(r, 0.0, Stroke::new(2.0f32, theme::SELECT_HL), egui::StrokeKind::Middle);
        }
        painter.text(
            rect.left_top() + vec2(8.0, 6.0),
            egui::Align2::LEFT_TOP,
            format!("[{}] [Paper]", t.doc.layouts[li].name),
            egui::FontId::proportional(11.5),
            Color32::from_gray(40),
        );
        // Cursor in paper millimetres; commands (MVIEW) take points here.
        let Some(hp) = hover else { return };
        self.cursor = t.paper_view.to_world(hp);
        let cur = self.cursor;
        if let Some(c) = &self.cmd {
            canvas::draw_preview(
                &painter,
                &self.tabs[self.active].paper_view,
                &self.tabs[self.active].doc,
                &c.preview(&self.tabs[self.active].doc, cur),
                true,
            );
        }
        ctx.set_cursor_icon(if panning { egui::CursorIcon::Grabbing } else { egui::CursorIcon::Crosshair });
        if panning {
            return;
        }
        if resp.clicked_by(PointerButton::Secondary) {
            if self.cmd.is_some() {
                self.submit("");
            }
            return;
        }
        if !resp.clicked_by(PointerButton::Primary) {
            return;
        }
        if self.cmd.as_ref().map(|c| c.wants()) == Some(Want::Point) {
            self.feed_point(cur);
            return;
        }
        // Select the viewport under the cursor (the last drawn wins).
        let t = &mut self.tabs[self.active];
        t.paper_vp = t.doc.layouts[li].viewports.iter().rposition(|v| v.rect().contains(cur));
    }

    /// Viewport and layout settings in the Properties palette.
    pub fn layout_properties(&mut self, ui: &mut egui::Ui, li: usize) {
        let t = &mut self.tabs[self.active];
        let Some(l) = t.doc.layouts.get(li) else { return };
        let before = l.clone();
        let ext = t.doc.extents();
        let mut changed = false;
        let mut delete = false;
        let l = &mut t.doc.layouts[li];
        ui.label(RichText::new(format!("Layout {}", l.name)).strong());
        egui::Grid::new("lay_props").num_columns(2).spacing([8.0, 4.0]).show(ui, |ui| {
            ui.label("Plot style");
            egui::ComboBox::from_id_salt("lay_style").selected_text(l.sheet.plot_style.name()).show_ui(ui, |ui| {
                for s in PlotStyle::ALL {
                    changed |= ui.selectable_value(&mut l.sheet.plot_style, s, s.name()).changed();
                }
            });
            ui.end_row();
            ui.label("Plot frames");
            changed |= ui.checkbox(&mut l.plot_frames, "").changed();
            ui.end_row();
        });
        ui.separator();
        match t.paper_vp.and_then(|i| l.viewports.get_mut(i).map(|v| (i, v))) {
            None => {
                ui.label(
                    RichText::new("Click a viewport to set its scale and view. MVIEW adds viewports.").size(11.5).color(theme::TEXT_DIM),
                );
            }
            Some((_, vp)) => {
                ui.label(RichText::new("Viewport").strong());
                egui::Grid::new("vp_props").num_columns(2).spacing([8.0, 4.0]).show(ui, |ui| {
                    ui.label("Scale");
                    ui.add_enabled_ui(!vp.locked, |ui| {
                        egui::ComboBox::from_id_salt("vp_scale").selected_text(scale_label(vp.scale)).show_ui(ui, |ui| {
                            for s in STANDARD_SCALES {
                                changed |= ui.selectable_value(&mut vp.scale, s, scale_label(s)).changed();
                            }
                        });
                    });
                    ui.end_row();
                    for (label, v) in [("View centre X", &mut vp.center.x), ("View centre Y", &mut vp.center.y)] {
                        ui.label(label);
                        changed |= ui.add_enabled(!vp.locked, egui::DragValue::new(v).speed(1.0)).changed();
                        ui.end_row();
                    }
                    for (label, v) in [("Left", &mut vp.min.x), ("Bottom", &mut vp.min.y), ("Right", &mut vp.max.x), ("Top", &mut vp.max.y)]
                    {
                        ui.label(format!("{label} (mm)"));
                        changed |= ui.add(egui::DragValue::new(v).speed(0.5)).changed();
                        ui.end_row();
                    }
                    ui.label("Locked");
                    changed |= ui.checkbox(&mut vp.locked, "").changed();
                    ui.end_row();
                });
                ui.horizontal(|ui| {
                    if ui.add_enabled(!vp.locked, egui::Button::new("Fit model")).clicked() {
                        vp.fit(&ext);
                        changed = true;
                    }
                    if ui.button("Delete viewport").clicked() {
                        delete = true;
                    }
                });
            }
        }
        if delete {
            if let Some(i) = t.paper_vp.take() {
                t.doc.layouts[li].viewports.remove(i);
                changed = true;
            }
        }
        if changed {
            // One undo step per change, holding the layout as it was.
            let mut prev = t.doc.clone();
            prev.layouts[li] = before;
            t.undo_stack.push(prev);
            t.redo_stack.clear();
            t.dirty = true;
        }
    }
}
