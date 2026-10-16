// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Proteen Das

//! Dimension association and dimension-driven rectification.
//!
//! Hand sketches are not to scale. Every dimension label found on the sketch
//! is attached to the feature it measures, then the drawing is re-proportioned
//! so that each dimensioned distance becomes exactly the written value:
//!
//! * x and y are solved independently as 1-D problems over the distinct
//!   coordinates ("stations") of the geometry;
//! * each horizontal/vertical dimension is a hard constraint between two
//!   stations; undimensioned gaps are springs that keep the sketch proportions;
//! * a weighted least-squares solve gives every station's true position and
//!   the drawing is mapped through a monotone piecewise-linear function, so
//!   connectivity, parallelism and alignment are preserved.

use crate::beautify::{is_h, is_v};
use crate::fit::Geo;
use crate::labels::{Label, LabelKind};
use icad_core::geom::*;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SpanKind {
    X,
    Y,
    Aligned,
}

/// A linear dimension: distance between `a` and `b` (px) along `kind`.
#[derive(Clone, Debug)]
pub struct Span {
    pub label: usize,
    pub kind: SpanKind,
    pub a: V2,
    pub b: V2,
    /// A point on the sketched dimension line (or the label centre).
    pub line_pt: V2,
    /// True if a sketched dimension line was found (vs. edge fallback).
    pub has_line: bool,
}

/// A radius/diameter dimension on a circle or arc (identified by geometry).
#[derive(Clone, Debug)]
pub struct Radial {
    pub label: usize,
    pub center: V2,
    pub r: f64,
}

/// An angle label between two outline edges meeting at `v` (px): the angle
/// runs counter-clockwise from the edge towards `a` to the edge towards `b`.
#[derive(Clone, Debug)]
pub struct AngleDim {
    pub label: usize,
    pub v: V2,
    pub a: V2,
    pub b: V2,
}

#[derive(Default)]
pub struct Classified {
    pub spans: Vec<Span>,
    pub radials: Vec<Radial>,
    pub angles: Vec<AngleDim>,
    /// Indices of lines recognised as dimension / extension / leader lines.
    pub claimed: Vec<usize>,
    pub matched: Vec<bool>,
}

fn dir_of(a: V2, b: V2) -> V2 {
    (b - a).norm()
}

/// Attach labels to sketch features. `part[i]` says whether geo `i` belongs to
/// a part-outline component (as opposed to free-standing annotation strokes).
pub fn associate(geos: &[Geo], part: &[bool], labels: &[Label], tol: f64) -> Classified {
    let mut out = Classified { matched: vec![false; labels.len()], ..Default::default() };
    let mut claimed = vec![false; geos.len()];

    // --- Linear labels: candidate dimension lines, assigned greedily by score.
    let mut cands: Vec<(f64, usize, usize)> = vec![];
    for (li, l) in labels.iter().enumerate() {
        if l.kind != LabelKind::Linear {
            continue;
        }
        let c = l.center();
        let th = l.bbox.width().min(l.bbox.height()).max(1.0);
        let tl = l.size().max(1.0);
        let text_vertical = l.bbox.height() > l.bbox.width() * 1.3;
        for (gi, g) in geos.iter().enumerate() {
            let Geo::Line(p, q) = *g else { continue };
            let len = p.dist(q);
            if len < th {
                continue;
            }
            let d = dir_of(p, q);
            let t = (c - p).dot(d);
            let perp = (c - p).cross(d).abs();
            let outside = (-t).max(t - len).max(0.0);
            if perp > th * 1.6 + tol || outside > tl * 0.6 + tol {
                continue;
            }
            let line_vertical = d.y.abs() > 0.9;
            let orient_pen = if (is_h(g) || is_v(g)) && text_vertical == line_vertical { 0.0 } else { 0.25 };
            let part_pen = if part[gi] { 0.6 } else { 0.0 };
            cands.push((perp / th + outside / tl + orient_pen + part_pen, li, gi));
        }
    }
    cands.sort_by(|a, b| a.0.total_cmp(&b.0));
    for (_, li, gi) in cands {
        if out.matched[li] || claimed[gi] {
            continue;
        }
        let Geo::Line(p, q) = geos[gi] else { continue };
        let l = &labels[li];
        let tl = l.size();
        // An outline edge is only a dimension line when extension lines meet
        // it square at both ends; otherwise the label dimensions the edge.
        if part[gi] {
            let d = dir_of(p, q);
            let square_at = |end: V2| {
                geos.iter().enumerate().any(|(k, g)| match *g {
                    Geo::Line(e1, e2) if k != gi => dist_point_segment(end, e1, e2) < tol * 1.5 && dir_of(e1, e2).dot(d).abs() < 0.17,
                    _ => false,
                })
            };
            if !(square_at(p) && square_at(q)) {
                let kind = if is_h(&geos[gi]) {
                    SpanKind::X
                } else if is_v(&geos[gi]) {
                    SpanKind::Y
                } else {
                    SpanKind::Aligned
                };
                out.matched[li] = true;
                out.spans.push(Span { label: li, kind, a: p, b: q, line_pt: l.center(), has_line: false });
                continue;
            }
        }
        claimed[gi] = true;
        out.claimed.push(gi);
        out.matched[li] = true;
        let d = dir_of(p, q);
        let n = d.perp();
        let mut tmin = 0.0f64;
        let mut tmax = p.dist(q);
        // Broken dimension line around the text: absorb collinear pieces.
        for (k, g) in geos.iter().enumerate() {
            let Geo::Line(a2, b2) = *g else { continue };
            if claimed[k] || dir_of(a2, b2).cross(d).abs() > 0.06 {
                continue;
            }
            if (a2 - p).dot(n).abs() > tol || (b2 - p).dot(n).abs() > tol {
                continue;
            }
            let (ta, tb) = ((a2 - p).dot(d), (b2 - p).dot(d));
            let gap = (ta.min(tb) - tmax).max(tmin - ta.max(tb));
            if gap < tl + 2.0 * tol {
                claimed[k] = true;
                out.claimed.push(k);
                tmin = tmin.min(ta.min(tb));
                tmax = tmax.max(ta.max(tb));
            }
        }
        let a = p + d * tmin;
        let b = p + d * tmax;
        // Extension lines and arrowhead strokes at both ends.
        for end in [a, b] {
            let mut chain: Vec<usize> = vec![];
            for (k, g) in geos.iter().enumerate() {
                let Geo::Line(e1, e2) = *g else { continue };
                if claimed[k] || dist_point_segment(end, e1, e2) >= tol * 1.5 {
                    continue;
                }
                let len = e1.dist(e2);
                let perpendicular = dir_of(e1, e2).dot(d).abs() < 0.17;
                let far = if e1.dist(end) > e2.dist(end) { e1 } else { e2 };
                // In a part outline, only claim a perpendicular stroke whose far end is
                // free or that continues straight through a junction (crosses an edge).
                let crosses = geos.iter().enumerate().any(|(j, g2)| match *g2 {
                    Geo::Line(f1, f2) if j != k => {
                        let kd = dir_of(e1, e2);
                        dir_of(f1, f2).cross(kd).abs() < 0.08
                            && (f1.dist(far) < tol || f2.dist(far) < tol)
                            && ((f1 - far).dot(far - end) > tol || (f2 - far).dot(far - end) > tol)
                    }
                    _ => false,
                });
                let free = dangling(geos, far, &[k, gi], tol * 0.5) || crosses;
                if (perpendicular && (!part[k] || free)) || len < 3.0 * tol {
                    claimed[k] = true;
                    out.claimed.push(k);
                    if perpendicular {
                        chain.push(k);
                    }
                }
            }
            // Follow an extension line through junctions (e.g. a centre line
            // crossing the outline) while the continuation stays free-ended.
            while let Some(k) = chain.pop() {
                let Geo::Line(e1, e2) = geos[k] else { continue };
                let kd = dir_of(e1, e2);
                for (j, g) in geos.iter().enumerate() {
                    let Geo::Line(f1, f2) = *g else { continue };
                    if claimed[j] || dir_of(f1, f2).cross(kd).abs() > 0.08 || (f1 - e1).cross(kd).abs() > tol * 0.5 {
                        continue;
                    }
                    let near = [f1, f2].iter().any(|p| p.dist(e1) < tol || p.dist(e2) < tol);
                    let other = if f1.dist(e1).min(f1.dist(e2)) < f2.dist(e1).min(f2.dist(e2)) { f2 } else { f1 };
                    if near && dangling(geos, other, &[j, k], tol * 0.5) {
                        claimed[j] = true;
                        out.claimed.push(j);
                        chain.push(j);
                    }
                }
            }
        }
        let kind = if is_h(&geos[gi]) {
            SpanKind::X
        } else if is_v(&geos[gi]) {
            SpanKind::Y
        } else {
            SpanKind::Aligned
        };
        out.spans.push(Span { label: li, kind, a, b, line_pt: a.mid(b), has_line: true });
    }

    // --- Linear labels without a sketched dimension line: dimension the nearest edge.
    for (li, l) in labels.iter().enumerate() {
        if out.matched[li] || l.kind != LabelKind::Linear {
            continue;
        }
        let c = l.center();
        let tl = l.size().max(1.0);
        let text_vertical = l.bbox.height() > l.bbox.width() * 1.3;
        let mut best: Option<(f64, usize)> = None;
        for (gi, g) in geos.iter().enumerate() {
            let Geo::Line(p, q) = *g else { continue };
            if claimed[gi] {
                continue;
            }
            let len = p.dist(q);
            let t = project_t(c, p, q);
            if !(-0.05..=1.05).contains(&t) || len < tl * 0.5 {
                continue;
            }
            let dist = dist_point_segment(c, p, q);
            if dist > 2.5 * tl + tol {
                continue;
            }
            let line_vertical = dir_of(p, q).y.abs() > 0.9;
            let score = dist / tl + if text_vertical == line_vertical { 0.0 } else { 0.3 };
            if best.is_none_or(|b| score < b.0) {
                best = Some((score, gi));
            }
        }
        if let Some((_, gi)) = best {
            let Geo::Line(p, q) = geos[gi] else { continue };
            let kind = if is_h(&geos[gi]) {
                SpanKind::X
            } else if is_v(&geos[gi]) {
                SpanKind::Y
            } else {
                SpanKind::Aligned
            };
            out.matched[li] = true;
            out.spans.push(Span { label: li, kind, a: p, b: q, line_pt: c, has_line: false });
        }
    }

    // --- Diameter / radius labels → nearest circle or arc.
    for (li, l) in labels.iter().enumerate() {
        if !matches!(l.kind, LabelKind::Diameter | LabelKind::Radius) {
            continue;
        }
        let c = l.center();
        let tl = l.size().max(1.0);
        let mut best: Option<(f64, usize)> = None;
        for (gi, g) in geos.iter().enumerate() {
            let (ctr, r, is_circle) = match *g {
                Geo::Circle(ctr, r) => (ctr, r, true),
                Geo::Arc { c: ctr, r, .. } => (ctr, r, false),
                _ => continue,
            };
            let d = (c.dist(ctr) - r).abs();
            if d > (2.0 * r).max(4.0 * tl) {
                continue;
            }
            let kind_pen = match (l.kind, is_circle) {
                (LabelKind::Diameter, true) | (LabelKind::Radius, false) => 0.0,
                _ => 0.5,
            };
            // A leader stroke from the label to this circle is decisive.
            let lb = l.bbox.expand(tol + tl * 0.6);
            let has_leader = geos.iter().any(|g2| match *g2 {
                Geo::Line(e1, e2) => {
                    let on = |p: V2| (p.dist(ctr) - r).abs() < 2.0 * tol;
                    (on(e1) && lb.contains(e2)) || (on(e2) && lb.contains(e1))
                }
                _ => false,
            });
            let score = d / r.max(tl) + kind_pen - if has_leader { 3.0 } else { 0.0 };
            if best.is_none_or(|b| score < b.0) {
                best = Some((score, gi));
            }
        }
        if let Some((_, gi)) = best {
            let (ctr, r) = match geos[gi] {
                Geo::Circle(c, r) => (c, r),
                Geo::Arc { c, r, .. } => (c, r),
                _ => unreachable!(),
            };
            out.matched[li] = true;
            out.radials.push(Radial { label: li, center: ctr, r });
            // Leader lines from the label to the circle.
            let lb = l.bbox.expand(tol + tl * 0.3);
            for (k, g) in geos.iter().enumerate() {
                let Geo::Line(e1, e2) = *g else { continue };
                if claimed[k] {
                    continue;
                }
                let on_circle = |p: V2| (p.dist(ctr) - r).abs() < tol * 1.5 || p.dist(ctr) < tol;
                if (on_circle(e1) && lb.contains(e2)) || (on_circle(e2) && lb.contains(e1)) {
                    claimed[k] = true;
                    out.claimed.push(k);
                }
            }
        }
    }

    // --- Angle labels → the corner of two outline edges they sit in.
    for (li, l) in labels.iter().enumerate() {
        if l.kind != LabelKind::Angle {
            continue;
        }
        let c = l.center();
        let tl = l.size().max(1.0);
        let want = l.value.to_radians();
        let mut best: Option<(f64, AngleDim)> = None;
        for (i, gi) in geos.iter().enumerate() {
            let Geo::Line(p1, q1) = *gi else { continue };
            if !part[i] || claimed[i] {
                continue;
            }
            for (j, gj) in geos.iter().enumerate().skip(i + 1) {
                let Geo::Line(p2, q2) = *gj else { continue };
                if !part[j] || claimed[j] || dir_of(p1, q1).cross(dir_of(p2, q2)).abs() < 0.1 {
                    continue;
                }
                // Shared corner and the far ends.
                let Some((v, a, b)) = [(p1, q1, p2, q2), (p1, q1, q2, p2), (q1, p1, p2, q2), (q1, p1, q2, p2)]
                    .into_iter()
                    .filter(|(s1, _, s2, _)| s1.dist(*s2) < 2.0 * tol)
                    .map(|(s1, f1, s2, f2)| (s1.mid(s2), f1, f2))
                    .next()
                else {
                    continue;
                };
                let dist = c.dist(v);
                if dist > 10.0 * tl + 4.0 * tol {
                    continue;
                }
                let inside = |a: V2, b: V2| angle_on_arc((c - v).angle(), (a - v).angle(), (b - v).angle());
                for (a, b) in [(a, b), (b, a)] {
                    let sweep = norm_angle((b - v).angle() - (a - v).angle());
                    let off = (sweep - want).abs();
                    if off > 30f64.to_radians() {
                        continue;
                    }
                    let score = dist / tl + off.to_degrees() / 10.0 + if inside(a, b) { 0.0 } else { 1.0 };
                    if best.as_ref().is_none_or(|x| score < x.0) {
                        best = Some((score, AngleDim { label: li, v, a, b }));
                    }
                }
            }
        }
        if let Some((_, ad)) = best {
            out.matched[li] = true;
            // The sketched dimension arc around the corner is annotation.
            for (k, g) in geos.iter().enumerate() {
                if let Geo::Arc { c: ac, r, .. } = *g {
                    if !claimed[k] && ac.dist(ad.v) < 3.0 * tol && (c.dist(ad.v) - r).abs() < 2.0 * tl + tol {
                        claimed[k] = true;
                        out.claimed.push(k);
                    }
                }
            }
            out.angles.push(ad);
        }
    }
    out
}

/// True if `p` touches no other stroke (other than `skip`) within `tol`.
fn dangling(geos: &[Geo], p: V2, skip: &[usize], tol: f64) -> bool {
    !geos.iter().enumerate().any(|(i, g)| !skip.contains(&i) && g.distance(p) < tol)
}

/// Monotone piecewise-linear map from sketch pixels to millimetres on one axis.
#[derive(Clone, Debug)]
pub struct Mapper {
    pub cs: Vec<f64>,
    pub xs: Vec<f64>,
    pub s: f64,
}

impl Mapper {
    pub fn map(&self, v: f64) -> f64 {
        let n = self.cs.len();
        if n == 0 {
            return v * self.s;
        }
        if v <= self.cs[0] {
            return self.xs[0] + (v - self.cs[0]) * self.s;
        }
        if v >= self.cs[n - 1] {
            return self.xs[n - 1] + (v - self.cs[n - 1]) * self.s;
        }
        let i = self.cs.partition_point(|c| *c <= v).max(1) - 1;
        let (c0, c1) = (self.cs[i], self.cs[i + 1]);
        let t = (v - c0) / (c1 - c0).max(1e-12);
        self.xs[i] + (self.xs[i + 1] - self.xs[i]) * t
    }
}

/// A 1-D distance constraint between two pixel coordinates.
#[derive(Clone, Debug)]
pub struct AxisCon {
    pub a: f64,
    pub b: f64,
    pub value: f64,
    pub label: usize,
}

pub struct AxisResult {
    pub mapper: Mapper,
    /// (label, achieved − requested) for constraints that could not be met.
    pub conflicts: Vec<(usize, f64)>,
    pub dropped: Vec<usize>,
    pub applied: Vec<usize>,
}

#[allow(clippy::needless_range_loop)]
pub(crate) fn solve_dense(mut a: Vec<Vec<f64>>, mut b: Vec<f64>) -> Option<Vec<f64>> {
    let n = b.len();
    for col in 0..n {
        let piv = (col..n).max_by(|i, j| a[*i][col].abs().total_cmp(&a[*j][col].abs()))?;
        if a[piv][col].abs() < 1e-14 {
            return None;
        }
        a.swap(col, piv);
        b.swap(col, piv);
        for r in col + 1..n {
            let f = a[r][col] / a[col][col];
            if f == 0.0 {
                continue;
            }
            for c in col..n {
                a[r][c] -= f * a[col][c];
            }
            b[r] -= f * b[col];
        }
    }
    let mut x = vec![0.0; n];
    for r in (0..n).rev() {
        let s: f64 = (r + 1..n).map(|c| a[r][c] * x[c]).sum();
        x[r] = (b[r] - s) / a[r][r];
    }
    Some(x)
}

/// Solve one axis. `coords` are all geometry coordinates on this axis (px).
pub fn solve_axis(coords: &[f64], cons: &[AxisCon], s: f64, tol: f64) -> AxisResult {
    let mut cs: Vec<f64> = coords.to_vec();
    cs.sort_by(f64::total_cmp);
    let mut stations: Vec<f64> = vec![];
    for v in cs {
        if stations.last().is_none_or(|l| v - l > 0.75) {
            stations.push(v);
        }
    }
    // Constraint ends not on any station become stations themselves.
    for c in cons {
        for v in [c.a, c.b] {
            if !stations.iter().any(|st| (st - v).abs() <= 2.0 * tol) {
                stations.push(v);
            }
        }
    }
    stations.sort_by(f64::total_cmp);
    let k = stations.len();
    let nearest = |v: f64| -> usize { (0..k).min_by(|i, j| (stations[*i] - v).abs().total_cmp(&(stations[*j] - v).abs())).unwrap_or(0) };
    let mut active: Vec<(usize, usize, f64, usize)> = cons
        .iter()
        .filter_map(|c| {
            let (ia, ib) = (nearest(c.a), nearest(c.b));
            let (ia, ib) = (ia.min(ib), ia.max(ib));
            (ia != ib).then_some((ia, ib, c.value, c.label))
        })
        .collect();
    let mut dropped = vec![];
    if k == 0 {
        return AxisResult { mapper: Mapper { cs: vec![], xs: vec![], s }, conflicts: vec![], dropped, applied: vec![] };
    }
    loop {
        let mut m = vec![vec![0.0; k]; k];
        let mut rhs = vec![0.0; k];
        let mut add = |coef: &[(usize, f64)], target: f64, w: f64| {
            for &(i, ci) in coef {
                for &(j, cj) in coef {
                    m[i][j] += w * ci * cj;
                }
                rhs[i] += w * ci * target;
            }
        };
        add(&[(0, 1.0)], stations[0] * s, 1e-6);
        for i in 0..k - 1 {
            let g = stations[i + 1] - stations[i];
            add(&[(i + 1, 1.0), (i, -1.0)], s * g, 1.0 / g.max(0.5));
        }
        for &(ia, ib, v, _) in &active {
            let g = stations[ib] - stations[ia];
            add(&[(ib, 1.0), (ia, -1.0)], v, 1e10 / g.max(0.5));
        }
        let xs = solve_dense(m, rhs).unwrap_or_else(|| stations.iter().map(|c| c * s).collect());
        let bad = (0..k.saturating_sub(1)).find(|i| xs[i + 1] - xs[*i] < 0.02 * s * (stations[i + 1] - stations[*i]));
        if let Some(i) = bad {
            let worst = active
                .iter()
                .enumerate()
                .filter(|(_, c)| c.0 <= i && i < c.1)
                .max_by(|x, y| {
                    let dev = |c: &(usize, usize, f64, usize)| (c.2 - s * (stations[c.1] - stations[c.0])).abs() / c.2;
                    dev(x.1).total_cmp(&dev(y.1))
                })
                .map(|(idx, _)| idx);
            if let Some(w) = worst {
                dropped.push(active.remove(w).3);
                continue;
            }
        }
        let conflicts = active
            .iter()
            .filter_map(|&(ia, ib, v, l)| {
                let r = xs[ib] - xs[ia] - v;
                (r.abs() > (0.002 * v).max(0.01)).then_some((l, r))
            })
            .collect();
        let applied = active.iter().map(|c| c.3).collect();
        return AxisResult { mapper: Mapper { cs: stations, xs, s }, conflicts, dropped, applied };
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dimensions_override_sketch_proportions() {
        // Sketch stations at 0, 100, 150 px; the user wrote 40 (0→100) and 60 (100→150).
        let coords = [0.0, 100.0, 150.0];
        let cons = [AxisCon { a: 0.0, b: 100.0, value: 40.0, label: 0 }, AxisCon { a: 100.0, b: 150.0, value: 60.0, label: 1 }];
        let r = solve_axis(&coords, &cons, 0.5, 3.0);
        assert!(r.conflicts.is_empty() && r.dropped.is_empty());
        let m = &r.mapper;
        assert!(((m.map(100.0) - m.map(0.0)) - 40.0).abs() < 1e-3);
        assert!(((m.map(150.0) - m.map(100.0)) - 60.0).abs() < 1e-3);
        // Midpoint of a dimensioned gap maps proportionally.
        assert!(((m.map(50.0) - m.map(0.0)) - 20.0).abs() < 1e-3);
    }

    #[test]
    fn contradictory_dimension_is_dropped_not_folded() {
        let coords = [0.0, 100.0, 200.0];
        let cons = [AxisCon { a: 0.0, b: 200.0, value: 100.0, label: 0 }, AxisCon { a: 0.0, b: 100.0, value: 150.0, label: 1 }];
        let r = solve_axis(&coords, &cons, 0.5, 3.0);
        let m = &r.mapper;
        assert!(m.map(200.0) > m.map(100.0) && m.map(100.0) > m.map(0.0));
        assert!(!r.dropped.is_empty() || !r.conflicts.is_empty());
    }
}
