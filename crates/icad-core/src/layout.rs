// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Proteen Das

//! Paper-space layouts: a sheet of paper with its own title block and
//! rectangular viewports that show the model at a chosen scale. (The
//! drawing's main sheet, `Document::sheet`, is the first layout and lives in
//! model space; these are the additional ones.)

use crate::document::{Document, RenderItem};
use crate::geom::*;
use crate::prim::Prim;
use crate::sheet::Sheet;
use crate::style::LineType;
use serde::{Deserialize, Serialize};

/// How colours are plotted.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum PlotStyle {
    /// Everything black.
    #[default]
    Monochrome,
    /// Colours as displayed (white/black plots black).
    Color,
    /// Colours as shades of grey.
    Grayscale,
    /// Colours lightened to 50 % (for backgrounds and references).
    Screened,
}

impl PlotStyle {
    pub const ALL: [PlotStyle; 4] = [PlotStyle::Monochrome, PlotStyle::Color, PlotStyle::Grayscale, PlotStyle::Screened];
    pub fn name(self) -> &'static str {
        match self {
            PlotStyle::Monochrome => "Monochrome",
            PlotStyle::Color => "Color (as displayed)",
            PlotStyle::Grayscale => "Grayscale",
            PlotStyle::Screened => "Screened 50%",
        }
    }
}

/// A window onto the model placed on the paper.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Viewport {
    /// Paper rectangle in millimetres.
    pub min: V2,
    pub max: V2,
    /// Model point shown at the rectangle's centre.
    pub center: V2,
    /// Drawing units per paper millimetre (1:N → N).
    pub scale: f64,
    /// Locked viewports keep their view when edited around.
    #[serde(default)]
    pub locked: bool,
}

impl Viewport {
    pub fn rect(&self) -> BBox {
        BBox::from_pts(self.min, self.max)
    }

    /// Model → paper.
    pub fn xform(&self) -> Xform {
        let k = 1.0 / self.scale.max(1e-12);
        let c = self.rect().center();
        Xform { m: [k, 0.0, 0.0, k], t: c - self.center * k }
    }

    pub fn to_model(&self, p: V2) -> V2 {
        self.center + (p - self.rect().center()) * self.scale
    }

    /// Show `ext` (model) centred at a standard scale that fits.
    pub fn fit(&mut self, ext: &BBox) {
        if ext.is_empty() {
            return;
        }
        let r = self.rect();
        let need = (ext.width() / (r.width() * 0.92)).max(ext.height() / (r.height() * 0.92)).max(1e-9);
        let scales = [0.1, 0.2, 0.5, 1.0, 2.0, 5.0, 10.0, 20.0, 50.0, 100.0, 200.0, 500.0, 1000.0, 2000.0, 5000.0];
        self.scale = scales.into_iter().find(|s| *s >= need).unwrap_or(need);
        self.center = ext.center();
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Layout {
    pub name: String,
    /// Paper, title block and plot style (origin unused: paper space).
    pub sheet: Sheet,
    #[serde(default)]
    pub viewports: Vec<Viewport>,
    /// Plot the viewport frames.
    #[serde(default)]
    pub plot_frames: bool,
}

impl Layout {
    /// A layout with one viewport filling the drawable area, fitted to `ext`.
    pub fn new(name: impl Into<String>, mut sheet: Sheet, ext: &BBox) -> Self {
        sheet.origin = V2::ZERO;
        let area = sheet.drawable_mm();
        let mut vp = Viewport { min: area.min, max: area.max, center: V2::ZERO, scale: 1.0, locked: false };
        vp.fit(ext);
        sheet.scale = vp.scale;
        Self { name: name.into(), sheet, viewports: vec![vp], plot_frames: false }
    }
}

// ------------------------------------------------------------------ clipping

fn clip_seg(a: V2, b: V2, r: &BBox) -> Option<(V2, V2)> {
    let d = b - a;
    let (mut t0, mut t1) = (0.0f64, 1.0f64);
    for (p, q) in [(-d.x, a.x - r.min.x), (d.x, r.max.x - a.x), (-d.y, a.y - r.min.y), (d.y, r.max.y - a.y)] {
        if p.abs() < 1e-15 {
            if q < 0.0 {
                return None;
            }
        } else {
            let t = q / p;
            if p < 0.0 {
                t0 = t0.max(t);
            } else {
                t1 = t1.min(t);
            }
            if t0 > t1 {
                return None;
            }
        }
    }
    Some((a + d * t0, a + d * t1))
}

fn clip_path(pts: &[V2], r: &BBox, out: &mut Vec<Prim>) {
    let mut cur: Vec<V2> = vec![];
    for w in pts.windows(2) {
        match clip_seg(w[0], w[1], r) {
            Some((a, b)) => {
                if cur.last().is_none_or(|l| l.dist(a) > 1e-9) {
                    if cur.len() >= 2 {
                        out.push(Prim::Poly(std::mem::take(&mut cur), false));
                    }
                    cur = vec![a];
                }
                cur.push(b);
            }
            None => {
                if cur.len() >= 2 {
                    out.push(Prim::Poly(std::mem::take(&mut cur), false));
                }
                cur.clear();
            }
        }
    }
    if cur.len() >= 2 {
        out.push(Prim::Poly(cur, false));
    }
}

/// One rectangle side: inside test and the crossing point of an edge.
type ClipEdge = (fn(V2, &BBox) -> bool, fn(V2, V2, &BBox) -> V2);

/// Sutherland–Hodgman against the rectangle (convex polygons stay convex).
fn clip_poly(pts: &[V2], r: &BBox) -> Vec<V2> {
    let mut poly = pts.to_vec();
    let edges: [ClipEdge; 4] = [
        (|p, r| p.x >= r.min.x, |a, b, r| a + (b - a) * ((r.min.x - a.x) / (b.x - a.x))),
        (|p, r| p.x <= r.max.x, |a, b, r| a + (b - a) * ((r.max.x - a.x) / (b.x - a.x))),
        (|p, r| p.y >= r.min.y, |a, b, r| a + (b - a) * ((r.min.y - a.y) / (b.y - a.y))),
        (|p, r| p.y <= r.max.y, |a, b, r| a + (b - a) * ((r.max.y - a.y) / (b.y - a.y))),
    ];
    for (inside, cut) in edges {
        if poly.is_empty() {
            break;
        }
        let input = std::mem::take(&mut poly);
        for i in 0..input.len() {
            let (a, b) = (input[(i + input.len() - 1) % input.len()], input[i]);
            match (inside(a, r), inside(b, r)) {
                (true, true) => poly.push(b),
                (true, false) => poly.push(cut(a, b, r)),
                (false, true) => {
                    poly.push(cut(a, b, r));
                    poly.push(b);
                }
                (false, false) => {}
            }
        }
    }
    poly
}

/// The parts of `p` inside `r`.
pub fn clip_prim(p: &Prim, r: &BBox) -> Vec<Prim> {
    let b = p.bbox();
    if b.is_empty() || !b.intersects(r) {
        return vec![];
    }
    if r.contains(b.min) && r.contains(b.max) {
        return vec![p.clone()];
    }
    let mut out = vec![];
    match p {
        Prim::Line(a, b) => {
            if let Some((a, b)) = clip_seg(*a, *b, r) {
                out.push(Prim::Line(a, b));
            }
        }
        Prim::Poly(pts, closed) => {
            let mut path = pts.clone();
            if *closed && !pts.is_empty() {
                path.push(pts[0]);
            }
            clip_path(&path, r, &mut out);
        }
        Prim::Circle(c, rad) => clip_path(&arc_points(*c, *rad, 0.0, std::f64::consts::TAU, rad * 0.002), r, &mut out),
        Prim::Arc { c, r: rad, start, end } => clip_path(&arc_points(*c, *rad, *start, *end, rad * 0.002), r, &mut out),
        Prim::Fill(pts) => {
            let c = clip_poly(pts, r);
            if c.len() >= 3 {
                out.push(Prim::Fill(c));
            }
        }
        // Text is shown when its insertion point is inside.
        Prim::Text(t) => {
            if r.contains(t.pos) {
                out.push(p.clone());
            }
        }
        Prim::Image(_) => out.push(p.clone()),
    }
    out
}

impl Document {
    /// Layout `index` (of `layouts`) in paper millimetres: border and title
    /// block, viewport frames (unless plotting without them) and the model
    /// seen through each viewport, clipped to it.
    pub fn layout_items(&self, index: usize, for_plot: bool) -> Vec<RenderItem> {
        let Some(l) = self.layouts.get(index) else { return vec![] };
        let mut out = vec![];
        if l.sheet.visible {
            let mut sh = l.sheet.clone();
            sh.origin = V2::ZERO;
            // The title block shows the main viewport's scale.
            sh.scale = l.viewports.first().map(|v| v.scale).unwrap_or(sh.scale);
            let k = 1.0 / sh.scale.max(1e-12);
            let to_paper = Xform { m: [k, 0.0, 0.0, k], t: V2::ZERO };
            for (p, lw) in self.sheet_prims_for(&sh) {
                out.push(RenderItem { prim: p.transformed(&to_paper), color: 7, linetype: LineType::Continuous, lineweight: lw });
            }
        }
        for vp in &l.viewports {
            let r = vp.rect();
            if !for_plot || l.plot_frames {
                out.push(RenderItem {
                    prim: Prim::Poly(r.corners().to_vec(), true),
                    color: 8,
                    linetype: LineType::Continuous,
                    lineweight: 0.25,
                });
            }
            let x = vp.xform();
            let view = BBox::from_pts(vp.to_model(r.min), vp.to_model(r.max));
            for e in &self.entities {
                let Some(layer) = self.layer(&e.layer) else { continue };
                if !layer.visible || (for_plot && !layer.plot) {
                    continue;
                }
                if !self.entity_bbox(e).intersects(&view) {
                    continue;
                }
                for it in self.render_items(e) {
                    for p in clip_prim(&it.prim.transformed(&x), &r) {
                        out.push(RenderItem { prim: p, color: it.color, linetype: it.linetype, lineweight: it.lineweight });
                    }
                }
            }
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::entity::Shape;

    #[test]
    fn clipping() {
        let r = BBox::from_pts(V2::ZERO, V2::new(10.0, 10.0));
        assert_eq!(
            clip_prim(&Prim::Line(V2::new(-5.0, 5.0), V2::new(15.0, 5.0)), &r),
            vec![Prim::Line(V2::new(0.0, 5.0), V2::new(10.0, 5.0))]
        );
        assert!(clip_prim(&Prim::Line(V2::new(-5.0, 20.0), V2::new(15.0, 20.0)), &r).is_empty());
        // A polyline leaving and re-entering gives two pieces.
        let zig = Prim::Poly(vec![V2::new(2.0, 2.0), V2::new(2.0, 20.0), V2::new(8.0, 20.0), V2::new(8.0, 2.0)], false);
        assert_eq!(clip_prim(&zig, &r).len(), 2);
        let tri = Prim::Fill(vec![V2::new(5.0, 5.0), V2::new(20.0, 5.0), V2::new(5.0, 20.0)]);
        let Prim::Fill(c) = &clip_prim(&tri, &r)[0] else { panic!() };
        assert!(c.iter().all(|p| r.expand(1e-9).contains(*p)) && c.len() == 4);
    }

    #[test]
    fn layout_shows_the_model_through_its_viewport() {
        let mut d = Document::new();
        d.add(Shape::Line { a: V2::new(0.0, 0.0), b: V2::new(1000.0, 0.0) });
        d.add(Shape::Circle { c: V2::new(5000.0, 5000.0), r: 10.0 });
        let mut l = Layout::new("Layout2", d.sheet.clone(), &BBox::from_pts(V2::new(0.0, -10.0), V2::new(1000.0, 10.0)));
        assert_eq!(l.viewports[0].scale, 5.0);
        l.sheet.title.title = "DETAIL".into();
        d.layouts.push(l);
        let items = d.layout_items(0, true);
        // The line, at 1:5, is 200 mm long on paper; the far circle is clipped away.
        assert!(items.iter().any(|it| matches!(it.prim, Prim::Line(a, b) if (a.dist(b) - 200.0).abs() < 1e-9)));
        assert!(!items.iter().any(|it| matches!(it.prim, Prim::Poly(ref p, _) if p.len() > 20)));
        assert!(items.iter().any(|it| matches!(&it.prim, Prim::Text(t) if t.content == "DETAIL")));
        assert!(items.iter().any(|it| matches!(&it.prim, Prim::Text(t) if t.content == "1:5")), "title block shows the scale");
    }
}
