// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Proteen Das

//! Plot preparation shared by the PDF and SVG writers: resolves colors,
//! linetypes and lineweights and maps model space onto the paper.

use icad_core::document::Document;
use icad_core::geom::*;
pub use icad_core::layout::PlotStyle;
use icad_core::prim::*;
use icad_core::sheet::Sheet;
use icad_core::style::*;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug)]
pub struct PlotOptions {
    /// Plot style; None = the sheet's own.
    pub style: Option<PlotStyle>,
    /// Include border and title block.
    pub include_sheet: bool,
    /// Honour layer lineweights (otherwise a hairline is used).
    pub lineweights: bool,
    /// Plot stamp printed in the margin of every page.
    pub stamp: Option<Stamp>,
    /// Write PDF/A-2b (archival: text as outlines, colour profile, metadata).
    pub pdfa: bool,
}

impl Default for PlotOptions {
    fn default() -> Self {
        Self { style: None, include_sheet: true, lineweights: true, stamp: None, pdfa: false }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum StampCorner {
    #[default]
    BottomLeft,
    BottomRight,
    TopLeft,
    TopRight,
}

impl StampCorner {
    pub const ALL: [StampCorner; 4] = [StampCorner::BottomLeft, StampCorner::BottomRight, StampCorner::TopLeft, StampCorner::TopRight];
    pub fn name(self) -> &'static str {
        match self {
            StampCorner::BottomLeft => "Bottom left",
            StampCorner::BottomRight => "Bottom right",
            StampCorner::TopLeft => "Top left",
            StampCorner::TopRight => "Top right",
        }
    }
}

/// What the plot stamp shows (kept in the settings).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct PlotStamp {
    pub on: bool,
    pub drawing: bool,
    pub layout: bool,
    pub date: bool,
    pub user: bool,
    pub paper: bool,
    pub scale: bool,
    /// Extra text, e.g. "PRELIMINARY" or a project number.
    pub custom: String,
    pub corner: StampCorner,
    /// Text height in paper mm.
    pub height: f64,
}

impl Default for PlotStamp {
    fn default() -> Self {
        Self {
            on: false,
            drawing: true,
            layout: true,
            date: true,
            user: true,
            paper: false,
            scale: false,
            custom: String::new(),
            corner: StampCorner::BottomLeft,
            height: 2.0,
        }
    }
}

/// A plot stamp with what it names filled in.
#[derive(Clone, Debug, PartialEq)]
pub struct Stamp {
    pub fields: PlotStamp,
    /// Drawing file name (or tab name).
    pub drawing: String,
    pub user: String,
    /// When it was plotted, e.g. "2026-10-16 14:05 UTC".
    pub when: String,
}

impl Stamp {
    pub fn new(fields: PlotStamp, drawing: impl Into<String>, user: impl Into<String>) -> Stamp {
        let [y, mo, d, h, mi, _] = icad_core::sheet::utc_now();
        Stamp { fields, drawing: drawing.into(), user: user.into(), when: format!("{y:04}-{mo:02}-{d:02} {h:02}:{mi:02} UTC") }
    }

    /// The stamp line for a page of `layout` on `sheet`.
    pub fn text(&self, layout: &str, sheet: &Sheet) -> String {
        let f = &self.fields;
        let mut parts: Vec<String> = vec![];
        let mut push = |on: bool, s: String| {
            if on && !s.trim().is_empty() {
                parts.push(s);
            }
        };
        push(f.drawing, self.drawing.clone());
        push(f.layout, layout.to_string());
        push(f.date, self.when.clone());
        push(f.user, self.user.clone());
        push(f.paper, format!("{} {}", sheet.paper.name(), if sheet.landscape { "landscape" } else { "portrait" }));
        push(f.scale, format!("Scale {}", sheet.scale_label()));
        push(true, f.custom.clone());
        parts.join("  |  ")
    }

    /// The stamp on a page of `layout`, in paper mm.
    pub fn items(&self, layout: &str, sheet: &Sheet) -> Vec<PlotItem> {
        let text = self.text(layout, sheet);
        if text.is_empty() {
            return vec![];
        }
        let (w, h) = sheet.paper_mm();
        let f = sheet.frame_mm();
        let th = self.fields.height.clamp(0.5, 10.0);
        // In the margin, lined up with the drawing frame.
        let (x, y, align) = match self.fields.corner {
            StampCorner::BottomLeft => (f.min.x, f.min.y / 2.0, HAlign::Left),
            StampCorner::BottomRight => (f.max.x, f.min.y / 2.0, HAlign::Right),
            StampCorner::TopLeft => (f.min.x, (h + f.max.y) / 2.0, HAlign::Left),
            StampCorner::TopRight => (f.max.x.min(w), (h + f.max.y) / 2.0, HAlign::Right),
        };
        let t = Text::new(V2::new(x, y), th, text).aligned(align, VAlign::Middle);
        vec![PlotItem { prim: Prim::Text(t), rgb: [80, 80, 80], dash: vec![], width: 0.18 }]
    }
}

pub struct PlotItem {
    /// Primitive in paper millimetres (origin at the sheet's lower-left).
    pub prim: Prim,
    pub rgb: [u8; 3],
    /// Dash pattern in paper mm (empty = continuous).
    pub dash: Vec<f64>,
    /// Line width in mm.
    pub width: f64,
}

pub fn plot_color(aci: u8, style: PlotStyle) -> [u8; 3] {
    let shown = match aci {
        7 | 0 | 255 => [0, 0, 0],
        2 => [190, 150, 0],
        4 => [0, 150, 170],
        3 => [0, 150, 0],
        c => aci_rgb(c),
    };
    match style {
        PlotStyle::Monochrome => [0, 0, 0],
        PlotStyle::Color => shown,
        PlotStyle::Grayscale => {
            let [r, g, b] = shown.map(|v| v as f64);
            let l = (0.299 * r + 0.587 * g + 0.114 * b).round() as u8;
            [l, l, l]
        }
        PlotStyle::Screened => shown.map(|v| (v as u16 + (255 - v as u16) / 2) as u8),
    }
}

/// Paper-space layout `index` ready to plot.
pub fn layout_plot_items(doc: &Document, index: usize, opts: &PlotOptions) -> Vec<PlotItem> {
    let Some(l) = doc.layouts.get(index) else { return vec![] };
    let style = opts.style.unwrap_or(l.sheet.plot_style);
    let lw = |w: f64| if opts.lineweights { w.max(0.09) } else { 0.18 };
    let mut out = vec![];
    for it in doc.layout_items(index, true) {
        let is_fill = matches!(it.prim, Prim::Text(_) | Prim::Fill(_) | Prim::Image(_));
        let dash: Vec<f64> = if is_fill { vec![] } else { it.linetype.pattern().iter().map(|x| x.abs() * doc.ltscale).collect() };
        for p in icad_core::fonts::expand(vec![it.prim]) {
            out.push(PlotItem { prim: p, rgb: plot_color(it.color, style), dash: dash.clone(), width: lw(it.lineweight) });
        }
    }
    if let Some(s) = &opts.stamp {
        out.extend(s.items(&l.name, &l.sheet));
    }
    out
}

/// Map a model-space primitive into paper millimetres.
pub fn to_paper(doc: &Document, p: &Prim) -> Prim {
    let sh = &doc.sheet;
    let k = 1.0 / sh.scale;
    let m = |v: V2| sh.m2p(v);
    match p {
        Prim::Line(a, b) => Prim::Line(m(*a), m(*b)),
        Prim::Poly(pts, c) => Prim::Poly(pts.iter().map(|v| m(*v)).collect(), *c),
        Prim::Circle(c, r) => Prim::Circle(m(*c), r * k),
        Prim::Arc { c, r, start, end } => Prim::Arc { c: m(*c), r: r * k, start: *start, end: *end },
        Prim::Fill(pts) => Prim::Fill(pts.iter().map(|v| m(*v)).collect()),
        Prim::Text(t) => {
            let mut t = t.clone();
            t.pos = m(t.pos);
            t.height *= k;
            Prim::Text(t)
        }
        Prim::Image(i) => {
            let mut i = i.clone();
            i.pos = m(i.pos);
            i.w *= k;
            i.h *= k;
            Prim::Image(i)
        }
    }
}

pub fn plot_items(doc: &Document, opts: &PlotOptions) -> Vec<PlotItem> {
    let style = opts.style.unwrap_or(doc.sheet.plot_style);
    let mut out = Vec::new();
    let lw = |w: f64| if opts.lineweights { w.max(0.09) } else { 0.18 };
    for e in &doc.entities {
        let Some(layer) = doc.layer(&e.layer) else { continue };
        if !layer.visible || !layer.plot {
            continue;
        }
        for it in doc.render_items(e) {
            let is_fill = matches!(it.prim, Prim::Text(_) | Prim::Fill(_) | Prim::Image(_));
            let dash: Vec<f64> = if is_fill { vec![] } else { it.linetype.pattern().iter().map(|x| x.abs() * doc.ltscale).collect() };
            // Styled text plots as its font outlines.
            for p in icad_core::fonts::expand(vec![it.prim]) {
                out.push(PlotItem {
                    prim: to_paper(doc, &p),
                    rgb: plot_color(it.color, style),
                    dash: dash.clone(),
                    width: lw(it.lineweight),
                });
            }
        }
    }
    if opts.include_sheet && doc.sheet.visible {
        for (p, w) in doc.sheet_prims() {
            out.push(PlotItem { prim: to_paper(doc, &p), rgb: [0, 0, 0], dash: vec![], width: lw(w) });
        }
    }
    if let Some(s) = &opts.stamp {
        out.extend(s.items("Layout1", &doc.sheet));
    }
    out
}
