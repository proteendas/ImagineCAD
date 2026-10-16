// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Proteen Das

//! Ribbon toolbar: tabs → panels → tool buttons. Every button issues a
//! command string, exactly as if it had been typed on the command line.
//!
//! Panels stay compact: related tools share a split button (main action plus
//! a ▾ list of variants), and each panel title opens a menu with its less
//! frequent tools, so a whole tab fits on screen without scrolling.

use crate::icons::{self, Icon};
use crate::theme;
use eframe::egui::{self, vec2, Align2, Color32, FontId, Id, PopupCloseBehavior, Rect, Response, RichText, Sense, Ui};

pub const TABS: [&str; 6] = ["Home", "Insert", "Annotate", "View", "Output", "AI"];

pub const RIBBON_H: f32 = 96.0;

/// A tool in a dropdown: icon, label, command.
type Item<'a> = (Icon, &'a str, &'a str);

fn label_w(ui: &Ui, s: &str, size: f32) -> f32 {
    ui.fonts(|f| f.layout_no_wrap(s.into(), FontId::proportional(size), Color32::WHITE).size().x)
}

/// One row of a dropdown menu.
fn menu_item(ui: &mut Ui, (icon, label, cmd): Item, out: &mut Vec<String>) {
    let w = (label_w(ui, label, 12.5) + 34.0).max(ui.available_width()).max(170.0);
    let (rect, resp) = ui.allocate_exact_size(vec2(w, 23.0), Sense::click());
    let p = ui.painter();
    if resp.hovered() {
        p.rect_filled(rect, 3.0, theme::ACTIVE);
    }
    icons::draw(p, Rect::from_center_size(rect.left_center() + vec2(12.0, 0.0), vec2(16.0, 16.0)), icon, theme::TEXT);
    p.text(rect.left_center() + vec2(27.0, 0.0), Align2::LEFT_CENTER, label, FontId::proportional(12.5), theme::TEXT);
    if resp.on_hover_text(cmd).clicked() {
        out.push(cmd.to_string());
        ui.memory_mut(|m| m.close_popup());
    }
}

/// Show `items` in a popup under `anchor` when `open` was clicked.
fn dropdown(ui: &mut Ui, id: Id, anchor: &Response, open: bool, items: &[Item], out: &mut Vec<String>) {
    if open {
        ui.memory_mut(|m| m.toggle_popup(id));
    }
    egui::popup::popup_below_widget(ui, id, anchor, PopupCloseBehavior::CloseOnClickOutside, |ui| {
        ui.spacing_mut().item_spacing.y = 1.0;
        for it in items {
            menu_item(ui, *it, out);
        }
    });
}

fn arrow(p: &egui::Painter, at: egui::Pos2, col: Color32) {
    p.add(egui::Shape::convex_polygon(vec![at + vec2(-3.5, -1.5), at + vec2(3.5, -1.5), at + vec2(0.0, 2.5)], col, egui::Stroke::NONE));
}

/// Large button; with `items`, the lower strip (▾) opens the variants.
fn big_split(ui: &mut Ui, icon: Icon, label: &str, tip: &str, cmd: &str, items: &[Item], out: &mut Vec<String>) {
    let w = (label.split('\n').map(|l| label_w(ui, l, 11.5)).fold(0.0, f32::max) + 12.0).max(50.0);
    let (rect, resp) = ui.allocate_exact_size(vec2(w, 72.0), Sense::hover());
    let id = ui.id().with(("big", label));
    let split = !items.is_empty();
    let main_r = if split { Rect::from_min_max(rect.min, rect.max - vec2(0.0, 15.0)) } else { rect };
    let main = ui.interact(main_r, id.with("main"), Sense::click());
    let p = ui.painter().clone();
    if main.hovered() {
        p.rect_filled(main_r, 3.0, theme::HOVER);
    }
    let ir = Rect::from_center_size(rect.center_top() + vec2(0.0, 19.0), vec2(30.0, 30.0));
    let col = if icon == Icon::ImageToCad { Color32::WHITE } else { theme::TEXT };
    icons::draw(&p, ir, icon, col);
    let mut y = rect.top() + 40.0;
    for line in label.split('\n') {
        p.text(egui::pos2(rect.center().x, y), Align2::CENTER_TOP, line, FontId::proportional(11.5), theme::TEXT);
        y += 13.0;
    }
    if main.on_hover_text(tip).clicked() {
        out.push(cmd.to_string());
    }
    if split {
        let ar = Rect::from_min_max(rect.max - vec2(rect.width(), 15.0), rect.max);
        let a = ui.interact(ar, id.with("arrow"), Sense::click());
        if a.hovered() {
            p.rect_filled(ar, 3.0, theme::HOVER);
        }
        arrow(&p, ar.center(), theme::TEXT_DIM);
        let open = a.clicked();
        dropdown(ui, id.with("pop"), &a, open, items, out);
    }
    let _ = resp;
}

fn big(ui: &mut Ui, icon: Icon, label: &str, tip: &str, cmd: &str, out: &mut Vec<String>) {
    big_split(ui, icon, label, tip, cmd, &[], out);
}

/// Small button with a label; with `items`, a ▾ opens the variants.
fn small_split(ui: &mut Ui, icon: Icon, label: &str, tip: &str, cmd: &str, items: &[Item], out: &mut Vec<String>) {
    let split = !items.is_empty();
    let w = 26.0 + label_w(ui, label, 12.0) + if split { 16.0 } else { 4.0 };
    let (rect, _) = ui.allocate_exact_size(vec2(w.max(30.0), 21.0), Sense::hover());
    let id = ui.id().with(("small", label, cmd));
    let main_r = if split { Rect::from_min_max(rect.min, rect.max - vec2(14.0, 0.0)) } else { rect };
    let main = ui.interact(main_r, id.with("main"), Sense::click());
    let p = ui.painter().clone();
    if main.hovered() {
        p.rect_filled(main_r, 3.0, theme::HOVER);
    }
    icons::draw(&p, Rect::from_center_size(rect.left_center() + vec2(11.0, 0.0), vec2(16.0, 16.0)), icon, theme::TEXT);
    p.text(rect.left_center() + vec2(23.0, 0.0), Align2::LEFT_CENTER, label, FontId::proportional(12.0), theme::TEXT);
    if main.on_hover_text(tip).clicked() {
        out.push(cmd.to_string());
    }
    if split {
        let ar = Rect::from_min_max(rect.max - vec2(14.0, rect.height()), rect.max);
        let a = ui.interact(ar, id.with("arrow"), Sense::click());
        if a.hovered() {
            p.rect_filled(ar, 3.0, theme::HOVER);
        }
        arrow(&p, ar.center(), theme::TEXT_DIM);
        let open = a.clicked();
        dropdown(ui, id.with("pop"), &a, open, items, out);
    }
}

fn small(ui: &mut Ui, icon: Icon, label: &str, tip: &str, cmd: &str, out: &mut Vec<String>) {
    small_split(ui, icon, label, tip, cmd, &[], out);
}

/// Icon-only button (the tooltip names it).
fn tool(ui: &mut Ui, icon: Icon, tip: &str, cmd: &str, out: &mut Vec<String>) {
    let (rect, resp) = ui.allocate_exact_size(vec2(24.0, 21.0), Sense::click());
    let p = ui.painter();
    if resp.hovered() {
        p.rect_filled(rect, 3.0, theme::HOVER);
    }
    icons::draw(p, Rect::from_center_size(rect.center(), vec2(16.0, 16.0)), icon, theme::TEXT);
    if resp.on_hover_text(tip).clicked() {
        out.push(cmd.to_string());
    }
}

/// Toggle-style small button (highlighted when on).
pub fn toggle(ui: &mut Ui, icon: Icon, label: &str, tip: &str, on: bool, cmd: &str, out: &mut Vec<String>) {
    let w = 26.0 + label_w(ui, label, 12.0);
    let (rect, resp) = ui.allocate_exact_size(vec2(w, 21.0), Sense::click());
    let p = ui.painter();
    if on {
        p.rect_filled(rect, 3.0, theme::ACTIVE);
    } else if resp.hovered() {
        p.rect_filled(rect, 3.0, theme::HOVER);
    }
    icons::draw(p, Rect::from_center_size(rect.left_center() + vec2(11.0, 0.0), vec2(16.0, 16.0)), icon, theme::TEXT);
    p.text(rect.left_center() + vec2(23.0, 0.0), Align2::LEFT_CENTER, label, FontId::proportional(12.0), theme::TEXT);
    if resp.on_hover_text(tip).clicked() {
        out.push(cmd.to_string());
    }
}

/// A ribbon panel with a caption bar underneath. With `more`, the caption
/// (`Title ▾`) opens a menu of the panel's other tools.
fn panel(ui: &mut Ui, title: &str, more: &[Item], out: &mut Vec<String>, add: impl FnOnce(&mut Ui, &mut Vec<String>)) {
    let resp = ui.vertical(|ui| {
        ui.set_min_height(RIBBON_H - 4.0);
        ui.horizontal(|ui| {
            ui.set_min_height(RIBBON_H - 22.0);
            add(ui, out);
        });
    });
    let r = resp.response.rect;
    let bar = Rect::from_min_max(egui::pos2(r.left() - 2.0, r.bottom() - 17.0), egui::pos2(r.right() + 2.0, r.bottom()));
    let id = ui.id().with(("panel", title));
    let b = ui.interact(bar, id, if more.is_empty() { Sense::hover() } else { Sense::click() });
    let p = ui.painter().clone();
    p.rect_filled(bar, 0.0, if b.hovered() && !more.is_empty() { theme::HOVER } else { theme::RIBBON_PANEL_TITLE });
    if more.is_empty() {
        p.text(bar.center(), Align2::CENTER_CENTER, title, FontId::proportional(11.0), theme::TEXT_DIM);
    } else {
        let tw = label_w(ui, title, 11.0);
        p.text(bar.center() - vec2(5.0, 0.0), Align2::CENTER_CENTER, title, FontId::proportional(11.0), theme::TEXT_DIM);
        arrow(&p, bar.center() + vec2(tw / 2.0 + 4.0, 0.0), theme::TEXT_DIM);
        let open = b.clone().on_hover_text(format!("More {title} tools")).clicked();
        dropdown(ui, id.with("more"), &b, open, more, out);
    }
    let x = r.right() + 5.0;
    p.line_segment(
        [egui::pos2(x, r.top() + 4.0), egui::pos2(x, r.bottom() - 4.0)],
        egui::Stroke::new(1.0f32, Color32::from_rgb(78, 90, 108)),
    );
    ui.add_space(6.0);
}

fn column(ui: &mut Ui, add: impl FnOnce(&mut Ui)) {
    ui.vertical(|ui| {
        ui.spacing_mut().item_spacing.y = 2.0;
        add(ui);
    });
}

pub struct RibbonState<'a> {
    pub grid: bool,
    pub lwt: bool,
    pub props: bool,
    pub sheet: bool,
    pub layout: bool,
    pub ai_ready: bool,
    pub chat: bool,
    pub layer_ui: &'a mut dyn FnMut(&mut Ui),
}

pub fn show(ui: &mut Ui, tab: &mut usize, st: RibbonState, out: &mut Vec<String>) {
    // Tab strip.
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 2.0;
        for (i, t) in TABS.iter().enumerate() {
            let sel = *tab == i;
            let txt = if *t == "AI" { RichText::new("AI").color(theme::AI_ACCENT).strong() } else { RichText::new(*t).color(theme::TEXT) };
            let b = egui::Button::new(txt).fill(if sel { theme::RIBBON } else { theme::RIBBON_TABS }).corner_radius(egui::CornerRadius {
                nw: 3,
                ne: 3,
                sw: 0,
                se: 0,
            });
            if ui.add(b).clicked() {
                *tab = i;
            }
        }
    });
    let frame = egui::Frame::NONE.fill(theme::RIBBON).inner_margin(egui::Margin::symmetric(6, 3));
    frame.show(ui, |ui| {
        ui.set_min_height(RIBBON_H);
        ui.set_width(ui.available_width());
        // Panels are sized to fit; scrolling only kicks in on very narrow windows.
        egui::ScrollArea::horizontal().id_salt("ribbon_scroll").show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 2.0;
                match *tab {
                    0 => home(ui, st, out),
                    1 => insert(ui, out),
                    2 => annotate(ui, out),
                    3 => view(ui, &st, out),
                    4 => output(ui, out),
                    _ => ai(ui, &st, out),
                }
            });
        });
    });
}

// ------------------------------------------------------------------ tool sets

const CIRCLES: &[Item] = &[
    (Icon::Circle, "Center, Radius", "CIRCLE"),
    (Icon::Circle, "Center, Diameter", "CIRCLE D"),
    (Icon::Circle, "2-Point", "CIRCLE 2P"),
    (Icon::Circle, "3-Point", "CIRCLE 3P"),
];
const ARCS: &[Item] = &[(Icon::Arc, "3-Point", "ARC"), (Icon::Arc, "Center, Start, End", "ARC C")];
const SHAPES: &[Item] =
    &[(Icon::Rect, "Rectangle", "RECTANG"), (Icon::Polygon, "Polygon", "POLYGON"), (Icon::Ellipse, "Ellipse", "ELLIPSE")];
const HATCHES: &[Item] = &[(Icon::Hatch, "Hatch", "HATCH"), (Icon::Hatch, "Solid fill", "HATCH P SOLID")];
const CURVES: &[Item] = &[(Icon::Spline, "Spline", "SPLINE"), (Icon::Ellipse, "Ellipse", "ELLIPSE"), (Icon::Point, "Point", "POINT")];
const DRAW_MORE: &[Item] = &[
    (Icon::Ellipse, "Ellipse", "ELLIPSE"),
    (Icon::Spline, "Spline", "SPLINE"),
    (Icon::Point, "Point", "POINT"),
    (Icon::XLine, "Construction Line", "XLINE"),
    (Icon::XLine, "Ray", "RAY"),
    (Icon::Polygon, "Polygon", "POLYGON"),
    (Icon::Point, "Divide", "DIVIDE"),
    (Icon::Measure, "Measure", "MEASURE"),
    (Icon::Hatch, "Hatch", "HATCH"),
    (Icon::Polyline, "Boundary", "BOUNDARY"),
    (Icon::Region, "Region", "REGION"),
];
const TRIMS: &[Item] = &[(Icon::Trim, "Trim", "TRIM"), (Icon::Extend, "Extend", "EXTEND")];
const CORNERS: &[Item] = &[(Icon::Fillet, "Fillet", "FILLET"), (Icon::Chamfer, "Chamfer", "CHAMFER")];
const ARRAYS: &[Item] = &[(Icon::Array, "Rectangular Array", "ARRAYRECT"), (Icon::PolarArray, "Polar Array", "ARRAYPOLAR")];
const MODIFY_MORE: &[Item] = &[
    (Icon::Extend, "Extend", "EXTEND"),
    (Icon::Chamfer, "Chamfer", "CHAMFER"),
    (Icon::Break, "Break", "BREAK"),
    (Icon::Break, "Break at Point", "BREAKATPOINT"),
    (Icon::Join, "Join", "JOIN"),
    (Icon::Lengthen, "Lengthen", "LENGTHEN"),
    (Icon::PolarArray, "Polar Array", "ARRAYPOLAR"),
    (Icon::Match, "Match Properties", "MATCHPROP"),
    (Icon::Point, "Divide", "DIVIDE"),
    (Icon::Measure, "Measure", "MEASURE"),
    (Icon::Erase, "Overkill (delete duplicates)", "OVERKILL"),
    (Icon::Undo, "Oops (restore erased)", "OOPS"),
    (Icon::Region, "Union (regions)", "UNION"),
    (Icon::Region, "Subtract (regions)", "SUBTRACT"),
    (Icon::Region, "Intersect (regions)", "INTERSECT"),
];
const TEXTS: &[Item] = &[
    (Icon::MText, "Multiline Text", "MTEXT"),
    (Icon::Text, "Single Line", "TEXT"),
    (Icon::Properties, "Edit Text", "TEXTEDIT"),
    (Icon::Text, "Text Style", "STYLE"),
    (Icon::Check, "Check Spelling", "SPELL"),
];
const DIMS: &[Item] = &[
    (Icon::DimLinear, "Linear", "DIMLINEAR"),
    (Icon::DimAligned, "Aligned", "DIMALIGNED"),
    (Icon::DimAngular, "Angular", "DIMANGULAR"),
    (Icon::DimArc, "Arc Length", "DIMARC"),
    (Icon::DimRadius, "Radius", "DIMRADIUS"),
    (Icon::DimDiameter, "Diameter", "DIMDIAMETER"),
    (Icon::DimOrdinate, "Ordinate", "DIMORDINATE"),
    (Icon::QDim, "Quick Dimension", "QDIM"),
    (Icon::DimBaseline, "Baseline", "DIMBASELINE"),
    (Icon::DimContinue, "Continue", "DIMCONTINUE"),
];
const LEADERS: &[Item] = &[
    (Icon::Leader, "Multileader", "MLEADER"),
    (Icon::Leader, "Leader", "LEADER"),
    (Icon::DimStyle, "Multileader Style", "MLEADERSTYLE"),
    (Icon::CenterMark, "Center Mark", "CENTERMARK"),
];
const SYMBOLS: &[Item] = &[
    (Icon::Tolerance, "Geometric Tolerance", "TOLERANCE"),
    (Icon::Datum, "Datum Symbol", "DATUM"),
    (Icon::Surface, "Surface Texture", "SURFFINISH"),
    (Icon::Weld, "Weld Symbol", "WELD"),
];
const ANNOT_MORE: &[Item] = &[
    (Icon::Tolerance, "Geometric Tolerance", "TOLERANCE"),
    (Icon::Datum, "Datum Symbol", "DATUM"),
    (Icon::Surface, "Surface Texture", "SURFFINISH"),
    (Icon::Weld, "Weld Symbol", "WELD"),
    (Icon::Table, "Table", "TABLE"),
    (Icon::CenterMark, "Center Mark", "CENTERMARK"),
    (Icon::DimStyle, "Dimension Style", "DIMSTYLE"),
    (Icon::DimStyle, "Multileader Style", "MLEADERSTYLE"),
    (Icon::Link, "Reassociate Dimensions", "DIMREASSOCIATE"),
    (Icon::Unlink, "Disassociate Dimensions", "DIMDISASSOCIATE"),
];
const LAYER_MORE: &[Item] = &[
    (Icon::Isolate, "Isolate", "LAYISO"),
    (Icon::Layers, "Unisolate", "LAYUNISO"),
    (Icon::Layers, "Turn Off (pick)", "LAYOFF"),
    (Icon::Layers, "All On", "LAYON"),
    (Icon::Lock, "Lock (pick)", "LAYLCK"),
    (Icon::Unlock, "Unlock (pick)", "LAYULK"),
    (Icon::Layers, "Make Current (pick)", "LAYMCUR"),
    (Icon::Erase, "Purge Unused", "PURGE"),
];
const BLOCKS: &[Item] = &[(Icon::Insert, "Blocks Palette", "BLOCKS"), (Icon::Insert, "Insert by Name", "INSERT")];
const BLOCK_MORE: &[Item] = &[
    (Icon::Block, "Create Block", "BLOCK"),
    (Icon::Text, "Define Attributes", "ATTDEF"),
    (Icon::Properties, "Edit Attributes", "ATTEDIT"),
    (Icon::List, "Export Attributes (CSV)", "ATTEXPORT"),
    (Icon::Explode, "Explode", "EXPLODE"),
];
const MEASURES: &[Item] = &[
    (Icon::Measure, "Distance", "DIST"),
    (Icon::Area, "Area", "AREA"),
    (Icon::Point, "Point Coordinates", "ID"),
    (Icon::List, "List", "LIST"),
    (Icon::Region, "Mass Properties", "MASSPROP"),
];
const SELECTS: &[Item] = &[(Icon::QSelect, "Quick Select", "QSELECT"), (Icon::SelectAll, "Select All", "SELECTALL")];

fn home(ui: &mut Ui, st: RibbonState, out: &mut Vec<String>) {
    panel(ui, "Draw", DRAW_MORE, out, |ui, out| {
        big(ui, Icon::Line, "Line", "LINE (L) — draw straight segments", "LINE", out);
        big(ui, Icon::Polyline, "Polyline", "PLINE (PL) — connected segments as one object", "PLINE", out);
        big_split(ui, Icon::Circle, "Circle", "CIRCLE (C) — center and radius", "CIRCLE", CIRCLES, out);
        big_split(ui, Icon::Arc, "Arc", "ARC (A) — three points", "ARC", ARCS, out);
        column(ui, |ui| {
            small_split(ui, Icon::Rect, "", "RECTANG (REC) — rectangle", "RECTANG", SHAPES, out);
            small_split(ui, Icon::Hatch, "", "HATCH (H) — click inside closed areas", "HATCH", HATCHES, out);
            small_split(ui, Icon::Spline, "", "SPLINE (SPL)", "SPLINE", CURVES, out);
        });
    });
    panel(ui, "Modify", MODIFY_MORE, out, |ui, out| {
        column(ui, |ui| {
            small(ui, Icon::Move, "Move", "MOVE (M)", "MOVE", out);
            small(ui, Icon::Copy, "Copy", "COPY (CO)", "COPY", out);
            small(ui, Icon::Stretch, "Stretch", "STRETCH (S) — select with a crossing window", "STRETCH", out);
        });
        column(ui, |ui| {
            small(ui, Icon::Rotate, "Rotate", "ROTATE (RO)", "ROTATE", out);
            small(ui, Icon::Mirror, "Mirror", "MIRROR (MI)", "MIRROR", out);
            small(ui, Icon::Scale, "Scale", "SCALE (SC)", "SCALE", out);
        });
        column(ui, |ui| {
            small_split(ui, Icon::Trim, "Trim", "TRIM (TR) — click the part to remove", "TRIM", TRIMS, out);
            small_split(ui, Icon::Fillet, "Fillet", "FILLET (F) — R sets radius", "FILLET", CORNERS, out);
            small_split(ui, Icon::Array, "Array", "ARRAYRECT (AR)", "ARRAYRECT", ARRAYS, out);
        });
        column(ui, |ui| {
            tool(ui, Icon::Erase, "ERASE (E) / Delete key", "ERASE", out);
            tool(ui, Icon::Explode, "EXPLODE (X)", "EXPLODE", out);
            tool(ui, Icon::Offset, "OFFSET (O)", "OFFSET", out);
        });
    });
    panel(ui, "Annotation", ANNOT_MORE, out, |ui, out| {
        big_split(ui, Icon::MText, "Text", "MTEXT (MT) — multiline text", "MTEXT", TEXTS, out);
        big_split(ui, Icon::DimLinear, "Dimension", "DIMLINEAR (DLI) — choose a type with ▾", "DIMLINEAR", DIMS, out);
        column(ui, |ui| {
            small_split(ui, Icon::QDim, "Quick", "QDIM — dimension a selection at once", "QDIM", DIMS, out);
            small_split(ui, Icon::Leader, "Leader", "MLEADER (MLD) — leader in the current multileader style", "MLEADER", LEADERS, out);
            small_split(ui, Icon::Tolerance, "Symbols", "TOLERANCE — geometric tolerance", "TOLERANCE", SYMBOLS, out);
        });
    });
    panel(ui, "Layers", LAYER_MORE, out, |ui, out| {
        big(ui, Icon::Layers, "Layer\nProperties", "LAYER (LA) — layer properties manager", "LAYER", out);
        column(ui, |ui| {
            (st.layer_ui)(ui);
            ui.horizontal(|ui| {
                tool(ui, Icon::Isolate, "LAYISO — show only the selected objects' layers", "LAYISO", out);
                tool(ui, Icon::Layers, "LAYUNISO — restore isolated layers", "LAYUNISO", out);
                tool(ui, Icon::Layers, "LAYOFF — turn off a picked object's layer", "LAYOFF", out);
                tool(ui, Icon::Layers, "LAYON — turn all layers on", "LAYON", out);
                tool(ui, Icon::Lock, "LAYLCK — lock a picked object's layer", "LAYLCK", out);
                tool(ui, Icon::Unlock, "LAYULK — unlock a picked object's layer", "LAYULK", out);
            });
        });
    });
    panel(ui, "Block", BLOCK_MORE, out, |ui, out| {
        big_split(ui, Icon::Insert, "Insert", "Blocks palette / INSERT (I)", "BLOCKS", BLOCKS, out);
        column(ui, |ui| {
            small(ui, Icon::Block, "Create", "BLOCK (B) — make a block from objects", "BLOCK", out);
            small(ui, Icon::Properties, "Edit Attributes", "ATTEDIT — values of a block reference (or double-click it)", "ATTEDIT", out);
        });
    });
    panel(ui, "Properties", &[], out, |ui, out| {
        big(ui, Icon::Match, "Match\nProperties", "MATCHPROP (MA)", "MATCHPROP", out);
        column(ui, |ui| {
            toggle(ui, Icon::Properties, "Palette", "PROPERTIES (PR) — properties palette", st.props, "PROPERTIES", out);
            toggle(ui, Icon::Grid, "LWT", "LWT — show lineweights", st.lwt, "LWT", out);
        });
    });
    panel(ui, "Utilities", &[], out, |ui, out| {
        big_split(ui, Icon::Measure, "Measure", "DIST (DI) — distance and angle", "DIST", MEASURES, out);
        column(ui, |ui| {
            small_split(ui, Icon::QSelect, "Select", "QSELECT — select by type, layer, colour", "QSELECT", SELECTS, out);
            small(ui, Icon::List, "List", "LIST (LI) — object details", "LIST", out);
        });
    });
    panel(ui, "Clipboard", &[], out, |ui, out| {
        big_split(ui, Icon::Paste, "Paste", "PASTECLIP (Ctrl+V)", "PASTECLIP", &[(Icon::Copy, "Copy (Ctrl+C)", "COPYCLIP")], out);
    });
}

fn insert(ui: &mut Ui, out: &mut Vec<String>) {
    panel(ui, "AI Sketch", &[], out, |ui, out| {
        big(
            ui,
            Icon::ImageToCad,
            "Image to\nCAD",
            "IMAGETOCAD (AI) — convert a photo/scan of a hand sketch into an editable drawing",
            "IMAGETOCAD",
            out,
        );
    });
    panel(ui, "Block", &[], out, |ui, out| {
        big_split(ui, Icon::Insert, "Insert", "Blocks palette / INSERT (I)", "BLOCKS", BLOCKS, out);
        big(ui, Icon::Block, "Create", "BLOCK (B)", "BLOCK", out);
        column(ui, |ui| {
            small(ui, Icon::Text, "Define Attributes", "ATTDEF — tag, prompt and default value of a block attribute", "ATTDEF", out);
            small(ui, Icon::Properties, "Edit Attributes", "ATTEDIT — values of a block reference (or double-click it)", "ATTEDIT", out);
            small(ui, Icon::List, "Export Attributes", "ATTEXPORT — all block attributes to a CSV file (parts list)", "ATTEXPORT", out);
        });
    });
    panel(ui, "Reference", &[], out, |ui, out| {
        big(ui, Icon::Image, "Attach\nImage", "IMAGEATTACH (IAT) — raster underlay", "IMAGEATTACH", out);
    });
    panel(ui, "Import", &[], out, |ui, out| {
        big_split(
            ui,
            Icon::Open,
            "Open",
            "OPEN — .icad, .dwg, .dxf",
            "OPEN",
            &[(Icon::Dwg, "DWG drawing", "OPEN"), (Icon::Dxf, "DXF drawing", "OPEN")],
            out,
        );
    });
}

fn annotate(ui: &mut Ui, out: &mut Vec<String>) {
    panel(ui, "Text", &[], out, |ui, out| {
        big(ui, Icon::MText, "Multiline\nText", "MTEXT (MT)", "MTEXT", out);
        column(ui, |ui| {
            small(ui, Icon::Text, "Single Line", "TEXT (DT)", "TEXT", out);
            small(ui, Icon::Properties, "Edit Text", "TEXTEDIT (ED) — or double-click text", "TEXTEDIT", out);
            small(ui, Icon::Check, "Check Spelling", "SPELL (SP) — selection, or the whole drawing", "SPELL", out);
        });
        big(ui, Icon::Text, "Text\nStyle", "STYLE (ST) — fonts (TrueType), width factor, slant", "STYLE", out);
    });
    panel(ui, "Dimensions", &ANNOT_MORE[4..], out, |ui, out| {
        big_split(ui, Icon::DimLinear, "Dimension", "DIMLINEAR (DLI) — choose a type with ▾", "DIMLINEAR", DIMS, out);
        column(ui, |ui| {
            small(ui, Icon::DimAligned, "Aligned", "DIMALIGNED (DAL)", "DIMALIGNED", out);
            small(ui, Icon::DimAngular, "Angular", "DIMANGULAR (DAN)", "DIMANGULAR", out);
            small(ui, Icon::DimArc, "Arc Length", "DIMARC (DAR)", "DIMARC", out);
        });
        column(ui, |ui| {
            small(ui, Icon::DimRadius, "Radius", "DIMRADIUS (DRA)", "DIMRADIUS", out);
            small(ui, Icon::DimDiameter, "Diameter", "DIMDIAMETER (DDI)", "DIMDIAMETER", out);
            small(ui, Icon::DimOrdinate, "Ordinate", "DIMORDINATE (DOR) — X/Y distance from a datum", "DIMORDINATE", out);
        });
        column(ui, |ui| {
            small(ui, Icon::QDim, "Quick", "QDIM — dimension a selection at once (continuous, baseline, ordinate, radius…)", "QDIM", out);
            small(ui, Icon::DimBaseline, "Baseline", "DIMBASELINE (DBA)", "DIMBASELINE", out);
            small(ui, Icon::DimContinue, "Continue", "DIMCONTINUE (DCO)", "DIMCONTINUE", out);
        });
        big(ui, Icon::DimStyle, "Dimension\nStyle", "DIMSTYLE (D) — text height, arrows, precision", "DIMSTYLE", out);
    });
    panel(ui, "Symbols", &[], out, |ui, out| {
        big(ui, Icon::Tolerance, "Tolerance", "TOLERANCE (TOL) — geometric tolerance feature control frame", "TOLERANCE", out);
        column(ui, |ui| {
            small(ui, Icon::Datum, "Datum", "DATUM — datum feature symbol", "DATUM", out);
            small(ui, Icon::Surface, "Surface", "SURFFINISH (SF) — surface texture symbol", "SURFFINISH", out);
            small(ui, Icon::Weld, "Weld", "WELD — weld symbol", "WELD", out);
        });
    });
    panel(ui, "Leaders", &[], out, |ui, out| {
        big_split(ui, Icon::Leader, "Multileader", "MLEADER (MLD) — leader in the current style", "MLEADER", LEADERS, out);
        column(ui, |ui| {
            small(ui, Icon::Leader, "Leader", "LEADER (LE)", "LEADER", out);
            small(ui, Icon::DimStyle, "Leader Style", "MLEADERSTYLE (MLS) — arrowheads, landing, balloons", "MLEADERSTYLE", out);
            small(ui, Icon::CenterMark, "Center Mark", "CENTERMARK (CM)", "CENTERMARK", out);
        });
    });
    panel(ui, "Tables", &[], out, |ui, out| {
        big(ui, Icon::Table, "Table", "TABLE — empty, from CSV, or a parts list from block attributes", "TABLE", out);
    });
    panel(ui, "Hatch", &[], out, |ui, out| {
        big_split(ui, Icon::Hatch, "Hatch", "HATCH (H) — P pattern, S scale, A angle", "HATCH", HATCHES, out);
    });
    panel(ui, "Sheet", &[], out, |ui, out| {
        big(ui, Icon::TitleBlock, "Title\nBlock", "TITLEBLOCK (TB) — company, title, drawing number, scale…", "TITLEBLOCK", out);
        column(ui, |ui| {
            small(ui, Icon::PageSetup, "Page Setup", "PAGESETUP — paper size and plot scale", "PAGESETUP", out);
            small(ui, Icon::Properties, "Units", "UNITS (UN)", "UNITS", out);
        });
    });
}

fn view(ui: &mut Ui, st: &RibbonState, out: &mut Vec<String>) {
    panel(ui, "Navigate", &[], out, |ui, out| {
        big_split(
            ui,
            Icon::ZoomExtents,
            "Zoom\nExtents",
            "ZOOM E — fit drawing (double-click middle button)",
            "ZOOM E",
            &[
                (Icon::ZoomExtents, "Extents", "ZOOM E"),
                (Icon::ZoomWindow, "Window", "ZOOM W"),
                (Icon::Undo, "Previous", "ZOOM P"),
                (Icon::TitleBlock, "Whole Sheet", "ZOOM A"),
            ],
            out,
        );
        column(ui, |ui| {
            small(ui, Icon::ZoomWindow, "Window", "ZOOM W", "ZOOM W", out);
            small(ui, Icon::Undo, "Previous", "ZOOM P", "ZOOM P", out);
            small(ui, Icon::Pan, "Pan", "Hold middle mouse button, or Alt/Option + drag", "PANINFO", out);
        });
    });
    panel(ui, "Display", &[], out, |ui, out| {
        column(ui, |ui| {
            toggle(ui, Icon::Grid, "Grid", "GRID (F7)", st.grid, "GRID", out);
            toggle(ui, Icon::Properties, "Lineweight", "LWT — show lineweights", st.lwt, "LWT", out);
            toggle(ui, Icon::Properties, "Properties", "PROPERTIES palette", st.props, "PROPERTIES", out);
        });
        column(ui, |ui| {
            toggle(ui, Icon::TitleBlock, "Sheet & Title", "SHEET — show border and title block", st.sheet, "SHEET", out);
            toggle(ui, Icon::PageSetup, "Layout (Paper)", "LAYOUT — white paper preview", st.layout, "LAYOUT", out);
            toggle(ui, Icon::Chat, "AI Chat", "AICHAT — conversational editing panel", st.chat, "AICHAT", out);
        });
        column(ui, |ui| {
            small(ui, Icon::XLine, "Polar Tracking", "POLAR (F10) — right-click POLAR in the status bar for the angle", "POLAR", out);
        });
    });
}

fn output(ui: &mut Ui, out: &mut Vec<String>) {
    panel(ui, "Plot", &[], out, |ui, out| {
        big(ui, Icon::Pdf, "Plot\nto PDF", "PLOT — true-scale vector PDF of the sheet", "PLOT", out);
        big(ui, Icon::PageSetup, "Page\nSetup", "PAGESETUP", "PAGESETUP", out);
    });
    panel(ui, "Export", &[], out, |ui, out| {
        big(ui, Icon::Dwg, "DWG", "Export DWG", "EXPORT DWG", out);
        big(ui, Icon::Dxf, "DXF", "Export DXF (2000; R12 in Export options)", "EXPORT DXF", out);
        column(ui, |ui| {
            small(ui, Icon::Pdf, "PDF", "Export PDF", "EXPORT PDF", out);
            small(ui, Icon::Svg, "SVG", "Export SVG", "EXPORT SVG", out);
            small(ui, Icon::Properties, "Options", "EXPORT — options", "EXPORT", out);
        });
    });
}

fn ai(ui: &mut Ui, st: &RibbonState, out: &mut Vec<String>) {
    panel(ui, "Sketch to CAD", &[], out, |ui, out| {
        big(ui, Icon::ImageToCad, "Image to\nCAD", "IMAGETOCAD — upload a photo/scan of a hand-drawn mechanical sketch", "IMAGETOCAD", out);
    });
    panel(ui, "Assistant", &[], out, |ui, out| {
        big(ui, Icon::Chat, "AI\nChat", "AICHAT — describe changes in plain language and the AI edits the drawing", "AICHAT", out);
    });
    panel(ui, "AI Engine", &[], out, |ui, out| {
        big(
            ui,
            Icon::Key,
            "Connect\nAPI Key",
            "AISETUP — paste any provider's API key; the model is configured automatically",
            "AISETUP",
            out,
        );
        big(ui, Icon::AiSettings, "AI\nSettings", "AISETTINGS — cloud key, Ollama, local servers, Tesseract", "AISETTINGS", out);
        column(ui, |ui| {
            let (txt, col) = if st.ai_ready {
                ("● AI engine connected", Color32::from_rgb(90, 210, 120))
            } else {
                ("○ AI engine not checked", theme::TEXT_DIM)
            };
            ui.label(RichText::new(txt).color(col).size(12.0));
            small(ui, Icon::Help, "Tips for best results", "How to photograph and dimension sketches", "AIHELP", out);
        });
    });
}
