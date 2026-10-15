// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Proteen Das

//! Blocks palette, Quick Select, text editor and Units dialogs.

use crate::app::App;
use crate::dialogs::QSelect;
use crate::theme;
use eframe::egui::{self, vec2, RichText};
use icad_core::entity::Shape;
use icad_core::style::{Color, ACI_NAMES};

impl App {
    pub fn blocks_dialog(&mut self, ctx: &egui::Context) {
        if !self.dlg.blocks {
            return;
        }
        let mut open = true;
        let mut insert: Option<String> = None;
        let mut delete: Option<String> = None;
        let t = &self.tabs[self.active];
        let usage = |name: &str| {
            t.doc.entities.iter().filter(|e| matches!(&e.shape, Shape::Insert(i) if i.name.eq_ignore_ascii_case(name))).count()
        };
        egui::Window::new("Blocks").open(&mut open).default_width(420.0).show(ctx, |ui| {
            if t.doc.blocks.is_empty() {
                ui.label("No blocks in this drawing. Select objects and run BLOCK (B) to create one.");
            }
            egui::Grid::new("blocks").striped(true).num_columns(4).show(ui, |ui| {
                ui.label(RichText::new("Name").strong());
                ui.label(RichText::new("Objects").strong());
                ui.label(RichText::new("Inserted").strong());
                ui.label("");
                ui.end_row();
                for b in &t.doc.blocks {
                    ui.label(&b.name);
                    ui.label(b.entities.len().to_string());
                    let n = usage(&b.name);
                    ui.label(n.to_string());
                    ui.horizontal(|ui| {
                        if ui.button("Insert").clicked() {
                            insert = Some(b.name.clone());
                        }
                        if n == 0 && ui.small_button("Delete").clicked() {
                            delete = Some(b.name.clone());
                        }
                    });
                    ui.end_row();
                }
            });
        });
        self.dlg.blocks = open;
        if let Some(n) = delete {
            let t = self.tab();
            t.checkpoint();
            t.doc.blocks.retain(|b| b.name != n);
        }
        if let Some(n) = insert {
            self.dlg.blocks = false;
            self.run(&format!("INSERT {n}"));
        }
    }

    pub fn qselect_dialog(&mut self, ctx: &egui::Context) {
        let Some(mut q) = self.dlg.qselect.take() else { return };
        let mut open = true;
        let mut apply = false;
        let t = &self.tabs[self.active];
        let mut kinds: Vec<&str> = t.doc.entities.iter().map(|e| e.shape.type_name()).collect();
        kinds.sort();
        kinds.dedup();
        egui::Window::new("Quick Select").open(&mut open).default_width(340.0).show(ctx, |ui| {
            egui::Grid::new("qs").num_columns(2).spacing([10.0, 6.0]).show(ui, |ui| {
                ui.label("Object type");
                egui::ComboBox::from_id_salt("qs_kind").selected_text(if q.kind.is_empty() { "Any" } else { &q.kind }).show_ui(ui, |ui| {
                    ui.selectable_value(&mut q.kind, String::new(), "Any");
                    for k in &kinds {
                        ui.selectable_value(&mut q.kind, k.to_string(), *k);
                    }
                });
                ui.end_row();
                ui.label("Layer");
                egui::ComboBox::from_id_salt("qs_layer").selected_text(if q.layer.is_empty() { "Any" } else { &q.layer }).show_ui(
                    ui,
                    |ui| {
                        ui.selectable_value(&mut q.layer, String::new(), "Any");
                        for l in &t.doc.layers {
                            ui.selectable_value(&mut q.layer, l.name.clone(), &l.name);
                        }
                    },
                );
                ui.end_row();
                ui.label("Colour");
                let ctext = q.color.map(icad_core::style::aci_name).unwrap_or_else(|| "Any".into());
                egui::ComboBox::from_id_salt("qs_color").selected_text(ctext).show_ui(ui, |ui| {
                    ui.selectable_value(&mut q.color, None, "Any");
                    for (i, n) in ACI_NAMES {
                        ui.selectable_value(&mut q.color, Some(i), n);
                    }
                });
                ui.end_row();
            });
            ui.checkbox(&mut q.append, "Append to current selection");
            if ui.button("Select").clicked() {
                apply = true;
            }
        });
        if apply {
            let t = &mut self.tabs[self.active];
            let doc = &t.doc;
            let ids: Vec<u64> = doc
                .entities
                .iter()
                .filter(|e| doc.is_visible(e) && !doc.is_locked(e))
                .filter(|e| q.kind.is_empty() || e.shape.type_name() == q.kind)
                .filter(|e| q.layer.is_empty() || e.layer.eq_ignore_ascii_case(&q.layer))
                .filter(|e| q.color.is_none_or(|c| doc.color_of(e) == c || e.color == Color::Aci(c)))
                .map(|e| e.id)
                .collect();
            let n = ids.len();
            if q.append {
                for id in ids {
                    if !t.selection.contains(&id) {
                        t.selection.push(id);
                    }
                }
            } else {
                t.selection = ids;
            }
            self.log.push(format!("{n} object(s) selected."));
            return;
        }
        if open {
            self.dlg.qselect = Some(q);
        }
    }

    pub fn text_edit_dialog(&mut self, ctx: &egui::Context) {
        let Some((id, mut text)) = self.dlg.text_edit.take() else { return };
        let mut open = true;
        let mut ok = false;
        egui::Window::new("Edit Text").open(&mut open).default_width(420.0).anchor(egui::Align2::CENTER_CENTER, vec2(0.0, 0.0)).show(
            ctx,
            |ui| {
                let r = ui.add(egui::TextEdit::multiline(&mut text).desired_rows(4).desired_width(400.0));
                r.request_focus();
                ui.label(
                    RichText::new("Use Ø ° ± directly; for dimensions, <> inserts the measured value.").size(11.0).color(theme::TEXT_DIM),
                );
                if ui.button("OK").clicked() {
                    ok = true;
                }
            },
        );
        if ok {
            let t = self.tab();
            t.checkpoint();
            if let Some(e) = t.doc.get_mut(id) {
                match &mut e.shape {
                    Shape::Text(tx) => tx.content = text.lines().next().unwrap_or("").to_string(),
                    Shape::MText(m) => m.content = text,
                    Shape::Leader(l) => l.text = text,
                    Shape::Dim(d) => d.text = if text.trim().is_empty() { None } else { Some(text) },
                    _ => {}
                }
            }
            return;
        }
        if open {
            self.dlg.text_edit = Some((id, text));
        }
    }

    /// Start editing the text of an entity (double-click / TEXTEDIT).
    pub fn edit_text_of(&mut self, id: u64) -> bool {
        let t = &self.tabs[self.active];
        let Some(e) = t.doc.get(id) else { return false };
        let text = match &e.shape {
            Shape::Text(tx) => tx.content.clone(),
            Shape::MText(m) => m.content.clone(),
            Shape::Leader(l) => l.text.clone(),
            Shape::Dim(d) => d.text.clone().unwrap_or_default(),
            _ => return false,
        };
        self.dlg.text_edit = Some((id, text));
        true
    }

    pub fn units_dialog(&mut self, ctx: &egui::Context) {
        if !self.dlg.units {
            return;
        }
        let mut open = true;
        let t = &mut self.tabs[self.active];
        egui::Window::new("Drawing Units").open(&mut open).default_width(320.0).show(ctx, |ui| {
            egui::Grid::new("units").num_columns(2).spacing([10.0, 6.0]).show(ui, |ui| {
                ui.label("Insertion units");
                egui::ComboBox::from_id_salt("units_name").selected_text(&t.doc.sheet.title.units).show_ui(ui, |ui| {
                    for u in ["mm", "cm", "m", "in", "ft"] {
                        if ui.selectable_value(&mut t.doc.sheet.title.units, u.to_string(), u).changed() {
                            t.dirty = true;
                        }
                    }
                });
                ui.end_row();
                ui.label("Linear precision");
                if ui.add(egui::DragValue::new(&mut t.doc.dimstyle.decimals).range(0..=8)).changed() {
                    t.dirty = true;
                }
                ui.end_row();
                ui.label("Example");
                ui.label(t.doc.dimstyle.format(1234.567891));
                ui.end_row();
            });
            ui.label(RichText::new("Angles: decimal degrees, counter-clockwise, 0° = east.").size(11.0).color(theme::TEXT_DIM));
        });
        self.dlg.units = open;
    }
}

#[allow(dead_code)]
fn _qs(_: QSelect) {}
