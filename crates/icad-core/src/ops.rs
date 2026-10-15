// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Proteen Das

//! Modify operations: TRIM, EXTEND, OFFSET, FILLET, CHAMFER.

use crate::entity::*;
use crate::geom::*;
use crate::prim::poly_segments;

const T_EPS: f64 = 1e-7;

/// Trim the portion of `shape` nearest to `pick` that is bounded by
/// intersections with `cutters` ("quick" trim mode: every other object is a cutting edge).
/// Returns the replacement shapes (empty = the whole entity is removed),
/// or `None` if the shape type cannot be trimmed.
pub fn trim(shape: &Shape, pick: V2, cutters: &[Curve]) -> Option<Vec<Shape>> {
    match shape {
        Shape::Line { a, b } => {
            let me = Curve::Seg(*a, *b);
            let mut ts: Vec<f64> = cutters
                .iter()
                .flat_map(|c| me.intersect(c))
                .map(|p| project_t(p, *a, *b))
                .filter(|t| *t > T_EPS && *t < 1.0 - T_EPS)
                .collect();
            ts.sort_by(f64::total_cmp);
            let tp = project_t(pick, *a, *b).clamp(0.0, 1.0);
            let lo = ts.iter().copied().filter(|t| *t < tp).fold(0.0, f64::max);
            let hi = ts.iter().copied().filter(|t| *t > tp).fold(1.0, f64::min);
            let mut out = vec![];
            if lo > 0.0 {
                out.push(Shape::Line { a: *a, b: a.lerp(*b, lo) });
            }
            if hi < 1.0 {
                out.push(Shape::Line { a: a.lerp(*b, hi), b: *b });
            }
            Some(out)
        }
        Shape::Circle { c, r } => {
            let me = Curve::Circle(*c, *r);
            let mut angs: Vec<f64> = cutters.iter().flat_map(|k| me.intersect(k)).map(|p| norm_angle((p - *c).angle())).collect();
            angs.sort_by(f64::total_cmp);
            angs.dedup_by(|x, y| (*x - *y).abs() < 1e-7);
            if angs.len() < 2 {
                return Some(vec![]);
            }
            let ap = norm_angle((pick - *c).angle());
            // Find the gap (lo → hi, CCW) containing the pick and keep the rest.
            let n = angs.len();
            for i in 0..n {
                let lo = angs[i];
                let hi = angs[(i + 1) % n];
                if angle_on_arc(ap, lo, hi) {
                    return Some(vec![Shape::Arc { c: *c, r: *r, start: hi, end: lo }]);
                }
            }
            Some(vec![])
        }
        Shape::Arc { c, r, start, end } => {
            let me = Curve::Arc(*c, *r, *start, *end);
            let sweep = arc_sweep(*start, *end);
            let mut us: Vec<f64> = cutters
                .iter()
                .flat_map(|k| me.intersect(k))
                .map(|p| norm_angle((p - *c).angle() - start))
                .filter(|u| *u > T_EPS && *u < sweep - T_EPS)
                .collect();
            us.sort_by(f64::total_cmp);
            let up = norm_angle((pick - *c).angle() - start).min(sweep);
            let lo = us.iter().copied().filter(|u| *u < up).fold(0.0, f64::max);
            let hi = us.iter().copied().filter(|u| *u > up).fold(sweep, f64::min);
            let mut out = vec![];
            if lo > 0.0 {
                out.push(Shape::Arc { c: *c, r: *r, start: *start, end: norm_angle(start + lo) });
            }
            if hi < sweep {
                out.push(Shape::Arc { c: *c, r: *r, start: norm_angle(start + hi), end: *end });
            }
            Some(out)
        }
        Shape::Polyline { pts, closed } => {
            // Explode, then trim the picked segment against everything else.
            let segs: Vec<(V2, V2)> = poly_segments(pts, *closed).collect();
            let (idx, _) =
                segs.iter().enumerate().map(|(i, (a, b))| (i, dist_point_segment(pick, *a, *b))).min_by(|x, y| x.1.total_cmp(&y.1))?;
            let mut all_cutters = cutters.to_vec();
            all_cutters.extend(segs.iter().enumerate().filter(|(i, _)| *i != idx).map(|(_, (a, b))| Curve::Seg(*a, *b)));
            let mut out: Vec<Shape> =
                segs.iter().enumerate().filter(|(i, _)| *i != idx).map(|(_, (a, b))| Shape::Line { a: *a, b: *b }).collect();
            let (a, b) = segs[idx];
            out.extend(trim(&Shape::Line { a, b }, pick, &all_cutters)?);
            Some(out)
        }
        _ => None,
    }
}

/// Extend the end of a line nearest to `pick` to the closest boundary.
pub fn extend(shape: &Shape, pick: V2, bounds: &[Curve]) -> Option<Shape> {
    let Shape::Line { a, b } = shape else { return None };
    let from_b = pick.dist(*b) <= pick.dist(*a);
    let mut best: Option<f64> = None;
    for c in bounds {
        let cands: Vec<f64> = match *c {
            Curve::Seg(p, q) => {
                line_line(*a, *b, p, q).filter(|(_, _, u)| (-1e-9..=1.0 + 1e-9).contains(u)).map(|(_, t, _)| vec![t]).unwrap_or_default()
            }
            Curve::Circle(cc, r) => line_circle(*a, *b, cc, r).into_iter().map(|(_, t)| t).collect(),
            Curve::Arc(cc, r, s, e) => {
                line_circle(*a, *b, cc, r).into_iter().filter(|(p, _)| angle_on_arc((*p - cc).angle(), s, e)).map(|(_, t)| t).collect()
            }
        };
        for t in cands {
            let ok = if from_b { t > 1.0 + T_EPS } else { t < -T_EPS };
            if ok {
                let d = if from_b { t - 1.0 } else { -t };
                if best.is_none_or(|bt| d < if from_b { bt - 1.0 } else { -bt }) {
                    best = Some(t);
                }
            }
        }
    }
    let t = best?;
    let p = a.lerp(*b, t);
    Some(if from_b { Shape::Line { a: *a, b: p } } else { Shape::Line { a: p, b: *b } })
}

/// Offset a shape by `dist` towards the side of `side_pt`.
pub fn offset(shape: &Shape, dist: f64, side_pt: V2) -> Option<Shape> {
    match shape {
        Shape::Line { a, b } => {
            let n = (*b - *a).norm().perp();
            let s = if (side_pt - *a).dot(n) >= 0.0 { 1.0 } else { -1.0 };
            Some(Shape::Line { a: *a + n * (dist * s), b: *b + n * (dist * s) })
        }
        Shape::Circle { c, r } => {
            let nr = if side_pt.dist(*c) < *r { r - dist } else { r + dist };
            (nr > 1e-9).then_some(Shape::Circle { c: *c, r: nr })
        }
        Shape::Arc { c, r, start, end } => {
            let nr = if side_pt.dist(*c) < *r { r - dist } else { r + dist };
            (nr > 1e-9).then_some(Shape::Arc { c: *c, r: nr, start: *start, end: *end })
        }
        Shape::Polyline { pts, closed } => {
            let segs: Vec<(V2, V2)> = poly_segments(pts, *closed).collect();
            if segs.is_empty() {
                return None;
            }
            let (_, (sa, sb)) = segs
                .iter()
                .enumerate()
                .min_by(|x, y| dist_point_segment(side_pt, x.1 .0, x.1 .1).total_cmp(&dist_point_segment(side_pt, y.1 .0, y.1 .1)))?;
            let sign = if (*sb - *sa).cross(side_pt - *sa) >= 0.0 { 1.0 } else { -1.0 };
            let off: Vec<(V2, V2)> = segs
                .iter()
                .map(|(a, b)| {
                    let n = (*b - *a).norm().perp() * (dist * sign);
                    (*a + n, *b + n)
                })
                .collect();
            let n = off.len();
            let mut out = Vec::with_capacity(pts.len());
            for i in 0..pts.len() {
                let prev = if i == 0 {
                    if *closed {
                        Some(off[n - 1])
                    } else {
                        None
                    }
                } else {
                    Some(off[i - 1])
                };
                let next = off.get(i).copied();
                let p = match (prev, next) {
                    (Some(p), Some(q)) => line_line(p.0, p.1, q.0, q.1).map(|x| x.0).unwrap_or(q.0),
                    (Some(p), None) => p.1,
                    (None, Some(q)) => q.0,
                    (None, None) => pts[i],
                };
                out.push(p);
            }
            Some(Shape::Polyline { pts: out, closed: *closed })
        }
        _ => None,
    }
}

pub struct CornerResult {
    pub first: Shape,
    pub second: Shape,
    pub joint: Option<Shape>,
}

/// FILLET (radius > 0, or sharp corner with 0) / CHAMFER between two lines.
pub fn corner(l1: (V2, V2), pick1: V2, l2: (V2, V2), pick2: V2, radius: f64, chamfer: bool) -> Result<CornerResult, String> {
    let (p, _, _) = line_line(l1.0, l1.1, l2.0, l2.1).ok_or("Lines are parallel")?;
    let keep = |l: (V2, V2), pick: V2| -> Result<(V2, V2), String> {
        let proj = l.0.lerp(l.1, project_t(pick, l.0, l.1));
        let mut u = (proj - p).norm();
        if u == V2::ZERO {
            u = if l.0.dist(p) > l.1.dist(p) { (l.0 - p).norm() } else { (l.1 - p).norm() };
        }
        let far = if (l.0 - p).dot(u) > (l.1 - p).dot(u) { l.0 } else { l.1 };
        if (far - p).dot(u) <= 0.0 {
            return Err("Nothing to keep on the picked side".into());
        }
        Ok((u, far))
    };
    let (u1, far1) = keep(l1, pick1)?;
    let (u2, far2) = keep(l2, pick2)?;
    if radius <= 1e-12 {
        return Ok(CornerResult { first: Shape::Line { a: p, b: far1 }, second: Shape::Line { a: p, b: far2 }, joint: None });
    }
    if chamfer {
        let t1 = p + u1 * radius;
        let t2 = p + u2 * radius;
        if (far1 - p).dot(u1) < radius || (far2 - p).dot(u2) < radius {
            return Err("Chamfer distance too large".into());
        }
        return Ok(CornerResult {
            first: Shape::Line { a: t1, b: far1 },
            second: Shape::Line { a: t2, b: far2 },
            joint: Some(Shape::Line { a: t1, b: t2 }),
        });
    }
    let theta = u1.dot(u2).clamp(-1.0, 1.0).acos();
    if theta < 1e-6 || (std::f64::consts::PI - theta) < 1e-6 {
        return Err("Lines are collinear".into());
    }
    let t = radius / (theta / 2.0).tan();
    if (far1 - p).dot(u1) < t - 1e-9 || (far2 - p).dot(u2) < t - 1e-9 {
        return Err("Radius is too large".into());
    }
    let t1 = p + u1 * t;
    let t2 = p + u2 * t;
    let c = p + (u1 + u2).norm() * (radius / (theta / 2.0).sin());
    let a1 = norm_angle((t1 - c).angle());
    let a2 = norm_angle((t2 - c).angle());
    let (s, e) = if arc_sweep(a1, a2) <= std::f64::consts::PI { (a1, a2) } else { (a2, a1) };
    Ok(CornerResult {
        first: Shape::Line { a: t1, b: far1 },
        second: Shape::Line { a: t2, b: far2 },
        joint: Some(Shape::Arc { c, r: radius, start: s, end: e }),
    })
}

/// Points along a shape at parameter fractions (for DIVIDE / MEASURE).
pub fn point_at_length(shape: &Shape, dist: f64) -> Option<V2> {
    match shape {
        Shape::Line { a, b } => {
            let l = a.dist(*b);
            (dist <= l + 1e-9).then(|| a.lerp(*b, dist / l.max(1e-12)))
        }
        Shape::Arc { c, r, start, end } => {
            let sw = arc_sweep(*start, *end);
            (dist <= r * sw + 1e-9).then(|| *c + V2::polar(*r, start + dist / r))
        }
        Shape::Circle { c, r } => Some(*c + V2::polar(*r, dist / r)),
        _ => {
            let ds = crate::style::DimStyle::default();
            let ctx = Ctx::new(&ds, 1.0);
            let pts: Vec<V2> = match shape.prims(&ctx).into_iter().next()? {
                crate::prim::Prim::Poly(p, closed) => {
                    let mut p = p;
                    if closed && !p.is_empty() {
                        p.push(p[0]);
                    }
                    p
                }
                _ => return None,
            };
            let mut acc = 0.0;
            for w in pts.windows(2) {
                let l = w[0].dist(w[1]);
                if acc + l >= dist - 1e-9 {
                    return Some(w[0].lerp(w[1], ((dist - acc) / l.max(1e-12)).clamp(0.0, 1.0)));
                }
                acc += l;
            }
            None
        }
    }
}

/// DIVIDE: `n - 1` points splitting the shape into `n` equal lengths.
pub fn divide(shape: &Shape, n: usize) -> Vec<V2> {
    let Some(len) = shape.length() else { return vec![] };
    let closed = matches!(shape, Shape::Circle { .. } | Shape::Polyline { closed: true, .. } | Shape::Spline { closed: true, .. });
    let count = if closed { n } else { n.saturating_sub(1) };
    (1..=count).filter_map(|i| point_at_length(shape, len * i as f64 / n as f64)).collect()
}

/// MEASURE: points every `step` along the shape.
pub fn measure(shape: &Shape, step: f64) -> Vec<V2> {
    let Some(len) = shape.length() else { return vec![] };
    if step <= 0.0 {
        return vec![];
    }
    let n = (len / step).floor() as usize;
    (1..=n.min(10_000)).filter_map(|i| point_at_length(shape, step * i as f64)).filter(|_| true).collect()
}

/// BREAK between two points on a line, arc, circle or polyline.
pub fn break_between(shape: &Shape, p1: V2, p2: V2) -> Option<Vec<Shape>> {
    match shape {
        Shape::Line { a, b } => {
            let (t1, t2) = (project_t(p1, *a, *b).clamp(0.0, 1.0), project_t(p2, *a, *b).clamp(0.0, 1.0));
            let (lo, hi) = (t1.min(t2), t1.max(t2));
            let mut out = vec![];
            if lo > 1e-9 {
                out.push(Shape::Line { a: *a, b: a.lerp(*b, lo) });
            }
            if hi < 1.0 - 1e-9 {
                out.push(Shape::Line { a: a.lerp(*b, hi), b: *b });
            }
            Some(out)
        }
        Shape::Circle { c, r } => {
            // Remove the CCW portion from p1 to p2.
            let a1 = norm_angle((p1 - *c).angle());
            let a2 = norm_angle((p2 - *c).angle());
            if (a1 - a2).abs() < 1e-9 {
                return None;
            }
            Some(vec![Shape::Arc { c: *c, r: *r, start: a2, end: a1 }])
        }
        Shape::Arc { c, r, start, end } => {
            let sw = arc_sweep(*start, *end);
            let u = |p: V2| norm_angle((p - *c).angle() - start).min(sw);
            let (u1, u2) = (u(p1), u(p2));
            let (lo, hi) = (u1.min(u2), u1.max(u2));
            let mut out = vec![];
            if lo > 1e-9 {
                out.push(Shape::Arc { c: *c, r: *r, start: *start, end: norm_angle(start + lo) });
            }
            if hi < sw - 1e-9 {
                out.push(Shape::Arc { c: *c, r: *r, start: norm_angle(start + hi), end: *end });
            }
            Some(out)
        }
        Shape::Polyline { pts, closed } => {
            // Work on the open vertex chain; parameters are (segment index, t).
            let mut chain = pts.clone();
            if *closed && !chain.is_empty() {
                chain.push(chain[0]);
            }
            let locate = |p: V2| -> (usize, f64) {
                (0..chain.len().saturating_sub(1))
                    .map(|i| (i, project_t(p, chain[i], chain[i + 1]).clamp(0.0, 1.0)))
                    .min_by(|x, y| {
                        let dx = p.dist(chain[x.0].lerp(chain[x.0 + 1], x.1));
                        let dy = p.dist(chain[y.0].lerp(chain[y.0 + 1], y.1));
                        dx.total_cmp(&dy)
                    })
                    .unwrap_or((0, 0.0))
            };
            let (mut s1, mut s2) = (locate(p1), locate(p2));
            if (s2.0, s2.1) < (s1.0, s1.1) {
                std::mem::swap(&mut s1, &mut s2);
            }
            let at = |s: (usize, f64)| chain[s.0].lerp(chain[s.0 + 1], s.1);
            let mut first: Vec<V2> = chain[..=s1.0].to_vec();
            first.push(at(s1));
            let mut second = vec![at(s2)];
            second.extend_from_slice(&chain[s2.0 + 1..]);
            if *closed {
                // Closed: the remaining piece runs from p2 around to p1.
                let mut joined = second;
                joined.extend(first.into_iter().skip(1));
                return Some(vec![Shape::Polyline { pts: joined, closed: false }]);
            }
            let mut out = vec![];
            if first.len() >= 2 && first[0].dist(first[first.len() - 1]) > 1e-9 {
                out.push(Shape::Polyline { pts: first, closed: false });
            }
            if second.len() >= 2 && second[0].dist(second[second.len() - 1]) > 1e-9 {
                out.push(Shape::Polyline { pts: second, closed: false });
            }
            Some(out)
        }
        _ => None,
    }
}

/// LENGTHEN by a delta at the end nearest `pick` (negative shortens).
pub fn lengthen(shape: &Shape, pick: V2, delta: f64) -> Option<Shape> {
    match shape {
        Shape::Line { a, b } => {
            let d = (*b - *a).norm();
            if pick.dist(*b) <= pick.dist(*a) {
                let nb = *b + d * delta;
                ((nb - *a).dot(d) > 1e-9).then_some(Shape::Line { a: *a, b: nb })
            } else {
                let na = *a - d * delta;
                ((*b - na).dot(d) > 1e-9).then_some(Shape::Line { a: na, b: *b })
            }
        }
        Shape::Arc { c, r, start, end } => {
            let da = delta / r;
            let ps = *c + V2::polar(*r, *start);
            let pe = *c + V2::polar(*r, *end);
            let sw = arc_sweep(*start, *end);
            if sw + da <= 1e-9 || sw + da >= std::f64::consts::TAU {
                return None;
            }
            Some(if pick.dist(pe) <= pick.dist(ps) {
                Shape::Arc { c: *c, r: *r, start: *start, end: norm_angle(end + da) }
            } else {
                Shape::Arc { c: *c, r: *r, start: norm_angle(start - da), end: *end }
            })
        }
        _ => None,
    }
}

/// JOIN: merge collinear/touching lines and same-circle arcs; chains of
/// connected lines and polylines become one polyline.
pub fn join(shapes: &[Shape], tol: f64) -> Vec<Shape> {
    let mut lines: Vec<(V2, V2)> = vec![];
    let mut arcs: Vec<(V2, f64, f64, f64)> = vec![];
    let mut others = vec![];
    for s in shapes {
        match s {
            Shape::Line { a, b } => lines.push((*a, *b)),
            Shape::Polyline { pts, closed: false } => lines.extend(pts.windows(2).map(|w| (w[0], w[1]))),
            Shape::Arc { c, r, start, end } => arcs.push((*c, *r, *start, *end)),
            o => others.push(o.clone()),
        }
    }
    // Merge arcs that share centre/radius and touch.
    let mut merged = true;
    while merged {
        merged = false;
        'o: for i in 0..arcs.len() {
            for j in 0..arcs.len() {
                if i == j {
                    continue;
                }
                let (a, b) = (arcs[i], arcs[j]);
                if a.0.dist(b.0) < tol && (a.1 - b.1).abs() < tol && (norm_angle(a.3 - b.2) < 1e-6 || norm_angle(b.2 - a.3) < 1e-6) {
                    arcs[i] = (a.0, a.1, a.2, b.3);
                    arcs.remove(j);
                    merged = true;
                    break 'o;
                }
            }
        }
    }
    let mut out: Vec<Shape> =
        arcs.into_iter()
            .map(|(c, r, s, e)| {
                if arc_sweep(s, e) > std::f64::consts::TAU - 1e-6 {
                    Shape::Circle { c, r }
                } else {
                    Shape::Arc { c, r, start: s, end: e }
                }
            })
            .collect();
    // Chain line segments end to end.
    let mut used = vec![false; lines.len()];
    for i in 0..lines.len() {
        if used[i] {
            continue;
        }
        used[i] = true;
        let mut chain = vec![lines[i].0, lines[i].1];
        loop {
            let mut grew = false;
            for j in 0..lines.len() {
                if used[j] {
                    continue;
                }
                let (a, b) = lines[j];
                let (head, tail) = (chain[0], *chain.last().unwrap());
                if tail.dist(a) < tol {
                    chain.push(b);
                } else if tail.dist(b) < tol {
                    chain.push(a);
                } else if head.dist(b) < tol {
                    chain.insert(0, a);
                } else if head.dist(a) < tol {
                    chain.insert(0, b);
                } else {
                    continue;
                }
                used[j] = true;
                grew = true;
            }
            if !grew {
                break;
            }
        }
        // Drop collinear interior vertices.
        let mut simplified: Vec<V2> = vec![chain[0]];
        for k in 1..chain.len() - 1 {
            let (p, q, r) = (*simplified.last().unwrap(), chain[k], chain[k + 1]);
            if (q - p).cross(r - q).abs() > 1e-9 * (q - p).len().max(1.0) * (r - q).len().max(1.0) {
                simplified.push(q);
            }
        }
        simplified.push(*chain.last().unwrap());
        let closed = simplified.len() > 3 && simplified[0].dist(*simplified.last().unwrap()) < tol;
        if closed {
            simplified.pop();
        }
        out.push(if simplified.len() == 2 && !closed {
            Shape::Line { a: simplified[0], b: simplified[1] }
        } else {
            Shape::Polyline { pts: simplified, closed }
        });
    }
    out.extend(others);
    out
}

/// STRETCH: move the defining points inside `window` by `d`.
pub fn stretch(shape: &mut Shape, window: &BBox, d: V2) {
    let mv = |p: &mut V2| {
        if window.contains(*p) {
            *p += d
        }
    };
    match shape {
        Shape::Line { a, b } => {
            mv(a);
            mv(b);
        }
        Shape::Polyline { pts, .. } | Shape::Spline { pts, .. } => pts.iter_mut().for_each(mv),
        Shape::Leader(l) => l.pts.iter_mut().for_each(mv),
        Shape::Arc { c, r, start, end } => {
            let ps = *c + V2::polar(*r, *start);
            let pe = *c + V2::polar(*r, *end);
            let pm = *c + V2::polar(*r, *start + arc_sweep(*start, *end) / 2.0);
            let (ins, ine) = (window.contains(ps), window.contains(pe));
            if ins && ine {
                *c += d;
            } else if ins || ine {
                let (a, b) = (if ins { ps + d } else { ps }, if ine { pe + d } else { pe });
                if let Some((nc, nr, ns, ne)) = crate::geom::arc_3p(a, pm + d * 0.5, b) {
                    let keep = angle_on_arc((pm - nc).angle(), (a - nc).angle(), (b - nc).angle());
                    *c = nc;
                    *r = nr;
                    if keep {
                        *start = norm_angle((a - nc).angle());
                        *end = norm_angle((b - nc).angle());
                    } else {
                        *start = ns;
                        *end = ne;
                    }
                }
            }
        }
        Shape::Dim(dm) => {
            mv(&mut dm.p1);
            mv(&mut dm.p2);
            mv(&mut dm.loc);
            if let Some(c) = dm.center.as_mut() {
                mv(c)
            }
        }
        other => {
            // Whole-object move when its key point is inside.
            let key = other.grips().first().copied();
            if key.is_some_and(|k| window.contains(k)) {
                other.transform(&Xform::translate(d));
            }
        }
    }
}

/// OVERKILL: indices of shapes that duplicate an earlier one.
pub fn duplicates(shapes: &[Shape], tol: f64) -> Vec<usize> {
    let same = |a: &Shape, b: &Shape| -> bool {
        match (a, b) {
            (Shape::Line { a: a1, b: b1 }, Shape::Line { a: a2, b: b2 }) => {
                (a1.dist(*a2) < tol && b1.dist(*b2) < tol) || (a1.dist(*b2) < tol && b1.dist(*a2) < tol)
            }
            (Shape::Circle { c: c1, r: r1 }, Shape::Circle { c: c2, r: r2 }) => c1.dist(*c2) < tol && (r1 - r2).abs() < tol,
            (Shape::Arc { c: c1, r: r1, start: s1, end: e1 }, Shape::Arc { c: c2, r: r2, start: s2, end: e2 }) => {
                c1.dist(*c2) < tol
                    && (r1 - r2).abs() < tol
                    && norm_angle(s1 - s2).min(norm_angle(s2 - s1)) < 1e-6
                    && norm_angle(e1 - e2).min(norm_angle(e2 - e1)) < 1e-6
            }
            _ => a == b,
        }
    };
    let mut out = vec![];
    for i in 0..shapes.len() {
        if (0..i).any(|j| !out.contains(&j) && same(&shapes[i], &shapes[j])) {
            out.push(i);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn break_join_lengthen_divide() {
        let l = Shape::Line { a: V2::new(0., 0.), b: V2::new(10., 0.) };
        let parts = break_between(&l, V2::new(3., 0.), V2::new(5., 0.)).unwrap();
        assert_eq!(parts.len(), 2);
        let joined =
            join(&[Shape::Line { a: V2::new(0., 0.), b: V2::new(5., 0.) }, Shape::Line { a: V2::new(5., 0.), b: V2::new(10., 0.) }], 1e-6);
        assert_eq!(joined, vec![Shape::Line { a: V2::new(0., 0.), b: V2::new(10., 0.) }]);
        let sq = join(
            &[
                Shape::Line { a: V2::new(0., 0.), b: V2::new(1., 0.) },
                Shape::Line { a: V2::new(1., 0.), b: V2::new(1., 1.) },
                Shape::Line { a: V2::new(1., 1.), b: V2::new(0., 1.) },
                Shape::Line { a: V2::new(0., 1.), b: V2::new(0., 0.) },
            ],
            1e-6,
        );
        assert!(matches!(&sq[0], Shape::Polyline { pts, closed: true } if pts.len() == 4));
        assert_eq!(lengthen(&l, V2::new(9., 0.), 2.0), Some(Shape::Line { a: V2::new(0., 0.), b: V2::new(12., 0.) }));
        assert_eq!(divide(&l, 4).len(), 3);
        assert_eq!(measure(&l, 3.0).len(), 3);
        assert_eq!(duplicates(&[l.clone(), Shape::Line { a: V2::new(10., 0.), b: V2::new(0., 0.) }], 1e-9), vec![1]);
    }

    #[test]
    fn trim_middle_of_line() {
        let l = Shape::Line { a: V2::new(0., 0.), b: V2::new(10., 0.) };
        let cut = [Curve::Seg(V2::new(3., -1.), V2::new(3., 1.)), Curve::Seg(V2::new(7., -1.), V2::new(7., 1.))];
        let r = trim(&l, V2::new(5., 0.1), &cut).unwrap();
        assert_eq!(r.len(), 2);
        let r = trim(&l, V2::new(1., 0.), &cut).unwrap();
        assert_eq!(r, vec![Shape::Line { a: V2::new(3., 0.), b: V2::new(10., 0.) }]);
    }

    #[test]
    fn fillet_square_corner() {
        let r =
            corner((V2::new(0., 0.), V2::new(10., 0.)), V2::new(5., 0.), (V2::new(0., 0.), V2::new(0., 10.)), V2::new(0., 5.), 2.0, false)
                .unwrap();
        match r.joint.unwrap() {
            Shape::Arc { c, r, .. } => {
                assert!((c - V2::new(2., 2.)).len() < 1e-9);
                assert!((r - 2.0).abs() < 1e-12);
            }
            _ => panic!(),
        }
        let Shape::Line { a, b } = r.first else { panic!() };
        assert!((a - V2::new(2., 0.)).len() < 1e-9 && (b - V2::new(10., 0.)).len() < 1e-9);
    }

    #[test]
    fn extend_to_boundary() {
        let l = Shape::Line { a: V2::new(0., 0.), b: V2::new(5., 0.) };
        let r = extend(&l, V2::new(4.5, 0.), &[Curve::Seg(V2::new(9., -5.), V2::new(9., 5.))]).unwrap();
        assert_eq!(r, Shape::Line { a: V2::new(0., 0.), b: V2::new(9., 0.) });
    }

    #[test]
    fn offset_rectangle_outwards() {
        let pts = vec![V2::new(0., 0.), V2::new(10., 0.), V2::new(10., 5.), V2::new(0., 5.)];
        let s = offset(&Shape::Polyline { pts, closed: true }, 1.0, V2::new(5., -3.)).unwrap();
        let Shape::Polyline { pts, .. } = s else { panic!() };
        assert!((pts[0] - V2::new(-1., -1.)).len() < 1e-9);
        assert!((pts[2] - V2::new(11., 6.)).len() < 1e-9);
    }
}
