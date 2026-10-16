// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Proteen Das

//! 2D vector math and analytic geometry helpers.

use serde::{Deserialize, Serialize};
use std::f64::consts::{PI, TAU};
use std::ops::{Add, AddAssign, Div, Mul, Neg, Sub, SubAssign};

pub const EPS: f64 = 1e-9;

#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct V2 {
    pub x: f64,
    pub y: f64,
}

impl V2 {
    pub const ZERO: V2 = V2 { x: 0.0, y: 0.0 };

    pub const fn new(x: f64, y: f64) -> Self {
        Self { x, y }
    }
    pub fn polar(len: f64, ang: f64) -> Self {
        Self::new(len * ang.cos(), len * ang.sin())
    }
    pub fn dot(self, o: V2) -> f64 {
        self.x * o.x + self.y * o.y
    }
    pub fn cross(self, o: V2) -> f64 {
        self.x * o.y - self.y * o.x
    }
    pub fn len2(self) -> f64 {
        self.dot(self)
    }
    pub fn len(self) -> f64 {
        self.len2().sqrt()
    }
    pub fn dist(self, o: V2) -> f64 {
        (self - o).len()
    }
    pub fn norm(self) -> V2 {
        let l = self.len();
        if l < EPS {
            V2::ZERO
        } else {
            self / l
        }
    }
    /// Counter-clockwise perpendicular.
    pub fn perp(self) -> V2 {
        V2::new(-self.y, self.x)
    }
    pub fn angle(self) -> f64 {
        self.y.atan2(self.x)
    }
    pub fn rotate(self, a: f64) -> V2 {
        let (s, c) = a.sin_cos();
        V2::new(self.x * c - self.y * s, self.x * s + self.y * c)
    }
    pub fn rotate_about(self, c: V2, a: f64) -> V2 {
        c + (self - c).rotate(a)
    }
    pub fn lerp(self, o: V2, t: f64) -> V2 {
        self + (o - self) * t
    }
    pub fn mid(self, o: V2) -> V2 {
        self.lerp(o, 0.5)
    }
    pub fn is_finite(self) -> bool {
        self.x.is_finite() && self.y.is_finite()
    }
}

impl Add for V2 {
    type Output = V2;
    fn add(self, o: V2) -> V2 {
        V2::new(self.x + o.x, self.y + o.y)
    }
}
impl AddAssign for V2 {
    fn add_assign(&mut self, o: V2) {
        self.x += o.x;
        self.y += o.y;
    }
}
impl Sub for V2 {
    type Output = V2;
    fn sub(self, o: V2) -> V2 {
        V2::new(self.x - o.x, self.y - o.y)
    }
}
impl SubAssign for V2 {
    fn sub_assign(&mut self, o: V2) {
        self.x -= o.x;
        self.y -= o.y;
    }
}
impl Mul<f64> for V2 {
    type Output = V2;
    fn mul(self, s: f64) -> V2 {
        V2::new(self.x * s, self.y * s)
    }
}
impl Div<f64> for V2 {
    type Output = V2;
    fn div(self, s: f64) -> V2 {
        V2::new(self.x / s, self.y / s)
    }
}
impl Neg for V2 {
    type Output = V2;
    fn neg(self) -> V2 {
        V2::new(-self.x, -self.y)
    }
}

/// Axis aligned bounding box.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct BBox {
    pub min: V2,
    pub max: V2,
}

impl Default for BBox {
    fn default() -> Self {
        Self::empty()
    }
}

impl BBox {
    pub fn empty() -> Self {
        Self { min: V2::new(f64::INFINITY, f64::INFINITY), max: V2::new(f64::NEG_INFINITY, f64::NEG_INFINITY) }
    }
    pub fn from_pts(a: V2, b: V2) -> Self {
        Self { min: V2::new(a.x.min(b.x), a.y.min(b.y)), max: V2::new(a.x.max(b.x), a.y.max(b.y)) }
    }
    pub fn of(pts: impl IntoIterator<Item = V2>) -> Self {
        let mut b = Self::empty();
        for p in pts {
            b.add(p);
        }
        b
    }
    pub fn is_empty(&self) -> bool {
        self.min.x > self.max.x || self.min.y > self.max.y
    }
    pub fn add(&mut self, p: V2) {
        self.min.x = self.min.x.min(p.x);
        self.min.y = self.min.y.min(p.y);
        self.max.x = self.max.x.max(p.x);
        self.max.y = self.max.y.max(p.y);
    }
    pub fn union(&self, o: &BBox) -> BBox {
        if self.is_empty() {
            return *o;
        }
        if o.is_empty() {
            return *self;
        }
        BBox {
            min: V2::new(self.min.x.min(o.min.x), self.min.y.min(o.min.y)),
            max: V2::new(self.max.x.max(o.max.x), self.max.y.max(o.max.y)),
        }
    }
    pub fn width(&self) -> f64 {
        self.max.x - self.min.x
    }
    pub fn height(&self) -> f64 {
        self.max.y - self.min.y
    }
    pub fn center(&self) -> V2 {
        self.min.mid(self.max)
    }
    pub fn expand(&self, d: f64) -> BBox {
        BBox { min: self.min - V2::new(d, d), max: self.max + V2::new(d, d) }
    }
    pub fn contains(&self, p: V2) -> bool {
        p.x >= self.min.x && p.x <= self.max.x && p.y >= self.min.y && p.y <= self.max.y
    }
    pub fn contains_box(&self, o: &BBox) -> bool {
        !o.is_empty() && self.contains(o.min) && self.contains(o.max)
    }
    pub fn intersects(&self, o: &BBox) -> bool {
        !(o.min.x > self.max.x || o.max.x < self.min.x || o.min.y > self.max.y || o.max.y < self.min.y)
    }
    pub fn corners(&self) -> [V2; 4] {
        [self.min, V2::new(self.max.x, self.min.y), self.max, V2::new(self.min.x, self.max.y)]
    }
}

/// Normalise an angle into `[0, 2π)`.
pub fn norm_angle(a: f64) -> f64 {
    let r = a.rem_euclid(TAU);
    if r >= TAU - 1e-12 {
        0.0
    } else {
        r
    }
}

/// Counter-clockwise sweep from `start` to `end`, in `(0, 2π]`.
pub fn arc_sweep(start: f64, end: f64) -> f64 {
    let s = norm_angle(end - start);
    if s < 1e-12 {
        TAU
    } else {
        s
    }
}

/// True if angle `a` lies on the CCW arc from `start` to `end`.
pub fn angle_on_arc(a: f64, start: f64, end: f64) -> bool {
    norm_angle(a - start) <= arc_sweep(start, end) + 1e-9
}

pub fn deg(r: f64) -> f64 {
    r * 180.0 / PI
}
pub fn rad(d: f64) -> f64 {
    d * PI / 180.0
}

/// Parameter of the projection of `p` onto the infinite line `a→b`.
pub fn project_t(p: V2, a: V2, b: V2) -> f64 {
    let d = b - a;
    let l2 = d.len2();
    if l2 < EPS {
        0.0
    } else {
        (p - a).dot(d) / l2
    }
}

pub fn closest_on_segment(p: V2, a: V2, b: V2) -> V2 {
    a.lerp(b, project_t(p, a, b).clamp(0.0, 1.0))
}

pub fn dist_point_segment(p: V2, a: V2, b: V2) -> f64 {
    p.dist(closest_on_segment(p, a, b))
}

/// Intersection of infinite lines; returns point and parameters along each line.
pub fn line_line(a1: V2, a2: V2, b1: V2, b2: V2) -> Option<(V2, f64, f64)> {
    let r = a2 - a1;
    let s = b2 - b1;
    let den = r.cross(s);
    if den.abs() < 1e-12 * (r.len() * s.len()).max(1e-12) {
        return None;
    }
    let t = (b1 - a1).cross(s) / den;
    let u = (b1 - a1).cross(r) / den;
    Some((a1 + r * t, t, u))
}

pub fn seg_seg(a1: V2, a2: V2, b1: V2, b2: V2) -> Option<V2> {
    let (p, t, u) = line_line(a1, a2, b1, b2)?;
    let e = 1e-9;
    if (-e..=1.0 + e).contains(&t) && (-e..=1.0 + e).contains(&u) {
        Some(p)
    } else {
        None
    }
}

/// Intersections of the infinite line `a→b` with a circle, as (point, t).
pub fn line_circle(a: V2, b: V2, c: V2, r: f64) -> Vec<(V2, f64)> {
    let d = b - a;
    let f = a - c;
    let qa = d.len2();
    if qa < EPS {
        return vec![];
    }
    let qb = 2.0 * f.dot(d);
    let qc = f.len2() - r * r;
    let disc = qb * qb - 4.0 * qa * qc;
    if disc < -1e-12 {
        return vec![];
    }
    let sq = disc.max(0.0).sqrt();
    let t1 = (-qb - sq) / (2.0 * qa);
    let t2 = (-qb + sq) / (2.0 * qa);
    if sq < 1e-12 {
        vec![(a + d * t1, t1)]
    } else {
        vec![(a + d * t1, t1), (a + d * t2, t2)]
    }
}

pub fn circle_circle(c1: V2, r1: f64, c2: V2, r2: f64) -> Vec<V2> {
    let d = c1.dist(c2);
    if d < EPS || d > r1 + r2 + 1e-9 || d < (r1 - r2).abs() - 1e-9 {
        return vec![];
    }
    let a = (r1 * r1 - r2 * r2 + d * d) / (2.0 * d);
    let h = (r1 * r1 - a * a).max(0.0).sqrt();
    let u = (c2 - c1) / d;
    let m = c1 + u * a;
    if h < 1e-12 {
        vec![m]
    } else {
        vec![m + u.perp() * h, m - u.perp() * h]
    }
}

/// Circle through three points.
pub fn circle_3p(a: V2, b: V2, c: V2) -> Option<(V2, f64)> {
    let d = 2.0 * (a.x * (b.y - c.y) + b.x * (c.y - a.y) + c.x * (a.y - b.y));
    if d.abs() < 1e-12 {
        return None;
    }
    let a2 = a.len2();
    let b2 = b.len2();
    let c2 = c.len2();
    let ux = (a2 * (b.y - c.y) + b2 * (c.y - a.y) + c2 * (a.y - b.y)) / d;
    let uy = (a2 * (c.x - b.x) + b2 * (a.x - c.x) + c2 * (b.x - a.x)) / d;
    let center = V2::new(ux, uy);
    Some((center, center.dist(a)))
}

/// Arc (center, radius, start, end) passing through p1 → pm → p2.
pub fn arc_3p(p1: V2, pm: V2, p2: V2) -> Option<(V2, f64, f64, f64)> {
    let (c, r) = circle_3p(p1, pm, p2)?;
    let a1 = (p1 - c).angle();
    let am = (pm - c).angle();
    let a2 = (p2 - c).angle();
    if angle_on_arc(am, a1, a2) {
        Some((c, r, norm_angle(a1), norm_angle(a2)))
    } else {
        Some((c, r, norm_angle(a2), norm_angle(a1)))
    }
}

pub fn seg_intersects_box(a: V2, b: V2, bx: &BBox) -> bool {
    if bx.contains(a) || bx.contains(b) {
        return true;
    }
    if !BBox::from_pts(a, b).intersects(bx) {
        return false;
    }
    let c = bx.corners();
    (0..4).any(|i| seg_seg(a, b, c[i], c[(i + 1) % 4]).is_some())
}

/// Tessellate an arc into a polyline with a chord tolerance in world units.
pub fn arc_points(c: V2, r: f64, start: f64, end: f64, tol: f64) -> Vec<V2> {
    let sweep = arc_sweep(start, end);
    let n = segments_for(r, sweep, tol);
    (0..=n).map(|i| c + V2::polar(r, start + sweep * i as f64 / n as f64)).collect()
}

pub fn segments_for(r: f64, sweep: f64, tol: f64) -> usize {
    let tol = tol.max(r * 1e-4).max(1e-9);
    let step = if tol >= r { PI / 2.0 } else { 2.0 * (1.0 - tol / r).acos() };
    ((sweep / step.max(1e-3)).ceil() as usize).clamp(4, 720)
}

/// Affine 2D transform `p' = M p + t`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Xform {
    pub m: [f64; 4],
    pub t: V2,
}

impl Xform {
    pub fn identity() -> Self {
        Self { m: [1.0, 0.0, 0.0, 1.0], t: V2::ZERO }
    }
    pub fn translate(d: V2) -> Self {
        Self { m: [1.0, 0.0, 0.0, 1.0], t: d }
    }
    pub fn rotate(c: V2, a: f64) -> Self {
        let (s, co) = a.sin_cos();
        let m = [co, -s, s, co];
        let x = Self { m, t: V2::ZERO };
        Self { m, t: c - x.apply_vec(c) }
    }
    pub fn scale(c: V2, k: f64) -> Self {
        Self { m: [k, 0.0, 0.0, k], t: c - c * k }
    }
    /// Mirror across the line through `a` and `b`.
    pub fn mirror(a: V2, b: V2) -> Self {
        let d = (b - a).norm();
        let (dx, dy) = (d.x, d.y);
        let m = [dx * dx - dy * dy, 2.0 * dx * dy, 2.0 * dx * dy, dy * dy - dx * dx];
        let x = Self { m, t: V2::ZERO };
        Self { m, t: a - x.apply_vec(a) }
    }
    pub fn apply(&self, p: V2) -> V2 {
        self.apply_vec(p) + self.t
    }
    pub fn apply_vec(&self, v: V2) -> V2 {
        V2::new(self.m[0] * v.x + self.m[1] * v.y, self.m[2] * v.x + self.m[3] * v.y)
    }
    pub fn det(&self) -> f64 {
        self.m[0] * self.m[3] - self.m[1] * self.m[2]
    }
    pub fn scale_factor(&self) -> f64 {
        self.det().abs().sqrt()
    }
    pub fn is_mirror(&self) -> bool {
        self.det() < 0.0
    }
    /// Map an angle (direction) through the linear part.
    pub fn apply_angle(&self, a: f64) -> f64 {
        self.apply_vec(V2::polar(1.0, a)).angle()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn arc_through_three_points() {
        let (c, r, s, e) = arc_3p(V2::new(1.0, 0.0), V2::new(0.0, 1.0), V2::new(-1.0, 0.0)).unwrap();
        assert!(c.len() < 1e-9 && (r - 1.0).abs() < 1e-9);
        assert!(s.abs() < 1e-9 && (e - PI).abs() < 1e-9);
        // Reverse direction keeps the arc on the same side.
        let (_, _, s2, e2) = arc_3p(V2::new(-1.0, 0.0), V2::new(0.0, 1.0), V2::new(1.0, 0.0)).unwrap();
        assert!((s2 - s).abs() < 1e-9 && (e2 - e).abs() < 1e-9);
    }

    #[test]
    fn mirror_reflects() {
        let m = Xform::mirror(V2::ZERO, V2::new(0.0, 1.0));
        let p = m.apply(V2::new(2.0, 3.0));
        assert!((p.x + 2.0).abs() < 1e-12 && (p.y - 3.0).abs() < 1e-12);
        assert!(m.is_mirror());
    }

    #[test]
    fn intersections() {
        let p = seg_seg(V2::new(0., 0.), V2::new(2., 2.), V2::new(0., 2.), V2::new(2., 0.)).unwrap();
        assert!((p - V2::new(1., 1.)).len() < 1e-12);
        assert_eq!(line_circle(V2::new(-5., 0.), V2::new(5., 0.), V2::ZERO, 1.0).len(), 2);
        assert_eq!(circle_circle(V2::ZERO, 1.0, V2::new(1.0, 0.0), 1.0).len(), 2);
    }
}
