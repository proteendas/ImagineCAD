// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Proteen Das

//! Parametric drafting: geometric constraints (horizontal, parallel,
//! tangent, coincident…) and dimensional constraints (named driving
//! dimensions whose values may be expressions of each other) between lines,
//! polylines, circles and arcs, kept true by a least-squares solve that moves
//! the geometry as little as possible.

use crate::document::Document;
use crate::entity::Shape;
use crate::geom::*;
use serde::{Deserialize, Serialize};

/// A point of an entity: line 0 = start, 1 = end; circle 0 = centre; arc
/// 0 = centre, 1 = start, 2 = end; polyline k = vertex k.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct PtRef {
    pub id: u64,
    pub k: u16,
}

/// A straight segment: a line, or one segment of a polyline.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SegRef {
    pub a: PtRef,
    pub b: PtRef,
}

impl SegRef {
    pub fn line(id: u64) -> Self {
        Self { a: PtRef { id, k: 0 }, b: PtRef { id, k: 1 } }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum DimKind {
    /// Straight distance between two points.
    Aligned,
    Horizontal,
    Vertical,
    /// Radius or diameter of a circle or arc (`b` unused).
    Radius,
    Diameter,
    /// Angle from segment `s` to segment `t` (degrees).
    Angle,
}

/// A driving dimension: `name = expr` (e.g. `d2 = d1 / 2 + 5`).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct DimConstraint {
    pub name: String,
    pub kind: DimKind,
    /// Points (Aligned/Horizontal/Vertical), the curve's centre for Radius/Diameter.
    pub a: PtRef,
    pub b: PtRef,
    /// Segments of an Angle.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub s: Option<(SegRef, SegRef)>,
    pub expr: String,
    /// The value `expr` evaluated to last.
    pub value: f64,
    /// Where the dimension is shown.
    pub loc: V2,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum Constraint {
    Horizontal(SegRef),
    Vertical(SegRef),
    Parallel(SegRef, SegRef),
    Perpendicular(SegRef, SegRef),
    Collinear(SegRef, SegRef),
    EqualLength(SegRef, SegRef),
    /// Equal radii of two circles/arcs (by their centre points).
    EqualRadius(PtRef, PtRef),
    Coincident(PtRef, PtRef),
    /// Point on a segment's line.
    PointOnLine(PtRef, SegRef),
    /// Point on a circle/arc (by its centre).
    PointOnCircle(PtRef, PtRef),
    Concentric(PtRef, PtRef),
    /// Segment tangent to a circle/arc (by its centre).
    TangentLine(SegRef, PtRef),
    /// Two circles/arcs touching (outside unless `inside`).
    TangentCircles(PtRef, PtRef, bool),
    /// A point held where it is.
    Fix(PtRef, V2),
    Dim(DimConstraint),
}

impl Constraint {
    pub fn label(&self) -> &'static str {
        match self {
            Constraint::Horizontal(_) => "Horizontal",
            Constraint::Vertical(_) => "Vertical",
            Constraint::Parallel(..) => "Parallel",
            Constraint::Perpendicular(..) => "Perpendicular",
            Constraint::Collinear(..) => "Collinear",
            Constraint::EqualLength(..) | Constraint::EqualRadius(..) => "Equal",
            Constraint::Coincident(..) => "Coincident",
            Constraint::PointOnLine(..) | Constraint::PointOnCircle(..) => "Point on object",
            Constraint::Concentric(..) => "Concentric",
            Constraint::TangentLine(..) | Constraint::TangentCircles(..) => "Tangent",
            Constraint::Fix(..) => "Fix",
            Constraint::Dim(_) => "Dimension",
        }
    }

    /// Entity ids this constraint refers to.
    pub fn ids(&self) -> Vec<u64> {
        let s = |x: &SegRef| [x.a.id, x.b.id];
        match self {
            Constraint::Horizontal(a) | Constraint::Vertical(a) => s(a).to_vec(),
            Constraint::Parallel(a, b) | Constraint::Perpendicular(a, b) | Constraint::Collinear(a, b) | Constraint::EqualLength(a, b) => {
                [s(a), s(b)].concat()
            }
            Constraint::EqualRadius(p, q)
            | Constraint::Coincident(p, q)
            | Constraint::PointOnCircle(p, q)
            | Constraint::Concentric(p, q)
            | Constraint::TangentCircles(p, q, _) => vec![p.id, q.id],
            Constraint::PointOnLine(p, a) | Constraint::TangentLine(a, p) => [vec![p.id], s(a).to_vec()].concat(),
            Constraint::Fix(p, _) => vec![p.id],
            Constraint::Dim(d) => {
                let mut v = vec![d.a.id, d.b.id];
                if let Some((x, y)) = &d.s {
                    v.extend(s(x));
                    v.extend(s(y));
                }
                v
            }
        }
    }
}

// ------------------------------------------------------------------ expressions

/// Evaluate `expr` (numbers, names, + - * / ^, parentheses, sqrt, sin, cos,
/// tan in degrees, pi) with `lookup` for names.
pub fn eval(expr: &str, lookup: &dyn Fn(&str) -> Option<f64>) -> Result<f64, String> {
    struct P<'a> {
        s: &'a [u8],
        i: usize,
        lookup: &'a dyn Fn(&str) -> Option<f64>,
    }
    impl P<'_> {
        fn ws(&mut self) {
            while self.i < self.s.len() && self.s[self.i].is_ascii_whitespace() {
                self.i += 1;
            }
        }
        fn peek(&mut self) -> Option<u8> {
            self.ws();
            self.s.get(self.i).copied()
        }
        fn sum(&mut self) -> Result<f64, String> {
            let mut v = self.prod()?;
            while let Some(c) = self.peek() {
                match c {
                    b'+' => {
                        self.i += 1;
                        v += self.prod()?
                    }
                    b'-' => {
                        self.i += 1;
                        v -= self.prod()?
                    }
                    _ => break,
                }
            }
            Ok(v)
        }
        fn prod(&mut self) -> Result<f64, String> {
            let mut v = self.pow()?;
            while let Some(c) = self.peek() {
                match c {
                    b'*' => {
                        self.i += 1;
                        v *= self.pow()?
                    }
                    b'/' => {
                        self.i += 1;
                        let d = self.pow()?;
                        if d == 0.0 {
                            return Err("division by zero".into());
                        }
                        v /= d
                    }
                    _ => break,
                }
            }
            Ok(v)
        }
        fn pow(&mut self) -> Result<f64, String> {
            let b = self.unary()?;
            if self.peek() == Some(b'^') {
                self.i += 1;
                return Ok(b.powf(self.pow()?));
            }
            Ok(b)
        }
        fn unary(&mut self) -> Result<f64, String> {
            match self.peek() {
                Some(b'-') => {
                    self.i += 1;
                    Ok(-self.unary()?)
                }
                Some(b'+') => {
                    self.i += 1;
                    self.unary()
                }
                _ => self.atom(),
            }
        }
        fn atom(&mut self) -> Result<f64, String> {
            match self.peek() {
                Some(b'(') => {
                    self.i += 1;
                    let v = self.sum()?;
                    if self.peek() != Some(b')') {
                        return Err("missing )".into());
                    }
                    self.i += 1;
                    Ok(v)
                }
                Some(c) if c.is_ascii_digit() || c == b'.' => {
                    let st = self.i;
                    while self.i < self.s.len() && (self.s[self.i].is_ascii_digit() || self.s[self.i] == b'.') {
                        self.i += 1;
                    }
                    std::str::from_utf8(&self.s[st..self.i]).unwrap().parse().map_err(|_| "bad number".into())
                }
                Some(c) if c.is_ascii_alphabetic() || c == b'_' => {
                    let st = self.i;
                    while self.i < self.s.len() && (self.s[self.i].is_ascii_alphanumeric() || self.s[self.i] == b'_') {
                        self.i += 1;
                    }
                    let name = std::str::from_utf8(&self.s[st..self.i]).unwrap().to_string();
                    if self.peek() == Some(b'(') {
                        self.i += 1;
                        let a = self.sum()?;
                        if self.peek() != Some(b')') {
                            return Err("missing )".into());
                        }
                        self.i += 1;
                        return match name.to_ascii_lowercase().as_str() {
                            "sqrt" => Ok(a.sqrt()),
                            "sin" => Ok(a.to_radians().sin()),
                            "cos" => Ok(a.to_radians().cos()),
                            "tan" => Ok(a.to_radians().tan()),
                            "abs" => Ok(a.abs()),
                            f => Err(format!("unknown function {f}")),
                        };
                    }
                    if name.eq_ignore_ascii_case("pi") {
                        return Ok(std::f64::consts::PI);
                    }
                    (self.lookup)(&name).ok_or_else(|| format!("unknown name {name}"))
                }
                _ => Err("expression expected".into()),
            }
        }
    }
    let mut p = P { s: expr.as_bytes(), i: 0, lookup };
    let v = p.sum()?;
    if p.peek().is_some() {
        return Err(format!("unexpected text at {}", p.i));
    }
    if !v.is_finite() {
        return Err("not a number".into());
    }
    Ok(v)
}

// ------------------------------------------------------------------ geometry access

/// Unknowns: every referenced point (x, y) and every circle's radius.
struct Vars {
    pts: Vec<PtRef>,
    radii: Vec<u64>,
    x: Vec<f64>,
}

impl Vars {
    fn pt(&self, x: &[f64], p: PtRef) -> V2 {
        let i = self.pts.iter().position(|q| *q == p).unwrap_or(0);
        V2::new(x[2 * i], x[2 * i + 1])
    }
    /// Radius of a circle (its own unknown) or an arc (distance to its start).
    fn radius(&self, x: &[f64], c: PtRef) -> f64 {
        match self.radii.iter().position(|id| *id == c.id) {
            Some(j) => x[2 * self.pts.len() + j],
            None => self.pt(x, c).dist(self.pt(x, PtRef { id: c.id, k: 1 })),
        }
    }
}

fn point_of(shape: &Shape, k: u16) -> Option<V2> {
    match shape {
        Shape::Line { a, b } => [*a, *b].get(k as usize).copied(),
        Shape::Polyline { pts, .. } => pts.get(k as usize).copied(),
        Shape::Circle { c, .. } => (k == 0).then_some(*c),
        Shape::Arc { c, r, start, end } => match k {
            0 => Some(*c),
            1 => Some(*c + V2::polar(*r, *start)),
            2 => Some(*c + V2::polar(*r, *end)),
            _ => None,
        },
        _ => None,
    }
}

fn dir(a: V2, b: V2) -> V2 {
    let d = b - a;
    let l = d.len();
    if l < 1e-12 {
        V2::new(1.0, 0.0)
    } else {
        d / l
    }
}

fn wrap(a: f64) -> f64 {
    let mut a = a;
    while a > std::f64::consts::PI {
        a -= std::f64::consts::TAU;
    }
    while a < -std::f64::consts::PI {
        a += std::f64::consts::TAU;
    }
    a
}

/// Residuals of one constraint (lengths; angles scaled by `scale`).
fn residuals(c: &Constraint, v: &Vars, x: &[f64], scale: f64) -> Vec<f64> {
    let p = |r: PtRef| v.pt(x, r);
    let seg = |s: &SegRef| (p(s.a), p(s.b));
    match c {
        Constraint::Horizontal(s) => {
            let (a, b) = seg(s);
            vec![b.y - a.y]
        }
        Constraint::Vertical(s) => {
            let (a, b) = seg(s);
            vec![b.x - a.x]
        }
        Constraint::Parallel(s, t) => {
            let ((a, b), (c, d)) = (seg(s), seg(t));
            vec![dir(a, b).cross(dir(c, d)) * scale]
        }
        Constraint::Perpendicular(s, t) => {
            let ((a, b), (c, d)) = (seg(s), seg(t));
            vec![dir(a, b).dot(dir(c, d)) * scale]
        }
        Constraint::Collinear(s, t) => {
            let ((a, b), (c, d)) = (seg(s), seg(t));
            let u = dir(a, b);
            vec![(c - a).cross(u), (d - a).cross(u)]
        }
        Constraint::EqualLength(s, t) => {
            let ((a, b), (c, d)) = (seg(s), seg(t));
            vec![a.dist(b) - c.dist(d)]
        }
        Constraint::EqualRadius(a, b) => vec![v.radius(x, *a) - v.radius(x, *b)],
        Constraint::Coincident(a, b) | Constraint::Concentric(a, b) => {
            let (q, r) = (p(*a), p(*b));
            vec![r.x - q.x, r.y - q.y]
        }
        Constraint::PointOnLine(q, s) => {
            let (a, b) = seg(s);
            vec![(p(*q) - a).cross(dir(a, b))]
        }
        Constraint::PointOnCircle(q, c) => vec![p(*q).dist(p(*c)) - v.radius(x, *c)],
        Constraint::TangentLine(s, c) => {
            let (a, b) = seg(s);
            vec![(p(*c) - a).cross(dir(a, b)).abs() - v.radius(x, *c)]
        }
        Constraint::TangentCircles(a, b, inside) => {
            let d = p(*a).dist(p(*b));
            let (r1, r2) = (v.radius(x, *a), v.radius(x, *b));
            vec![if *inside { d - (r1 - r2).abs() } else { d - (r1 + r2) }]
        }
        Constraint::Fix(q, at) => {
            let r = p(*q);
            vec![r.x - at.x, r.y - at.y]
        }
        Constraint::Dim(d) => {
            let (a, b) = (p(d.a), p(d.b));
            match d.kind {
                DimKind::Aligned => vec![a.dist(b) - d.value],
                DimKind::Horizontal => vec![(b.x - a.x).abs() - d.value],
                DimKind::Vertical => vec![(b.y - a.y).abs() - d.value],
                DimKind::Radius => vec![v.radius(x, d.a) - d.value],
                DimKind::Diameter => vec![2.0 * v.radius(x, d.a) - d.value],
                DimKind::Angle => match &d.s {
                    Some((s, t)) => {
                        let ((a, b), (c, e)) = (seg(s), seg(t));
                        vec![
                            wrap((e - c).angle() - (b - a).angle() - d.value.to_radians())
                                .abs()
                                .min(wrap((e - c).angle() - (b - a).angle() + d.value.to_radians()).abs())
                                * scale,
                        ]
                    }
                    None => vec![],
                },
            }
        }
    }
}

/// Built-in relation of an arc: both ends on one circle.
fn arc_residuals(v: &Vars, x: &[f64], doc: &Document) -> Vec<f64> {
    let mut out = vec![];
    for id in v.pts.iter().map(|p| p.id).collect::<std::collections::BTreeSet<_>>() {
        if matches!(doc.get(id).map(|e| &e.shape), Some(Shape::Arc { .. })) {
            let c = v.pt(x, PtRef { id, k: 0 });
            out.push(c.dist(v.pt(x, PtRef { id, k: 1 })) - c.dist(v.pt(x, PtRef { id, k: 2 })));
        }
    }
    out
}

/// What a solve did.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct SolveReport {
    /// Constraints that could not be met (indices into `doc.constraints`).
    pub unmet: Vec<usize>,
    /// Remaining degrees of freedom of the constrained geometry.
    pub dof: usize,
    /// Expressions that could not be evaluated (name, message).
    pub bad_exprs: Vec<(String, String)>,
}

impl Document {
    /// Name for the next driving dimension (d1, d2, …).
    pub fn next_dim_name(&self) -> String {
        let mut k = 1;
        while self.constraints.iter().any(|c| matches!(c, Constraint::Dim(d) if d.name.eq_ignore_ascii_case(&format!("d{k}")))) {
            k += 1;
        }
        format!("d{k}")
    }

    /// Value of driving dimension `name`.
    pub fn param(&self, name: &str) -> Option<f64> {
        self.constraints.iter().find_map(|c| match c {
            Constraint::Dim(d) if d.name.eq_ignore_ascii_case(name) => Some(d.value),
            _ => None,
        })
    }

    /// Current measurement of a driving dimension's geometry.
    pub fn measure(&self, d: &DimConstraint) -> Option<f64> {
        let pt = |r: PtRef| self.get(r.id).and_then(|e| point_of(&e.shape, r.k));
        let radius = |c: PtRef| match self.get(c.id).map(|e| &e.shape) {
            Some(Shape::Circle { r, .. }) | Some(Shape::Arc { r, .. }) => Some(*r),
            _ => None,
        };
        Some(match d.kind {
            DimKind::Aligned => pt(d.a)?.dist(pt(d.b)?),
            DimKind::Horizontal => (pt(d.b)?.x - pt(d.a)?.x).abs(),
            DimKind::Vertical => (pt(d.b)?.y - pt(d.a)?.y).abs(),
            DimKind::Radius => radius(d.a)?,
            DimKind::Diameter => 2.0 * radius(d.a)?,
            DimKind::Angle => {
                let (s, t) = d.s.as_ref()?;
                let a = (pt(s.b)? - pt(s.a)?).angle();
                let b = (pt(t.b)? - pt(t.a)?).angle();
                wrap(b - a).abs().to_degrees()
            }
        })
    }

    /// Drop constraints whose geometry no longer exists.
    pub fn prune_constraints(&mut self) {
        let ok = |doc: &Document, c: &Constraint| c.ids().iter().all(|id| doc.get(*id).is_some_and(|e| point_of(&e.shape, 0).is_some()));
        let keep: Vec<bool> = self.constraints.iter().map(|c| ok(self, c)).collect();
        let mut i = 0;
        self.constraints.retain(|_| {
            i += 1;
            keep[i - 1]
        });
    }

    /// Re-evaluate the expressions, then move the geometry so that every
    /// constraint holds, changing it as little as possible. Points in
    /// `prefer` (e.g. a dragged grip) are held more firmly than the rest.
    pub fn solve_constraints(&mut self, prefer: &[PtRef]) -> SolveReport {
        // First try to keep the preferred points exactly where they are.
        if !prefer.is_empty() {
            let saved = (self.entities.clone(), self.constraints.clone());
            let n = self.constraints.len();
            for p in prefer {
                if let Some(at) = self.get(p.id).and_then(|e| point_of(&e.shape, p.k)) {
                    self.constraints.push(Constraint::Fix(*p, at));
                }
            }
            let held = self.solve_free(&[]);
            self.constraints.truncate(n);
            if held.unmet.is_empty() && held.bad_exprs.is_empty() {
                // Freedom without the temporary holds (the geometry is solved already).
                return self.solve_free(&[]);
            }
            (self.entities, self.constraints) = saved;
        }
        self.solve_free(prefer)
    }

    fn solve_free(&mut self, prefer: &[PtRef]) -> SolveReport {
        let mut report = SolveReport::default();
        self.prune_constraints();
        if self.constraints.is_empty() {
            return report;
        }
        // Expressions, in order, so names may use earlier ones (a few passes for forward references).
        for _ in 0..3 {
            report.bad_exprs.clear();
            for i in 0..self.constraints.len() {
                let Constraint::Dim(d) = &self.constraints[i] else { continue };
                let (name, expr) = (d.name.clone(), d.expr.clone());
                let doc = &*self;
                match eval(&expr, &|n: &str| if n.eq_ignore_ascii_case(&name) { None } else { doc.param(n) }) {
                    Ok(v) if v > 0.0 => {
                        if let Constraint::Dim(d) = &mut self.constraints[i] {
                            d.value = v;
                        }
                    }
                    Ok(v) => report.bad_exprs.push((name, format!("{v} is not a positive size"))),
                    Err(e) => report.bad_exprs.push((name, e)),
                }
            }
        }
        // Unknowns: every point of every constrained entity (so the whole
        // entity moves together), plus circle radii.
        let mut ids: Vec<u64> = self.constraints.iter().flat_map(|c| c.ids()).collect();
        ids.sort_unstable();
        ids.dedup();
        let mut v = Vars { pts: vec![], radii: vec![], x: vec![] };
        for id in &ids {
            let Some(e) = self.get(*id) else { continue };
            let n = match &e.shape {
                Shape::Line { .. } => 2,
                Shape::Polyline { pts, .. } => pts.len(),
                Shape::Circle { .. } => 1,
                Shape::Arc { .. } => 3,
                _ => 0,
            };
            for k in 0..n {
                if let Some(p) = point_of(&e.shape, k as u16) {
                    v.pts.push(PtRef { id: *id, k: k as u16 });
                    v.x.extend([p.x, p.y]);
                }
            }
        }
        for id in &ids {
            if let Some(Shape::Circle { r, .. }) = self.get(*id).map(|e| &e.shape) {
                v.radii.push(*id);
                v.x.push(*r);
            }
        }
        let ext = v.pts.iter().map(|p| v.pt(&v.x, *p)).fold(BBox::empty(), |mut b, p| {
            b.add(p);
            b
        });
        let scale = ext.width().max(ext.height()).max(1.0);
        let start = v.x.clone();
        let weights: Vec<f64> =
            (0..v.x.len()).map(|i| if i < 2 * v.pts.len() && prefer.contains(&v.pts[i / 2]) { 1e-1 } else { 1e-4 }).collect();
        let cons = self.constraints.clone();
        let doc = &*self;
        let all = |x: &[f64]| -> Vec<f64> {
            let mut r: Vec<f64> = cons.iter().flat_map(|c| residuals(c, &v, x, scale)).collect();
            r.extend(arc_residuals(&v, x, doc));
            r
        };
        let x = least_squares(&all, &start, &weights, scale);
        // Unmet constraints and the remaining freedom.
        let tol = (scale * 1e-6).max(1e-7);
        for (i, c) in cons.iter().enumerate() {
            if residuals(c, &v, &x, scale).iter().any(|r| r.abs() > tol) {
                report.unmet.push(i);
            }
        }
        report.dof = v.x.len().saturating_sub(rank(&all, &x, scale));
        // Write the geometry back.
        let n_pts = v.pts.len();
        for id in ids {
            let pts: Vec<(u16, V2)> =
                v.pts.iter().enumerate().filter(|(_, p)| p.id == id).map(|(i, p)| (p.k, V2::new(x[2 * i], x[2 * i + 1]))).collect();
            let rad = v.radii.iter().position(|r| *r == id).map(|j| x[2 * n_pts + j]);
            let Some(e) = self.get_mut(id) else { continue };
            let at = |k: u16| pts.iter().find(|p| p.0 == k).map(|p| p.1);
            match &mut e.shape {
                Shape::Line { a, b } => {
                    *a = at(0).unwrap_or(*a);
                    *b = at(1).unwrap_or(*b);
                }
                Shape::Polyline { pts: vs, .. } => {
                    for (k, q) in vs.iter_mut().enumerate() {
                        *q = at(k as u16).unwrap_or(*q);
                    }
                }
                Shape::Circle { c, r } => {
                    *c = at(0).unwrap_or(*c);
                    *r = rad.unwrap_or(*r).max(1e-9);
                }
                Shape::Arc { c, r, start, end } => {
                    let (cc, p1, p2) = (at(0).unwrap_or(*c), at(1), at(2));
                    if let (Some(p1), Some(p2)) = (p1, p2) {
                        *c = cc;
                        *r = (cc.dist(p1) + cc.dist(p2)) / 2.0;
                        *start = norm_angle((p1 - cc).angle());
                        *end = norm_angle((p2 - cc).angle());
                    }
                }
                _ => {}
            }
        }
        report
    }

    /// Add `c` if it can be met together with the others; otherwise leave
    /// the drawing unchanged and say why.
    pub fn add_constraint(&mut self, c: Constraint) -> Result<SolveReport, String> {
        let before = (self.entities.clone(), self.constraints.clone());
        // A driving dimension keeps its first point and moves the second.
        let hold: Vec<PtRef> = if let Constraint::Dim(d) = &c { vec![d.a] } else { vec![] };
        self.constraints.push(c);
        let r = self.solve_constraints(&hold);
        if r.unmet.contains(&(self.constraints.len() - 1)) || !r.unmet.is_empty() {
            (self.entities, self.constraints) = before;
            return Err("That constraint conflicts with the existing constraints (over-constrained); it was not added.".into());
        }
        Ok(r)
    }
}

/// Damped Gauss–Newton with a weak pull to the start (re-anchored as it goes,
/// so the constraints end up met exactly while the geometry moves least).
#[allow(clippy::needless_range_loop)]
fn least_squares(f: &dyn Fn(&[f64]) -> Vec<f64>, x0: &[f64], w: &[f64], scale: f64) -> Vec<f64> {
    let n = x0.len();
    let mut x = x0.to_vec();
    let mut anchor = x0.to_vec();
    let cost = |x: &[f64], anchor: &[f64]| -> f64 {
        f(x).iter().map(|r| r * r).sum::<f64>() + x.iter().zip(anchor).zip(w).map(|((a, b), w)| w * (a - b).powi(2)).sum::<f64>()
    };
    let mut lambda = 1e-3;
    let mut current = cost(&x, &anchor);
    let h = (scale * 1e-7).max(1e-9);
    for iter in 0..300 {
        let r0 = f(&x);
        let m = r0.len();
        if r0.iter().all(|r| r.abs() < scale * 1e-11) {
            break;
        }
        // Jacobian by central differences.
        let mut jac = vec![vec![0.0; n]; m];
        for j in 0..n {
            let (mut xp, mut xm) = (x.clone(), x.clone());
            xp[j] += h;
            xm[j] -= h;
            let (fp, fm) = (f(&xp), f(&xm));
            for i in 0..m {
                jac[i][j] = (fp[i] - fm[i]) / (2.0 * h);
            }
        }
        let mut jtj = vec![vec![0.0; n]; n];
        let mut jtr = vec![0.0; n];
        for i in 0..m {
            for a in 0..n {
                if jac[i][a] == 0.0 {
                    continue;
                }
                jtr[a] += jac[i][a] * r0[i];
                for b in 0..n {
                    jtj[a][b] += jac[i][a] * jac[i][b];
                }
            }
        }
        for a in 0..n {
            jtj[a][a] += w[a];
            jtr[a] += w[a] * (x[a] - anchor[a]);
        }
        let mut improved = false;
        for _ in 0..10 {
            let mut aa = jtj.clone();
            for (i, row) in aa.iter_mut().enumerate() {
                row[i] += lambda * (1.0 + row[i]);
            }
            let Some(d) = solve_dense(aa, jtr.iter().map(|v| -v).collect()) else { break };
            let trial: Vec<f64> = x.iter().zip(&d).map(|(a, b)| a + b).collect();
            let c = cost(&trial, &anchor);
            if c < current {
                x = trial;
                current = c;
                lambda = (lambda * 0.3).max(1e-12);
                improved = true;
                break;
            }
            lambda *= 10.0;
        }
        if !improved || iter % 25 == 24 {
            if anchor == x && !improved {
                break;
            }
            anchor = x.clone();
            current = cost(&x, &anchor);
            lambda = 1e-3;
        }
    }
    x
}

/// Rank of the constraint Jacobian at `x` (independent equations).
#[allow(clippy::needless_range_loop)]
fn rank(f: &dyn Fn(&[f64]) -> Vec<f64>, x: &[f64], scale: f64) -> usize {
    let n = x.len();
    let h = (scale * 1e-7).max(1e-9);
    let m = f(x).len();
    let mut a = vec![vec![0.0; n]; m];
    for j in 0..n {
        let (mut xp, mut xm) = (x.to_vec(), x.to_vec());
        xp[j] += h;
        xm[j] -= h;
        let (fp, fm) = (f(&xp), f(&xm));
        for i in 0..m {
            a[i][j] = (fp[i] - fm[i]) / (2.0 * h);
        }
    }
    let mut r = 0;
    for col in 0..n {
        let Some(piv) = (r..m).max_by(|i, j| a[*i][col].abs().total_cmp(&a[*j][col].abs())) else { break };
        if a[piv][col].abs() < 1e-7 {
            continue;
        }
        a.swap(r, piv);
        for i in r + 1..m {
            let k = a[i][col] / a[r][col];
            for c in col..n {
                a[i][c] -= k * a[r][c];
            }
        }
        r += 1;
        if r == m {
            break;
        }
    }
    r
}

#[allow(clippy::needless_range_loop)]
fn solve_dense(mut a: Vec<Vec<f64>>, mut b: Vec<f64>) -> Option<Vec<f64>> {
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

#[cfg(test)]
mod tests {
    use super::*;

    fn rect(doc: &mut Document) -> [u64; 4] {
        // A sloppy rectangle of four lines.
        let l = |doc: &mut Document, a: (f64, f64), b: (f64, f64)| doc.add(Shape::Line { a: V2::new(a.0, a.1), b: V2::new(b.0, b.1) });
        [
            l(doc, (0.0, 0.0), (98.0, 3.0)),
            l(doc, (98.0, 3.0), (101.0, 52.0)),
            l(doc, (101.0, 52.0), (2.0, 49.0)),
            l(doc, (2.0, 49.0), (0.0, 0.0)),
        ]
    }

    #[test]
    fn expressions() {
        let names = |n: &str| if n == "d1" { Some(40.0) } else { None };
        assert_eq!(eval("d1 / 2 + 5", &names).unwrap(), 25.0);
        assert_eq!(eval("-(2 + 3) * 2 ^ 2", &names).unwrap(), -20.0);
        assert!((eval("sqrt(d1*d1/4)", &names).unwrap() - 20.0).abs() < 1e-12);
        assert!(eval("d9 + 1", &names).is_err());
        assert!(eval("1 +", &names).is_err());
    }

    #[test]
    fn parametric_rectangle() {
        let mut doc = Document::new();
        let [b, r, t, l] = rect(&mut doc);
        let end = |id, k| PtRef { id, k };
        for (p, q) in [(end(b, 1), end(r, 0)), (end(r, 1), end(t, 0)), (end(t, 1), end(l, 0)), (end(l, 1), end(b, 0))] {
            doc.add_constraint(Constraint::Coincident(p, q)).unwrap();
        }
        doc.add_constraint(Constraint::Horizontal(SegRef::line(b))).unwrap();
        doc.add_constraint(Constraint::Horizontal(SegRef::line(t))).unwrap();
        doc.add_constraint(Constraint::Vertical(SegRef::line(l))).unwrap();
        doc.add_constraint(Constraint::Vertical(SegRef::line(r))).unwrap();
        doc.add_constraint(Constraint::Fix(end(b, 0), V2::ZERO)).unwrap();
        let dim = |name: &str, kind: DimKind, a: PtRef, b2: PtRef, expr: &str| {
            Constraint::Dim(DimConstraint { name: name.into(), kind, a, b: b2, s: None, expr: expr.into(), value: 0.0, loc: V2::ZERO })
        };
        doc.add_constraint(dim("d1", DimKind::Horizontal, end(b, 0), end(b, 1), "120")).unwrap();
        let rep = doc.add_constraint(dim("d2", DimKind::Vertical, end(r, 0), end(r, 1), "d1 / 2")).unwrap();
        assert_eq!(rep.dof, 0, "fully constrained");
        fn p(doc: &Document, id: u64) -> (V2, V2) {
            match doc.get(id).unwrap().shape {
                Shape::Line { a, b } => (a, b),
                _ => unreachable!(),
            }
        }
        let (a, c) = (p(&doc, b).0, p(&doc, t).0);
        assert!(a.len() < 1e-6 && (c.x - 120.0).abs() < 1e-6 && (c.y - 60.0).abs() < 1e-6, "{a:?} {c:?}");
        // Driving the width resizes the height through the expression.
        if let Some(Constraint::Dim(d)) = doc.constraints.iter_mut().find(|c| matches!(c, Constraint::Dim(d) if d.name == "d1")) {
            d.expr = "200".into();
        }
        let rep = doc.solve_constraints(&[]);
        assert!(rep.unmet.is_empty());
        let c = p(&doc, t).0;
        assert!((c.x - 200.0).abs() < 1e-6 && (c.y - 100.0).abs() < 1e-6, "{c:?}");
        // A contradicting constraint is refused and changes nothing.
        let before = doc.entities.clone();
        assert!(doc.add_constraint(dim("d3", DimKind::Vertical, end(l, 0), end(l, 1), "30")).is_err());
        assert_eq!(doc.entities, before);
    }

    #[test]
    fn tangent_and_equal_circles() {
        let mut doc = Document::new();
        let line = doc.add(Shape::Line { a: V2::new(0.0, 0.0), b: V2::new(100.0, 0.0) });
        let c1 = doc.add(Shape::Circle { c: V2::new(30.0, 12.0), r: 8.0 });
        let c2 = doc.add(Shape::Circle { c: V2::new(70.0, 15.0), r: 14.0 });
        let ctr = |id| PtRef { id, k: 0 };
        doc.add_constraint(Constraint::TangentLine(SegRef::line(line), ctr(c1))).unwrap();
        doc.add_constraint(Constraint::TangentLine(SegRef::line(line), ctr(c2))).unwrap();
        doc.add_constraint(Constraint::EqualRadius(ctr(c1), ctr(c2))).unwrap();
        doc.add_constraint(Constraint::TangentCircles(ctr(c1), ctr(c2), false)).unwrap();
        let get = |id| match doc.get(id).unwrap().shape {
            Shape::Circle { c, r } => (c, r),
            _ => unreachable!(),
        };
        let ((p, r1), (q, r2)) = (get(c1), get(c2));
        // The line may move too: measure to where it is now.
        let Shape::Line { a, b } = doc.get(line).unwrap().shape else { unreachable!() };
        let off = |c: V2| (c - a).cross((b - a).norm()).abs();
        assert!((r1 - r2).abs() < 1e-6 && (off(p) - r1).abs() < 1e-6 && (off(q) - r2).abs() < 1e-6, "{p:?} {r1} {q:?} {r2}");
        assert!((p.dist(q) - 2.0 * r1).abs() < 1e-6);
    }
}
