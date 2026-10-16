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

fn base_fonts() -> egui::FontDefinitions {
    // The UI font lacks arrows and block symbols (→ ● ▏ ⇄ …); fall back to the
    // bundled monospace font for those instead of drawing empty boxes.
    let mut fonts = egui::FontDefinitions::default();
    if let Some(prop) = fonts.families.get_mut(&egui::FontFamily::Proportional) {
        prop.push("Hack".to_owned());
    }
    fonts
}

/// Installed fonts for Chinese or Japanese, best first.
fn script_font_names(lang: &str) -> &'static [&'static str] {
    match lang {
        "zh" => &[
            "PingFang SC",
            "Hiragino Sans GB",
            ".Hiragino Sans GB Interface",
            "STHeiti",
            "Heiti SC",
            "Microsoft YaHei",
            "Noto Sans CJK SC",
            "Noto Sans SC",
            "Source Han Sans SC",
            "WenQuanYi Micro Hei",
            "SimHei",
            "SimSun",
            "Noto Sans CJK JP",
        ],
        "ja" => &[
            "Hiragino Sans",
            "Hiragino Kaku Gothic ProN",
            "Yu Gothic",
            "Yu Gothic UI",
            "Meiryo",
            "Noto Sans CJK JP",
            "Noto Sans JP",
            "Source Han Sans JP",
            "IPAGothic",
            "MS Gothic",
            "Noto Sans CJK SC",
        ],
        _ => &[],
    }
}

/// Fonts for the interface language: languages with their own script get an
/// installed font for it as a fallback (looked up off the UI thread; the
/// interface shows the glyphs as soon as it is loaded).
pub fn set_language_fonts(ctx: &egui::Context, lang: &str) {
    let names = script_font_names(lang);
    ctx.set_fonts(base_fonts());
    if names.is_empty() {
        return;
    }
    let ctx = ctx.clone();
    let lang = lang.to_string();
    std::thread::spawn(move || {
        let load = |path: &std::path::Path, index: u32| {
            let Ok(data) = std::fs::read(path) else { return false };
            let mut defs = base_fonts();
            let mut fd = egui::FontData::from_owned(data);
            fd.index = index;
            defs.font_data.insert("script".to_owned(), std::sync::Arc::new(fd));
            for fam in [egui::FontFamily::Proportional, egui::FontFamily::Monospace] {
                if let Some(list) = defs.families.get_mut(&fam) {
                    list.push("script".to_owned());
                }
            }
            ctx.set_fonts(defs);
            ctx.request_repaint();
            true
        };
        // Common font files first (quick), then any installed family by name.
        let probe = if lang == "ja" { "日本語あア" } else { "中文简体" };
        for (path, index) in script_font_files(&lang) {
            let p = std::path::Path::new(path);
            if p.is_file() && icad_core::fonts::has_glyphs(p, *index, probe) && load(p, *index) {
                return;
            }
        }
        let fonts = icad_core::fonts::system_fonts();
        // Faces are named with their weight ("Hiragino Sans W3"): take the
        // regular weight of the first family that is installed.
        let found = names.iter().find_map(|n| {
            let family: Vec<_> = fonts
                .iter()
                .filter(|f| {
                    f.name.eq_ignore_ascii_case(n) || f.name.to_ascii_lowercase().starts_with(&format!("{} ", n.to_ascii_lowercase()))
                })
                .collect();
            family
                .iter()
                .find(|f| f.name.eq_ignore_ascii_case(n))
                .or_else(|| family.iter().find(|f| f.name.ends_with(" W3") || f.name.ends_with(" Regular")))
                .or(family.first())
                .copied()
        });
        if let Some(f) = found {
            load(&f.path, f.index);
        }
    });
}

/// Font files (and collection index) that usually carry the script.
fn script_font_files(lang: &str) -> &'static [(&'static str, u32)] {
    match lang {
        "zh" => &[
            ("/System/Library/Fonts/Hiragino Sans GB.ttc", 0),
            ("/System/Library/Fonts/STHeiti Light.ttc", 1),
            ("C:\\Windows\\Fonts\\msyh.ttc", 0),
            ("C:\\Windows\\Fonts\\simhei.ttf", 0),
            ("C:\\Windows\\Fonts\\simsun.ttc", 0),
            ("/usr/share/fonts/opentype/noto/NotoSansCJK-Regular.ttc", 2),
            ("/usr/share/fonts/noto-cjk/NotoSansCJK-Regular.ttc", 2),
            ("/usr/share/fonts/google-noto-cjk/NotoSansCJK-Regular.ttc", 2),
            ("/usr/share/fonts/truetype/wqy/wqy-microhei.ttc", 0),
            ("/usr/share/fonts/wenquanyi/wqy-microhei/wqy-microhei.ttc", 0),
        ],
        "ja" => &[
            ("/System/Library/Fonts/ヒラギノ角ゴシック W3.ttc", 0),
            // The same name as stored on disk (decomposed).
            ("/System/Library/Fonts/ヒラキ\u{3099}ノ角コ\u{3099}シック W3.ttc", 0),
            ("C:\\Windows\\Fonts\\YuGothR.ttc", 0),
            ("C:\\Windows\\Fonts\\meiryo.ttc", 0),
            ("C:\\Windows\\Fonts\\msgothic.ttc", 0),
            ("/usr/share/fonts/opentype/noto/NotoSansCJK-Regular.ttc", 0),
            ("/usr/share/fonts/noto-cjk/NotoSansCJK-Regular.ttc", 0),
            ("/usr/share/fonts/google-noto-cjk/NotoSansCJK-Regular.ttc", 0),
            ("/usr/share/fonts/opentype/ipafont-gothic/ipag.ttf", 0),
        ],
        _ => &[],
    }
}

pub fn apply(ctx: &egui::Context) {
    set_language_fonts(ctx, &crate::i18n::language());

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
