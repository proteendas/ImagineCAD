// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Proteen Das

//! Drawing views generated from the geometry: section views from a cutting
//! line through a part of constant thickness, and enlarged detail views of a
//! circular area.

use crate::document::{layers, Document};
use crate::entity::{Curve, Entity, Hatch, Leader, Shape};
use crate::geom::*;
use crate::prim::Text;
use crate::region::{BoolOp, Region};
use crate::style::{LineType, LtRef};
use crate::{HAlign, VAlign};
use std::f64::consts::TAU;

/// Whether `p` lies inside the closed boundaries made by `curves` (even–odd,
/// by counting crossings of a ray at an unusual angle so it rarely meets a
/// vertex).
pub fn inside(curves: &[Curve], p: V2) -> bool {
    let ray = Curve::Seg(p, p + V2::polar(1e7, 0.618_034));
    let mut hits: Vec<V2> = curves.iter().flat_map(|c| ray.intersect(c)).collect();
    hits.sort_by(|a, b| a.x.total_cmp(&b.x).then(a.y.total_cmp(&b.y)));
    hits.dedup_by(|a, b| a.dist(*b) < 1e-7);
    hits.len() % 2 == 1
}

/// Stretches of the cutting line `a`–`b` that run through material, as
/// distances from `a`.
pub fn cut_intervals(curves: &[Curve], a: V2, b: V2) -> Vec<(f64, f64)> {
    let len = a.dist(b);
    if len < 1e-9 {
        return vec![];
    }
    let cut = Curve::Seg(a, b);
    let mut ts: Vec<f64> = vec![0.0, len];
    ts.extend(curves.iter().flat_map(|c| cut.intersect(c)).map(|p| p.dist(a)));
    ts.sort_by(f64::total_cmp);
    ts.dedup_by(|x, y| (*x - *y).abs() < 1e-7);
    let d = (b - a) / len;
    let mut out: Vec<(f64, f64)> = vec![];
    for w in ts.windows(2) {
        if w[1] - w[0] < 1e-7 || !inside(curves, a + d * ((w[0] + w[1]) / 2.0)) {
            continue;
        }
        match out.last_mut() {
            Some(last) if (last.1 - w[0]).abs() < 1e-7 => last.1 = w[1],
            _ => out.push((w[0], w[1])),
        }
    }
    out
}

/// A shape of a generated view with the properties it is drawn with.
#[derive(Clone, Debug, PartialEq)]
pub struct Piece {
    pub layer: String,
    pub linetype: LtRef,
    pub lineweight: Option<f64>,
    pub shape: Shape,
}

impl Piece {
    fn on(layer: &str, shape: Shape) -> Piece {
        Piece { layer: layer.into(), linetype: LtRef::ByLayer, lineweight: None, shape }
    }
}

/// Letters for views, skipping I, O and Q (easily misread).
pub fn next_view_letter(doc: &Document) -> String {
    let used: Vec<String> = doc
        .entities
        .iter()
        .filter_map(|e| match &e.shape {
            Shape::Text(t) => Some(t.content.as_str()),
            Shape::MText(m) => Some(m.content.as_str()),
            _ => None,
        })
        .filter_map(|c| {
            let c = c.trim();
            let rest = c.strip_prefix("SECTION ").or_else(|| c.strip_prefix("DETAIL "))?;
            Some(rest.chars().take_while(|ch| ch.is_ascii_alphabetic()).collect())
        })
        .collect();
    let letters: Vec<char> = ('A'..='Z').filter(|c| !matches!(c, 'I' | 'O' | 'Q')).collect();
    for n in 1.. {
        for &c in &letters {
            let s: String = std::iter::repeat_n(c, n).collect();
            if !used.contains(&s) {
                return s;
            }
        }
    }
    unreachable!()
}

/// The cutting-line symbol: a thin chain line with thick ends, arrows
/// pointing in the viewing direction `view` and the letter at each end.
pub fn cutting_line(a: V2, b: V2, view: V2, letter: &str, text_height: f64) -> Vec<Piece> {
    let d = (b - a).norm();
    let v = view.norm();
    let h = text_height;
    let stroke = 3.0 * h;
    let mut out =
        vec![Piece { layer: layers::CENTER.into(), linetype: LtRef::Lt(LineType::Center), lineweight: None, shape: Shape::Line { a, b } }];
    for (end, inward) in [(a, d), (b, -d)] {
        out.push(Piece {
            layer: layers::OUTLINE.into(),
            linetype: LtRef::Lt(LineType::Continuous),
            lineweight: Some(0.7),
            shape: Shape::Line { a: end, b: end + inward * stroke },
        });
        let tail = end - v * (2.5 * stroke);
        out.push(Piece::on(layers::DIMENSIONS, Shape::Leader(Leader { pts: vec![end, tail], text: String::new(), style: None })));
        out.push(Piece::on(
            layers::DIMENSIONS,
            Shape::Text(Text::new(tail - v * (0.8 * h) - inward * (0.4 * h), h * 1.4, letter).aligned(HAlign::Center, VAlign::Middle)),
        ));
    }
    out
}

/// A section view of a part `depth` thick, cut along `a`–`b` through the
/// closed boundaries `curves`: the cut faces laid out from `origin` along x
/// (as seen looking along the arrows) and hatched, with its title below.
pub fn section_view(curves: &[Curve], a: V2, b: V2, depth: f64, origin: V2, letter: &str, text_height: f64) -> Vec<Piece> {
    let spans = cut_intervals(curves, a, b);
    let mut out = vec![];
    for (s0, s1) in &spans {
        let pts = vec![origin + V2::new(*s0, 0.0), origin + V2::new(*s1, 0.0), origin + V2::new(*s1, depth), origin + V2::new(*s0, depth)];
        out.push(Piece::on(
            layers::HATCH,
            Shape::Hatch(Hatch { loops: vec![pts.clone()], pattern: "ANSI31".into(), scale: 1.0, angle: 0.0 }),
        ));
        out.push(Piece::on(layers::OUTLINE, Shape::Polyline { pts, closed: true }));
    }
    if let (Some(first), Some(last)) = (spans.first(), spans.last()) {
        let mid = origin + V2::new((first.0 + last.1) / 2.0, -2.5 * text_height);
        out.push(Piece::on(
            layers::TEXT,
            Shape::Text(Text::new(mid, text_height * 1.4, format!("SECTION {letter}–{letter}")).aligned(HAlign::Center, VAlign::Top)),
        ));
    }
    out
}

/// The parts of `c` inside the circle (`o`, `r`).
pub fn clip_to_circle(c: &Curve, o: V2, r: f64) -> Vec<Curve> {
    let within = |p: V2| p.dist(o) <= r + 1e-9;
    match *c {
        Curve::Seg(a, b) => {
            let mut ts = vec![0.0, 1.0];
            ts.extend(line_circle(a, b, o, r).into_iter().map(|(_, t)| t).filter(|t| (0.0..=1.0).contains(t)));
            ts.sort_by(f64::total_cmp);
            ts.windows(2)
                .filter(|w| w[1] - w[0] > 1e-9 && within(a.lerp(b, (w[0] + w[1]) / 2.0)))
                .map(|w| Curve::Seg(a.lerp(b, w[0]), a.lerp(b, w[1])))
                .collect()
        }
        Curve::Circle(c, rc) => {
            let cuts: Vec<f64> = circle_circle(c, rc, o, r).into_iter().map(|p| norm_angle((p - c).angle())).collect();
            if cuts.len() < 2 {
                return if within(c + V2::new(rc, 0.0)) { vec![Curve::Circle(c, rc)] } else { vec![] };
            }
            arc_pieces(c, rc, 0.0, TAU, cuts, within)
        }
        Curve::Arc(c, rc, s, e) => {
            let cuts: Vec<f64> =
                circle_circle(c, rc, o, r).into_iter().map(|p| norm_angle((p - c).angle())).filter(|a| angle_on_arc(*a, s, e)).collect();
            let sweep = norm_angle(e - s);
            arc_pieces(c, rc, s, if sweep < 1e-12 { TAU } else { sweep }, cuts, within)
        }
    }
}

/// Pieces of the arc from `s` sweeping `sweep` split at angles `cuts`, kept
/// where `keep` holds at their middle.
fn arc_pieces(c: V2, r: f64, s: f64, sweep: f64, cuts: Vec<f64>, keep: impl Fn(V2) -> bool) -> Vec<Curve> {
    let mut us: Vec<f64> = vec![0.0, sweep];
    us.extend(cuts.into_iter().map(|a| norm_angle(a - s)).filter(|u| *u < sweep));
    us.sort_by(f64::total_cmp);
    us.dedup_by(|x, y| (*x - *y).abs() < 1e-9);
    us.windows(2)
        .filter(|w| w[1] - w[0] > 1e-9 && keep(c + V2::polar(r, s + (w[0] + w[1]) / 2.0)))
        .map(|w| Curve::Arc(c, r, norm_angle(s + w[0]), norm_angle(s + w[1])))
        .collect()
}

/// A detail view: the geometry of `ids` inside the circle (`o`, `r`),
/// enlarged `k` times about `o` and placed with its centre at `at`, inside
/// its own circle and titled; plus the marking circle and letter on the
/// original view.
#[allow(clippy::too_many_arguments)]
pub fn detail_view(doc: &Document, ids: &[u64], o: V2, r: f64, k: f64, at: V2, letter: &str, text_height: f64) -> Vec<Piece> {
    let map = |p: V2| at + (p - o) * k;
    let mut out = vec![];
    let disc: Vec<V2> = (0..96).map(|i| o + V2::polar(r, TAU * i as f64 / 96.0)).collect();
    for id in ids {
        let Some(e) = doc.get(*id) else { continue };
        let piece = |shape: Shape| Piece { layer: e.layer.clone(), linetype: e.linetype, lineweight: e.lineweight, shape };
        match &e.shape {
            Shape::Hatch(h) => {
                let clipped = Region::boolean(&Region::from_loops(&h.loops), &[Region { loops: vec![disc.clone()] }], BoolOp::Intersect);
                let loops: Vec<Vec<V2>> =
                    clipped.iter().flat_map(|g| g.loops.iter().map(|l| l.iter().map(|p| map(*p)).collect())).collect();
                if !loops.is_empty() {
                    out.push(piece(Shape::Hatch(Hatch { loops, ..h.clone() })));
                }
            }
            // Annotation is not carried into a detail; it is dimensioned anew.
            Shape::Dim(_)
            | Shape::Text(_)
            | Shape::MText(_)
            | Shape::Leader(_)
            | Shape::Table(_)
            | Shape::XLine { .. }
            | Shape::Ray { .. } => {}
            _ => {
                for c in doc.curves_of(e) {
                    for part in clip_to_circle(&c, o, r) {
                        out.push(piece(match part {
                            Curve::Seg(a, b) => Shape::Line { a: map(a), b: map(b) },
                            Curve::Circle(c, rc) => Shape::Circle { c: map(c), r: rc * k },
                            Curve::Arc(c, rc, s, e) => Shape::Arc { c: map(c), r: rc * k, start: s, end: e },
                        }));
                    }
                }
            }
        }
    }
    let h = text_height;
    let thin =
        |shape: Shape| Piece { layer: layers::DIMENSIONS.into(), linetype: LtRef::Lt(LineType::Continuous), lineweight: None, shape };
    out.push(thin(Shape::Circle { c: at, r: r * k }));
    out.push(thin(Shape::Circle { c: o, r }));
    let tag = o + V2::polar(r + 1.2 * h, std::f64::consts::FRAC_PI_4);
    out.push(Piece::on(layers::TEXT, Shape::Text(Text::new(tag, h * 1.4, letter).aligned(HAlign::Left, VAlign::Bottom))));
    out.push(Piece::on(
        layers::TEXT,
        Shape::Text(
            Text::new(at - V2::new(0.0, r * k + 2.0 * h), h * 1.4, format!("DETAIL {letter} ({} : 1)", fmt_scale(k)))
                .aligned(HAlign::Center, VAlign::Top),
        ),
    ));
    out
}

fn fmt_scale(k: f64) -> String {
    let s = format!("{k:.3}");
    s.trim_end_matches('0').trim_end_matches('.').to_string()
}

/// Add generated pieces to the document; returns their ids.
pub fn add_pieces(doc: &mut Document, pieces: Vec<Piece>) -> Vec<u64> {
    // Drawings from elsewhere may lack the standard layers the views use.
    let standard = Document::new();
    for p in &pieces {
        if doc.layer(&p.layer).is_none() {
            if let Some(l) = standard.layer(&p.layer) {
                doc.ensure_layer(l.clone());
            }
        }
    }
    pieces
        .into_iter()
        .map(|p| {
            let id = doc.alloc_id();
            doc.entities.push(Entity {
                id,
                layer: p.layer,
                color: Default::default(),
                linetype: p.linetype,
                lineweight: p.lineweight,
                shape: p.shape,
            });
            id
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn plate() -> Vec<Curve> {
        // 100 × 40 plate with a Ø10 hole at (30, 20).
        let mut c: Vec<Curve> = vec![
            Curve::Seg(V2::new(0.0, 0.0), V2::new(100.0, 0.0)),
            Curve::Seg(V2::new(100.0, 0.0), V2::new(100.0, 40.0)),
            Curve::Seg(V2::new(100.0, 40.0), V2::new(0.0, 40.0)),
            Curve::Seg(V2::new(0.0, 40.0), V2::new(0.0, 0.0)),
        ];
        c.push(Curve::Circle(V2::new(30.0, 20.0), 5.0));
        c
    }

    #[test]
    fn section_through_a_plate_with_a_hole() {
        let c = plate();
        let spans = cut_intervals(&c, V2::new(-10.0, 20.0), V2::new(110.0, 20.0));
        assert_eq!(spans.len(), 2, "{spans:?}");
        assert!((spans[0].0 - 10.0).abs() < 1e-9 && (spans[0].1 - 35.0).abs() < 1e-9);
        assert!((spans[1].0 - 45.0).abs() < 1e-9 && (spans[1].1 - 110.0).abs() < 1e-9);
        // Through a corner: the cut still starts and ends on the boundary.
        let diag = cut_intervals(&c, V2::new(-10.0, -10.0), V2::new(50.0, 50.0));
        assert_eq!(diag.len(), 1);
        let v = section_view(&c, V2::new(-10.0, 20.0), V2::new(110.0, 20.0), 8.0, V2::new(0.0, -100.0), "A", 2.5);
        let hatches = v.iter().filter(|p| matches!(p.shape, Shape::Hatch(_))).count();
        assert_eq!(hatches, 2);
        assert!(v.iter().any(|p| matches!(&p.shape, Shape::Text(t) if t.content == "SECTION A–A")));
        let line = cutting_line(V2::new(-10.0, 20.0), V2::new(110.0, 20.0), V2::new(0.0, 1.0), "A", 2.5);
        assert_eq!(line.iter().filter(|p| matches!(p.shape, Shape::Leader(_))).count(), 2);
    }

    #[test]
    fn detail_clips_and_enlarges() {
        // Circle around the hole corner region.
        let o = V2::new(30.0, 20.0);
        let parts = clip_to_circle(&Curve::Seg(V2::new(0.0, 20.0), V2::new(100.0, 20.0)), o, 8.0);
        assert_eq!(parts, vec![Curve::Seg(V2::new(22.0, 20.0), V2::new(38.0, 20.0))]);
        assert_eq!(clip_to_circle(&Curve::Circle(o, 5.0), o, 8.0), vec![Curve::Circle(o, 5.0)]);
        let arcs = clip_to_circle(&Curve::Circle(V2::new(38.0, 20.0), 4.0), o, 8.0);
        assert_eq!(arcs.len(), 1);
        let Curve::Arc(_, _, s, e) = arcs[0] else { panic!() };
        // The left part, from 104.5° to 255.5° (meeting points at x = 37).
        assert!((s.to_degrees() - 104.48).abs() < 0.01 && (e.to_degrees() - 255.52).abs() < 0.01, "{s} {e}");

        let mut doc = Document::new();
        let line = doc.add(Shape::Line { a: V2::new(0.0, 20.0), b: V2::new(100.0, 20.0) });
        let hole = doc.add(Shape::Circle { c: o, r: 5.0 });
        doc.add(Shape::Text(Text::new(o, 2.5, "NOTE")));
        let v = detail_view(&doc, &[line, hole, 3], o, 8.0, 2.0, V2::new(200.0, 0.0), &next_view_letter(&doc), 2.5);
        assert!(v.iter().any(|p| p.shape == Shape::Line { a: V2::new(184.0, 0.0), b: V2::new(216.0, 0.0) }));
        assert!(v.iter().any(|p| p.shape == Shape::Circle { c: V2::new(200.0, 0.0), r: 10.0 }));
        assert!(v.iter().any(|p| matches!(&p.shape, Shape::Text(t) if t.content == "DETAIL A (2 : 1)")));
        add_pieces(&mut doc, v);
        assert_eq!(next_view_letter(&doc), "B");
    }
}
