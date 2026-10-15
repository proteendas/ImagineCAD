// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Proteen Das

//! ImagineCAD dark theme.

use eframe::egui::{self, Color32, FontId, TextStyle};

pub const CANVAS: Color32 = Color32::from_rgb(33, 40, 48);
pub const PAPER: Color32 = Color32::from_rgb(255, 255, 255);
pub const PAPER_SURROUND: Color32 = Color32::from_rgb(90, 96, 106);
pub const GRID_MINOR: Color32 = Color32::from_rgb(40, 48, 58);
pub const GRID_MAJOR: Color32 = Color32::from_rgb(52, 62, 76);
pub const TITLE_BAR: Color32 = Color32::from_rgb(34, 41, 51);
pub const RIBBON_TABS: Color32 = Color32::from_rgb(42, 49, 61);
pub const RIBBON: Color32 = Color32::from_rgb(59, 68, 83);
pub const RIBBON_PANEL_TITLE: Color32 = Color32::from_rgb(50, 58, 72);
pub const PANEL: Color32 = Color32::from_rgb(47, 55, 67);
pub const STATUS: Color32 = Color32::from_rgb(42, 49, 61);
pub const CMDLINE: Color32 = Color32::from_rgb(36, 43, 53);
pub const TEXT: Color32 = Color32::from_rgb(222, 226, 232);
pub const TEXT_DIM: Color32 = Color32::from_rgb(150, 158, 170);
/// Brand colour (matches the app icon).
pub const BRAND: Color32 = Color32::from_rgb(12, 118, 132);
pub const ACCENT: Color32 = Color32::from_rgb(51, 153, 255);
pub const AI_ACCENT: Color32 = Color32::from_rgb(170, 120, 255);
pub const HOVER: Color32 = Color32::from_rgb(74, 86, 104);
pub const ACTIVE: Color32 = Color32::from_rgb(38, 96, 160);
pub const GRIP: Color32 = Color32::from_rgb(0, 118, 255);
pub const GRIP_HOT: Color32 = Color32::from_rgb(255, 40, 40);
pub const SNAP: Color32 = Color32::from_rgb(255, 214, 10);
pub const SELECT_HL: Color32 = Color32::from_rgb(120, 190, 255);
pub const WINDOW_FILL: Color32 = Color32::from_rgba_premultiplied(0, 40, 110, 60);
pub const CROSSING_FILL: Color32 = Color32::from_rgba_premultiplied(0, 90, 30, 60);
pub const PREVIEW: Color32 = Color32::from_rgb(160, 220, 255);
pub const WARN: Color32 = Color32::from_rgb(255, 196, 64);
pub const ERR: Color32 = Color32::from_rgb(255, 110, 100);

pub fn apply(ctx: &egui::Context) {
    // The UI font lacks arrows and block symbols (→ ● ▏ ⇄ …); fall back to the
    // bundled monospace font for those instead of drawing empty boxes.
    let mut fonts = egui::FontDefinitions::default();
    if let Some(prop) = fonts.families.get_mut(&egui::FontFamily::Proportional) {
        prop.push("Hack".to_owned());
    }
    ctx.set_fonts(fonts);

    let mut v = egui::Visuals::dark();
    v.panel_fill = PANEL;
    v.window_fill = Color32::from_rgb(52, 60, 74);
    v.extreme_bg_color = Color32::from_rgb(30, 36, 44);
    v.faint_bg_color = Color32::from_rgb(56, 65, 80);
    v.selection.bg_fill = ACTIVE;
    v.hyperlink_color = ACCENT;
    v.widgets.inactive.weak_bg_fill = Color32::from_rgb(66, 76, 92);
    v.widgets.inactive.bg_fill = Color32::from_rgb(66, 76, 92);
    v.widgets.hovered.weak_bg_fill = HOVER;
    v.widgets.hovered.bg_fill = HOVER;
    v.widgets.active.weak_bg_fill = ACTIVE;
    v.widgets.noninteractive.fg_stroke.color = TEXT;
    v.widgets.inactive.fg_stroke.color = TEXT;
    ctx.set_visuals(v);
    let mut style = (*ctx.style()).clone();
    style.text_styles.insert(TextStyle::Body, FontId::proportional(13.0));
    style.text_styles.insert(TextStyle::Button, FontId::proportional(13.0));
    style.text_styles.insert(TextStyle::Small, FontId::proportional(11.0));
    style.text_styles.insert(TextStyle::Monospace, FontId::monospace(13.0));
    style.spacing.item_spacing = egui::vec2(6.0, 4.0);
    style.spacing.button_padding = egui::vec2(6.0, 3.0);
    ctx.set_style(style);
}

pub fn aci(i: u8, on_paper: bool) -> Color32 {
    if on_paper && (i == 7 || i == 255) {
        return Color32::BLACK;
    }
    let [r, g, b] = icad_core::style::aci_rgb(i);
    if on_paper {
        match i {
            2 => return Color32::from_rgb(190, 150, 0),
            3 => return Color32::from_rgb(0, 150, 0),
            4 => return Color32::from_rgb(0, 140, 165),
            _ => {}
        }
    }
    Color32::from_rgb(r, g, b)
}
