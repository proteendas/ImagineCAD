// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Proteen Das

//! Text styles (STYLE) with TrueType fonts.

use crate::app::App;
use crate::theme;
use eframe::egui::{self, RichText};
use icad_core::fonts::{self, TextStyle};
use icad_core::geom::V2;
use icad_core::prim::{Prim, Text};
use icad_core::Shape;

/// STYLE dialog state.
#[derive(Default)]
pub struct StyleDialog {
    pub font_filter: String,
    pub loading: bool,
}

impl App {
    pub fn style_dialog(&mut self, ctx: &egui::Context) {
        let Some(mut d) = self.dlg.style.take() else { return };
        // List the installed fonts once, in the background.
        if !d.loading && fonts::system_fonts_if_ready().is_none() {
            d.loading = true;
            let c = ctx.clone();
            std::thread::spawn(move || {
                fonts::system_fonts();
                c.request_repaint();
            });
        }
        let mut open = true;
        let t = &mut self.tabs[self.active];
        let before = (t.doc.text_styles.clone(), t.doc.text_style.clone());
        egui::Window::new(crate::i18n::tr("Text Styles")).open(&mut open).collapsible(false).default_width(460.0).show(ctx, |ui| {
            let doc = &mut t.doc;
            ui.horizontal(|ui| {
                ui.label(crate::i18n::tr("Current style"));
                egui::ComboBox::from_id_salt("ts_cur").selected_text(doc.text_style.clone()).show_ui(ui, |ui| {
                    for s in &doc.text_styles {
                        ui.selectable_value(&mut doc.text_style, s.name.clone(), &s.name);
                    }
                });
                if ui.button(crate::i18n::tr("New")).clicked() {
                    let mut s = doc.text_style(&doc.text_style).cloned().unwrap_or_else(TextStyle::standard);
                    let mut k = 1;
                    while doc.text_style(&format!("Style{k}")).is_some() {
                        k += 1;
                    }
                    s.name = format!("Style{k}");
                    doc.text_style = s.name.clone();
                    doc.text_styles.push(s);
                }
                let cur = doc.text_style.clone();
                let in_use = doc.entities.iter().any(|e| match &e.shape {
                    Shape::Text(t) => t.style.as_deref().is_some_and(|s| s.eq_ignore_ascii_case(&cur)),
                    Shape::MText(m) => m.style.as_deref().is_some_and(|s| s.eq_ignore_ascii_case(&cur)),
                    _ => false,
                });
                let deletable = !cur.eq_ignore_ascii_case("Standard") && !in_use;
                if ui
                    .add_enabled(deletable, egui::Button::new("Delete"))
                    .on_disabled_hover_text("Standard and styles in use cannot be deleted")
                    .clicked()
                {
                    doc.text_styles.retain(|s| !s.name.eq_ignore_ascii_case(&cur));
                    doc.text_style = "Standard".into();
                }
            });
            ui.separator();
            let cur = doc.text_style.clone();
            let Some(st) = doc.text_styles.iter_mut().find(|s| s.name.eq_ignore_ascii_case(&cur)) else { return };
            egui::Grid::new("ts").num_columns(2).spacing([10.0, 6.0]).show(ui, |ui| {
                ui.label(crate::i18n::tr("Name"));
                let mut name = st.name.clone();
                let standard = st.name.eq_ignore_ascii_case("Standard");
                if ui.add_enabled(!standard, egui::TextEdit::singleline(&mut name)).changed() && !name.trim().is_empty() {
                    st.name = name.clone();
                    doc.text_style = name;
                }
                ui.end_row();
                ui.label(crate::i18n::tr("Font"));
                ui.vertical(|ui| {
                    let shown = if st.file.is_empty() { "Standard drafting font".to_string() } else { st.font.clone() };
                    egui::ComboBox::from_id_salt("ts_font")
                        .width(260.0)
                        .height(360.0)
                        .selected_text(shown)
                        .close_behavior(egui::PopupCloseBehavior::CloseOnClickOutside)
                        .show_ui(ui, |ui| {
                            ui.add(egui::TextEdit::singleline(&mut d.font_filter).hint_text(crate::i18n::tr("Search fonts")));
                            if ui.selectable_label(st.file.is_empty(), "Standard drafting font").clicked() {
                                st.file.clear();
                                st.font.clear();
                                ui.memory_mut(|m| m.close_popup());
                            }
                            match fonts::system_fonts_if_ready() {
                                None => {
                                    ui.horizontal(|ui| {
                                        ui.spinner();
                                        ui.label(crate::i18n::tr("Listing installed fonts…"));
                                    });
                                }
                                Some(list) => {
                                    let q = d.font_filter.to_lowercase();
                                    for f in list.iter().filter(|f| f.name.to_lowercase().contains(&q)).take(400) {
                                        if ui.selectable_label(st.font == f.name, &f.name).clicked() {
                                            st.file = f.file_ref();
                                            st.font = f.name.clone();
                                            ui.memory_mut(|m| m.close_popup());
                                        }
                                    }
                                }
                            }
                        });
                });
                ui.end_row();
                ui.label(crate::i18n::tr("Width factor"));
                ui.add(egui::DragValue::new(&mut st.width_factor).speed(0.01).range(0.1..=10.0));
                ui.end_row();
                ui.label(crate::i18n::tr("Oblique angle °"));
                ui.add(egui::DragValue::new(&mut st.oblique).speed(0.5).range(-85.0..=85.0));
                ui.end_row();
                ui.label(crate::i18n::tr("Fixed height"));
                ui.add(egui::DragValue::new(&mut st.height).speed(0.1).range(0.0..=1e6))
                    .on_hover_text(crate::i18n::tr("0 = choose the height for each text"));
                ui.end_row();
            });
            // Preview with the style applied.
            let mut sample = Text::new(V2::ZERO, 5.0, "ImagineCAD 0123 Ø25 ±0.1");
            sample.font = st.file.clone();
            sample.width_factor = st.width_factor;
            sample.oblique = st.oblique.to_radians();
            crate::canvas::preview_prims(ui, &[Prim::Text(sample)], egui::vec2(420.0, 70.0));
            ui.label(
                RichText::new(crate::i18n::tr("New text uses the current style. Change a text's style in the Properties palette."))
                    .size(11.0)
                    .color(theme::TEXT_DIM),
            );
        });
        // Renamed styles carry their texts along.
        for (o, n) in before.0.iter().zip(&t.doc.text_styles) {
            if o.name != n.name {
                for e in &mut t.doc.entities {
                    let s = match &mut e.shape {
                        Shape::Text(t) => &mut t.style,
                        Shape::MText(m) => &mut m.style,
                        _ => continue,
                    };
                    if s.as_deref() == Some(o.name.as_str()) {
                        *s = Some(n.name.clone());
                    }
                }
            }
        }
        if (t.doc.text_styles.clone(), t.doc.text_style.clone()) != before {
            t.dirty = true;
        }
        if open {
            self.dlg.style = Some(d);
        }
    }
}
