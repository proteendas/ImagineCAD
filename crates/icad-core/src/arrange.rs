// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Proteen Das

//! Keeping dimension text clear: a dimension whose text overlaps other text
//! or runs through the geometry is moved to the nearest clear place, first
//! along its dimension line, then further out from the part.

use crate::document::Document;
use crate::entity::{DimKind, Dimension, Shape};
use crate::geom::*;
use crate::prim::Prim;

/// Text boxes of a set of primitives.
fn text_boxes(prims: &[Prim]) -> Vec<BBox> {
    prims.iter().filter_map(|p| if let Prim::Text(t) = p { Some(BBox::of(t.corners())) } else { None }).collect()
}

fn seg_hits_box(a: V2, b: V2, r: &BBox) -> bool {
    if r.contains(a) || r.contains(b) {
        return true;
    }
    let c = [r.min, V2::new(r.max.x, r.min.y), r.max, V2::new(r.min.x, r.max.y)];
    (0..4).any(|i| seg_seg(a, b, c[i], c[(i + 1) % 4]).is_some())
}

fn seg_seg(a: V2, b: V2, c: V2, d: V2) -> Option<V2> {
    let (r, s) = (b - a, d - c);
    let den = r.cross(s);
    if den.abs() < 1e-12 {
        return None;
    }
    let t = (c - a).cross(s) / den;
    let u = (c - a).cross(r) / den;
    ((0.0..=1.0).contains(&t) && (0.0..=1.0).contains(&u)).then(|| a + r * t)
}

/// What a dimension's text must keep clear of.
struct Obstacles {
    texts: Vec<BBox>,
    /// Geometry edges (lines and flattened curves) of the part.
    edges: Vec<(V2, V2)>,
}

impl Obstacles {
    fn hit(&self, b: &BBox) -> bool {
        let shrunk = b.expand(-b.width().min(b.height()) * 0.08);
        self.texts.iter().any(|t| t.intersects(&shrunk)) || self.edges.iter().any(|(a, c)| seg_hits_box(*a, *c, &shrunk))
    }
}

impl Document {
    fn obstacles(&self, skip: &[u64]) -> Obstacles {
        let mut texts = vec![];
        let mut edges = vec![];
        for e in self.entities.iter().filter(|e| self.is_visible(e) && !skip.contains(&e.id)) {
            let prims = self.prims_of(e);
            texts.extend(text_boxes(&prims));
            if e.shape.is_annotation() || matches!(e.shape, Shape::Hatch(_)) {
                continue;
            }
            for p in prims {
                match p {
                    Prim::Line(a, b) => edges.push((a, b)),
                    Prim::Poly(v, closed) => {
                        edges.extend(v.windows(2).map(|w| (w[0], w[1])));
                        if closed && v.len() > 2 {
                            edges.push((v[v.len() - 1], v[0]));
                        }
                    }
                    Prim::Circle(c, r) => {
                        let n = 32;
                        edges.extend((0..n).map(|i| {
                            let (a0, a1) = (std::f64::consts::TAU * i as f64 / n as f64, std::f64::consts::TAU * (i + 1) as f64 / n as f64);
                            (c + V2::polar(r, a0), c + V2::polar(r, a1))
                        }));
                    }
                    Prim::Arc { c, r, start, end } => {
                        let sw = arc_sweep(start, end);
                        let n = ((sw / 0.2).ceil() as usize).max(2);
                        edges.extend((0..n).map(|i| {
                            (c + V2::polar(r, start + sw * i as f64 / n as f64), c + V2::polar(r, start + sw * (i + 1) as f64 / n as f64))
                        }));
                    }
                    _ => {}
                }
            }
        }
        Obstacles { texts, edges }
    }

    fn dim_text_box(&self, d: &Dimension) -> Option<BBox> {
        let ctx = self.ctx();
        text_boxes(&d.prims(ctx.ds, ctx.scale)).into_iter().reduce(|a, b| a.union(&b))
    }

    /// Move dimension `id`'s text to the nearest clear place. Returns true
    /// when it moved.
    pub fn declutter_dim(&mut self, id: u64) -> bool {
        let Some(Shape::Dim(d)) = self.get(id).map(|e| e.shape.clone()) else { return false };
        if !matches!(d.kind, DimKind::Horizontal | DimKind::Vertical | DimKind::Aligned | DimKind::Radius | DimKind::Diameter) {
            return false;
        }
        let obs = self.obstacles(&[id]);
        let Some(tb) = self.dim_text_box(&d) else { return false };
        if !obs.hit(&tb) {
            return false;
        }
        let th = self.dimstyle.text_height * self.ascale();
        let ext = self.entities.iter().filter(|e| !e.shape.is_annotation()).fold(BBox::empty(), |b, e| b.union(&self.entity_bbox(e)));
        let mut cands: Vec<(f64, Dimension)> = vec![];
        match d.kind {
            DimKind::Radius | DimKind::Diameter => {
                // Swing the leader round the circle and out.
                let c = d.p1;
                let r0 = d.loc.dist(c).max(d.p1.dist(d.p2) * 1.3);
                let a0 = (d.loc - c).angle();
                for ring in 0..4 {
                    for k in 0..24 {
                        let da = (k as f64 / 2.0).ceil() * if k % 2 == 0 { 1.0 } else { -1.0 } * 0.26;
                        let rr = r0 + ring as f64 * 2.0 * th;
                        let mut n = d.clone();
                        n.loc = c + V2::polar(rr, a0 + da);
                        cands.push((da.abs() * rr + ring as f64 * 2.0 * th, n));
                    }
                }
            }
            _ => {
                let dir = d.direction();
                let n = dir.perp();
                // Outward: away from the part's centre.
                let out = if (d.loc - ext.center()).dot(n) >= 0.0 { n } else { -n };
                for row in 0..6 {
                    for k in 0..17 {
                        let along = (k as f64 / 2.0).ceil() * if k % 2 == 0 { 1.0 } else { -1.0 } * th;
                        let mut m = d.clone();
                        m.loc = d.loc + dir * along + out * (row as f64 * 2.0 * th);
                        // Sliding far along the line is worse than one row out.
                        cands.push((along.abs() * 0.7 + row as f64 * 2.0 * th, m));
                    }
                }
            }
        }
        cands.sort_by(|a, b| a.0.total_cmp(&b.0));
        for (_, m) in cands.into_iter().skip(1) {
            if let Some(b) = self.dim_text_box(&m) {
                if !obs.hit(&b) {
                    if let Some(e) = self.get_mut(id) {
                        e.shape = Shape::Dim(m);
                    }
                    return true;
                }
            }
        }
        false
    }

    /// Declutter every dimension in `ids` (all dimensions when empty);
    /// returns how many moved.
    pub fn declutter_dims(&mut self, ids: &[u64]) -> usize {
        let targets: Vec<u64> = if ids.is_empty() {
            self.entities.iter().filter(|e| matches!(e.shape, Shape::Dim(_))).map(|e| e.id).collect()
        } else {
            ids.to_vec()
        };
        targets.into_iter().filter(|id| self.declutter_dim(*id)).count()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn overlapping_texts_are_separated() {
        let mut doc = Document::new();
        doc.add(Shape::Line { a: V2::new(0.0, 0.0), b: V2::new(100.0, 0.0) });
        doc.add(Shape::Line { a: V2::new(0.0, 0.0), b: V2::new(0.0, 20.0) });
        // Two dimensions on the same line: their texts sit on top of each other.
        let a = doc.add(Shape::Dim(Dimension::new(DimKind::Horizontal, V2::new(0.0, 0.0), V2::new(100.0, 0.0), V2::new(50.0, -10.0))));
        let b = doc.add(Shape::Dim(Dimension::new(DimKind::Horizontal, V2::new(40.0, 0.0), V2::new(60.0, 0.0), V2::new(50.0, -10.0))));
        let box_of = |doc: &Document, id: u64| match &doc.get(id).unwrap().shape {
            Shape::Dim(d) => doc.dim_text_box(d).unwrap(),
            _ => unreachable!(),
        };
        assert!(box_of(&doc, a).intersects(&box_of(&doc, b)));
        assert!(doc.declutter_dim(b));
        assert!(!box_of(&doc, a).intersects(&box_of(&doc, b)));
        // A clear dimension stays where it is.
        assert!(!doc.declutter_dim(a));
        // Text crossing the part's edge moves off it.
        let c = doc.add(Shape::Dim(Dimension::new(DimKind::Vertical, V2::new(0.0, 0.0), V2::new(0.0, 20.0), V2::new(2.0, 10.0))));
        assert!(doc.declutter_dim(c));
        let obs = doc.obstacles(&[c]);
        assert!(!obs.hit(&box_of(&doc, c)));
    }
}
