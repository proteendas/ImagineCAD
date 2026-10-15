// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Proteen Das

//! Ellipse and spline evaluation.
//!
//! Splines are interpolating (they pass through their fit points): uniform
//! Catmull–Rom segments, which convert exactly to cubic Bézier segments and
//! hence to a clamped cubic B-spline for DXF export.

use crate::geom::*;
use std::f64::consts::TAU;

/// Point on an ellipse at parameter `t`.
pub fn ellipse_point(c: V2, major: V2, ratio: f64, t: f64) -> V2 {
    c + major * t.cos() + major.perp() * (ratio * t.sin())
}

/// Parameter of the ellipse point closest (angularly) to `p`.
pub fn ellipse_param(c: V2, major: V2, ratio: f64, p: V2) -> f64 {
    let a = major.len().max(1e-12);
    let u = major / a;
    let d = p - c;
    let x = d.dot(u) / a;
    let y = d.dot(u.perp()) / (a * ratio.max(1e-12));
    norm_angle(y.atan2(x))
}

pub fn ellipse_points(c: V2, major: V2, ratio: f64, start: f64, end: f64, tol: f64) -> Vec<V2> {
    let sweep = if (end - start).abs() < 1e-12 || ((end - start).abs() - TAU).abs() < 1e-12 { TAU } else { arc_sweep(start, end) };
    let n = segments_for(major.len().max(1e-9), sweep, tol).max(16);
    (0..=n).map(|i| ellipse_point(c, major, ratio, start + sweep * i as f64 / n as f64)).collect()
}

pub fn is_full_ellipse(start: f64, end: f64) -> bool {
    let s = norm_angle(end - start);
    s < 1e-9 || (TAU - s) < 1e-9
}

/// Cubic Bézier segments of the interpolating spline.
pub fn spline_beziers(pts: &[V2], closed: bool) -> Vec<[V2; 4]> {
    let n = pts.len();
    if n < 2 {
        return vec![];
    }
    let get = |i: isize| -> V2 {
        if closed {
            pts[i.rem_euclid(n as isize) as usize]
        } else if i < 0 {
            pts[0] * 2.0 - pts[1]
        } else if i as usize >= n {
            pts[n - 1] * 2.0 - pts[n - 2]
        } else {
            pts[i as usize]
        }
    };
    let segs = if closed { n } else { n - 1 };
    (0..segs as isize)
        .map(|i| {
            let (p0, p1, p2, p3) = (get(i - 1), get(i), get(i + 1), get(i + 2));
            [p1, p1 + (p2 - p0) / 6.0, p2 - (p3 - p1) / 6.0, p2]
        })
        .collect()
}

pub fn bezier_point(b: &[V2; 4], t: f64) -> V2 {
    let u = 1.0 - t;
    b[0] * (u * u * u) + b[1] * (3.0 * u * u * t) + b[2] * (3.0 * u * t * t) + b[3] * (t * t * t)
}

pub fn spline_points(pts: &[V2], closed: bool, tol: f64) -> Vec<V2> {
    let mut out = vec![];
    for b in spline_beziers(pts, closed) {
        let len = b[0].dist(b[1]) + b[1].dist(b[2]) + b[2].dist(b[3]);
        let n = ((len / tol.max(1e-9)).sqrt().ceil() as usize).clamp(4, 64);
        let start = if out.is_empty() { 0 } else { 1 };
        for i in start..=n {
            out.push(bezier_point(&b, i as f64 / n as f64));
        }
    }
    out
}

/// Clamped cubic B-spline (control points, knots) equivalent to the spline.
pub fn spline_bspline(pts: &[V2], closed: bool) -> (Vec<V2>, Vec<f64>) {
    let bz = spline_beziers(pts, closed);
    let mut ctrl = vec![];
    let mut knots = vec![0.0; 4];
    for (i, b) in bz.iter().enumerate() {
        if i == 0 {
            ctrl.push(b[0]);
        }
        ctrl.extend_from_slice(&b[1..]);
        let k = (i + 1) as f64;
        if i + 1 < bz.len() {
            knots.extend([k, k, k]);
        } else {
            knots.extend([k, k, k, k]);
        }
    }
    (ctrl, knots)
}

/// Evaluate a (non-rational) B-spline with de Boor's algorithm; used to import
/// DXF SPLINE entities as fit points.
pub fn eval_bspline(ctrl: &[V2], knots: &[f64], degree: usize, t: f64) -> V2 {
    let n = ctrl.len();
    if n == 0 {
        return V2::ZERO;
    }
    let p = degree.min(n - 1);
    let mut k = p;
    while k + 1 < n && knots.get(k + 1).is_some_and(|kn| *kn <= t) {
        k += 1;
    }
    let mut d: Vec<V2> = (0..=p).map(|j| ctrl[(j + k).saturating_sub(p).min(n - 1)]).collect();
    for r in 1..=p {
        for j in (r..=p).rev() {
            let i = j + k - p;
            let (a, b) = (knots.get(i).copied().unwrap_or(0.0), knots.get(i + p + 1 - r).copied().unwrap_or(1.0));
            let alpha = if (b - a).abs() < 1e-12 { 0.0 } else { (t - a) / (b - a) };
            d[j] = d[j - 1] * (1.0 - alpha) + d[j] * alpha;
        }
    }
    d[p]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn spline_passes_through_points_and_bspline_matches() {
        let pts = vec![V2::new(0.0, 0.0), V2::new(10.0, 5.0), V2::new(20.0, 0.0), V2::new(30.0, 8.0)];
        let poly = spline_points(&pts, false, 0.01);
        for p in &pts {
            assert!(poly.iter().any(|q| q.dist(*p) < 1e-9));
        }
        let (ctrl, knots) = spline_bspline(&pts, false);
        assert_eq!(knots.len(), ctrl.len() + 4);
        let mid = eval_bspline(&ctrl, &knots, 3, 1.0);
        assert!(mid.dist(pts[1]) < 1e-9, "{mid:?}");
        let half = eval_bspline(&ctrl, &knots, 3, 1.5);
        let bz = spline_beziers(&pts, false);
        assert!(half.dist(bezier_point(&bz[1], 0.5)) < 1e-9);
    }

    #[test]
    fn ellipse_param_roundtrip() {
        let (c, m, r) = (V2::new(1.0, 2.0), V2::new(3.0, 4.0), 0.5);
        for t in [0.1, 1.0, 2.5, 4.0] {
            let p = ellipse_point(c, m, r, t);
            assert!((ellipse_param(c, m, r, p) - t).abs() < 1e-9);
        }
    }
}
