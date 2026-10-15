// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Proteen Das

//! Ribbon toolbar: tabs → panels → tool buttons. Every button issues a
//! command string, exactly as if it had been typed on the command line.

use crate::icons::{self, Icon};
use crate::theme;
use eframe::egui::{self, vec2, Align2, Color32, FontId, Rect, RichText, Sense, Ui};

pub const TABS: [&str; 6] = ["Home", "Insert", "Annotate", "View", "Output", "AI"];

pub const RIBBON_H: f32 = 96.0;

fn big(ui: &mut Ui, icon: Icon, label: &str, tip: &str, cmd: &str, out: &mut Vec<String>) {
    let (rect, resp) = ui.allocate_exact_size(vec2(58.0, 70.0), Sense::click());
    let p = ui.painter();
    if resp.hovered() {
        p.rect_filled(rect, 3.0, theme::HOVER);
    }
    let ir = Rect::from_center_size(rect.center_top() + vec2(0.0, 20.0), vec2(30.0, 30.0));
    let col = if icon == Icon::ImageToCad { Color32::WHITE } else { theme::TEXT };
    icons::draw(p, ir, icon, col);
    let mut y = rect.top() + 42.0;
    for line in label.split('\n') {
        p.text(egui::pos2(rect.center().x, y), Align2::CENTER_TOP, line, FontId::proportional(11.5), theme::TEXT);
        y += 13.0;
    }
    if resp.on_hover_text(tip).clicked() {
        out.push(cmd.to_string());
    }
}

fn small(ui: &mut Ui, icon: Icon, label: &str, tip: &str, cmd: &str, out: &mut Vec<String>) {
    let w = 26.0 + ui.fonts(|f| f.layout_no_wrap(label.into(), FontId::proportional(12.0), Color32::WHITE).size().x);
    let (rect, resp) = ui.allocate_exact_size(vec2(w.max(30.0), 21.0), Sense::click());
    let p = ui.painter();
    if resp.hovered() {
        p.rect_filled(rect, 3.0, theme::HOVER);
    }
    icons::draw(p, Rect::from_center_size(rect.left_center() + vec2(11.0, 0.0), vec2(16.0, 16.0)), icon, theme::TEXT);
    p.text(rect.left_center() + vec2(23.0, 0.0), Align2::LEFT_CENTER, label, FontId::proportional(12.0), theme::TEXT);
    if resp.on_hover_text(tip).clicked() {
        out.push(cmd.to_string());
    }
}

/// Toggle-style small button (highlighted when on).
pub fn toggle(ui: &mut Ui, icon: Icon, label: &str, tip: &str, on: bool, cmd: &str, out: &mut Vec<String>) {
    let w = 26.0 + ui.fonts(|f| f.layout_no_wrap(label.into(), FontId::proportional(12.0), Color32::WHITE).size().x);
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

/// A ribbon panel with a caption bar underneath, separated by a divider.
fn panel(ui: &mut Ui, title: &str, add: impl FnOnce(&mut Ui)) {
    let resp = ui.vertical(|ui| {
        ui.set_min_height(RIBBON_H - 4.0);
        ui.horizontal(|ui| {
            ui.set_min_height(RIBBON_H - 22.0);
            add(ui);
        });
        let r = ui.available_rect_before_wrap();
        let _ = r;
    });
    let r = resp.response.rect;
    let bar = Rect::from_min_max(egui::pos2(r.left() - 2.0, r.bottom() - 17.0), egui::pos2(r.right() + 2.0, r.bottom()));
    ui.painter().rect_filled(bar, 0.0, theme::RIBBON_PANEL_TITLE);
    ui.painter().text(bar.center(), Align2::CENTER_CENTER, title, FontId::proportional(11.0), theme::TEXT_DIM);
    let x = r.right() + 5.0;
    ui.painter().line_segment(
        [egui::pos2(x, r.top() + 4.0), egui::pos2(x, r.bottom() - 4.0)],
        egui::Stroke::new(1.0f32, Color32::from_rgb(78, 90, 108)),
    );
    ui.add_space(8.0);
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

fn home(ui: &mut Ui, st: RibbonState, out: &mut Vec<String>) {
    panel(ui, "Draw", |ui| {
        big(ui, Icon::Line, "Line", "LINE (L) — draw straight segments", "LINE", out);
        big(ui, Icon::Polyline, "Polyline", "PLINE (PL) — connected segments as one object", "PLINE", out);
        big(ui, Icon::Circle, "Circle", "CIRCLE (C) — center/radius, diameter, 2P, 3P", "CIRCLE", out);
        big(ui, Icon::Arc, "Arc", "ARC (A) — three-point or center/start/end", "ARC", out);
        column(ui, |ui| {
            small(ui, Icon::Rect, "Rectangle", "RECTANG (REC)", "RECTANG", out);
            small(ui, Icon::Polygon, "Polygon", "POLYGON (POL)", "POLYGON", out);
            small(ui, Icon::Ellipse, "Ellipse", "ELLIPSE (EL) — axis endpoints or center", "ELLIPSE", out);
        });
        column(ui, |ui| {
            small(ui, Icon::Spline, "Spline", "SPLINE (SPL) — smooth curve through fit points", "SPLINE", out);
            small(ui, Icon::Hatch, "Hatch", "HATCH (H) — click inside closed areas", "HATCH", out);
            small(ui, Icon::Point, "Point", "POINT (PO)", "POINT", out);
        });
        column(ui, |ui| {
            small(ui, Icon::XLine, "Construction", "XLINE (XL) — infinite construction line (H/V options)", "XLINE", out);
            small(ui, Icon::XLine, "Ray", "RAY — semi-infinite construction line", "RAY", out);
        });
    });
    panel(ui, "Modify", |ui| {
        column(ui, |ui| {
            small(ui, Icon::Move, "Move", "MOVE (M)", "MOVE", out);
            small(ui, Icon::Copy, "Copy", "COPY (CO)", "COPY", out);
            small(ui, Icon::Rotate, "Rotate", "ROTATE (RO)", "ROTATE", out);
        });
        column(ui, |ui| {
            small(ui, Icon::Mirror, "Mirror", "MIRROR (MI)", "MIRROR", out);
            small(ui, Icon::Scale, "Scale", "SCALE (SC)", "SCALE", out);
            small(ui, Icon::Stretch, "Stretch", "STRETCH (S) — select with a crossing window", "STRETCH", out);
        });
        column(ui, |ui| {
            small(ui, Icon::Trim, "Trim", "TRIM (TR) — click the part to remove", "TRIM", out);
            small(ui, Icon::Extend, "Extend", "EXTEND (EX)", "EXTEND", out);
            small(ui, Icon::Offset, "Offset", "OFFSET (O)", "OFFSET", out);
        });
        column(ui, |ui| {
            small(ui, Icon::Fillet, "Fillet", "FILLET (F) — R sets radius", "FILLET", out);
            small(ui, Icon::Chamfer, "Chamfer", "CHAMFER (CHA)", "CHAMFER", out);
            small(ui, Icon::Break, "Break", "BREAK (BR) / BREAKATPOINT (BAP)", "BREAK", out);
        });
        column(ui, |ui| {
            small(ui, Icon::Join, "Join", "JOIN (J)", "JOIN", out);
            small(ui, Icon::Lengthen, "Lengthen", "LENGTHEN (LEN) — DE sets the delta", "LENGTHEN", out);
            small(ui, Icon::Explode, "Explode", "EXPLODE (X)", "EXPLODE", out);
        });
        column(ui, |ui| {
            small(ui, Icon::Array, "Array", "ARRAYRECT (AR)", "ARRAYRECT", out);
            small(ui, Icon::PolarArray, "Polar Array", "ARRAYPOLAR (APO)", "ARRAYPOLAR", out);
            small(ui, Icon::Erase, "Erase", "ERASE (E) / Delete key", "ERASE", out);
        });
        column(ui, |ui| {
            small(ui, Icon::Match, "Match", "MATCHPROP (MA)", "MATCHPROP", out);
            small(ui, Icon::Point, "Divide", "DIVIDE (DIV) — equal segments", "DIVIDE", out);
            small(ui, Icon::Measure, "Measure", "MEASURE (ME) — points at a spacing", "MEASURE", out);
        });
    });
    panel(ui, "Annotation", |ui| {
        big(ui, Icon::DimLinear, "Linear", "DIMLINEAR (DLI) — horizontal / vertical dimension", "DIMLINEAR", out);
        column(ui, |ui| {
            small(ui, Icon::DimAligned, "Aligned", "DIMALIGNED (DAL)", "DIMALIGNED", out);
            small(ui, Icon::DimAngular, "Angular", "DIMANGULAR (DAN)", "DIMANGULAR", out);
            small(ui, Icon::DimRadius, "Radius", "DIMRADIUS (DRA)", "DIMRADIUS", out);
        });
        column(ui, |ui| {
            small(ui, Icon::DimDiameter, "Diameter", "DIMDIAMETER (DDI)", "DIMDIAMETER", out);
            small(ui, Icon::DimBaseline, "Baseline", "DIMBASELINE (DBA)", "DIMBASELINE", out);
            small(ui, Icon::DimContinue, "Continue", "DIMCONTINUE (DCO)", "DIMCONTINUE", out);
        });
        column(ui, |ui| {
            small(ui, Icon::MText, "Multiline Text", "MTEXT (MT)", "MTEXT", out);
            small(ui, Icon::Leader, "Leader", "LEADER (LE)", "LEADER", out);
            small(ui, Icon::CenterMark, "Center Mark", "CENTERMARK (CM)", "CENTERMARK", out);
        });
    });
    panel(ui, "Layers", |ui| {
        big(ui, Icon::Layers, "Layer\nProperties", "LAYER (LA) — layer properties manager", "LAYER", out);
        column(ui, |ui| {
            (st.layer_ui)(ui);
            ui.horizontal(|ui| {
                small(ui, Icon::Isolate, "Isolate", "LAYISO — show only the selected objects' layers", "LAYISO", out);
                small(ui, Icon::Layers, "Unisolate", "LAYUNISO", "LAYUNISO", out);
            });
        });
        column(ui, |ui| {
            small(ui, Icon::Layers, "Off", "LAYOFF — turn off a picked object's layer", "LAYOFF", out);
            small(ui, Icon::Layers, "All On", "LAYON", "LAYON", out);
            small(ui, Icon::Layers, "Make Current", "LAYMCUR — make a picked object's layer current", "LAYMCUR", out);
        });
    });
    panel(ui, "Block", |ui| {
        big(ui, Icon::Insert, "Insert", "Blocks palette / INSERT (I)", "BLOCKS", out);
        column(ui, |ui| {
            small(ui, Icon::Block, "Create", "BLOCK (B) — make a block from objects", "BLOCK", out);
            small(ui, Icon::Explode, "Explode", "EXPLODE (X)", "EXPLODE", out);
        });
    });
    panel(ui, "Utilities", |ui| {
        column(ui, |ui| {
            small(ui, Icon::Measure, "Distance", "DIST (DI) — distance and angle", "DIST", out);
            small(ui, Icon::Area, "Area", "AREA (AA) — points or object", "AREA", out);
            small(ui, Icon::List, "List", "LIST (LI) — object details", "LIST", out);
        });
        column(ui, |ui| {
            small(ui, Icon::QSelect, "Quick Select", "QSELECT — select by type, layer, colour", "QSELECT", out);
            small(ui, Icon::SelectAll, "Select All", "SELECTALL (Ctrl+A)", "SELECTALL", out);
            small(ui, Icon::Properties, "Properties", "PROPERTIES (PR)", "PROPERTIES", out);
        });
    });
    panel(ui, "Clipboard", |ui| {
        column(ui, |ui| {
            small(ui, Icon::Paste, "Paste", "PASTECLIP (Ctrl+V)", "PASTECLIP", out);
            small(ui, Icon::Copy, "Copy Clip", "COPYCLIP (Ctrl+C)", "COPYCLIP", out);
        });
    });
}

fn insert(ui: &mut Ui, out: &mut Vec<String>) {
    panel(ui, "AI Sketch", |ui| {
        big(
            ui,
            Icon::ImageToCad,
            "Image to\nCAD",
            "IMAGETOCAD (AI) — convert a photo/scan of a hand sketch into an editable drawing",
            "IMAGETOCAD",
            out,
        );
    });
    panel(ui, "Block", |ui| {
        big(ui, Icon::Insert, "Insert\nBlock", "Blocks palette / INSERT (I)", "BLOCKS", out);
        big(ui, Icon::Block, "Create\nBlock", "BLOCK (B)", "BLOCK", out);
    });
    panel(ui, "Reference", |ui| {
        big(ui, Icon::Image, "Attach\nImage", "IMAGEATTACH (IAT) — raster underlay", "IMAGEATTACH", out);
    });
    panel(ui, "Import", |ui| {
        big(ui, Icon::Open, "Open\nDrawing", "OPEN — .icad, .dwg, .dxf", "OPEN", out);
        big(ui, Icon::Dwg, "Open\nDWG", "Open a DWG drawing", "OPEN", out);
        big(ui, Icon::Dxf, "Open\nDXF", "Open a DXF drawing", "OPEN", out);
    });
}

fn annotate(ui: &mut Ui, out: &mut Vec<String>) {
    panel(ui, "Text", |ui| {
        big(ui, Icon::MText, "Multiline\nText", "MTEXT (MT)", "MTEXT", out);
        column(ui, |ui| {
            small(ui, Icon::Text, "Single Line", "TEXT (DT)", "TEXT", out);
            small(ui, Icon::Properties, "Edit Text", "TEXTEDIT (ED) — or double-click text", "TEXTEDIT", out);
        });
    });
    panel(ui, "Dimensions", |ui| {
        big(ui, Icon::DimLinear, "Linear", "DIMLINEAR (DLI)", "DIMLINEAR", out);
        column(ui, |ui| {
            small(ui, Icon::DimAligned, "Aligned", "DIMALIGNED (DAL)", "DIMALIGNED", out);
            small(ui, Icon::DimAngular, "Angular", "DIMANGULAR (DAN)", "DIMANGULAR", out);
            small(ui, Icon::DimRadius, "Radius", "DIMRADIUS (DRA)", "DIMRADIUS", out);
        });
        column(ui, |ui| {
            small(ui, Icon::DimDiameter, "Diameter", "DIMDIAMETER (DDI)", "DIMDIAMETER", out);
            small(ui, Icon::DimBaseline, "Baseline", "DIMBASELINE (DBA)", "DIMBASELINE", out);
            small(ui, Icon::DimContinue, "Continue", "DIMCONTINUE (DCO)", "DIMCONTINUE", out);
        });
        big(ui, Icon::DimStyle, "Dimension\nStyle", "DIMSTYLE (D) — text height, arrows, precision", "DIMSTYLE", out);
    });
    panel(ui, "Leaders & Marks", |ui| {
        big(ui, Icon::Leader, "Leader", "LEADER (LE)", "LEADER", out);
        big(ui, Icon::CenterMark, "Center\nMark", "CENTERMARK (CM)", "CENTERMARK", out);
    });
    panel(ui, "Hatch", |ui| {
        big(ui, Icon::Hatch, "Hatch", "HATCH (H) — P pattern, S scale, A angle", "HATCH", out);
    });
    panel(ui, "Sheet", |ui| {
        big(ui, Icon::TitleBlock, "Title\nBlock", "TITLEBLOCK (TB) — company, title, drawing number, scale…", "TITLEBLOCK", out);
        big(ui, Icon::PageSetup, "Page\nSetup", "PAGESETUP — paper size and plot scale", "PAGESETUP", out);
        big(ui, Icon::Properties, "Units", "UNITS (UN)", "UNITS", out);
    });
}

fn view(ui: &mut Ui, st: &RibbonState, out: &mut Vec<String>) {
    panel(ui, "Navigate", |ui| {
        big(ui, Icon::ZoomExtents, "Zoom\nExtents", "ZOOM E — fit drawing (double-click middle button)", "ZOOM E", out);
        big(ui, Icon::ZoomWindow, "Zoom\nWindow", "ZOOM W", "ZOOM W", out);
        big(ui, Icon::Undo, "Zoom\nPrevious", "ZOOM P", "ZOOM P", out);
        big(ui, Icon::TitleBlock, "Zoom\nSheet", "ZOOM A — whole sheet", "ZOOM A", out);
        big(ui, Icon::Pan, "Pan", "Hold middle mouse button, or Alt/Option + drag", "PANINFO", out);
    });
    panel(ui, "Display", |ui| {
        column(ui, |ui| {
            toggle(ui, Icon::Grid, "Grid", "GRID (F7)", st.grid, "GRID", out);
            toggle(ui, Icon::Properties, "Lineweight", "LWT — show lineweights", st.lwt, "LWT", out);
            toggle(ui, Icon::Properties, "Properties", "PROPERTIES palette", st.props, "PROPERTIES", out);
        });
        column(ui, |ui| {
            toggle(ui, Icon::TitleBlock, "Sheet & Title", "SHEET — show border and title block", st.sheet, "SHEET", out);
            toggle(ui, Icon::PageSetup, "Layout (Paper)", "LAYOUT — white paper preview", st.layout, "LAYOUT", out);
            small(ui, Icon::XLine, "Polar Tracking", "POLAR (F10) — right-click POLAR in the status bar for the angle", "POLAR", out);
        });
    });
}

fn output(ui: &mut Ui, out: &mut Vec<String>) {
    panel(ui, "Plot", |ui| {
        big(ui, Icon::Pdf, "Plot\nto PDF", "PLOT — true-scale vector PDF of the sheet", "PLOT", out);
        big(ui, Icon::PageSetup, "Page\nSetup", "PAGESETUP", "PAGESETUP", out);
    });
    panel(ui, "Export", |ui| {
        big(ui, Icon::Dwg, "DWG", "Export DWG", "EXPORT DWG", out);
        big(ui, Icon::Dxf, "DXF", "Export DXF (2000; R12 in Export options)", "EXPORT DXF", out);
        big(ui, Icon::Pdf, "PDF", "Export PDF", "EXPORT PDF", out);
        big(ui, Icon::Svg, "SVG", "Export SVG", "EXPORT SVG", out);
        big(ui, Icon::Properties, "Options", "EXPORT — options", "EXPORT", out);
    });
}

fn ai(ui: &mut Ui, st: &RibbonState, out: &mut Vec<String>) {
    panel(ui, "Sketch to CAD", |ui| {
        big(ui, Icon::ImageToCad, "Image to\nCAD", "IMAGETOCAD — upload a photo/scan of a hand-drawn mechanical sketch", "IMAGETOCAD", out);
    });
    panel(ui, "Assistant", |ui| {
        big(ui, Icon::Chat, "AI\nChat", "AICHAT — describe changes in plain language and the AI edits the drawing", "AICHAT", out);
    });
    panel(ui, "AI Engine", |ui| {
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
