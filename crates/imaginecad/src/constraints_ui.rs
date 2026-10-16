// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Proteen Das

//! Parametric drafting in the editor: geometric constraint commands
//! (GCHORIZONTAL, GCPARALLEL, GCTANGENT…), AUTOCONSTRAIN, driving dimensions
//! (DCHORIZONTAL, DCALIGNED, DCRADIUS, DCANGULAR…), the Parameters manager,
//! constraint display on the canvas, and solving after every edit.

use crate::app::App;
use crate::canvas::{prim_shapes, Style};
use crate::commands::{fmt, Command, Cx, Flow, Want};
use crate::theme;
use crate::view::View;
use eframe::egui::{self, Align2, Color32, FontId, Painter, RichText, Stroke};
use icad_core::constraints::{Constraint, DimConstraint, DimKind, PtRef, SegRef};
use icad_core::entity::{Dimension, Shape};
use icad_core::geom::*;
use icad_core::Document;

/// Constraint colour on the canvas.
const CONSTRAINT: Color32 = Color32::from_rgb(80, 200, 200);

// ------------------------------------------------------------------ picking

/// Points of an entity a constraint can use, with their index.
fn points_of(s: &Shape) -> Vec<(u16, V2)> {
    match s {
        Shape::Line { a, b } => vec![(0, *a), (1, *b)],
        Shape::Polyline { pts, .. } => pts.iter().enumerate().map(|(k, p)| (k as u16, *p)).collect(),
        Shape::Circle { c, .. } => vec![(0, *c)],
        Shape::Arc { c, r, start, end } => vec![(0, *c), (1, *c + V2::polar(*r, *start)), (2, *c + V2::polar(*r, *end))],
        _ => vec![],
    }
}

/// The constraint point nearest `p` within `tol`.
pub fn pick_point(doc: &Document, p: V2, tol: f64) -> Option<(PtRef, V2)> {
    doc.entities
        .iter()
        .filter(|e| doc.is_visible(e))
        .flat_map(|e| points_of(&e.shape).into_iter().map(move |(k, q)| (PtRef { id: e.id, k }, q)))
        .filter(|(_, q)| q.dist(p) <= tol)
        .min_by(|a, b| a.1.dist(p).total_cmp(&b.1.dist(p)))
}

/// The straight segment of entity `id` nearest `p` (a line, or a polyline segment).
pub fn segment_of(doc: &Document, id: u64, p: V2) -> Option<SegRef> {
    match &doc.get(id)?.shape {
        Shape::Line { .. } => Some(SegRef::line(id)),
        Shape::Polyline { pts, closed } => {
            let n = pts.len();
            let segs = (0..n.saturating_sub(1)).map(|k| (k, k + 1)).chain((*closed && n > 2).then_some((n - 1, 0)));
            segs.min_by(|a, b| dist_point_segment(p, pts[a.0], pts[a.1]).total_cmp(&dist_point_segment(p, pts[b.0], pts[b.1])))
                .map(|(i, j)| SegRef { a: PtRef { id, k: i as u16 }, b: PtRef { id, k: j as u16 } })
        }
        _ => None,
    }
}

/// The centre reference of a circle or arc.
fn curve_of(doc: &Document, id: u64) -> Option<PtRef> {
    matches!(doc.get(id)?.shape, Shape::Circle { .. } | Shape::Arc { .. }).then_some(PtRef { id, k: 0 })
}

fn pt(doc: &Document, r: PtRef) -> Option<V2> {
    doc.get(r.id).and_then(|e| points_of(&e.shape).into_iter().find(|(k, _)| *k == r.k).map(|(_, p)| p))
}

fn seg_pts(doc: &Document, s: &SegRef) -> Option<(V2, V2)> {
    Some((pt(doc, s.a)?, pt(doc, s.b)?))
}

// ------------------------------------------------------------------ geometric constraint commands

#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Geo {
    Horizontal,
    Vertical,
    Parallel,
    Perpendicular,
    Collinear,
    Equal,
    Coincident,
    Concentric,
    Tangent,
    Fix,
}

impl Geo {
    fn name(self) -> &'static str {
        match self {
            Geo::Horizontal => "GCHORIZONTAL",
            Geo::Vertical => "GCVERTICAL",
            Geo::Parallel => "GCPARALLEL",
            Geo::Perpendicular => "GCPERPENDICULAR",
            Geo::Collinear => "GCCOLLINEAR",
            Geo::Equal => "GCEQUAL",
            Geo::Coincident => "GCCOINCIDENT",
            Geo::Concentric => "GCCONCENTRIC",
            Geo::Tangent => "GCTANGENT",
            Geo::Fix => "GCFIX",
        }
    }
    /// Picks needed and whether they are points (else objects).
    fn picks(self) -> (usize, bool) {
        match self {
            Geo::Horizontal | Geo::Vertical => (1, false),
            Geo::Fix => (1, true),
            Geo::Coincident => (2, true),
            _ => (2, false),
        }
    }
}

pub struct GeoCmd {
    kind: Geo,
    /// Picked (entity id, pick point).
    picked: Vec<(u64, V2)>,
    points: Vec<PtRef>,
}

impl GeoCmd {
    pub fn new(kind: Geo) -> Self {
        Self { kind, picked: vec![], points: vec![] }
    }

    fn build(&self, doc: &Document) -> Result<Constraint, String> {
        let seg = |i: usize| segment_of(doc, self.picked[i].0, self.picked[i].1).ok_or("pick a line or polyline segment");
        let curve = |i: usize| curve_of(doc, self.picked[i].0).ok_or("pick a circle or arc");
        Ok(match self.kind {
            Geo::Horizontal => Constraint::Horizontal(seg(0)?),
            Geo::Vertical => Constraint::Vertical(seg(0)?),
            Geo::Parallel => Constraint::Parallel(seg(0)?, seg(1)?),
            Geo::Perpendicular => Constraint::Perpendicular(seg(0)?, seg(1)?),
            Geo::Collinear => Constraint::Collinear(seg(0)?, seg(1)?),
            Geo::Equal => match (seg(0), seg(1)) {
                (Ok(a), Ok(b)) => Constraint::EqualLength(a, b),
                _ => Constraint::EqualRadius(curve(0)?, curve(1)?),
            },
            Geo::Concentric => Constraint::Concentric(curve(0)?, curve(1)?),
            Geo::Tangent => match (seg(0), seg(1)) {
                (Ok(s), Err(_)) => Constraint::TangentLine(s, curve(1)?),
                (Err(_), Ok(s)) => Constraint::TangentLine(s, curve(0)?),
                _ => {
                    let (a, b) = (curve(0)?, curve(1)?);
                    let (ca, cb) = (pt(doc, a).unwrap_or_default(), pt(doc, b).unwrap_or_default());
                    let r = |id: u64| match doc.get(id).map(|e| &e.shape) {
                        Some(Shape::Circle { r, .. }) | Some(Shape::Arc { r, .. }) => *r,
                        _ => 0.0,
                    };
                    // Inside when one circle is mostly within the other.
                    let inside = ca.dist(cb) < (r(a.id) - r(b.id)).abs() + 0.5 * r(a.id).min(r(b.id));
                    Constraint::TangentCircles(a, b, inside)
                }
            },
            Geo::Coincident => Constraint::Coincident(self.points[0], self.points[1]),
            Geo::Fix => Constraint::Fix(self.points[0], pt(doc, self.points[0]).unwrap_or_default()),
        })
    }

    fn finish(&mut self, cx: &mut Cx) -> Flow {
        let c = match self.build(&cx.tab.doc) {
            Ok(c) => c,
            Err(e) => {
                cx.msg(e);
                self.picked.clear();
                self.points.clear();
                return Flow::Continue;
            }
        };
        cx.checkpoint();
        let label = c.label();
        match cx.doc().add_constraint(c) {
            Ok(r) => cx.msg(format!("{label} constraint added; {} degree(s) of freedom left.", r.dof)),
            Err(e) => cx.msg(e),
        }
        self.picked.clear();
        self.points.clear();
        Flow::Continue
    }
}

impl Command for GeoCmd {
    fn name(&self) -> &'static str {
        self.kind.name()
    }
    fn prompt(&self, _: &Cx) -> String {
        let (n, pts) = self.kind.picks();
        let done = if pts { self.points.len() } else { self.picked.len() };
        let what = if pts { "point" } else { "object" };
        if n == 1 {
            format!("Select {what} <exit>:")
        } else if done == 0 {
            format!("Select first {what} <exit>:")
        } else {
            format!("Select second {what}:")
        }
    }
    fn wants(&self) -> Want {
        if self.kind.picks().1 {
            Want::Point
        } else {
            Want::Entity
        }
    }
    fn point(&mut self, cx: &mut Cx, p: V2) -> Flow {
        let tol = cx.tab.view.px(10.0);
        match pick_point(&cx.tab.doc, p, tol) {
            Some((r, _)) => self.points.push(r),
            None => {
                cx.msg("No end point, vertex or centre there (use object snaps).");
                return Flow::Continue;
            }
        }
        if self.points.len() >= self.kind.picks().0 {
            return self.finish(cx);
        }
        Flow::Continue
    }
    fn entity(&mut self, cx: &mut Cx, id: u64, at: V2) -> Flow {
        self.picked.push((id, at));
        if self.picked.len() >= self.kind.picks().0 {
            return self.finish(cx);
        }
        Flow::Continue
    }
    fn enter(&mut self, _: &mut Cx) -> Flow {
        Flow::Done
    }
}

// ------------------------------------------------------------------ AUTOCONSTRAIN

pub struct AutoConstrainCmd;

impl Command for AutoConstrainCmd {
    fn name(&self) -> &'static str {
        "AUTOCONSTRAIN"
    }
    fn prompt(&self, _: &Cx) -> String {
        "Select objects to constrain <all>:".into()
    }
    fn wants(&self) -> Want {
        Want::Select
    }
    fn selected(&mut self, cx: &mut Cx, ids: Vec<u64>) -> Flow {
        let ids: Vec<u64> = if ids.is_empty() {
            cx.tab.doc.entities.iter().filter(|e| !points_of(&e.shape).is_empty()).map(|e| e.id).collect()
        } else {
            ids
        };
        cx.checkpoint();
        let n = auto_constrain(cx.doc(), &ids);
        let dof = cx.doc().solve_constraints(&[]).dof;
        cx.msg(format!("AUTOCONSTRAIN added {n} constraint(s); {dof} degree(s) of freedom left."));
        Flow::Done
    }
    fn enter(&mut self, cx: &mut Cx) -> Flow {
        self.selected(cx, vec![])
    }
}

/// Infer the obvious constraints among `ids`: horizontal and vertical lines,
/// joined end points, tangent lines and arcs, concentric circles.
pub fn auto_constrain(doc: &mut Document, ids: &[u64]) -> usize {
    let ext = ids.iter().filter_map(|id| doc.get(*id)).fold(BBox::empty(), |b, e| b.union(&doc.entity_bbox(e)));
    let tol = (ext.width().max(ext.height()) * 1e-4).max(1e-6);
    let ang = 0.5f64.to_radians();
    let mut new: Vec<Constraint> = vec![];
    let exists = |doc: &Document, new: &[Constraint], c: &Constraint| doc.constraints.contains(c) || new.contains(c);
    // Segments.
    let mut segs: Vec<SegRef> = vec![];
    for id in ids {
        match doc.get(*id).map(|e| &e.shape) {
            Some(Shape::Line { .. }) => segs.push(SegRef::line(*id)),
            Some(Shape::Polyline { pts, closed }) => {
                let n = pts.len();
                for k in 0..n.saturating_sub(1) {
                    segs.push(SegRef { a: PtRef { id: *id, k: k as u16 }, b: PtRef { id: *id, k: k as u16 + 1 } });
                }
                if *closed && n > 2 {
                    segs.push(SegRef { a: PtRef { id: *id, k: n as u16 - 1 }, b: PtRef { id: *id, k: 0 } });
                }
            }
            _ => {}
        }
    }
    for s in &segs {
        let Some((a, b)) = seg_pts(doc, s) else { continue };
        let t = (b - a).angle();
        let c = if t.sin().abs() < ang.sin() {
            Constraint::Horizontal(*s)
        } else if t.cos().abs() < ang.sin() {
            Constraint::Vertical(*s)
        } else {
            continue;
        };
        if !exists(doc, &new, &c) {
            new.push(c);
        }
    }
    // Joined end points of different entities (a polyline's own vertices are joined already).
    let pts: Vec<(PtRef, V2)> = ids
        .iter()
        .filter_map(|id| doc.get(*id).map(|e| (id, e)))
        .flat_map(|(id, e)| {
            points_of(&e.shape)
                .into_iter()
                .filter(|(k, _)| !matches!(e.shape, Shape::Circle { .. } | Shape::Arc { .. }) || *k > 0)
                .map(move |(k, p)| (PtRef { id: *id, k }, p))
        })
        .collect();
    for i in 0..pts.len() {
        for j in i + 1..pts.len() {
            if pts[i].0.id != pts[j].0.id && pts[i].1.dist(pts[j].1) <= tol {
                let c = Constraint::Coincident(pts[i].0, pts[j].0);
                let rev = Constraint::Coincident(pts[j].0, pts[i].0);
                if !exists(doc, &new, &c) && !exists(doc, &new, &rev) {
                    new.push(c);
                }
            }
        }
    }
    // Tangent lines and arcs; concentric circles.
    let curves: Vec<(PtRef, V2, f64)> = ids
        .iter()
        .filter_map(|id| match doc.get(*id).map(|e| &e.shape) {
            Some(Shape::Circle { c, r }) | Some(Shape::Arc { c, r, .. }) => Some((PtRef { id: *id, k: 0 }, *c, *r)),
            _ => None,
        })
        .collect();
    for (c, cp, r) in &curves {
        for s in &segs {
            let Some((a, b)) = seg_pts(doc, s) else { continue };
            if ((*cp - a).cross((b - a).norm()).abs() - r).abs() <= tol * 10.0 && dist_point_segment(*cp, a, b) <= r + tol * 10.0 {
                let k = Constraint::TangentLine(*s, *c);
                if !exists(doc, &new, &k) {
                    new.push(k);
                }
            }
        }
    }
    for i in 0..curves.len() {
        for j in i + 1..curves.len() {
            if curves[i].1.dist(curves[j].1) <= tol {
                new.push(Constraint::Concentric(curves[i].0, curves[j].0));
            }
        }
    }
    // Add one at a time, skipping any that conflict.
    let mut added = 0;
    for c in new {
        if doc.add_constraint(c).is_ok() {
            added += 1;
        }
    }
    added
}

// ------------------------------------------------------------------ driving dimensions

pub struct DimConstraintCmd {
    kind: DimKind,
    a: Option<PtRef>,
    b: Option<PtRef>,
    segs: Vec<SegRef>,
    /// Created, waiting for its value.
    made: Option<usize>,
}

impl DimConstraintCmd {
    pub fn new(kind: DimKind) -> Self {
        Self { kind, a: None, b: None, segs: vec![], made: None }
    }
    fn ready(&self) -> bool {
        match self.kind {
            DimKind::Radius | DimKind::Diameter => self.a.is_some(),
            DimKind::Angle => self.segs.len() == 2,
            _ => self.a.is_some() && self.b.is_some(),
        }
    }
}

impl Command for DimConstraintCmd {
    fn name(&self) -> &'static str {
        match self.kind {
            DimKind::Aligned => "DCALIGNED",
            DimKind::Horizontal => "DCHORIZONTAL",
            DimKind::Vertical => "DCVERTICAL",
            DimKind::Radius => "DCRADIUS",
            DimKind::Diameter => "DCDIAMETER",
            DimKind::Angle => "DCANGULAR",
        }
    }
    fn prompt(&self, cx: &Cx) -> String {
        if let Some(i) = self.made {
            if let Some(Constraint::Dim(d)) = cx.tab.doc.constraints.get(i) {
                return format!("{} = value or expression <{}>:", d.name, d.expr);
            }
        }
        if self.ready() {
            return "Specify dimension location:".into();
        }
        match self.kind {
            DimKind::Radius | DimKind::Diameter => "Select arc or circle:".into(),
            DimKind::Angle => format!("Select {} line:", if self.segs.is_empty() { "first" } else { "second" }),
            _ => format!("Specify {} constraint point:", if self.a.is_none() { "first" } else { "second" }),
        }
    }
    fn wants(&self) -> Want {
        if self.made.is_some() {
            Want::Value
        } else if !self.ready() && matches!(self.kind, DimKind::Radius | DimKind::Diameter | DimKind::Angle) {
            Want::Entity
        } else {
            Want::Point
        }
    }
    fn entity(&mut self, cx: &mut Cx, id: u64, at: V2) -> Flow {
        match self.kind {
            DimKind::Angle => match segment_of(&cx.tab.doc, id, at) {
                Some(s) => self.segs.push(s),
                None => cx.msg("Select a line or polyline segment."),
            },
            _ => match curve_of(&cx.tab.doc, id) {
                Some(c) => self.a = Some(c),
                None => cx.msg("Select a circle or arc."),
            },
        }
        Flow::Continue
    }
    fn point(&mut self, cx: &mut Cx, p: V2) -> Flow {
        if !self.ready() {
            let tol = cx.tab.view.px(10.0);
            match pick_point(&cx.tab.doc, p, tol) {
                Some((r, _)) if self.a.is_none() => self.a = Some(r),
                Some((r, _)) => self.b = Some(r),
                None => cx.msg("No end point, vertex or centre there (use object snaps)."),
            }
            return Flow::Continue;
        }
        let a = self.a.or_else(|| self.segs.first().map(|s| s.a)).unwrap();
        let b = self.b.or_else(|| self.segs.get(1).map(|s| s.a)).unwrap_or(a);
        let name = cx.tab.doc.next_dim_name();
        let mut d = DimConstraint {
            name,
            kind: self.kind.clone(),
            a,
            b,
            s: (self.kind == DimKind::Angle).then(|| (self.segs[0], self.segs[1])),
            expr: String::new(),
            value: 0.0,
            loc: p,
        };
        let Some(v) = cx.tab.doc.measure(&d) else {
            cx.msg("Cannot measure that.");
            return Flow::Done;
        };
        if v <= 0.0 {
            cx.msg("The points coincide; nothing to dimension.");
            return Flow::Done;
        }
        d.value = v;
        d.expr = fmt(v);
        cx.checkpoint();
        match cx.doc().add_constraint(Constraint::Dim(d)) {
            Ok(_) => self.made = Some(cx.tab.doc.constraints.len() - 1),
            Err(e) => {
                cx.msg(e);
                return Flow::Done;
            }
        }
        Flow::Continue
    }
    fn value(&mut self, cx: &mut Cx, s: &str) -> Flow {
        let Some(i) = self.made else {
            cx.msg(format!("Invalid input: {s}"));
            return Flow::Continue;
        };
        set_expr(cx, i, s.trim());
        Flow::Done
    }
    fn enter(&mut self, _: &mut Cx) -> Flow {
        Flow::Done
    }
}

/// Change a driving dimension's expression; the geometry follows, or the
/// change is undone when it cannot be met.
fn set_expr(cx: &mut Cx, i: usize, expr: &str) {
    let before = (cx.tab.doc.entities.clone(), cx.tab.doc.constraints.clone());
    let mut hold = vec![];
    if let Some(Constraint::Dim(d)) = cx.doc().constraints.get_mut(i) {
        d.expr = expr.to_string();
        hold.push(d.a);
    }
    // The first point stays; the geometry grows or shrinks from it.
    let r = cx.doc().solve_constraints(&hold);
    if !r.bad_exprs.is_empty() || !r.unmet.is_empty() {
        let why = r.bad_exprs.first().map(|(n, e)| format!("{n}: {e}")).unwrap_or_else(|| "it conflicts with other constraints".into());
        (cx.doc().entities, cx.doc().constraints) = before;
        cx.msg(format!("Not changed: {why}."));
    }
}

// ------------------------------------------------------------------ DELCONSTRAINT

pub struct DelConstraintCmd;

impl Command for DelConstraintCmd {
    fn name(&self) -> &'static str {
        "DELCONSTRAINT"
    }
    fn prompt(&self, _: &Cx) -> String {
        "Select objects whose constraints to delete <all>:".into()
    }
    fn wants(&self) -> Want {
        Want::Select
    }
    fn selected(&mut self, cx: &mut Cx, ids: Vec<u64>) -> Flow {
        cx.checkpoint();
        let before = cx.tab.doc.constraints.len();
        if ids.is_empty() {
            cx.doc().constraints.clear();
        } else {
            cx.doc().constraints.retain(|c| !c.ids().iter().any(|i| ids.contains(i)));
        }
        let n = before - cx.tab.doc.constraints.len();
        cx.msg(format!("{n} constraint(s) deleted."));
        Flow::Done
    }
    fn enter(&mut self, cx: &mut Cx) -> Flow {
        self.selected(cx, vec![])
    }
}

pub fn create(name: &str) -> Option<Box<dyn Command>> {
    let geo = |g| Some(Box::new(GeoCmd::new(g)) as Box<dyn Command>);
    let dim = |k| Some(Box::new(DimConstraintCmd::new(k)) as Box<dyn Command>);
    match name {
        "GCHORIZONTAL" | "GCH" => geo(Geo::Horizontal),
        "GCVERTICAL" | "GCV" => geo(Geo::Vertical),
        "GCPARALLEL" | "GCPA" => geo(Geo::Parallel),
        "GCPERPENDICULAR" | "GCPE" => geo(Geo::Perpendicular),
        "GCCOLLINEAR" | "GCCL" => geo(Geo::Collinear),
        "GCEQUAL" | "GCE" => geo(Geo::Equal),
        "GCCOINCIDENT" | "GCC" => geo(Geo::Coincident),
        "GCCONCENTRIC" | "GCCO" => geo(Geo::Concentric),
        "GCTANGENT" | "GCT" => geo(Geo::Tangent),
        "GCFIX" | "GCF" => geo(Geo::Fix),
        "AUTOCONSTRAIN" | "AC" => Some(Box::new(AutoConstrainCmd)),
        "DCALIGNED" | "DCA" => dim(DimKind::Aligned),
        "DCHORIZONTAL" | "DCH" | "DCLINEAR" => dim(DimKind::Horizontal),
        "DCVERTICAL" | "DCV" => dim(DimKind::Vertical),
        "DCRADIUS" | "DCR" => dim(DimKind::Radius),
        "DCDIAMETER" | "DCD" => dim(DimKind::Diameter),
        "DCANGULAR" | "DCAN" => dim(DimKind::Angle),
        "DELCONSTRAINT" | "DELCON" => Some(Box::new(DelConstraintCmd)),
        _ => None,
    }
}

// ------------------------------------------------------------------ display

/// A driving dimension drawn like an ordinary dimension, labelled `d1=50`.
fn dim_shape(doc: &Document, d: &DimConstraint) -> Option<Dimension> {
    let a = pt(doc, d.a)?;
    let b = pt(doc, d.b)?;
    let label = format!("{}={}", d.name, if d.expr == fmt(d.value) { fmt(d.value) } else { format!("{} ({})", d.expr, fmt(d.value)) });
    let mut dim = match d.kind {
        DimKind::Aligned => Dimension::new(icad_core::entity::DimKind::Aligned, a, b, d.loc),
        DimKind::Horizontal => Dimension::new(icad_core::entity::DimKind::Horizontal, a, b, d.loc),
        DimKind::Vertical => Dimension::new(icad_core::entity::DimKind::Vertical, a, b, d.loc),
        DimKind::Radius | DimKind::Diameter => {
            let r = match doc.get(d.a.id).map(|e| &e.shape) {
                Some(Shape::Circle { r, .. }) | Some(Shape::Arc { r, .. }) => *r,
                _ => return None,
            };
            let u = (d.loc - a).norm();
            let kind = if d.kind == DimKind::Radius { icad_core::entity::DimKind::Radius } else { icad_core::entity::DimKind::Diameter };
            Dimension::new(kind, a, a + if u == V2::ZERO { V2::new(r, 0.0) } else { u * r }, d.loc)
        }
        DimKind::Angle => {
            let (s, t) = d.s.as_ref()?;
            let ((p1, p2), (q1, q2)) = (seg_pts(doc, s)?, seg_pts(doc, t)?);
            // Corner: where the two lines meet.
            let (u, w) = (p2 - p1, q2 - q1);
            let den = u.cross(w);
            let c = if den.abs() > 1e-12 { p1 + u * ((q1 - p1).cross(w) / den) } else { p1 };
            let far = |x: V2, y: V2| if x.dist(c) > y.dist(c) { x } else { y };
            Dimension::angular(c, far(p1, p2), far(q1, q2), d.loc)
        }
    };
    dim.text = Some(label);
    Some(dim)
}

fn glyph(c: &Constraint) -> &'static str {
    match c {
        Constraint::Horizontal(_) => "H",
        Constraint::Vertical(_) => "V",
        Constraint::Parallel(..) => "//",
        Constraint::Perpendicular(..) => "⊥",
        Constraint::Collinear(..) => "Y",
        Constraint::EqualLength(..) | Constraint::EqualRadius(..) => "=",
        Constraint::Coincident(..) => "",
        Constraint::PointOnLine(..) | Constraint::PointOnCircle(..) => "o",
        Constraint::Concentric(..) => "◎",
        Constraint::TangentLine(..) | Constraint::TangentCircles(..) => "T",
        Constraint::Fix(..) => "F",
        Constraint::Dim(_) => "",
    }
}

/// Draw constraint glyphs next to their geometry and the driving dimensions.
pub fn draw_overlay(painter: &Painter, view: &View, doc: &Document, glyphs: bool) {
    if doc.constraints.is_empty() {
        return;
    }
    let ctx = doc.ctx();
    let st = Style { color: CONSTRAINT, width: 1.0, dash: vec![] };
    let mut out = vec![];
    for c in &doc.constraints {
        if let Constraint::Dim(d) = c {
            if let Some(dim) = dim_shape(doc, d) {
                for p in dim.prims(ctx.ds, ctx.scale) {
                    match &p {
                        icad_core::prim::Prim::Text(t) => crate::canvas::draw_text(painter, view, t, CONSTRAINT),
                        _ => prim_shapes(view, &p, &st, &mut out),
                    }
                }
            }
        }
    }
    painter.extend(out);
    if !glyphs {
        return;
    }
    // Glyphs sit beside each segment's middle, several stacked along it.
    let mut placed: Vec<(eframe::egui::Pos2, usize)> = vec![];
    let mut draw = |at: V2, off: V2, label: &str| {
        let s = view.to_screen(at);
        let base = s + eframe::egui::vec2(off.x as f32, -off.y as f32) * 14.0;
        let n = placed.iter().filter(|(p, _)| (*p - base).length() < 12.0).count();
        let pos = base + eframe::egui::vec2(13.0 * n as f32, 0.0);
        placed.push((base, n));
        let r = eframe::egui::Rect::from_center_size(pos, eframe::egui::vec2(13.0, 13.0));
        painter.rect_filled(r, 2.0, Color32::from_rgb(22, 48, 52));
        painter.rect_stroke(r, 2.0, Stroke::new(1.0f32, CONSTRAINT), egui::StrokeKind::Middle);
        painter.text(pos, Align2::CENTER_CENTER, label, FontId::proportional(9.5), CONSTRAINT);
    };
    for c in &doc.constraints {
        let label = glyph(c);
        let mid_of = |s: &SegRef| seg_pts(doc, s).map(|(a, b)| (a.mid(b), (b - a).norm().perp()));
        match c {
            Constraint::Horizontal(s) | Constraint::Vertical(s) => {
                if let Some((m, n)) = mid_of(s) {
                    draw(m, n, label);
                }
            }
            Constraint::Parallel(s, t) | Constraint::Perpendicular(s, t) | Constraint::Collinear(s, t) | Constraint::EqualLength(s, t) => {
                for x in [s, t] {
                    if let Some((m, n)) = mid_of(x) {
                        draw(m, n, label);
                    }
                }
            }
            Constraint::TangentLine(s, k) => {
                if let (Some((m, n)), Some(_)) = (mid_of(s), pt(doc, *k)) {
                    draw(m, n, label);
                }
            }
            Constraint::Coincident(a, _) => {
                if let Some(p) = pt(doc, *a) {
                    painter.circle_filled(view.to_screen(p), 3.0, CONSTRAINT);
                }
            }
            Constraint::EqualRadius(a, b)
            | Constraint::Concentric(a, b)
            | Constraint::TangentCircles(a, b, _)
            | Constraint::PointOnCircle(a, b) => {
                for x in [a, b] {
                    if let Some(p) = pt(doc, *x) {
                        draw(p, V2::new(0.7, 0.7), label);
                    }
                }
            }
            Constraint::PointOnLine(a, _) | Constraint::Fix(a, _) => {
                if let Some(p) = pt(doc, *a) {
                    draw(p, V2::new(0.7, 0.7), label);
                }
            }
            Constraint::Dim(_) => {}
        }
    }
}

// ------------------------------------------------------------------ app: solving and Parameters

impl App {
    /// Keep the constraints true after this frame's edits. The points the edit
    /// moved are held most firmly, so the rest of the drawing adapts to them.
    pub fn solve_constraints_frame(&mut self) {
        let t = &mut self.tabs[self.active];
        if t.doc.constraints.is_empty() {
            t.cons_state = None;
            return;
        }
        let snapshot = |doc: &Document| -> Vec<(PtRef, V2, f64)> {
            let mut ids: Vec<u64> = doc.constraints.iter().flat_map(|c| c.ids()).collect();
            ids.sort_unstable();
            ids.dedup();
            ids.iter()
                .filter_map(|id| doc.get(*id).map(|e| (id, e)))
                .flat_map(|(id, e)| {
                    let r = match e.shape {
                        Shape::Circle { r, .. } | Shape::Arc { r, .. } => r,
                        _ => 0.0,
                    };
                    points_of(&e.shape).into_iter().map(move |(k, p)| (PtRef { id: *id, k }, p, r))
                })
                .collect()
        };
        let now = snapshot(&t.doc);
        let sig = format!("{now:?}{:?}", t.doc.constraints);
        if t.cons_state.as_ref().is_some_and(|(s, _)| *s == sig) {
            return;
        }
        let prefer: Vec<PtRef> = match &t.cons_state {
            Some((_, before)) => now
                .iter()
                .filter(|(r, p, rad)| {
                    before.iter().find(|(q, _, _)| q == r).is_none_or(|(_, bp, br)| bp.dist(*p) > 1e-9 || (br - rad).abs() > 1e-9)
                })
                .map(|(r, _, _)| *r)
                .collect(),
            None => vec![],
        };
        let rep = t.doc.solve_constraints(&prefer);
        let after = snapshot(&t.doc);
        t.cons_state = Some((format!("{after:?}{:?}", t.doc.constraints), after));
        let msg = if !rep.bad_exprs.is_empty() {
            Some(format!(
                "Constraint parameters: {}",
                rep.bad_exprs.iter().map(|(n, e)| format!("{n}: {e}")).collect::<Vec<_>>().join("; ")
            ))
        } else if !rep.unmet.is_empty() {
            Some(format!("{} constraint(s) cannot be met with this edit (over-constrained).", rep.unmet.len()))
        } else {
            None
        };
        if let Some(m) = msg {
            if self.log.last() != Some(&m) {
                self.log.push(m);
            }
        }
    }

    /// The Parameters manager: driving dimensions with their expressions.
    pub fn parameters_dialog(&mut self, ctx: &egui::Context) {
        if !self.dlg.parameters {
            return;
        }
        let mut open = true;
        let mut edit: Option<(usize, String)> = None;
        let mut delete: Option<usize> = None;
        let (rows, n_geo) = {
            let doc = &self.tabs[self.active].doc;
            let rows: Vec<(usize, String, String, f64)> = doc
                .constraints
                .iter()
                .enumerate()
                .filter_map(|(i, c)| if let Constraint::Dim(d) = c { Some((i, d.name.clone(), d.expr.clone(), d.value)) } else { None })
                .collect();
            let n_geo = doc.constraints.len() - rows.len();
            (rows, n_geo)
        };
        let buf_id = egui::Id::new("param_bufs");
        let mut bufs: Vec<String> = ctx.data(|d| d.get_temp::<Vec<String>>(buf_id)).unwrap_or_default();
        if bufs.len() != rows.len() {
            bufs = rows.iter().map(|r| r.2.clone()).collect();
        }
        egui::Window::new("Parameters").open(&mut open).default_width(420.0).show(ctx, |ui| {
            ui.label(
                RichText::new(format!(
                    "{} driving dimension(s), {} geometric constraint(s). Expressions may use the other names, e.g. d1 / 2 + 5.",
                    rows.len(),
                    n_geo
                ))
                .color(theme::TEXT_DIM),
            );
            ui.separator();
            egui::Grid::new("params").num_columns(4).striped(true).spacing([8.0, 4.0]).show(ui, |ui| {
                ui.label(RichText::new("Name").strong());
                ui.label(RichText::new("Expression").strong());
                ui.label(RichText::new("Value").strong());
                ui.label("");
                ui.end_row();
                for (k, (i, name, _, value)) in rows.iter().enumerate() {
                    ui.label(name);
                    let r = ui.add(egui::TextEdit::singleline(&mut bufs[k]).desired_width(160.0).min_size(egui::vec2(160.0, 0.0)));
                    if r.lost_focus() && bufs[k].trim() != rows[k].2 {
                        edit = Some((*i, bufs[k].trim().to_string()));
                    }
                    ui.label(fmt(*value));
                    if ui.small_button("×").on_hover_text("Delete this driving dimension").clicked() {
                        delete = Some(*i);
                    }
                    ui.end_row();
                }
            });
            if rows.is_empty() {
                ui.label(
                    RichText::new("No driving dimensions yet: use DCHORIZONTAL, DCVERTICAL, DCALIGNED, DCRADIUS or DCANGULAR.")
                        .color(theme::TEXT_DIM),
                );
            }
        });
        ctx.data_mut(|d| d.insert_temp(buf_id, bufs));
        self.dlg.parameters = open;
        if let Some((i, expr)) = edit {
            let mut cx = Cx { tab: &mut self.tabs[self.active], log: &mut self.log, mem: &mut self.mem, view_req: &mut self.view_req };
            cx.checkpoint();
            set_expr(&mut cx, i, &expr);
            ctx.data_mut(|d| d.remove::<Vec<String>>(buf_id));
        }
        if let Some(i) = delete {
            let t = self.tab();
            t.checkpoint();
            t.doc.constraints.remove(i);
            ctx.data_mut(|d| d.remove::<Vec<String>>(buf_id));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn auto_constrain_a_sketched_bracket() {
        let mut doc = Document::new();
        let pl = doc.add(Shape::Polyline {
            pts: vec![
                V2::new(0.0, 0.0),
                V2::new(100.0, 0.0),
                V2::new(100.0, 30.0),
                V2::new(30.0, 30.0),
                V2::new(30.0, 80.0),
                V2::new(0.0, 80.0),
            ],
            closed: true,
        });
        let hole = doc.add(Shape::Circle { c: V2::new(15.0, 60.0), r: 6.0 });
        let n = auto_constrain(&mut doc, &[pl, hole]);
        // Six edges, each horizontal or vertical.
        assert_eq!(n, 6, "{:?}", doc.constraints);
        // Dragging one corner keeps the edges square.
        if let Some(Shape::Polyline { pts, .. }) = doc.get_mut(pl).map(|e| &mut e.shape) {
            pts[2] = V2::new(120.0, 40.0);
        }
        let r = doc.solve_constraints(&[PtRef { id: pl, k: 2 }]);
        assert!(r.unmet.is_empty());
        let Shape::Polyline { pts, .. } = &doc.get(pl).unwrap().shape else { unreachable!() };
        assert!((pts[1].x - 120.0).abs() < 1e-6 && (pts[3].y - 40.0).abs() < 1e-6 && pts[1].y.abs() < 1e-6, "{pts:?}");
    }

    #[test]
    fn driving_dimension_grows_from_its_first_point() {
        let mut doc = Document::new();
        let pl = doc.add(Shape::Polyline {
            pts: vec![V2::new(0.0, 0.0), V2::new(120.0, 0.0), V2::new(120.0, 60.0), V2::new(0.0, 60.0)],
            closed: true,
        });
        auto_constrain(&mut doc, &[pl]);
        let a = PtRef { id: pl, k: 0 };
        let b = PtRef { id: pl, k: 1 };
        let d = DimConstraint {
            name: "d1".into(),
            kind: DimKind::Horizontal,
            a,
            b,
            s: None,
            expr: "120".into(),
            value: 120.0,
            loc: V2::new(60.0, -15.0),
        };
        doc.add_constraint(Constraint::Dim(d)).unwrap();
        if let Some(Constraint::Dim(d)) = doc.constraints.last_mut() {
            d.expr = "150".into();
        }
        let r = doc.solve_constraints(&[a]);
        assert!(r.unmet.is_empty(), "{r:?}");
        let Shape::Polyline { pts, .. } = &doc.get(pl).unwrap().shape else { unreachable!() };
        assert!(pts[0].len() < 1e-6 && (pts[1].x - 150.0).abs() < 1e-6, "{pts:?}");
    }
}
