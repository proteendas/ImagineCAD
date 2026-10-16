// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Proteen Das

//! Drawing entities and their geometric behaviour (picking, grips, snaps, transforms).

use crate::curves::*;
use crate::geom::*;
use crate::hatch;
use crate::prim::*;
use crate::style::*;
use crate::symbols::*;
use crate::table::*;
use serde::{Deserialize, Serialize};
use std::f64::consts::{FRAC_PI_2, PI, TAU};

/// Length used to draw "infinite" construction lines.
pub const INFINITE: f64 = 1.0e6;

/// Everything an entity needs to produce its graphics.
#[derive(Clone, Copy)]
pub struct Ctx<'a> {
    pub ds: &'a DimStyle,
    /// Annotation scale (sheet scale).
    pub scale: f64,
    pub blocks: &'a [BlockDef],
    pub depth: u8,
    /// Multileader styles of the drawing.
    pub mstyles: &'a [MLeaderStyle],
}

impl<'a> Ctx<'a> {
    pub fn new(ds: &'a DimStyle, scale: f64) -> Self {
        Self { ds, scale, blocks: &[], depth: 0, mstyles: &[] }
    }
    pub fn block(&self, name: &str) -> Option<&'a BlockDef> {
        self.blocks.iter().find(|b| b.name.eq_ignore_ascii_case(name))
    }
    fn deeper(&self) -> Ctx<'a> {
        Ctx { depth: self.depth + 1, ..*self }
    }
}

/// A named block definition (reusable group of entities around a base point).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct BlockDef {
    pub name: String,
    pub base: V2,
    pub entities: Vec<Entity>,
    #[serde(default)]
    pub description: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum DimKind {
    Horizontal,
    Vertical,
    Aligned,
    Radius,
    Diameter,
    /// Angle between the rays `center→p1` and `center→p2`.
    Angular,
    /// X distance of `p1` from the datum `center` (vertical leader to `loc`).
    OrdinateX,
    /// Y distance of `p1` from the datum `center` (horizontal leader to `loc`).
    OrdinateY,
    /// Length of the arc around `center` from `p1` counter-clockwise to `p2`,
    /// drawn at the radius of `loc`.
    ArcLength,
}

/// A point of a dimension tied to a feature (snap point) of another entity.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct GeomRef {
    pub id: u64,
    /// Index into that entity's `snap_points()`.
    pub index: usize,
}

/// What an associative dimension follows. Radial and arc-length dimensions
/// follow a whole circle or arc (`curve`); the others follow points.
#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct DimAssoc {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub p1: Option<GeomRef>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub p2: Option<GeomRef>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub center: Option<GeomRef>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub curve: Option<u64>,
}

impl DimAssoc {
    pub fn is_empty(&self) -> bool {
        self.p1.is_none() && self.p2.is_none() && self.center.is_none() && self.curve.is_none()
    }
}

/// A dimension. For linear kinds `p1`/`p2` are the measured feature points;
/// for radial kinds `p1` is the centre and `p2` a point on the circle; for
/// angular dimensions `center` is the vertex and `p1`/`p2` lie on the two legs.
/// `loc` is the dimension-line / text location.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Dimension {
    pub kind: DimKind,
    pub p1: V2,
    pub p2: V2,
    pub loc: V2,
    /// Text override; `<>` is replaced by the measured value (DXF convention).
    #[serde(default)]
    pub text: Option<String>,
    #[serde(default)]
    pub center: Option<V2>,
    /// Associativity with the dimensioned geometry.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub assoc: Option<DimAssoc>,
}

impl Dimension {
    pub fn new(kind: DimKind, p1: V2, p2: V2, loc: V2) -> Self {
        Self { kind, p1, p2, loc, text: None, center: None, assoc: None }
    }

    pub fn angular(center: V2, p1: V2, p2: V2, loc: V2) -> Self {
        Self { kind: DimKind::Angular, p1, p2, loc, text: None, center: Some(center), assoc: None }
    }

    /// Ordinate dimension of `feature` from `datum`; the axis follows the
    /// leader direction (mostly vertical → X ordinate).
    pub fn ordinate(datum: V2, feature: V2, leader_end: V2) -> Self {
        let d = leader_end - feature;
        let kind = if d.y.abs() >= d.x.abs() { DimKind::OrdinateX } else { DimKind::OrdinateY };
        Self { kind, p1: feature, p2: feature, loc: leader_end, text: None, center: Some(datum), assoc: None }
    }

    /// Arc-length dimension of the arc around `c` from `a` counter-clockwise to `b`.
    pub fn arc_length(c: V2, a: V2, b: V2, loc: V2) -> Self {
        Self { kind: DimKind::ArcLength, p1: a, p2: b, loc, text: None, center: Some(c), assoc: None }
    }

    pub fn is_ordinate(&self) -> bool {
        matches!(self.kind, DimKind::OrdinateX | DimKind::OrdinateY)
    }

    /// Start angle and counter-clockwise sweep of an arc-length dimension.
    pub fn arc_span(&self) -> (f64, f64) {
        let c = self.center.unwrap_or(V2::ZERO);
        let s = norm_angle((self.p1 - c).angle());
        (s, arc_sweep(s, norm_angle((self.p2 - c).angle())))
    }

    /// For angular dimensions: (vertex, start, end) of the measured CCW sector.
    pub fn sector(&self) -> (V2, f64, f64) {
        let c = self.center.unwrap_or(V2::ZERO);
        let a1 = norm_angle((self.p1 - c).angle());
        let a2 = norm_angle((self.p2 - c).angle());
        let al = (self.loc - c).angle();
        if angle_on_arc(al, a1, a2) {
            (c, a1, a2)
        } else {
            (c, a2, a1)
        }
    }

    pub fn measurement(&self) -> f64 {
        match self.kind {
            DimKind::Horizontal => (self.p2.x - self.p1.x).abs(),
            DimKind::Vertical => (self.p2.y - self.p1.y).abs(),
            DimKind::Aligned | DimKind::Radius => self.p1.dist(self.p2),
            DimKind::Diameter => 2.0 * self.p1.dist(self.p2),
            DimKind::Angular => {
                let (_, s, e) = self.sector();
                deg(arc_sweep(s, e))
            }
            DimKind::OrdinateX => (self.p1.x - self.center.unwrap_or(V2::ZERO).x).abs(),
            DimKind::OrdinateY => (self.p1.y - self.center.unwrap_or(V2::ZERO).y).abs(),
            DimKind::ArcLength => self.center.unwrap_or(V2::ZERO).dist(self.p1) * self.arc_span().1,
        }
    }

    pub fn measured_text(&self, ds: &DimStyle) -> String {
        let v = ds.format(self.measurement());
        match self.kind {
            DimKind::Radius => format!("R{v}"),
            DimKind::Diameter => format!("Ø{v}"),
            DimKind::Angular => format!("{v}°"),
            _ => v,
        }
    }

    pub fn label(&self, ds: &DimStyle) -> String {
        let m = self.measured_text(ds);
        match &self.text {
            Some(t) if !t.trim().is_empty() => t.replace("<>", &m),
            _ => m,
        }
    }

    /// Direction of the dimension line for linear kinds.
    pub fn direction(&self) -> V2 {
        match self.kind {
            DimKind::Horizontal | DimKind::OrdinateY => V2::new(1.0, 0.0),
            DimKind::Vertical | DimKind::OrdinateX => V2::new(0.0, 1.0),
            _ => {
                let d = (self.p2 - self.p1).norm();
                if d == V2::ZERO {
                    V2::new(1.0, 0.0)
                } else {
                    d
                }
            }
        }
    }

    /// Feet of the extension lines on the dimension line.
    pub fn dim_line_points(&self) -> (V2, V2) {
        let d = self.direction();
        (self.loc + d * (self.p1 - self.loc).dot(d), self.loc + d * (self.p2 - self.loc).dot(d))
    }

    pub fn prims(&self, ds: &DimStyle, scale: f64) -> Vec<Prim> {
        let s = scale.max(1e-9);
        let th = ds.text_height * s;
        let ar = ds.arrow_size * s;
        let gap = ds.text_gap * s;
        let label = self.label(ds);
        let tw = text_width(&label, th);
        let mut out = Vec::new();
        let readable = |ang: f64| -> f64 {
            let mut a = ang;
            if a > FRAC_PI_2 + 1e-6 || a <= -FRAC_PI_2 + 1e-6 {
                a += PI;
            }
            a
        };
        match self.kind {
            DimKind::Horizontal | DimKind::Vertical | DimKind::Aligned => {
                let d = self.direction();
                let (q1, q2) = self.dim_line_points();
                for (p, q) in [(self.p1, q1), (self.p2, q2)] {
                    let v = q - p;
                    let l = v.len();
                    if l > ds.ext_offset * s {
                        let u = v / l;
                        out.push(Prim::Line(p + u * (ds.ext_offset * s), q + u * (ds.ext_beyond * s)));
                    }
                }
                let t1 = (q1 - self.loc).dot(d);
                let t2 = (q2 - self.loc).dot(d);
                let (tlo, thi) = (t1.min(t2), t1.max(t2));
                let lo = self.loc + d * tlo;
                let hi = self.loc + d * thi;
                let ang = readable(d.angle());
                let up = V2::polar(1.0, ang + FRAC_PI_2);
                let (mut a, mut b) = (tlo, thi);
                if thi - tlo >= 2.5 * ar {
                    out.push(arrow(lo, -d, ar));
                    out.push(arrow(hi, d, ar));
                } else {
                    out.push(arrow(lo, d, ar));
                    out.push(arrow(hi, -d, ar));
                    a -= 2.0 * ar;
                    b += 2.0 * ar;
                }
                a = a.min(-tw / 2.0);
                b = b.max(tw / 2.0);
                if 0.0 < tlo || 0.0 > thi || thi - tlo < 2.5 * ar {
                    out.push(Prim::Line(self.loc + d * a, self.loc + d * b));
                } else {
                    out.push(Prim::Line(lo, hi));
                }
                out.push(Prim::Text(
                    Text::new(self.loc + up * gap, th, label).aligned(HAlign::Center, VAlign::Bottom).rotated(norm_angle(ang)),
                ));
            }
            DimKind::Radius | DimKind::Diameter => {
                let c = self.p1;
                let r = self.p1.dist(self.p2);
                let mut u = (self.loc - c).norm();
                if u == V2::ZERO {
                    u = (self.p2 - c).norm();
                }
                let tip = c + u * r;
                if self.loc.dist(c) > r {
                    out.push(Prim::Line(tip, self.loc));
                    out.push(arrow(tip, -u, ar));
                    let sx = if u.x >= 0.0 { 1.0 } else { -1.0 };
                    let land = self.loc + V2::new(sx * ar, 0.0);
                    out.push(Prim::Line(self.loc, land));
                    let h = if sx > 0.0 { HAlign::Left } else { HAlign::Right };
                    out.push(Prim::Text(Text::new(land + V2::new(sx * gap, 0.0), th, label).aligned(h, VAlign::Middle)));
                } else {
                    let from = if self.kind == DimKind::Diameter { c - u * r } else { c };
                    out.push(Prim::Line(from, tip));
                    out.push(arrow(tip, u, ar));
                    if self.kind == DimKind::Diameter {
                        out.push(arrow(from, -u, ar));
                    }
                    let ang = readable(u.angle());
                    let up = V2::polar(1.0, ang + FRAC_PI_2);
                    out.push(Prim::Text(
                        Text::new(self.loc + up * gap, th, label).aligned(HAlign::Center, VAlign::Bottom).rotated(norm_angle(ang)),
                    ));
                }
            }
            DimKind::Angular => {
                let (c, st, en) = self.sector();
                let r = self.loc.dist(c).max(1e-9);
                for p in [self.p1, self.p2] {
                    let u = (p - c).norm();
                    let dp = p.dist(c);
                    if dp < r {
                        out.push(Prim::Line(c + u * (dp + ds.ext_offset * s), c + u * (r + ds.ext_beyond * s)));
                    }
                }
                out.push(Prim::Arc { c, r, start: st, end: en });
                let tan = |a: f64| V2::new(-a.sin(), a.cos());
                out.push(arrow(c + V2::polar(r, st), -tan(st), ar));
                out.push(arrow(c + V2::polar(r, en), tan(en), ar));
                let mid = st + arc_sweep(st, en) / 2.0;
                let mut rot = norm_angle(mid - FRAC_PI_2);
                let mut va = VAlign::Bottom;
                if rot > FRAC_PI_2 + 1e-6 && rot <= 3.0 * FRAC_PI_2 + 1e-6 {
                    rot = norm_angle(rot + PI);
                    va = VAlign::Top;
                }
                out.push(Prim::Text(Text::new(c + V2::polar(r + gap, mid), th, label).aligned(HAlign::Center, va).rotated(rot)));
            }
            DimKind::OrdinateX | DimKind::OrdinateY => {
                // Leader from the feature (after the extension gap) to `loc`,
                // jogged when the end is offset sideways; text beyond the end.
                let x_axis = self.kind == DimKind::OrdinateX;
                let along = if x_axis { V2::new(0.0, 1.0) } else { V2::new(1.0, 0.0) };
                let side = (self.loc - self.p1).dot(along);
                let dir = if side >= 0.0 { along } else { -along };
                let total = side.abs();
                let start = self.p1 + dir * (ds.ext_offset * s).min(total * 0.5);
                let lateral = (self.loc - self.p1) - along * side;
                if lateral.len() > 1e-9 {
                    let j1 = self.p1 + dir * (total / 3.0);
                    let j2 = j1 + lateral + dir * (total / 3.0);
                    out.push(Prim::Poly(vec![start, j1, j2, self.loc], false));
                } else {
                    out.push(Prim::Line(start, self.loc));
                }
                let (h, rot) = match (x_axis, side >= 0.0) {
                    (true, true) => (HAlign::Left, FRAC_PI_2),
                    (true, false) => (HAlign::Right, FRAC_PI_2),
                    (false, true) => (HAlign::Left, 0.0),
                    (false, false) => (HAlign::Right, 0.0),
                };
                out.push(Prim::Text(Text::new(self.loc + dir * gap, th, label).aligned(h, VAlign::Middle).rotated(rot)));
            }
            DimKind::ArcLength => {
                let c = self.center.unwrap_or(V2::ZERO);
                let (st, sw) = self.arc_span();
                let en = st + sw;
                let r = self.loc.dist(c).max(1e-9);
                for p in [self.p1, self.p2] {
                    let u = (p - c).norm();
                    let dp = p.dist(c);
                    let (a, b) = if dp < r {
                        (dp + ds.ext_offset * s, r + ds.ext_beyond * s)
                    } else {
                        (dp - ds.ext_offset * s, r - ds.ext_beyond * s)
                    };
                    out.push(Prim::Line(c + u * a, c + u * b));
                }
                out.push(Prim::Arc { c, r, start: st, end: norm_angle(en) });
                let tan = |a: f64| V2::new(-a.sin(), a.cos());
                out.push(arrow(c + V2::polar(r, st), -tan(st), ar));
                out.push(arrow(c + V2::polar(r, en), tan(en), ar));
                let mid = st + sw / 2.0;
                let mut rot = norm_angle(mid - FRAC_PI_2);
                let mut va = VAlign::Bottom;
                let mut lift = gap;
                if rot > FRAC_PI_2 + 1e-6 && rot <= 3.0 * FRAC_PI_2 + 1e-6 {
                    rot = norm_angle(rot + PI);
                    va = VAlign::Top;
                    lift = -gap;
                }
                let t = Text::new(c + V2::polar(r + lift, mid), th, label).aligned(HAlign::Center, va).rotated(rot);
                // Arc symbol drawn over the value (ISO 129).
                let up = V2::polar(1.0, rot + FRAC_PI_2);
                let base = if va == VAlign::Bottom { t.pos + up * (th * 1.25) } else { t.pos + up * (th * 0.25) };
                let half = (tw / 2.0).min(th * 1.2).max(th * 0.5);
                let sym_r = half / (0.9f64).sin();
                let sym_c = base - up * (sym_r * 0.9f64.cos());
                let a0 = rot + FRAC_PI_2;
                out.push(Prim::Arc { c: sym_c, r: sym_r, start: norm_angle(a0 - 0.9), end: norm_angle(a0 + 0.9) });
                out.push(Prim::Text(t));
            }
        }
        out
    }
}

/// Multi-line text with optional word wrapping.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct MText {
    pub pos: V2,
    pub height: f64,
    /// Wrap width (0 = no wrapping).
    pub width: f64,
    pub rotation: f64,
    /// Lines separated by `\n`.
    pub content: String,
    /// Attachment point 1–9: top-left, top-center, top-right, middle-…, bottom-….
    pub attach: u8,
    /// Line spacing factor.
    #[serde(default = "one")]
    pub spacing: f64,
}

fn one() -> f64 {
    1.0
}

impl MText {
    pub fn new(pos: V2, height: f64, width: f64, content: impl Into<String>) -> Self {
        Self { pos, height, width, rotation: 0.0, content: content.into(), attach: 1, spacing: 1.0 }
    }

    /// Wrapped lines.
    pub fn lines(&self) -> Vec<String> {
        let mut out = vec![];
        for para in self.content.split('\n') {
            if self.width <= 0.0 {
                out.push(para.to_string());
                continue;
            }
            let mut line = String::new();
            for word in para.split(' ') {
                let cand = if line.is_empty() { word.to_string() } else { format!("{line} {word}") };
                if !line.is_empty() && text_width(&cand, self.height) > self.width {
                    out.push(std::mem::take(&mut line));
                    line = word.to_string();
                } else {
                    line = cand;
                }
            }
            out.push(line);
        }
        out
    }

    pub fn texts(&self) -> Vec<Text> {
        let lines = self.lines();
        let pitch = self.height * 1.667 * self.spacing;
        let n = lines.len().max(1) as f64;
        let total = (n - 1.0) * pitch + self.height;
        let row = (self.attach.clamp(1, 9) - 1) / 3;
        let col = (self.attach.clamp(1, 9) - 1) % 3;
        let top = match row {
            0 => 0.0,
            1 => total / 2.0,
            _ => total,
        };
        let h = match col {
            0 => HAlign::Left,
            1 => HAlign::Center,
            _ => HAlign::Right,
        };
        lines
            .into_iter()
            .enumerate()
            .filter(|(_, l)| !l.is_empty())
            .map(|(i, l)| {
                let local = V2::new(0.0, top - i as f64 * pitch);
                Text::new(self.pos + local.rotate(self.rotation), self.height, l).aligned(h, VAlign::Top).rotated(self.rotation)
            })
            .collect()
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Hatch {
    /// Boundary loops; filled with the even–odd rule (islands are holes).
    pub loops: Vec<Vec<V2>>,
    /// Pattern name (`SOLID`, `ANSI31`, …).
    pub pattern: String,
    /// Pattern scale (multiplies the paper-mm spacing).
    pub scale: f64,
    /// Pattern rotation (radians).
    pub angle: f64,
}

impl Hatch {
    pub fn is_solid(&self) -> bool {
        self.pattern.eq_ignore_ascii_case("SOLID")
    }
    pub fn area(&self) -> f64 {
        hatch::region_area(&self.loops)
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Insert {
    pub name: String,
    pub pos: V2,
    pub sx: f64,
    pub sy: f64,
    pub rotation: f64,
    /// Attribute values of this reference (by tag).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub attribs: Vec<Attrib>,
}

/// Attribute definition inside a block: a labelled text slot whose value is
/// entered per block reference.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct AttDef {
    pub tag: String,
    #[serde(default)]
    pub prompt: String,
    #[serde(default)]
    pub default: String,
    /// Position, height, rotation and alignment (the content is ignored).
    pub text: Text,
    #[serde(default)]
    pub invisible: bool,
}

/// An attribute value of a block reference.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Attrib {
    pub tag: String,
    pub value: String,
}

impl BlockDef {
    /// The block's attribute definitions, in drawing order.
    pub fn attdefs(&self) -> impl Iterator<Item = &AttDef> {
        self.entities.iter().filter_map(|e| if let Shape::AttDef(a) = &e.shape { Some(a) } else { None })
    }
}

impl Insert {
    pub fn new(name: impl Into<String>, pos: V2) -> Self {
        Self { name: name.into(), pos, sx: 1.0, sy: 1.0, rotation: 0.0, attribs: vec![] }
    }
    /// Value of attribute `tag` (case-insensitive), if set on this reference.
    pub fn value_of(&self, tag: &str) -> Option<&str> {
        self.attribs.iter().find(|a| a.tag.eq_ignore_ascii_case(tag)).map(|a| a.value.as_str())
    }
    /// Set (or add) the value of attribute `tag`.
    pub fn set_value(&mut self, tag: &str, value: impl Into<String>) {
        let value = value.into();
        match self.attribs.iter_mut().find(|a| a.tag.eq_ignore_ascii_case(tag)) {
            Some(a) => a.value = value,
            None => self.attribs.push(Attrib { tag: tag.to_string(), value }),
        }
    }
    /// Attribute texts of this reference in block coordinates (before the
    /// insertion transform), with values filled in; invisible ones omitted.
    pub fn attribute_texts(&self, block: &BlockDef) -> Vec<Text> {
        block
            .attdefs()
            .filter(|a| !a.invisible)
            .map(|a| {
                let mut t = a.text.clone();
                t.content = self.value_of(&a.tag).unwrap_or(&a.default).to_string();
                t
            })
            .filter(|t| !t.content.is_empty())
            .collect()
    }
    /// Block space → model space.
    pub fn xform(&self, base: V2) -> Xform {
        let (s, c) = self.rotation.sin_cos();
        let m = [c * self.sx, -s * self.sy, s * self.sx, c * self.sy];
        let lin = Xform { m, t: V2::ZERO };
        Xform { m, t: self.pos - lin.apply_vec(base) }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Leader {
    /// Arrow tip first, then vertices; text goes after the last point.
    pub pts: Vec<V2>,
    pub text: String,
    /// Multileader style (None = the dimension style's arrow and text).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub style: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum Shape {
    Line {
        a: V2,
        b: V2,
    },
    Circle {
        c: V2,
        r: f64,
    },
    /// Counter-clockwise arc from `start` to `end` (radians).
    Arc {
        c: V2,
        r: f64,
        start: f64,
        end: f64,
    },
    Polyline {
        pts: Vec<V2>,
        closed: bool,
    },
    Text(Text),
    Dim(Dimension),
    /// Ellipse with centre `c`, major-axis vector `major` (centre → end),
    /// minor/major `ratio` and parameter range (full ellipse: 0..2π).
    Ellipse {
        c: V2,
        major: V2,
        ratio: f64,
        start: f64,
        end: f64,
    },
    /// Interpolating spline through fit points.
    Spline {
        pts: Vec<V2>,
        closed: bool,
    },
    Point {
        p: V2,
    },
    /// Infinite construction line.
    XLine {
        p: V2,
        dir: V2,
    },
    /// Semi-infinite construction line.
    Ray {
        p: V2,
        dir: V2,
    },
    MText(MText),
    Hatch(Hatch),
    Insert(Insert),
    Leader(Leader),
    Image(ImageRef),
    /// Geometric tolerance feature control frame.
    Tolerance(Fcf),
    Table(Table),
    /// Attribute definition (in a block; shows its tag elsewhere).
    AttDef(AttDef),
    /// Datum feature symbol.
    Datum(DatumFeature),
    /// Surface texture symbol.
    Surface(SurfaceFinish),
    /// Weld symbol.
    Weld(WeldSymbol),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum SnapKind {
    End,
    Mid,
    Center,
    Quadrant,
    Intersection,
    Node,
    Insertion,
    Nearest,
    Perpendicular,
}

impl SnapKind {
    pub fn label(self) -> &'static str {
        match self {
            SnapKind::End => "Endpoint",
            SnapKind::Mid => "Midpoint",
            SnapKind::Center => "Center",
            SnapKind::Quadrant => "Quadrant",
            SnapKind::Intersection => "Intersection",
            SnapKind::Node => "Node",
            SnapKind::Insertion => "Insertion",
            SnapKind::Nearest => "Nearest",
            SnapKind::Perpendicular => "Perpendicular",
        }
    }
}

/// Analytic curve used for intersections, trimming and extending.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Curve {
    Seg(V2, V2),
    Circle(V2, f64),
    Arc(V2, f64, f64, f64),
}

impl Curve {
    pub fn intersect(&self, o: &Curve) -> Vec<V2> {
        use Curve::*;
        match (*self, *o) {
            (Seg(a1, a2), Seg(b1, b2)) => seg_seg(a1, a2, b1, b2).into_iter().collect(),
            (Seg(a, b), c) | (c, Seg(a, b)) => line_circle(a, b, c.center(), c.radius())
                .into_iter()
                .filter(|(p, t)| (-1e-9..=1.0 + 1e-9).contains(t) && c.on_curve_angle(*p))
                .map(|(p, _)| p)
                .collect(),
            (a, b) => circle_circle(a.center(), a.radius(), b.center(), b.radius())
                .into_iter()
                .filter(|p| a.on_curve_angle(*p) && b.on_curve_angle(*p))
                .collect(),
        }
    }

    fn center(&self) -> V2 {
        match *self {
            Curve::Circle(c, _) | Curve::Arc(c, ..) => c,
            Curve::Seg(a, b) => a.mid(b),
        }
    }
    fn radius(&self) -> f64 {
        match *self {
            Curve::Circle(_, r) | Curve::Arc(_, r, ..) => r,
            Curve::Seg(..) => 0.0,
        }
    }
    fn on_curve_angle(&self, p: V2) -> bool {
        match *self {
            Curve::Arc(c, _, s, e) => angle_on_arc((p - c).angle(), s, e),
            _ => true,
        }
    }

    pub fn nearest(&self, p: V2) -> V2 {
        match *self {
            Curve::Seg(a, b) => closest_on_segment(p, a, b),
            Curve::Circle(c, r) => c + (p - c).norm() * r,
            Curve::Arc(c, r, s, e) => {
                let a = (p - c).angle();
                if angle_on_arc(a, s, e) {
                    c + V2::polar(r, a)
                } else {
                    let ps = c + V2::polar(r, s);
                    let pe = c + V2::polar(r, e);
                    if p.dist(ps) < p.dist(pe) {
                        ps
                    } else {
                        pe
                    }
                }
            }
        }
    }

    pub fn transformed(&self, x: &Xform) -> Vec<Curve> {
        match *self {
            Curve::Seg(a, b) => vec![Curve::Seg(x.apply(a), x.apply(b))],
            Curve::Circle(c, r) => match Prim::Circle(c, r).transformed(x) {
                Prim::Circle(c, r) => vec![Curve::Circle(c, r)],
                Prim::Poly(p, _) => poly_curves(&p, true),
                _ => vec![],
            },
            Curve::Arc(c, r, s, e) => match (Prim::Arc { c, r, start: s, end: e }).transformed(x) {
                Prim::Arc { c, r, start, end } => vec![Curve::Arc(c, r, start, end)],
                Prim::Poly(p, _) => poly_curves(&p, false),
                _ => vec![],
            },
        }
    }
}

fn poly_curves(pts: &[V2], closed: bool) -> Vec<Curve> {
    poly_segments(pts, closed).map(|(a, b)| Curve::Seg(a, b)).collect()
}

fn ellipse_tol(major: V2) -> f64 {
    (major.len() * 0.002).max(1e-6)
}

fn point_marker(p: V2, size: f64) -> Vec<Prim> {
    vec![Prim::Line(p - V2::new(size, 0.0), p + V2::new(size, 0.0)), Prim::Line(p - V2::new(0.0, size), p + V2::new(0.0, size))]
}

impl Shape {
    pub fn type_name(&self) -> &'static str {
        match self {
            Shape::Line { .. } => "Line",
            Shape::Circle { .. } => "Circle",
            Shape::Arc { .. } => "Arc",
            Shape::Polyline { .. } => "Polyline",
            Shape::Text(_) => "Text",
            Shape::Dim(d) => match d.kind {
                DimKind::Horizontal | DimKind::Vertical => "Rotated Dimension",
                DimKind::Aligned => "Aligned Dimension",
                DimKind::Radius => "Radial Dimension",
                DimKind::Diameter => "Diametric Dimension",
                DimKind::Angular => "Angular Dimension",
                DimKind::OrdinateX | DimKind::OrdinateY => "Ordinate Dimension",
                DimKind::ArcLength => "Arc Length Dimension",
            },
            Shape::Ellipse { .. } => "Ellipse",
            Shape::Spline { .. } => "Spline",
            Shape::Point { .. } => "Point",
            Shape::XLine { .. } => "Construction Line",
            Shape::Ray { .. } => "Ray",
            Shape::MText(_) => "MText",
            Shape::Hatch(_) => "Hatch",
            Shape::Insert(_) => "Block Reference",
            Shape::Leader(_) => "Leader",
            Shape::Image(_) => "Raster Image",
            Shape::Tolerance(_) => "Tolerance",
            Shape::Table(_) => "Table",
            Shape::AttDef(_) => "Attribute Definition",
            Shape::Datum(_) => "Datum Symbol",
            Shape::Surface(_) => "Surface Texture Symbol",
            Shape::Weld(_) => "Weld Symbol",
        }
    }

    pub fn is_annotation(&self) -> bool {
        matches!(
            self,
            Shape::Text(_)
                | Shape::Dim(_)
                | Shape::MText(_)
                | Shape::Leader(_)
                | Shape::Tolerance(_)
                | Shape::Datum(_)
                | Shape::Surface(_)
                | Shape::Weld(_)
                | Shape::AttDef(_)
                | Shape::Table(_)
        )
    }

    /// Construction geometry that is excluded from extents.
    pub fn is_construction(&self) -> bool {
        matches!(self, Shape::XLine { .. } | Shape::Ray { .. })
    }

    pub fn prims(&self, ctx: &Ctx) -> Vec<Prim> {
        match self {
            Shape::Line { a, b } => vec![Prim::Line(*a, *b)],
            Shape::Circle { c, r } => vec![Prim::Circle(*c, *r)],
            Shape::Arc { c, r, start, end } => vec![Prim::Arc { c: *c, r: *r, start: *start, end: *end }],
            Shape::Polyline { pts, closed } => vec![Prim::Poly(pts.clone(), *closed)],
            Shape::Text(t) => vec![Prim::Text(t.clone())],
            Shape::Dim(d) => d.prims(ctx.ds, ctx.scale),
            Shape::Ellipse { c, major, ratio, start, end } => {
                let full = is_full_ellipse(*start, *end);
                let (s, e) = if full { (0.0, TAU) } else { (*start, *end) };
                vec![Prim::Poly(ellipse_points(*c, *major, *ratio, s, e, ellipse_tol(*major)), full)]
            }
            Shape::Spline { pts, closed } => {
                let ext = BBox::of(pts.iter().copied());
                let tol = (ext.width().max(ext.height()) * 0.0005).max(1e-6);
                vec![Prim::Poly(spline_points(pts, *closed, tol), *closed)]
            }
            Shape::Point { p } => point_marker(*p, 1.0 * ctx.scale),
            Shape::XLine { p, dir } => {
                let d = dir.norm();
                vec![Prim::Line(*p - d * INFINITE, *p + d * INFINITE)]
            }
            Shape::Ray { p, dir } => vec![Prim::Line(*p, *p + dir.norm() * INFINITE)],
            Shape::MText(m) => m.texts().into_iter().map(Prim::Text).collect(),
            Shape::Hatch(h) => {
                if h.is_solid() {
                    hatch::solid_trapezoids(&h.loops).into_iter().map(|t| Prim::Fill(t.to_vec())).collect()
                } else {
                    hatch::pattern_segments(&h.loops, &h.pattern, h.scale * ctx.scale, h.angle)
                        .into_iter()
                        .map(|(a, b)| Prim::Line(a, b))
                        .collect()
                }
            }
            Shape::Insert(ins) => {
                if ctx.depth > 8 {
                    return vec![];
                }
                match ctx.block(&ins.name) {
                    Some(b) => {
                        let x = ins.xform(b.base);
                        let inner = ctx.deeper();
                        let mut out: Vec<Prim> = b
                            .entities
                            .iter()
                            .filter(|e| !matches!(e.shape, Shape::AttDef(_)))
                            .flat_map(|e| e.shape.prims(&inner))
                            .map(|p| p.transformed(&x))
                            .collect();
                        out.extend(ins.attribute_texts(b).into_iter().map(|t| Prim::Text(t).transformed(&x)));
                        out
                    }
                    None => point_marker(ins.pos, 2.0 * ctx.scale),
                }
            }
            Shape::Leader(l) => leader_prims(l, ctx),
            Shape::Image(i) => vec![Prim::Image(i.clone())],
            Shape::Tolerance(f) => f.prims(ctx.ds.text_height * ctx.scale, ctx.ds.arrow_size * ctx.scale),
            Shape::Table(t) => t.prims(),
            Shape::AttDef(a) => {
                let mut t = a.text.clone();
                t.content = a.tag.clone();
                vec![Prim::Text(t)]
            }
            Shape::Datum(d) => d.prims(ctx.ds.text_height * ctx.scale),
            Shape::Surface(s) => s.prims(ctx.ds.text_height * ctx.scale),
            Shape::Weld(w) => w.prims(ctx.ds.text_height * ctx.scale, ctx.ds.arrow_size * ctx.scale),
        }
    }

    pub fn bbox(&self, ctx: &Ctx) -> BBox {
        self.prims(ctx).iter().fold(BBox::empty(), |b, p| b.union(&p.bbox()))
    }

    pub fn curves(&self) -> Vec<Curve> {
        match self {
            Shape::Line { a, b } => vec![Curve::Seg(*a, *b)],
            Shape::Circle { c, r } => vec![Curve::Circle(*c, *r)],
            Shape::Arc { c, r, start, end } => vec![Curve::Arc(*c, *r, *start, *end)],
            Shape::Polyline { pts, closed } => poly_curves(pts, *closed),
            Shape::Ellipse { c, major, ratio, start, end } => {
                let full = is_full_ellipse(*start, *end);
                let (s, e) = if full { (0.0, TAU) } else { (*start, *end) };
                poly_curves(&ellipse_points(*c, *major, *ratio, s, e, ellipse_tol(*major)), false)
            }
            Shape::Spline { pts, closed } => {
                let ext = BBox::of(pts.iter().copied());
                poly_curves(&spline_points(pts, *closed, (ext.width().max(ext.height()) * 0.0005).max(1e-6)), false)
            }
            Shape::XLine { p, dir } => {
                let d = dir.norm();
                vec![Curve::Seg(*p - d * INFINITE, *p + d * INFINITE)]
            }
            Shape::Ray { p, dir } => vec![Curve::Seg(*p, *p + dir.norm() * INFINITE)],
            Shape::Leader(l) => poly_curves(&l.pts, false),
            _ => vec![],
        }
    }

    pub fn distance(&self, p: V2, ctx: &Ctx) -> f64 {
        self.prims(ctx).iter().map(|q| q.distance(p)).fold(f64::INFINITY, f64::min)
    }

    pub fn crosses(&self, bx: &BBox, ctx: &Ctx) -> bool {
        self.prims(ctx).iter().any(|q| q.crosses(bx))
    }

    pub fn nearest(&self, p: V2) -> Option<V2> {
        self.curves().iter().map(|c| c.nearest(p)).min_by(|a, b| a.dist(p).total_cmp(&b.dist(p)))
    }

    pub fn snap_points(&self) -> Vec<(SnapKind, V2)> {
        use SnapKind::*;
        match self {
            Shape::Line { a, b } => vec![(End, *a), (End, *b), (Mid, a.mid(*b))],
            Shape::Circle { c, r } => {
                let mut v = vec![(Center, *c)];
                for k in 0..4 {
                    v.push((Quadrant, *c + V2::polar(*r, k as f64 * FRAC_PI_2)));
                }
                v
            }
            Shape::Arc { c, r, start, end } => {
                let sw = arc_sweep(*start, *end);
                let mut v = vec![
                    (End, *c + V2::polar(*r, *start)),
                    (End, *c + V2::polar(*r, *end)),
                    (Mid, *c + V2::polar(*r, start + sw / 2.0)),
                    (Center, *c),
                ];
                for k in 0..4 {
                    let a = k as f64 * FRAC_PI_2;
                    if angle_on_arc(a, *start, *end) {
                        v.push((Quadrant, *c + V2::polar(*r, a)));
                    }
                }
                v
            }
            Shape::Polyline { pts, closed } => {
                let mut v: Vec<_> = pts.iter().map(|p| (End, *p)).collect();
                v.extend(poly_segments(pts, *closed).map(|(a, b)| (Mid, a.mid(b))));
                v
            }
            Shape::Text(t) => vec![(Insertion, t.pos)],
            Shape::Dim(d) => vec![(Node, d.p1), (Node, d.p2), (Node, d.loc)],
            Shape::Ellipse { c, major, ratio, start, end } => {
                let full = is_full_ellipse(*start, *end);
                let mut v = vec![(Center, *c)];
                for k in 0..4 {
                    let t = k as f64 * FRAC_PI_2;
                    if full || angle_on_arc(t, *start, *end) {
                        v.push((Quadrant, ellipse_point(*c, *major, *ratio, t)));
                    }
                }
                if !full {
                    v.push((End, ellipse_point(*c, *major, *ratio, *start)));
                    v.push((End, ellipse_point(*c, *major, *ratio, *end)));
                }
                v
            }
            Shape::Spline { pts, closed } => {
                let mut v: Vec<_> = pts.iter().map(|p| (Node, *p)).collect();
                if !closed && pts.len() >= 2 {
                    v.push((End, pts[0]));
                    v.push((End, pts[pts.len() - 1]));
                }
                v
            }
            Shape::Point { p } | Shape::XLine { p, .. } => vec![(Node, *p)],
            Shape::Ray { p, .. } => vec![(End, *p)],
            Shape::MText(m) => vec![(Insertion, m.pos)],
            Shape::Insert(i) => vec![(Insertion, i.pos)],
            Shape::Leader(l) => l.pts.iter().map(|p| (End, *p)).collect(),
            Shape::Image(i) => i.corners().into_iter().map(|p| (End, p)).collect(),
            Shape::Hatch(_) => vec![],
            Shape::Tolerance(f) => vec![(Insertion, f.pos)],
            Shape::Table(t) => vec![(Insertion, t.pos)],
            Shape::AttDef(a) => vec![(Insertion, a.text.pos)],
            Shape::Datum(d) => vec![(Insertion, d.pos), (End, d.tip)],
            Shape::Surface(s) => vec![(Insertion, s.pos)],
            Shape::Weld(w) => vec![(End, w.tip), (Insertion, w.elbow)],
        }
    }

    /// Grip points shown when the entity is selected.
    pub fn grips(&self) -> Vec<V2> {
        match self {
            Shape::Line { a, b } => vec![*a, *b, a.mid(*b)],
            Shape::Circle { c, r } => {
                let mut v = vec![*c];
                for k in 0..4 {
                    v.push(*c + V2::polar(*r, k as f64 * FRAC_PI_2));
                }
                v
            }
            Shape::Arc { c, r, start, end } => {
                let sw = arc_sweep(*start, *end);
                vec![*c, *c + V2::polar(*r, *start), *c + V2::polar(*r, *end), *c + V2::polar(*r, start + sw / 2.0)]
            }
            Shape::Polyline { pts, .. } | Shape::Spline { pts, .. } => pts.clone(),
            Shape::Text(t) => vec![t.pos],
            Shape::Dim(d) => {
                let mut v = vec![d.p1, d.p2, d.loc];
                if let Some(c) = d.center {
                    v.push(c);
                }
                v
            }
            Shape::Ellipse { c, major, ratio, .. } => {
                let minor = major.perp() * *ratio;
                vec![*c, *c + *major, *c + minor, *c - *major, *c - minor]
            }
            Shape::Point { p } => vec![*p],
            Shape::XLine { p, dir } | Shape::Ray { p, dir } => vec![*p, *p + dir.norm() * 10.0],
            Shape::MText(m) => vec![m.pos],
            Shape::Insert(i) => vec![i.pos],
            Shape::Leader(l) => l.pts.clone(),
            Shape::Image(i) => vec![i.pos, i.pos + V2::new(i.w, i.h)],
            Shape::Hatch(h) => {
                let b = BBox::of(h.loops.iter().flatten().copied());
                if b.is_empty() {
                    vec![]
                } else {
                    vec![b.center()]
                }
            }
            Shape::Tolerance(f) => std::iter::once(f.pos).chain(f.leader.iter().copied()).collect(),
            Shape::Table(t) => vec![t.pos],
            Shape::AttDef(a) => vec![a.text.pos],
            Shape::Datum(d) => vec![d.pos, d.tip],
            Shape::Surface(s) => vec![s.pos],
            Shape::Weld(w) => vec![w.tip, w.elbow],
        }
    }

    /// Move grip `i` to `p` ("stretch" grip behaviour).
    pub fn set_grip(&mut self, i: usize, p: V2) {
        match self {
            Shape::Line { a, b } => match i {
                0 => *a = p,
                1 => *b = p,
                _ => {
                    let d = p - a.mid(*b);
                    *a += d;
                    *b += d;
                }
            },
            Shape::Circle { c, r } => {
                if i == 0 {
                    *c = p
                } else {
                    *r = p.dist(*c).max(1e-6)
                }
            }
            Shape::Arc { c, r, start, end } => {
                let ps = *c + V2::polar(*r, *start);
                let pe = *c + V2::polar(*r, *end);
                let pm = *c + V2::polar(*r, *start + arc_sweep(*start, *end) / 2.0);
                let (a, m, b) = match i {
                    0 => {
                        *c = p;
                        return;
                    }
                    1 => (p, pm, pe),
                    2 => (ps, pm, p),
                    _ => (ps, p, pe),
                };
                if let Some((nc, nr, ns, ne)) = arc_3p(a, m, b) {
                    *c = nc;
                    *r = nr;
                    if angle_on_arc((m - nc).angle(), (a - nc).angle(), (b - nc).angle()) {
                        *start = norm_angle((a - nc).angle());
                        *end = norm_angle((b - nc).angle());
                    } else {
                        *start = ns;
                        *end = ne;
                    }
                }
            }
            Shape::Polyline { pts, .. } | Shape::Spline { pts, .. } => {
                if let Some(q) = pts.get_mut(i) {
                    *q = p
                }
            }
            Shape::Text(t) => t.pos = p,
            Shape::Dim(d) => match i {
                0 => d.p1 = p,
                1 => d.p2 = p,
                2 => d.loc = p,
                _ => d.center = Some(p),
            },
            Shape::Ellipse { c, major, ratio, .. } => {
                let minor_len = major.len() * *ratio;
                match i {
                    0 => *c = p,
                    1 | 3 => {
                        let v = if i == 1 { p - *c } else { *c - p };
                        if v.len() > 1e-9 {
                            *major = v;
                            *ratio = (minor_len / v.len()).clamp(1e-6, 1.0);
                        }
                    }
                    _ => {
                        let d = (p - *c).dot(major.norm().perp()).abs();
                        *ratio = (d / major.len().max(1e-9)).clamp(1e-6, 1.0);
                    }
                }
            }
            Shape::Point { p: q } => *q = p,
            Shape::XLine { p: q, dir } | Shape::Ray { p: q, dir } => {
                if i == 0 {
                    *q = p
                } else if p.dist(*q) > 1e-9 {
                    *dir = (p - *q).norm()
                }
            }
            Shape::MText(m) => m.pos = p,
            Shape::Insert(ins) => ins.pos = p,
            Shape::Leader(l) => {
                if let Some(q) = l.pts.get_mut(i) {
                    *q = p
                }
            }
            Shape::Image(img) => {
                if i == 0 {
                    img.pos = p
                } else {
                    let aspect = img.h / img.w.max(1e-9);
                    img.w = (p.x - img.pos.x).abs().max(1e-6);
                    img.h = img.w * aspect;
                }
            }
            Shape::Hatch(h) => {
                let b = BBox::of(h.loops.iter().flatten().copied());
                let d = p - b.center();
                h.loops.iter_mut().flatten().for_each(|q| *q += d);
            }
            Shape::Tolerance(f) => match i {
                0 => f.pos = p,
                _ => {
                    if let Some(q) = f.leader.get_mut(i - 1) {
                        *q = p
                    }
                }
            },
            Shape::Datum(d) => {
                if i == 0 {
                    d.pos = p
                } else {
                    d.tip = p
                }
            }
            Shape::Surface(s) => s.pos = p,
            Shape::Table(t) => t.pos = p,
            Shape::AttDef(a) => a.text.pos = p,
            Shape::Weld(w) => {
                if i == 0 {
                    w.tip = p
                } else {
                    w.elbow = p
                }
            }
        }
    }

    pub fn transform(&mut self, x: &Xform) {
        let k = x.scale_factor();
        let rot = x.apply_vec(V2::new(1.0, 0.0)).angle();
        match self {
            Shape::Line { a, b } => {
                *a = x.apply(*a);
                *b = x.apply(*b);
            }
            Shape::Circle { c, r } => {
                *c = x.apply(*c);
                *r *= k;
            }
            Shape::Arc { c, r, start, end } => {
                *c = x.apply(*c);
                *r *= k;
                let s = norm_angle(x.apply_angle(*start));
                let e = norm_angle(x.apply_angle(*end));
                if x.is_mirror() {
                    *start = e;
                    *end = s;
                } else {
                    *start = s;
                    *end = e;
                }
            }
            Shape::Polyline { pts, .. } | Shape::Spline { pts, .. } => pts.iter_mut().for_each(|p| *p = x.apply(*p)),
            Shape::Text(t) => {
                t.pos = x.apply(t.pos);
                t.height *= k;
                if !x.is_mirror() {
                    t.rotation = norm_angle(x.apply_angle(t.rotation));
                }
            }
            Shape::Dim(d) => {
                d.p1 = x.apply(d.p1);
                d.p2 = x.apply(d.p2);
                d.loc = x.apply(d.loc);
                d.center = d.center.map(|c| x.apply(c));
                if matches!(d.kind, DimKind::Horizontal | DimKind::Vertical) && (rot.abs() > 1e-6 && (rot.abs() - PI).abs() > 1e-6) {
                    let quarter = (rot.abs() - FRAC_PI_2).abs() < 1e-6;
                    d.kind = match (d.kind, quarter) {
                        (DimKind::Horizontal, true) => DimKind::Vertical,
                        (DimKind::Vertical, true) => DimKind::Horizontal,
                        _ => DimKind::Aligned,
                    };
                }
                if d.is_ordinate() && (rot.abs() - FRAC_PI_2).abs() < 1e-6 {
                    d.kind = if d.kind == DimKind::OrdinateX { DimKind::OrdinateY } else { DimKind::OrdinateX };
                }
                // A mirrored arc runs the other way round.
                if d.kind == DimKind::ArcLength && x.is_mirror() {
                    std::mem::swap(&mut d.p1, &mut d.p2);
                }
            }
            Shape::Ellipse { c, major, start, end, .. } => {
                *c = x.apply(*c);
                *major = x.apply_vec(*major);
                if x.is_mirror() && !is_full_ellipse(*start, *end) {
                    let (s, e) = (norm_angle(-*end), norm_angle(-*start));
                    *start = s;
                    *end = e;
                }
            }
            Shape::Point { p } => *p = x.apply(*p),
            Shape::XLine { p, dir } | Shape::Ray { p, dir } => {
                *p = x.apply(*p);
                *dir = x.apply_vec(*dir).norm();
            }
            Shape::MText(m) => {
                m.pos = x.apply(m.pos);
                m.height *= k;
                m.width *= k;
                if !x.is_mirror() {
                    m.rotation = norm_angle(x.apply_angle(m.rotation));
                }
            }
            Shape::Hatch(h) => {
                h.loops.iter_mut().flatten().for_each(|p| *p = x.apply(*p));
                h.scale *= k;
                if !x.is_mirror() {
                    h.angle = norm_angle(h.angle + rot);
                }
            }
            Shape::Insert(i) => {
                i.pos = x.apply(i.pos);
                i.sx *= k;
                i.sy *= k;
                i.rotation = norm_angle(x.apply_angle(i.rotation));
                if x.is_mirror() {
                    i.sy = -i.sy;
                }
            }
            Shape::Leader(l) => l.pts.iter_mut().for_each(|p| *p = x.apply(*p)),
            Shape::Image(img) => {
                let b = BBox::of(img.corners().map(|p| x.apply(p)));
                img.pos = b.min;
                img.w = b.width();
                img.h = b.height();
            }
            Shape::Table(t) => {
                t.pos = x.apply(t.pos);
                t.col_widths.iter_mut().for_each(|w| *w *= k);
                t.row_heights.iter_mut().for_each(|h| *h *= k);
                t.text_height *= k;
            }
            Shape::AttDef(a) => {
                a.text.pos = x.apply(a.text.pos);
                a.text.height *= k;
                if !x.is_mirror() {
                    a.text.rotation = norm_angle(x.apply_angle(a.text.rotation));
                }
            }
            // Symbols move with the geometry but keep their paper size.
            Shape::Tolerance(f) => {
                f.pos = x.apply(f.pos);
                f.leader.iter_mut().for_each(|p| *p = x.apply(*p));
            }
            Shape::Datum(d) => {
                d.pos = x.apply(d.pos);
                d.tip = x.apply(d.tip);
            }
            Shape::Surface(s) => {
                s.pos = x.apply(s.pos);
                if !x.is_mirror() {
                    s.rotation = norm_angle(x.apply_angle(s.rotation));
                }
            }
            Shape::Weld(w) => {
                w.tip = x.apply(w.tip);
                w.elbow = x.apply(w.elbow);
            }
        }
    }

    /// Explode into simpler shapes. Block references are exploded at the
    /// document level (they need layer information).
    pub fn explode(&self, ctx: &Ctx) -> Option<Vec<Shape>> {
        let from_prims = |prims: Vec<Prim>| -> Vec<Shape> {
            prims
                .into_iter()
                .filter_map(|p| match p {
                    Prim::Line(a, b) => Some(Shape::Line { a, b }),
                    Prim::Text(t) => Some(Shape::Text(t)),
                    Prim::Fill(pts) => Some(Shape::Hatch(Hatch { loops: vec![pts], pattern: "SOLID".into(), scale: 1.0, angle: 0.0 })),
                    Prim::Poly(pts, closed) => Some(Shape::Polyline { pts, closed }),
                    Prim::Arc { c, r, start, end } => Some(Shape::Arc { c, r, start, end }),
                    Prim::Circle(c, r) => Some(Shape::Circle { c, r }),
                    Prim::Image(_) => None,
                })
                .collect()
        };
        match self {
            Shape::Polyline { pts, closed } => Some(poly_segments(pts, *closed).map(|(a, b)| Shape::Line { a, b }).collect()),
            Shape::Dim(_)
            | Shape::Leader(_)
            | Shape::MText(_)
            | Shape::Tolerance(_)
            | Shape::Datum(_)
            | Shape::Surface(_)
            | Shape::Weld(_)
            | Shape::Table(_) => Some(from_prims(self.prims(ctx))),
            Shape::Hatch(h) if !h.is_solid() => Some(from_prims(self.prims(ctx))),
            Shape::Hatch(h) => Some(h.loops.iter().map(|l| Shape::Polyline { pts: l.clone(), closed: true }).collect()),
            _ => None,
        }
    }

    pub fn length(&self) -> Option<f64> {
        let poly_len = |pts: &[V2], closed: bool| poly_segments(pts, closed).map(|(a, b)| a.dist(b)).sum::<f64>();
        match self {
            Shape::Line { a, b } => Some(a.dist(*b)),
            Shape::Circle { r, .. } => Some(TAU * r),
            Shape::Arc { r, start, end, .. } => Some(r * arc_sweep(*start, *end)),
            Shape::Polyline { pts, closed } => Some(poly_len(pts, *closed)),
            Shape::Ellipse { .. } | Shape::Spline { .. } => {
                let ds = DimStyle::default();
                let ctx = Ctx::new(&ds, 1.0);
                match self.prims(&ctx).first() {
                    Some(Prim::Poly(p, c)) => Some(poly_len(p, *c)),
                    _ => None,
                }
            }
            Shape::Leader(l) => Some(poly_len(&l.pts, false)),
            _ => None,
        }
    }

    /// Enclosed area for closed shapes.
    pub fn area(&self) -> Option<f64> {
        match self {
            Shape::Circle { r, .. } => Some(PI * r * r),
            Shape::Polyline { pts, closed: true } => Some(hatch::signed_area(pts).abs()),
            Shape::Ellipse { major, ratio, start, end, .. } if is_full_ellipse(*start, *end) => Some(PI * major.len().powi(2) * ratio),
            Shape::Hatch(h) => Some(h.area()),
            _ => self.closed_loop().map(|l| hatch::signed_area(&l).abs()),
        }
    }

    /// Closed boundary loop (for HATCH "select objects" and AREA).
    pub fn closed_loop(&self) -> Option<Vec<V2>> {
        match self {
            Shape::Circle { c, r } => {
                let mut v = arc_points(*c, *r, 0.0, TAU, r * 0.001);
                v.pop();
                Some(v)
            }
            Shape::Polyline { pts, closed: true } => Some(pts.clone()),
            Shape::Ellipse { c, major, ratio, start, end } if is_full_ellipse(*start, *end) => {
                let mut v = ellipse_points(*c, *major, *ratio, 0.0, TAU, ellipse_tol(*major));
                v.pop();
                Some(v)
            }
            Shape::Spline { pts, closed: true } => {
                let ext = BBox::of(pts.iter().copied());
                let mut v = spline_points(pts, true, (ext.width().max(ext.height()) * 0.0005).max(1e-6));
                v.pop();
                Some(v)
            }
            _ => None,
        }
    }
}

fn leader_prims(l: &Leader, ctx: &Ctx) -> Vec<Prim> {
    let mut out = vec![];
    if l.pts.len() < 2 {
        return out;
    }
    if let Some(st) = l.style.as_ref().and_then(|n| ctx.mstyles.iter().find(|s| s.name.eq_ignore_ascii_case(n))) {
        return st.prims(&l.pts, &l.text, ctx.scale, ctx.ds.text_gap);
    }
    let s = ctx.scale.max(1e-9);
    let ar = ctx.ds.arrow_size * s;
    let th = ctx.ds.text_height * s;
    out.push(Prim::Poly(l.pts.clone(), false));
    out.push(arrow(l.pts[0], l.pts[0] - l.pts[1], ar));
    let last = l.pts[l.pts.len() - 1];
    let prev = l.pts[l.pts.len() - 2];
    let sx = if last.x >= prev.x { 1.0 } else { -1.0 };
    let land = last + V2::new(sx * ar, 0.0);
    out.push(Prim::Line(last, land));
    if !l.text.is_empty() {
        let h = if sx > 0.0 { HAlign::Left } else { HAlign::Right };
        let pitch = th * 1.667;
        for (i, line) in l.text.split('\n').enumerate() {
            out.push(Prim::Text(
                Text::new(land + V2::new(sx * ctx.ds.text_gap * s, -(i as f64) * pitch), th, line).aligned(h, VAlign::Middle),
            ));
        }
    }
    out
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Entity {
    pub id: u64,
    pub layer: String,
    #[serde(default)]
    pub color: Color,
    #[serde(default)]
    pub linetype: LtRef,
    /// Lineweight override in mm (None = ByLayer).
    #[serde(default)]
    pub lineweight: Option<f64>,
    pub shape: Shape,
}
