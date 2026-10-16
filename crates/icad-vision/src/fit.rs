// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Proteen Das

//! Fitting pixel chains to CAD primitives: lines (Douglas–Peucker), circles and
//! arcs (algebraic least-squares circle fit).

use icad_core::geom::*;
use std::f64::consts::{PI, TAU};

/// Geometry in (y-up) pixel or millimetre space.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Geo {
    Line(V2, V2),
    Circle(V2, f64),
    /// Counter-clockwise arc.
    Arc {
        c: V2,
        r: f64,
        start: f64,
        end: f64,
    },
}

impl Geo {
    pub fn length(&self) -> f64 {
        match *self {
            Geo::Line(a, b) => a.dist(b),
            Geo::Circle(_, r) => TAU * r,
            Geo::Arc { r, start, end, .. } => r * arc_sweep(start, end),
        }
    }
    pub fn endpoints(&self) -> Option<(V2, V2)> {
        match *self {
            Geo::Line(a, b) => Some((a, b)),
            Geo::Arc { c, r, start, end } => Some((c + V2::polar(r, start), c + V2::polar(r, end))),
            Geo::Circle(..) => None,
        }
    }
    pub fn arc_mid(&self) -> Option<V2> {
        match *self {
            Geo::Arc { c, r, start, end } => Some(c + V2::polar(r, start + arc_sweep(start, end) / 2.0)),
            _ => None,
        }
    }
    pub fn distance(&self, p: V2) -> f64 {
        match *self {
            Geo::Line(a, b) => dist_point_segment(p, a, b),
            Geo::Circle(c, r) => (p.dist(c) - r).abs(),
            Geo::Arc { c, r, start, end } => {
                if angle_on_arc((p - c).angle(), start, end) {
                    (p.dist(c) - r).abs()
                } else {
                    let (a, b) = self.endpoints().unwrap();
                    p.dist(a).min(p.dist(b))
                }
            }
        }
    }
    pub fn bbox(&self) -> BBox {
        match *self {
            Geo::Line(a, b) => BBox::from_pts(a, b),
            Geo::Circle(c, r) => BBox::from_pts(c - V2::new(r, r), c + V2::new(r, r)),
            Geo::Arc { .. } => {
                let (a, b) = self.endpoints().unwrap();
                BBox::of([a, b, self.arc_mid().unwrap()])
            }
        }
    }
}

/// Douglas–Peucker simplification; returns indices of kept points.
pub fn rdp(pts: &[V2], eps: f64) -> Vec<usize> {
    if pts.len() < 3 {
        return (0..pts.len()).collect();
    }
    let mut keep = vec![false; pts.len()];
    keep[0] = true;
    keep[pts.len() - 1] = true;
    let mut stack = vec![(0usize, pts.len() - 1)];
    while let Some((s, e)) = stack.pop() {
        let (mut best, mut bi) = (0.0, 0);
        for i in s + 1..e {
            let d = dist_point_segment(pts[i], pts[s], pts[e]);
            if d > best {
                best = d;
                bi = i;
            }
        }
        if best > eps {
            keep[bi] = true;
            stack.push((s, bi));
            stack.push((bi, e));
        }
    }
    (0..pts.len()).filter(|i| keep[*i]).collect()
}

/// Kåsa algebraic circle fit. Returns (centre, radius, rms residual).
pub fn fit_circle(pts: &[V2]) -> Option<(V2, f64, f64)> {
    if pts.len() < 3 {
        return None;
    }
    let n = pts.len() as f64;
    let m = pts.iter().fold(V2::ZERO, |a, p| a + *p) / n;
    let (mut suu, mut svv, mut suv, mut suuu, mut svvv, mut suvv, mut svuu) = (0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0);
    for p in pts {
        let u = p.x - m.x;
        let v = p.y - m.y;
        suu += u * u;
        svv += v * v;
        suv += u * v;
        suuu += u * u * u;
        svvv += v * v * v;
        suvv += u * v * v;
        svuu += v * u * u;
    }
    let det = suu * svv - suv * suv;
    if det.abs() < 1e-9 {
        return None;
    }
    let b1 = 0.5 * (suuu + suvv);
    let b2 = 0.5 * (svvv + svuu);
    let uc = (b1 * svv - b2 * suv) / det;
    let vc = (suu * b2 - suv * b1) / det;
    let c = V2::new(uc + m.x, vc + m.y);
    let r = (uc * uc + vc * vc + (suu + svv) / n).sqrt();
    let rms = (pts.iter().map(|p| (p.dist(c) - r).powi(2)).sum::<f64>() / n).sqrt();
    (r.is_finite() && c.is_finite()).then_some((c, r, rms))
}

/// Signed angular travel of a chain around a centre.
pub fn angular_travel(pts: &[V2], c: V2) -> f64 {
    let mut total = 0.0;
    for w in pts.windows(2) {
        let mut d = (w[1] - c).angle() - (w[0] - c).angle();
        while d > PI {
            d -= TAU;
        }
        while d < -PI {
            d += TAU;
        }
        total += d;
    }
    total
}

pub struct FitParams {
    /// Douglas–Peucker tolerance (px).
    pub eps: f64,
    /// Typical stroke width (px).
    pub stroke: f64,
    /// Image diagonal (px).
    pub diag: f64,
}

fn arc_ok(sub: &[V2], p: &FitParams) -> Option<Geo> {
    let (c, r, rms) = fit_circle(sub)?;
    if r > 0.6 * p.diag || r < 2.0 * p.stroke {
        return None;
    }
    if rms > (0.045 * r).max(0.9 * p.stroke).max(1.2) {
        return None;
    }
    let travel = angular_travel(sub, c);
    if travel.abs() < 25f64.to_radians() {
        return None;
    }
    // Must be visibly curved (sagitta beyond the line tolerance).
    let a = sub[0];
    let b = sub[sub.len() - 1];
    let sag = sub.iter().map(|q| dist_point_segment(*q, a, b)).fold(0.0, f64::max);
    if sag < p.eps * 0.9 {
        return None;
    }
    // No sharp corner inside (an obtuse polyline corner also fits a big circle).
    let w = ((3.0 * p.stroke).ceil() as usize).max(3);
    if sub.len() > 2 * w {
        for k in w..sub.len() - w {
            let (din, dout) = (sub[k] - sub[k - w], sub[k + w] - sub[k]);
            let turn = din.cross(dout).atan2(din.dot(dout)).abs();
            let expect = (din.len() + dout.len()) / (2.0 * r);
            if turn > expect + 20f64.to_radians() {
                return None;
            }
        }
    }
    // Evenly curved: a straight run joined to a tangent fillet also fits a
    // big circle, but turns unevenly along its length.
    if sub.len() >= 12 && travel.abs() < 300f64.to_radians() {
        let n = sub.len() - 1;
        let chord = |i: usize, j: usize| (sub[j] - sub[i]).angle();
        let (t1, t2, t3) = (chord(0, n / 3), chord(n / 3, 2 * n / 3), chord(2 * n / 3, n));
        let (a, b) = (norm_angle(t2 - t1 + PI) - PI, norm_angle(t3 - t2 + PI) - PI);
        if (a - b).abs() > 0.4 * a.abs().max(b.abs()) + 6f64.to_radians() {
            return None;
        }
    }
    if travel.abs() > 300f64.to_radians() {
        return Some(Geo::Circle(c, r));
    }
    let (s, e) = if travel > 0.0 { ((a - c).angle(), (b - c).angle()) } else { ((b - c).angle(), (a - c).angle()) };
    Some(Geo::Arc { c, r, start: norm_angle(s), end: norm_angle(e) })
}

/// Convert one traced chain into primitives.
pub fn fit_path(pts: &[V2], closed: bool, p: &FitParams) -> Vec<Geo> {
    if pts.len() < 2 {
        return vec![];
    }
    let closed = closed || (pts.len() > 8 && pts[0].dist(pts[pts.len() - 1]) < 2.5 * p.stroke.max(1.5));
    if closed && pts.len() >= 8 {
        if let Some((c, r, rms)) = fit_circle(pts) {
            let bb = BBox::of(pts.iter().copied());
            let aspect = bb.width() / bb.height().max(1e-9);
            if r > 2.0 * p.stroke && rms < (0.06 * r).max(1.2) && (0.75..1.33).contains(&aspect) {
                return vec![Geo::Circle(c, r)];
            }
        }
        // Closed polygon: start the chain at the point farthest from its start so
        // Douglas–Peucker keeps the real corners.
        let far = (0..pts.len()).max_by(|a, b| pts[*a].dist(pts[0]).total_cmp(&pts[*b].dist(pts[0]))).unwrap_or(0);
        let mut rot: Vec<V2> = pts[far..].iter().chain(pts[..far].iter()).copied().collect();
        rot.push(rot[0]);
        return fit_open(&rot, p);
    }
    fit_open(pts, p)
}

fn fit_open(pts: &[V2], p: &FitParams) -> Vec<Geo> {
    let idx = rdp(pts, p.eps);
    let mut out = Vec::new();
    if idx.len() <= 2 {
        if pts[0].dist(pts[pts.len() - 1]) > 1e-9 {
            out.push(Geo::Line(pts[0], pts[pts.len() - 1]));
        }
        return out;
    }
    let k = idx.len();
    let mut i = 0;
    while i < k - 1 {
        let mut found = None;
        let mut j = k - 1;
        while j >= i + 2 {
            let sub = &pts[idx[i]..=idx[j]];
            if let Some(g) = arc_ok(sub, p) {
                found = Some((j, g));
                break;
            }
            j -= 1;
        }
        match found {
            Some((j, g)) => {
                out.push(g);
                i = j;
            }
            None => {
                out.push(Geo::Line(pts[idx[i]], pts[idx[i + 1]]));
                i += 1;
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn circle_fit_exact() {
        let pts: Vec<V2> = (0..40).map(|i| V2::new(5.0, -3.0) + V2::polar(12.0, i as f64 * 0.157)).collect();
        let (c, r, rms) = fit_circle(&pts).unwrap();
        assert!((c - V2::new(5.0, -3.0)).len() < 1e-6 && (r - 12.0).abs() < 1e-6 && rms < 1e-6);
    }

    #[test]
    fn l_shape_becomes_two_lines() {
        let mut pts = vec![];
        for i in 0..=100 {
            pts.push(V2::new(i as f64, 0.0));
        }
        for i in 1..=60 {
            pts.push(V2::new(100.0, i as f64));
        }
        let g = fit_path(&pts, false, &FitParams { eps: 3.0, stroke: 3.0, diag: 1000.0 });
        assert_eq!(g.len(), 2);
    }

    #[test]
    fn obtuse_corner_is_not_an_arc() {
        // 600 px along, then a 37° turn: a big circle fits within tolerance.
        let mut pts: Vec<V2> = (0..=600).map(|i| V2::new(i as f64, 0.0)).collect();
        pts.extend((1..=250).map(|i| V2::new(600.0 + 0.8 * i as f64, 0.6 * i as f64)));
        let g = fit_path(&pts, false, &FitParams { eps: 6.0, stroke: 4.0, diag: 1400.0 });
        assert!(g.iter().all(|g| matches!(g, Geo::Line(..))), "{g:?}");
    }
}
