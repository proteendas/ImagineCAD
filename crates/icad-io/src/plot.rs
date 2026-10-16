// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Proteen Das

//! Plot preparation shared by the PDF and SVG writers: resolves colors,
//! linetypes and lineweights and maps model space onto the paper.

use icad_core::document::Document;
use icad_core::geom::*;
pub use icad_core::layout::PlotStyle;
use icad_core::prim::*;
use icad_core::style::*;

#[derive(Clone, Debug)]
pub struct PlotOptions {
    /// Plot style; None = the sheet's own.
    pub style: Option<PlotStyle>,
    /// Include border and title block.
    pub include_sheet: bool,
    /// Honour layer lineweights (otherwise a hairline is used).
    pub lineweights: bool,
}

impl Default for PlotOptions {
    fn default() -> Self {
        Self { style: None, include_sheet: true, lineweights: true }
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
    out
}
