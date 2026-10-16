// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Proteen Das

//! SECTIONVIEW (a section from a cutting line through a part of constant
//! thickness) and DETAILVIEW (an enlarged view of a circular area).

use crate::commands::{fmt, num, Command, Cx, Flow, Want};
use icad_core::entity::{Curve, Shape};
use icad_core::geom::*;
use icad_core::views::{self, Piece};
use icad_core::Document;

pub fn create(name: &str) -> Option<Box<dyn Command>> {
    let c: Box<dyn Command> = match name {
        "SECTIONVIEW" | "SECTION" | "VIEWSECTION" => Box::new(SectionCmd::default()),
        "DETAILVIEW" | "DETAIL" | "VIEWDETAIL" => Box::new(DetailCmd::default()),
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
    a: Option<V2>,
    b: Option<V2>,
    depth: Option<f64>,
    /// Arrows toward the view (first-angle habit) instead of away from it.
    toward: bool,
}

impl SectionCmd {
    /// Section and cutting line with the view centred at `at`.
    fn pieces(&self, doc: &Document, at: V2) -> Option<(Vec<Piece>, Vec<Piece>)> {
        let (mut a, mut b, depth) = (self.a?, self.b?, self.depth?);
        // The section reads left to right (or bottom to top).
        let u = b - a;
        if u.x < -1e-9 || (u.x.abs() <= 1e-9 && u.y < 0.0) {
            std::mem::swap(&mut a, &mut b);
        }
        let h = text_height(doc);
        let letter = views::next_view_letter(doc);
        let mut sec = views::section_view(&self.curves, a, b, depth, V2::ZERO, &letter, h);
        let spans = views::cut_intervals(&self.curves, a, b);
        let (first, last) = (spans.first()?, spans.last()?);
        let centre = V2::new((first.0 + last.1) / 2.0, depth / 2.0);
        let rot = (b - a).angle();
        for p in sec.iter_mut() {
            if let Shape::Hatch(hh) = &mut p.shape {
                hh.scale = doc.ascale();
            }
        }
        place(&mut sec, &Xform::translate(-centre));
        place(&mut sec, &Xform::rotate(V2::ZERO, rot));
        place(&mut sec, &Xform::translate(at));
        // Arrows point away from the view (or toward it).
        let n = (b - a).norm().perp();
        let side = (at - a).dot(n);
        let away = if side >= 0.0 { -n } else { n };
        let view = if self.toward { -away } else { away };
        Some((sec, views::cutting_line(a, b, view, &letter, h)))
    }
}

impl Command for SectionCmd {
    fn name(&self) -> &'static str {
        "SECTIONVIEW"
    }
    fn prompt(&self, cx: &Cx) -> String {
        match (&self.ids, self.a, self.b, self.depth) {
            (None, ..) => "Select the boundaries of the part to cut <all>:".into(),
            (_, None, ..) => "Specify start point of the cutting line (outside the part):".into(),
            (_, _, None, _) => "Specify end point of the cutting line:".into(),
            (.., None) => {
                let d = cx.mem.section_depth.map(fmt).unwrap_or_else(|| "10".into());
                format!("Part thickness (depth of the section) <{d}>:")
            }
            _ => format!("Specify location of the section view or [Arrows {}]:", if self.toward { "toward" } else { "away" }),
        }
    }
    fn wants(&self) -> Want {
        match (&self.ids, self.a, self.b, self.depth) {
            (None, ..) => Want::Select,
            (_, _, Some(_), None) => Want::Value,
            _ => Want::Point,
        }
    }
    fn base(&self) -> Option<V2> {
        if self.b.is_none() {
            self.a
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
        match (&self.ids, self.b, self.depth) {
            (None, ..) => self.selected(cx, vec![]),
            (_, Some(_), None) => {
                self.depth = Some(cx.mem.section_depth.unwrap_or(10.0));
                Flow::Continue
            }
            _ => Flow::Done,
        }
    }
    fn value(&mut self, cx: &mut Cx, s: &str) -> Flow {
        if self.b.is_some() && self.depth.is_none() {
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
        if self.a.is_none() {
            self.a = Some(p);
            return Flow::Continue;
        }
        if self.b.is_none() {
            if self.a.is_some_and(|a| a.dist(p) < 1e-9) {
                return Flow::Continue;
            }
            self.b = Some(p);
            if views::cut_intervals(&self.curves, self.a.unwrap_or_default(), p).is_empty() {
                cx.msg("The cutting line does not pass through the part.");
                self.b = None;
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
        match (self.a, self.b, self.depth) {
            (Some(a), None, _) => vec![Shape::Line { a, b: c }],
            (Some(a), Some(b), None) => vec![Shape::Line { a, b }],
            _ => self.pieces(doc, c).map(|(s, l)| s.into_iter().chain(l).map(|p| p.shape).collect()).unwrap_or_default(),
        }
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
