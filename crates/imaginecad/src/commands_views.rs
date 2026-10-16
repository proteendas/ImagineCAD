// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Proteen Das

//! SECTIONVIEW (a section from a cutting line through a part of constant
//! thickness) and DETAILVIEW (an enlarged view of a circular area).

use crate::commands::{fmt, num, Command, Cx, Flow, Want};
use icad_core::document::layers;
use icad_core::entity::Hatch;
use icad_core::entity::{Curve, Shape};
use icad_core::geom::*;
use icad_core::region::{BoolOp, Region};
use icad_core::style::{LineType, LtRef};
use icad_core::views::{self, Piece};
use icad_core::Document;

pub fn create(name: &str) -> Option<Box<dyn Command>> {
    let c: Box<dyn Command> = match name {
        "SECTIONVIEW" | "SECTION" | "VIEWSECTION" => Box::new(SectionCmd::default()),
        "DETAILVIEW" | "DETAIL" | "VIEWDETAIL" => Box::new(DetailCmd::default()),
        "BROKENOUT" | "BROKENSECTION" => Box::new(BrokenOutCmd::default()),
        other => return crate::blockedit_ui::create(other),
    };
    Some(c)
}

fn text_height(doc: &Document) -> f64 {
    doc.dimstyle.text_height * doc.ascale()
}

/// The geometry of the picked entities, or of everything visible.
fn pick_ids(cx: &Cx, ids: Vec<u64>) -> Vec<u64> {
    if !ids.is_empty() {
        return ids;
    }
    let doc = &cx.tab.doc;
    doc.entities.iter().filter(|e| doc.is_visible(e)).map(|e| e.id).collect()
}

/// Only drawing geometry bounds material (not annotation).
fn boundary_curves(doc: &Document, ids: &[u64]) -> Vec<Curve> {
    ids.iter()
        .filter_map(|id| doc.get(*id))
        .filter(|e| {
            !matches!(
                e.shape,
                Shape::Dim(_)
                    | Shape::Text(_)
                    | Shape::MText(_)
                    | Shape::Leader(_)
                    | Shape::Table(_)
                    | Shape::Hatch(_)
                    | Shape::XLine { .. }
                    | Shape::Ray { .. }
            )
        })
        .flat_map(|e| doc.curves_of(e))
        .collect()
}

fn place(pieces: &mut [Piece], x: &Xform) {
    for p in pieces {
        p.shape.transform(x);
    }
}

// ------------------------------------------------------------------ SECTIONVIEW

#[derive(Default)]
struct SectionCmd {
    ids: Option<Vec<u64>>,
    curves: Vec<Curve>,
    /// The cutting line: two points, or more for a stepped (offset) cut.
    path: Vec<V2>,
    depth: Option<f64>,
    /// Arrows toward the view (first-angle habit) instead of away from it.
    toward: bool,
    /// A turned part: the round cross-section instead of a cut of a plate.
    revolved: bool,
}

impl SectionCmd {
    /// Section and cutting line with the view centred at `at`.
    fn pieces(&self, doc: &Document, at: V2) -> Option<(Vec<Piece>, Vec<Piece>)> {
        let depth = self.depth?;
        if self.path.len() < 2 {
            return None;
        }
        let mut path = self.path.clone();
        // The section reads left to right (or bottom to top).
        let u = path[path.len() - 1] - path[0];
        if u.x < -1e-9 || (u.x.abs() <= 1e-9 && u.y < 0.0) {
            path.reverse();
        }
        let (a, b) = (path[0], path[path.len() - 1]);
        // A stepped cut is laid out along its legs (they are parallel).
        let dir = (path[1] - path[0]).norm();
        let h = text_height(doc);
        let letter = views::next_view_letter(doc);
        let n = dir.perp();
        let side = (at - a).dot(n);
        let away = if side >= 0.0 { -n } else { n };
        let view = if self.toward { -away } else { away };
        let hatch_scale = |sec: &mut Vec<Piece>| {
            for p in sec.iter_mut() {
                if let Shape::Hatch(hh) = &mut p.shape {
                    hh.scale = doc.ascale();
                }
            }
        };
        if self.revolved {
            let mut sec = views::revolved_section(&self.curves, a, b, at, &letter, h);
            hatch_scale(&mut sec);
            return (!sec.is_empty()).then(|| (sec, views::cutting_path(&path, view, &letter, h)));
        }
        let mut sec = views::section_view_path(&self.curves, &path, depth, V2::ZERO, &letter, h);
        let spans = views::path_intervals(&self.curves, &path);
        let (first, last) = (spans.first()?, spans.last()?);
        let centre = V2::new((first.0 + last.1) / 2.0, depth / 2.0);
        hatch_scale(&mut sec);
        place(&mut sec, &Xform::translate(-centre));
        place(&mut sec, &Xform::rotate(V2::ZERO, dir.angle()));
        place(&mut sec, &Xform::translate(at));
        Some((sec, views::cutting_path(&path, view, &letter, h)))
    }
}

impl Command for SectionCmd {
    fn name(&self) -> &'static str {
        "SECTIONVIEW"
    }
    fn prompt(&self, cx: &Cx) -> String {
        let d = cx.mem.section_depth.map(fmt).unwrap_or_else(|| "10".into());
        match (&self.ids, self.path.len(), self.depth) {
            (None, ..) => "Select the boundaries of the part to cut <all>:".into(),
            (_, 0, _) => "Specify start point of the cutting line (outside the part):".into(),
            (_, 1, _) => "Specify next point of the cutting line:".into(),
            (_, _, None) => format!("Next point (a stepped cut), or part thickness [Revolved (turned part)] <{d}>:"),
            _ => format!("Specify location of the section view or [Arrows {}]:", if self.toward { "toward" } else { "away" }),
        }
    }
    fn wants(&self) -> Want {
        match (&self.ids, self.depth) {
            (None, _) => Want::Select,
            _ => Want::Point,
        }
    }
    fn base(&self) -> Option<V2> {
        // Typed numbers after the second point are the thickness, not a distance.
        if self.path.len() == 1 {
            self.path.last().copied()
        } else {
            None
        }
    }
    fn selected(&mut self, cx: &mut Cx, ids: Vec<u64>) -> Flow {
        let ids = pick_ids(cx, ids);
        self.curves = boundary_curves(&cx.tab.doc, &ids);
        if self.curves.is_empty() {
            cx.msg("No part geometry to cut.");
            return Flow::Done;
        }
        self.ids = Some(ids);
        Flow::Continue
    }
    fn enter(&mut self, cx: &mut Cx) -> Flow {
        match (&self.ids, self.path.len(), self.depth) {
            (None, ..) => self.selected(cx, vec![]),
            (_, n, None) if n >= 2 => {
                self.depth = Some(cx.mem.section_depth.unwrap_or(10.0));
                Flow::Continue
            }
            (_, _, None) => Flow::Continue,
            _ => Flow::Done,
        }
    }
    fn value(&mut self, cx: &mut Cx, s: &str) -> Flow {
        if self.path.len() >= 2 && self.depth.is_none() {
            if matches!(s.to_ascii_uppercase().as_str(), "R" | "REVOLVED") {
                if self.path.len() > 2 {
                    cx.msg("A revolved section is cut along a single straight line.");
                    return Flow::Continue;
                }
                self.revolved = true;
                self.depth = Some(0.0);
                return Flow::Continue;
            }
            match num(s).filter(|v| *v > 0.0) {
                Some(v) => {
                    self.depth = Some(v);
                    cx.mem.section_depth = Some(v);
                }
                None => cx.msg("Enter a positive thickness."),
            }
        } else if self.depth.is_some() && matches!(s.to_ascii_uppercase().as_str(), "A" | "ARROWS" | "F" | "FLIP") {
            self.toward = !self.toward;
        } else {
            cx.msg(format!("Invalid option: {s}"));
        }
        Flow::Continue
    }
    fn point(&mut self, cx: &mut Cx, p: V2) -> Flow {
        if self.depth.is_none() {
            if self.path.last().is_some_and(|l| l.dist(p) < 1e-9) {
                return Flow::Continue;
            }
            self.path.push(p);
            if self.path.len() >= 2 && views::path_intervals(&self.curves, &self.path).is_empty() {
                cx.msg("The cutting line does not pass through the part.");
                self.path.pop();
            }
            return Flow::Continue;
        }
        let Some((sec, line)) = self.pieces(&cx.tab.doc, p) else { return Flow::Done };
        let letter = views::next_view_letter(&cx.tab.doc);
        cx.checkpoint();
        views::add_pieces(cx.doc(), line);
        views::add_pieces(cx.doc(), sec);
        cx.msg(format!("Section {letter}–{letter} created."));
        Flow::Done
    }
    fn preview(&self, doc: &Document, c: V2) -> Vec<Shape> {
        match (self.path.len(), self.depth) {
            (0, _) => vec![],
            (_, None) => {
                let mut pts = self.path.clone();
                pts.push(c);
                vec![Shape::Polyline { pts, closed: false }]
            }
            _ => self.pieces(doc, c).map(|(s, l)| s.into_iter().chain(l).map(|p| p.shape).collect()).unwrap_or_default(),
        }
    }
}

// ------------------------------------------------------------------ BROKENOUT

/// A broken-out section: the material inside a freehand boundary drawn on
/// the view is shown cut (hatched), the boundary as a thin break line.
#[derive(Default)]
struct BrokenOutCmd {
    loops: Option<Vec<Vec<V2>>>,
    pts: Vec<V2>,
}

impl BrokenOutCmd {
    fn finish(&mut self, cx: &mut Cx) -> Flow {
        if self.pts.len() < 3 {
            cx.msg("The boundary needs at least three points.");
            return Flow::Continue;
        }
        let part = Region::from_loops(self.loops.as_deref().unwrap_or_default());
        let cut = Region::boolean(&part, &[Region { loops: vec![self.pts.clone()] }], BoolOp::Intersect);
        let loops: Vec<Vec<V2>> = cut.into_iter().flat_map(|r| r.loops).collect();
        if loops.is_empty() {
            cx.msg("The boundary does not cover any of the part.");
            return Flow::Done;
        }
        let scale = cx.tab.doc.ascale();
        cx.checkpoint();
        let mut pieces = vec![Piece {
            layer: layers::HATCH.into(),
            linetype: LtRef::ByLayer,
            lineweight: None,
            shape: Shape::Hatch(Hatch { loops, pattern: "ANSI31".into(), scale, angle: 0.0 }),
        }];
        // The thin freehand break line, only where it crosses the material.
        let ext = BBox::of(self.pts.iter().copied());
        let curve = icad_core::curves::spline_points(&self.pts, true, (ext.width().max(ext.height()) * 0.002).max(1e-6));
        let part_loops = self.loops.as_deref().unwrap_or_default();
        let mut run: Vec<V2> = vec![];
        let mut runs: Vec<Vec<V2>> = vec![];
        for p in curve {
            if icad_core::hatch::region_contains(part_loops, p) {
                run.push(p);
            } else if run.len() >= 2 {
                runs.push(std::mem::take(&mut run));
            } else {
                run.clear();
            }
        }
        if run.len() >= 2 {
            // A run crossing the start of the closed curve continues the first one.
            match runs.first_mut() {
                Some(first)
                    if first.first().is_some_and(|f| run.last().is_some_and(|l| l.dist(*f) < ext.width().max(ext.height()) * 0.05)) =>
                {
                    run.extend(first.drain(..));
                    *first = run;
                }
                _ => runs.push(run),
            }
        }
        for r in runs {
            pieces.push(Piece {
                layer: layers::OUTLINE.into(),
                linetype: LtRef::Lt(LineType::Continuous),
                lineweight: Some(0.25),
                shape: Shape::Polyline { pts: r, closed: false },
            });
        }
        views::add_pieces(cx.doc(), pieces);
        cx.msg("Broken-out section created.");
        Flow::Done
    }
}

impl Command for BrokenOutCmd {
    fn name(&self) -> &'static str {
        "BROKENOUT"
    }
    fn prompt(&self, _: &Cx) -> String {
        match (&self.loops, self.pts.len()) {
            (None, _) => "Select the outline of the part <all>:".into(),
            (_, 0) => "Specify first point of the break boundary:".into(),
            (_, n) if n < 3 => "Specify next point:".into(),
            _ => "Specify next point or [Close] <close>:".into(),
        }
    }
    fn wants(&self) -> Want {
        if self.loops.is_none() {
            Want::Select
        } else {
            Want::Point
        }
    }
    fn base(&self) -> Option<V2> {
        self.pts.last().copied()
    }
    fn selected(&mut self, cx: &mut Cx, ids: Vec<u64>) -> Flow {
        let ids = pick_ids(cx, ids);
        let (loops, _, _) = crate::commands_region::loops_of(&cx.tab.doc, &ids);
        if loops.is_empty() {
            cx.msg("No closed outline found in the selection.");
            return Flow::Done;
        }
        self.loops = Some(loops);
        Flow::Continue
    }
    fn point(&mut self, _: &mut Cx, p: V2) -> Flow {
        self.pts.push(p);
        Flow::Continue
    }
    fn value(&mut self, cx: &mut Cx, s: &str) -> Flow {
        if matches!(s.to_ascii_uppercase().as_str(), "C" | "CLOSE") {
            return self.finish(cx);
        }
        cx.msg(format!("Invalid option: {s}"));
        Flow::Continue
    }
    fn enter(&mut self, cx: &mut Cx) -> Flow {
        if self.loops.is_none() {
            return self.selected(cx, vec![]);
        }
        if self.pts.is_empty() {
            return Flow::Done;
        }
        self.finish(cx)
    }
    fn preview(&self, _: &Document, c: V2) -> Vec<Shape> {
        if self.pts.is_empty() {
            return vec![];
        }
        let mut pts = self.pts.clone();
        pts.push(c);
        vec![Shape::Polyline { pts, closed: true }]
    }
}

// ------------------------------------------------------------------ DETAILVIEW

#[derive(Default)]
struct DetailCmd {
    ids: Option<Vec<u64>>,
    centre: Option<V2>,
    radius: Option<f64>,
    scale: Option<f64>,
}

impl DetailCmd {
    fn pieces(&self, doc: &Document, at: V2) -> Option<Vec<Piece>> {
        let mut v = views::detail_view(
            doc,
            self.ids.as_deref()?,
            self.centre?,
            self.radius?,
            self.scale?,
            at,
            &views::next_view_letter(doc),
            text_height(doc),
        );
        // The view's frame is a thin line in the drawing's own scale.
        v.retain(|p| !matches!(p.shape, Shape::Dim(_)));
        Some(v)
    }
}

impl Command for DetailCmd {
    fn name(&self) -> &'static str {
        "DETAILVIEW"
    }
    fn prompt(&self, cx: &Cx) -> String {
        match (&self.ids, self.centre, self.radius, self.scale) {
            (None, ..) => "Select objects to show in the detail <all>:".into(),
            (_, None, ..) => "Specify centre of the detail area:".into(),
            (_, _, None, _) => "Specify radius of the detail area:".into(),
            (.., None) => format!("Detail scale <{}>:", fmt(cx.mem.detail_scale.unwrap_or(2.0))),
            _ => "Specify location of the detail view:".into(),
        }
    }
    fn wants(&self) -> Want {
        match (&self.ids, self.centre, self.radius, self.scale) {
            (None, ..) => Want::Select,
            (_, Some(_), Some(_), None) => Want::Value,
            _ => Want::Point,
        }
    }
    fn base(&self) -> Option<V2> {
        if self.radius.is_none() {
            self.centre
        } else {
            None
        }
    }
    fn selected(&mut self, cx: &mut Cx, ids: Vec<u64>) -> Flow {
        self.ids = Some(pick_ids(cx, ids));
        Flow::Continue
    }
    fn enter(&mut self, cx: &mut Cx) -> Flow {
        match (&self.ids, self.radius, self.scale) {
            (None, ..) => self.selected(cx, vec![]),
            (_, Some(_), None) => {
                self.scale = Some(cx.mem.detail_scale.unwrap_or(2.0));
                Flow::Continue
            }
            _ => Flow::Done,
        }
    }
    fn value(&mut self, cx: &mut Cx, s: &str) -> Flow {
        if self.centre.is_some() && self.radius.is_none() {
            match num(s).filter(|v| *v > 0.0) {
                Some(v) => self.radius = Some(v),
                None => cx.msg("Enter a positive radius."),
            }
            return Flow::Continue;
        }
        if self.radius.is_some() && self.scale.is_none() {
            // "2", "2:1" or "1:2" all accepted.
            let k = match s.split_once(':') {
                Some((a, b)) => num(a).zip(num(b)).filter(|(_, b)| *b > 0.0).map(|(a, b)| a / b),
                None => num(s),
            };
            match k.filter(|v| *v > 0.0) {
                Some(v) => {
                    self.scale = Some(v);
                    cx.mem.detail_scale = Some(v);
                }
                None => cx.msg("Enter a positive scale, such as 2 or 5:1."),
            }
            return Flow::Continue;
        }
        cx.msg(format!("Invalid option: {s}"));
        Flow::Continue
    }
    fn point(&mut self, cx: &mut Cx, p: V2) -> Flow {
        match (self.centre, self.radius) {
            (None, _) => self.centre = Some(p),
            (Some(c), None) => {
                if c.dist(p) > 1e-9 {
                    self.radius = Some(c.dist(p));
                }
            }
            _ if self.scale.is_some() => {
                let Some(v) = self.pieces(&cx.tab.doc, p) else { return Flow::Done };
                let letter = views::next_view_letter(&cx.tab.doc);
                cx.checkpoint();
                views::add_pieces(cx.doc(), v);
                cx.msg(format!("Detail {letter} created."));
                return Flow::Done;
            }
            _ => {}
        }
        Flow::Continue
    }
    fn preview(&self, doc: &Document, c: V2) -> Vec<Shape> {
        match (self.centre, self.radius, self.scale) {
            (Some(o), None, _) => vec![Shape::Circle { c: o, r: o.dist(c).max(1e-9) }],
            (Some(o), Some(r), None) => vec![Shape::Circle { c: o, r }],
            _ => self.pieces(doc, c).map(|v| v.into_iter().map(|p| p.shape).collect()).unwrap_or_default(),
        }
    }
}
