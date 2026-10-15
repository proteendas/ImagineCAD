// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Proteen Das

//! Drawing primitives: what every entity (and the title block) explodes into.
//! The screen renderer, PDF, SVG and the exploded DXF writer all consume these.

use crate::geom::*;
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum HAlign {
    #[default]
    Left,
    Center,
    Right,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum VAlign {
    #[default]
    Baseline,
    Bottom,
    Middle,
    Top,
}

/// Single-line text. `height` is the cap height in drawing units.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Text {
    pub pos: V2,
    pub height: f64,
    /// Radians, counter-clockwise.
    pub rotation: f64,
    pub content: String,
    #[serde(default)]
    pub halign: HAlign,
    #[serde(default)]
    pub valign: VAlign,
}

impl Text {
    pub fn new(pos: V2, height: f64, content: impl Into<String>) -> Self {
        Self { pos, height, rotation: 0.0, content: content.into(), halign: HAlign::Left, valign: VAlign::Baseline }
    }
    pub fn aligned(mut self, h: HAlign, v: VAlign) -> Self {
        self.halign = h;
        self.valign = v;
        self
    }
    pub fn rotated(mut self, r: f64) -> Self {
        self.rotation = r;
        self
    }

    pub fn width(&self) -> f64 {
        text_width(&self.content, self.height)
    }

    /// Lower-left of the text box in local (unrotated) coordinates relative to `pos`.
    pub fn local_origin(&self) -> V2 {
        let w = self.width();
        let x = match self.halign {
            HAlign::Left => 0.0,
            HAlign::Center => -w / 2.0,
            HAlign::Right => -w,
        };
        let y = match self.valign {
            VAlign::Baseline | VAlign::Bottom => 0.0,
            VAlign::Middle => -self.height / 2.0,
            VAlign::Top => -self.height,
        };
        V2::new(x, y)
    }

    /// Text box corners in world coordinates (counter-clockwise from lower-left).
    pub fn corners(&self) -> [V2; 4] {
        let o = self.local_origin();
        let w = self.width();
        let h = self.height;
        [o, o + V2::new(w, 0.0), o + V2::new(w, h), o + V2::new(0.0, h)].map(|p| self.pos + p.rotate(self.rotation))
    }
}

/// Approximate advance width of a string in a Helvetica-like font,
/// for a font whose cap height is `h`.
pub fn text_width(s: &str, h: f64) -> f64 {
    // Font size ≈ cap height / 0.72 for Helvetica.
    let size = h / 0.72;
    s.chars().map(|c| char_width(c) * size).sum()
}

/// Helvetica advance widths (per 1 unit font size).
pub fn char_width(c: char) -> f64 {
    let w = match c {
        ' ' | '.' | ',' | ':' | ';' | '!' | '\'' | '|' | 'i' | 'j' | 'l' | 'I' => 278,
        'f' | 't' | '/' | '\\' | '[' | ']' | '(' | ')' | '-' | 'r' => 333,
        '"' => 355,
        '*' | '^' => 389,
        '°' => 400,
        'J' | 'c' | 'k' | 's' | 'v' | 'x' | 'y' | 'z' => 500,
        '0'..='9' | 'a' | 'b' | 'd' | 'e' | 'g' | 'h' | 'n' | 'o' | 'p' | 'q' | 'u' | '_' | '$' | '#' | '?' | '±' => 556,
        '+' | '<' | '>' | '=' | '~' => 584,
        'F' | 'T' | 'Z' => 611,
        'B' | 'E' | 'K' | 'P' | 'S' | 'V' | 'X' | 'Y' | 'A' | '&' => 667,
        'C' | 'D' | 'H' | 'N' | 'R' | 'U' | 'w' => 722,
        'G' | 'O' | 'Q' | 'Ø' | 'ø' | '⌀' => 778,
        'M' | 'm' => 833,
        '%' => 889,
        'W' => 944,
        'L' => 556,
        _ => 600,
    };
    w as f64 / 1000.0
}

#[derive(Clone, Debug, PartialEq)]
pub enum Prim {
    Line(V2, V2),
    Poly(Vec<V2>, bool),
    Circle(V2, f64),
    Arc {
        c: V2,
        r: f64,
        start: f64,
        end: f64,
    },
    /// Filled convex polygon (arrowheads, hatch trapezoids).
    Fill(Vec<V2>),
    Text(Text),
    /// Raster image placed with its lower-left corner at `pos`.
    Image(ImageRef),
}

/// A raster image reference (the pixels are loaded by the consumer).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ImageRef {
    pub path: String,
    pub pos: V2,
    pub w: f64,
    pub h: f64,
    /// 0 = opaque … 100 = invisible.
    #[serde(default)]
    pub fade: u8,
}

impl ImageRef {
    pub fn corners(&self) -> [V2; 4] {
        [self.pos, self.pos + V2::new(self.w, 0.0), self.pos + V2::new(self.w, self.h), self.pos + V2::new(0.0, self.h)]
    }
}

impl Prim {
    pub fn bbox(&self) -> BBox {
        match self {
            Prim::Line(a, b) => BBox::from_pts(*a, *b),
            Prim::Poly(p, _) | Prim::Fill(p) => BBox::of(p.iter().copied()),
            Prim::Circle(c, r) => BBox::from_pts(*c - V2::new(*r, *r), *c + V2::new(*r, *r)),
            Prim::Arc { c, r, start, end } => {
                let mut b = BBox::of([*c + V2::polar(*r, *start), *c + V2::polar(*r, *end)]);
                for k in 0..4 {
                    let a = k as f64 * std::f64::consts::FRAC_PI_2;
                    if angle_on_arc(a, *start, *end) {
                        b.add(*c + V2::polar(*r, a));
                    }
                }
                b
            }
            Prim::Text(t) => BBox::of(t.corners()),
            Prim::Image(i) => BBox::of(i.corners()),
        }
    }

    pub fn distance(&self, p: V2) -> f64 {
        match self {
            Prim::Line(a, b) => dist_point_segment(p, *a, *b),
            Prim::Poly(pts, closed) => poly_dist(p, pts, *closed),
            Prim::Fill(pts) => {
                if point_in_poly(p, pts) {
                    0.0
                } else {
                    poly_dist(p, pts, true)
                }
            }
            Prim::Circle(c, r) => (p.dist(*c) - r).abs(),
            Prim::Arc { c, r, start, end } => {
                let a = (p - *c).angle();
                if angle_on_arc(a, *start, *end) {
                    (p.dist(*c) - r).abs()
                } else {
                    p.dist(*c + V2::polar(*r, *start)).min(p.dist(*c + V2::polar(*r, *end)))
                }
            }
            Prim::Text(t) => {
                let c = t.corners();
                if point_in_poly(p, &c) {
                    0.0
                } else {
                    poly_dist(p, &c, true)
                }
            }
            // Images are picked by their frame so underlays don't swallow clicks.
            Prim::Image(i) => poly_dist(p, &i.corners(), true),
        }
    }

    /// Crossing-window test.
    pub fn crosses(&self, bx: &BBox) -> bool {
        if !self.bbox().intersects(bx) {
            return false;
        }
        match self {
            Prim::Line(a, b) => seg_intersects_box(*a, *b, bx),
            Prim::Poly(pts, closed) => {
                poly_segments(pts, *closed).any(|(a, b)| seg_intersects_box(a, b, bx)) || (*closed && point_in_poly(bx.center(), pts))
            }
            Prim::Fill(pts) => poly_segments(pts, true).any(|(a, b)| seg_intersects_box(a, b, bx)) || point_in_poly(bx.center(), pts),
            Prim::Circle(c, r) => {
                let pts = arc_points(*c, *r, 0.0, std::f64::consts::TAU, r * 0.01);
                let hit = poly_segments(&pts, false).any(|(a, b)| seg_intersects_box(a, b, bx));
                hit
            }
            Prim::Arc { c, r, start, end } => {
                let pts = arc_points(*c, *r, *start, *end, r * 0.01);
                let hit = poly_segments(&pts, false).any(|(a, b)| seg_intersects_box(a, b, bx));
                hit
            }
            Prim::Text(t) => {
                let c = t.corners();
                poly_segments(&c, true).any(|(a, b)| seg_intersects_box(a, b, bx)) || point_in_poly(bx.center(), &c)
            }
            Prim::Image(i) => poly_segments(&i.corners(), true).any(|(a, b)| seg_intersects_box(a, b, bx)),
        }
    }

    /// Apply an affine transform. Circles and arcs stay analytic under
    /// similarity transforms and are tessellated under non-uniform scaling.
    pub fn transformed(&self, x: &Xform) -> Prim {
        let k = x.scale_factor();
        let similar = {
            let a = x.apply_vec(V2::new(1.0, 0.0));
            let b = x.apply_vec(V2::new(0.0, 1.0));
            (a.len() - b.len()).abs() < 1e-9 * a.len().max(1.0) && a.dot(b).abs() < 1e-9 * a.len().max(1.0)
        };
        let pts = |v: &[V2]| v.iter().map(|p| x.apply(*p)).collect::<Vec<_>>();
        match self {
            Prim::Line(a, b) => Prim::Line(x.apply(*a), x.apply(*b)),
            Prim::Poly(v, c) => Prim::Poly(pts(v), *c),
            Prim::Fill(v) => Prim::Fill(pts(v)),
            Prim::Circle(c, r) if similar => Prim::Circle(x.apply(*c), r * k),
            Prim::Circle(c, r) => Prim::Poly(pts(&arc_points(*c, *r, 0.0, std::f64::consts::TAU, r * 0.002)), true),
            Prim::Arc { c, r, start, end } if similar => {
                let s = norm_angle(x.apply_angle(*start));
                let e = norm_angle(x.apply_angle(*end));
                let (s, e) = if x.is_mirror() { (e, s) } else { (s, e) };
                Prim::Arc { c: x.apply(*c), r: r * k, start: s, end: e }
            }
            Prim::Arc { c, r, start, end } => Prim::Poly(pts(&arc_points(*c, *r, *start, *end, r * 0.002)), false),
            Prim::Text(t) => {
                let mut t = t.clone();
                let up = x.apply_vec(V2::polar(t.height, t.rotation + std::f64::consts::FRAC_PI_2));
                t.pos = x.apply(t.pos);
                t.rotation = norm_angle(x.apply_angle(t.rotation));
                t.height = up.len();
                Prim::Text(t)
            }
            Prim::Image(i) => {
                let b = BBox::of(i.corners().map(|p| x.apply(p)));
                Prim::Image(ImageRef { path: i.path.clone(), pos: b.min, w: b.width(), h: b.height(), fade: i.fade })
            }
        }
    }
}

pub fn poly_segments(pts: &[V2], closed: bool) -> impl Iterator<Item = (V2, V2)> + '_ {
    let n = pts.len();
    let count = if closed && n > 2 { n } else { n.saturating_sub(1) };
    (0..count).map(move |i| (pts[i], pts[(i + 1) % n]))
}

fn poly_dist(p: V2, pts: &[V2], closed: bool) -> f64 {
    if pts.len() == 1 {
        return p.dist(pts[0]);
    }
    poly_segments(pts, closed).map(|(a, b)| dist_point_segment(p, a, b)).fold(f64::INFINITY, f64::min)
}

pub fn point_in_poly(p: V2, pts: &[V2]) -> bool {
    let mut inside = false;
    let n = pts.len();
    let mut j = n.wrapping_sub(1);
    for i in 0..n {
        let (a, b) = (pts[i], pts[j]);
        if (a.y > p.y) != (b.y > p.y) && p.x < (b.x - a.x) * (p.y - a.y) / (b.y - a.y) + a.x {
            inside = !inside;
        }
        j = i;
    }
    inside
}

/// Filled arrowhead with its tip at `tip`, pointing along `dir`.
pub fn arrow(tip: V2, dir: V2, size: f64) -> Prim {
    let d = dir.norm();
    let n = d.perp() * (size / 6.0);
    let base = tip - d * size;
    Prim::Fill(vec![tip, base + n, base - n])
}
