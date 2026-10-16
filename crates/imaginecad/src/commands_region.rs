// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Proteen Das

//! BOUNDARY, REGION, the region booleans UNION / SUBTRACT / INTERSECT, and
//! MASSPROP.

use crate::commands::{fmt, Command, Cx, Flow, Want};
use icad_core::entity::Shape;
use icad_core::geom::*;
use icad_core::hatch;
use icad_core::region::{chain_loops, BoolOp, MassProps, Region};
use icad_core::Document;

pub fn create(name: &str) -> Option<Box<dyn Command>> {
    let c: Box<dyn Command> = match name {
        "BOUNDARY" | "BO" | "BPOLY" => Box::new(BoundaryCmd { region: false, made: 0 }),
        "REGION" | "REG" => Box::new(RegionCmd),
        "UNION" | "UNI" => Box::new(BoolCmd::new(BoolOp::Union)),
        "SUBTRACT" | "SU" => Box::new(BoolCmd::new(BoolOp::Subtract)),
        "INTERSECT" | "IN" => Box::new(BoolCmd::new(BoolOp::Intersect)),
        "MASSPROP" => Box::new(MassPropCmd),
        _ => return None,
    };
    Some(c)
}

fn add_region(cx: &mut Cx, r: Region) -> u64 {
    let layer = cx.tab.doc.current_layer.clone();
    cx.doc().add_on(&layer, Shape::Region(r))
}

// ------------------------------------------------------------------ BOUNDARY

struct BoundaryCmd {
    region: bool,
    made: usize,
}

impl Command for BoundaryCmd {
    fn name(&self) -> &'static str {
        "BOUNDARY"
    }
    fn prompt(&self, _: &Cx) -> String {
        format!("Pick an internal point or [Object type: {}] <done>:", if self.region { "Region" } else { "Polyline" })
    }
    fn wants(&self) -> Want {
        Want::Point
    }
    fn point(&mut self, cx: &mut Cx, p: V2) -> Flow {
        let segs = cx.tab.doc.boundary_segments();
        let Some(loops) = hatch::find_boundary(&segs, p) else {
            cx.msg("No closed boundary found around that point.");
            return Flow::Continue;
        };
        cx.checkpoint();
        if self.region {
            for r in Region::from_loops(&loops) {
                add_region(cx, r);
            }
        } else {
            let layer = cx.tab.doc.current_layer.clone();
            for l in loops {
                cx.doc().add_on(&layer, Shape::Polyline { pts: l, closed: true });
            }
        }
        self.made += 1;
        Flow::Continue
    }
    fn value(&mut self, cx: &mut Cx, s: &str) -> Flow {
        match s.to_ascii_uppercase().as_str() {
            "O" | "OBJECT" => self.region = !self.region,
            "R" | "REGION" => self.region = true,
            "P" | "POLYLINE" => self.region = false,
            _ => cx.msg(format!("Invalid input: {s}")),
        }
        Flow::Continue
    }
    fn enter(&mut self, cx: &mut Cx) -> Flow {
        cx.msg(format!("BOUNDARY created {} boundary object(s).", self.made));
        Flow::Done
    }
}

// ------------------------------------------------------------------ REGION

struct RegionCmd;

/// Closed loops from the selection: closed objects directly, and lines,
/// arcs and open polylines chained end to end.
pub fn loops_of(doc: &Document, ids: &[u64]) -> (Vec<Vec<V2>>, Vec<u64>, usize) {
    let ctx = doc.ctx();
    let mut loops = vec![];
    let mut used = vec![];
    let mut open: Vec<(u64, Vec<V2>)> = vec![];
    for id in ids {
        let Some(e) = doc.get(*id) else { continue };
        match &e.shape {
            Shape::Region(r) => {
                loops.extend(r.loops.clone());
                used.push(*id);
            }
            s if s.closed_loop().is_some() => {
                loops.push(s.closed_loop().unwrap());
                used.push(*id);
            }
            Shape::Line { .. }
            | Shape::Arc { .. }
            | Shape::Polyline { closed: false, .. }
            | Shape::Spline { closed: false, .. }
            | Shape::Ellipse { .. } => {
                // Tessellated path of the object.
                let pts: Vec<V2> = s_points(&e.shape, &ctx);
                if pts.len() >= 2 {
                    open.push((*id, pts));
                }
            }
            _ => {}
        }
    }
    let ext = doc.extents();
    let tol = (ext.width().max(ext.height()) * 1e-7).max(1e-9);
    let (chained, unclosed) = chain_loops(open.iter().map(|(_, p)| p.clone()).collect(), tol);
    if !chained.is_empty() {
        used.extend(open.iter().map(|(id, _)| *id));
    }
    loops.extend(chained);
    (loops, used, unclosed)
}

fn s_points(s: &Shape, ctx: &icad_core::Ctx) -> Vec<V2> {
    match s {
        Shape::Line { a, b } => vec![*a, *b],
        _ => match s.prims(ctx).into_iter().next() {
            Some(icad_core::Prim::Poly(p, _)) => p,
            Some(icad_core::Prim::Arc { c, r, start, end }) => arc_points(c, r, start, end, r * 0.002),
            _ => vec![],
        },
    }
}

impl Command for RegionCmd {
    fn name(&self) -> &'static str {
        "REGION"
    }
    fn start(&mut self, cx: &mut Cx) -> Flow {
        let ids = cx.take_selection();
        if ids.is_empty() {
            Flow::Continue
        } else {
            self.selected(cx, ids)
        }
    }
    fn prompt(&self, _: &Cx) -> String {
        "Select closed objects (or lines and arcs that form loops):".into()
    }
    fn wants(&self) -> Want {
        Want::Select
    }
    fn selected(&mut self, cx: &mut Cx, ids: Vec<u64>) -> Flow {
        let (loops, used, open) = loops_of(&cx.tab.doc, &ids);
        if loops.is_empty() {
            cx.msg("No closed loops in the selection.");
            return Flow::Done;
        }
        cx.checkpoint();
        // Each loop is its own region (as when picking separate outlines).
        let mut n = 0;
        for l in &loops {
            for r in Region::from_loops(std::slice::from_ref(l)) {
                add_region(cx, r);
                n += 1;
            }
        }
        cx.doc().remove(&used);
        cx.msg(format!("{n} region(s) created.{}", if open > 0 { format!(" {open} open path(s) ignored.") } else { String::new() }));
        Flow::Done
    }
}

// ------------------------------------------------------------------ booleans

struct BoolCmd {
    op: BoolOp,
    first: Option<Vec<u64>>,
}

impl BoolCmd {
    fn new(op: BoolOp) -> Self {
        Self { op, first: None }
    }
    fn regions(doc: &Document, ids: &[u64]) -> (Vec<Region>, Vec<u64>) {
        let mut rs = vec![];
        let mut used = vec![];
        for id in ids {
            if let Some(Shape::Region(r)) = doc.get(*id).map(|e| &e.shape) {
                rs.push(r.clone());
                used.push(*id);
            }
        }
        (rs, used)
    }
    fn apply(&self, cx: &mut Cx, a_ids: &[u64], b_ids: &[u64]) -> Flow {
        let (a, ua) = Self::regions(&cx.tab.doc, a_ids);
        let (b, ub) = Self::regions(&cx.tab.doc, b_ids);
        if a.is_empty() {
            cx.msg("No regions selected. Create them with REGION or BOUNDARY first.");
            return Flow::Done;
        }
        let out = match self.op {
            // Union / intersect of all selected regions.
            BoolOp::Union => Region::boolean(&a, &[], BoolOp::Union),
            BoolOp::Intersect => {
                a.iter().skip(1).fold(vec![a[0].clone()], |acc, r| Region::boolean(&acc, std::slice::from_ref(r), BoolOp::Intersect))
            }
            BoolOp::Subtract => Region::boolean(&a, &b, BoolOp::Subtract),
        };
        cx.checkpoint();
        let layer = cx.tab.doc.get(ua[0]).map(|e| e.layer.clone()).unwrap_or_else(|| cx.tab.doc.current_layer.clone());
        cx.doc().remove(&ua);
        cx.doc().remove(&ub);
        let n = out.len();
        for r in out {
            cx.doc().add_on(&layer, Shape::Region(r));
        }
        cx.msg(if n == 0 { "The result is empty (null region); nothing is left.".to_string() } else { format!("{n} region(s).") });
        Flow::Done
    }
}

impl Command for BoolCmd {
    fn name(&self) -> &'static str {
        match self.op {
            BoolOp::Union => "UNION",
            BoolOp::Subtract => "SUBTRACT",
            BoolOp::Intersect => "INTERSECT",
        }
    }
    fn start(&mut self, cx: &mut Cx) -> Flow {
        let ids = cx.take_selection();
        if ids.is_empty() {
            Flow::Continue
        } else {
            self.selected(cx, ids)
        }
    }
    fn prompt(&self, _: &Cx) -> String {
        match (self.op, &self.first) {
            (BoolOp::Subtract, None) => "Select regions to subtract from:".into(),
            (BoolOp::Subtract, Some(_)) => "Select regions to subtract:".into(),
            _ => "Select regions:".into(),
        }
    }
    fn wants(&self) -> Want {
        Want::Select
    }
    fn selected(&mut self, cx: &mut Cx, ids: Vec<u64>) -> Flow {
        if ids.is_empty() {
            return Flow::Done;
        }
        match (self.op, self.first.take()) {
            (BoolOp::Subtract, None) => {
                self.first = Some(ids);
                Flow::Continue
            }
            (BoolOp::Subtract, Some(a)) => self.apply(cx, &a, &ids),
            _ => self.apply(cx, &ids, &[]),
        }
    }
}

// ------------------------------------------------------------------ MASSPROP

struct MassPropCmd;

/// Report lines for MASSPROP.
pub fn massprop_report(m: &MassProps, units: &str) -> Vec<String> {
    let u = if units.is_empty() { "mm" } else { units };
    vec![
        "----------------  REGIONS  ----------------".into(),
        format!("Area:                    {} {u}²", fmt(m.area)),
        format!("Perimeter:               {} {u}", fmt(m.perimeter)),
        format!("Bounding box:         X: {} -- {}", fmt(m.bbox.min.x), fmt(m.bbox.max.x)),
        format!("                      Y: {} -- {}", fmt(m.bbox.min.y), fmt(m.bbox.max.y)),
        format!("Centroid:             X: {}", fmt(m.centroid.x)),
        format!("                      Y: {}", fmt(m.centroid.y)),
        format!("Moments of inertia:   X: {} {u}⁴", fmt(m.ixx)),
        format!("                      Y: {} {u}⁴", fmt(m.iyy)),
        format!("Product of inertia:  XY: {} {u}⁴", fmt(m.ixy)),
        format!("Radii of gyration:    X: {} {u}", fmt(m.kx)),
        format!("  (about centroid)    Y: {} {u}", fmt(m.ky)),
        format!("Centroidal moments:  Ix: {}   Iy: {}   Ixy: {}", fmt(m.ixx_c), fmt(m.iyy_c), fmt(m.ixy_c)),
        format!(
            "Principal moments about centroid:  I: {} along [{} {}]   J: {} along [{} {}]",
            fmt(m.i1),
            fmt(m.principal_angle.cos()),
            fmt(m.principal_angle.sin()),
            fmt(m.i2),
            fmt(-m.principal_angle.sin()),
            fmt(m.principal_angle.cos())
        ),
    ]
}

impl Command for MassPropCmd {
    fn name(&self) -> &'static str {
        "MASSPROP"
    }
    fn start(&mut self, cx: &mut Cx) -> Flow {
        let ids = cx.take_selection();
        if ids.is_empty() {
            Flow::Continue
        } else {
            self.selected(cx, ids)
        }
    }
    fn prompt(&self, _: &Cx) -> String {
        "Select regions or closed objects:".into()
    }
    fn wants(&self) -> Want {
        Want::Select
    }
    fn selected(&mut self, cx: &mut Cx, ids: Vec<u64>) -> Flow {
        // Regions as they are; other closed objects each count as a solid area.
        let mut regions = vec![];
        for id in &ids {
            match cx.tab.doc.get(*id).map(|e| &e.shape) {
                Some(Shape::Region(r)) => regions.push(r.clone()),
                Some(s) => {
                    if let Some(l) = s.closed_loop() {
                        regions.extend(Region::from_loops(&[l]));
                    }
                }
                None => {}
            }
        }
        if regions.is_empty() {
            cx.msg("Select regions or closed objects.");
            return Flow::Done;
        }
        let m = MassProps::of(&regions);
        let units = cx.tab.doc.sheet.title.units.clone();
        for line in massprop_report(&m, &units) {
            cx.msg(line);
        }
        Flow::Done
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn region_from_chained_lines() {
        let mut doc = Document::new();
        let pts = [V2::new(0.0, 0.0), V2::new(40.0, 0.0), V2::new(40.0, 20.0), V2::new(0.0, 20.0)];
        let ids: Vec<u64> = (0..4).map(|i| doc.add(Shape::Line { a: pts[i], b: pts[(i + 1) % 4] })).collect();
        let c = doc.add(Shape::Circle { c: V2::new(20.0, 10.0), r: 5.0 });
        let (loops, used, open) = loops_of(&doc, &[ids.clone(), vec![c]].concat());
        assert_eq!((loops.len(), used.len(), open), (2, 5, 0));
        // Both loops together: a plate with a hole.
        let r = Region::from_loops(&loops);
        assert_eq!(r.len(), 1);
        // Circles are tessellated, so allow a little less area than πr².
        assert!((r[0].area() - (800.0 - std::f64::consts::PI * 25.0)).abs() < 0.25);
        let rep = massprop_report(&r[0].mass_properties(), "mm");
        assert!(rep.iter().any(|l| l.contains("Centroid") && l.contains("20")));
    }
}
