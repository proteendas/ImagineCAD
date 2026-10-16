// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Proteen Das

//! REVCLOUD (revision clouds), BOM (bill of materials table) and BALLOON
//! (item balloons matching the BOM), and DIMARRANGE (dimension text kept
//! clear of other annotation).

use crate::commands::{dim_layer, fmt, num, rect_pts, Command, Cx, Flow, Want};
use icad_core::entity::{Leader, Shape};
use icad_core::geom::*;
use icad_core::markup::RevCloud;
use icad_core::Document;

pub fn create(name: &str) -> Option<Box<dyn Command>> {
    let c: Box<dyn Command> = match name {
        "REVCLOUD" | "RC" => Box::new(RevCloudCmd::default()),
        "BOM" | "PARTSLIST" | "BILLOFMATERIALS" => Box::new(BomCmd { table: None }),
        "BALLOON" | "BAL" => Box::new(BalloonCmd::default()),
        "DIMARRANGE" | "DARR" | "DIMDECLUTTER" => Box::new(ArrangeCmd),
        _ => return None,
    };
    Some(c)
}

// ------------------------------------------------------------------ REVCLOUD

#[derive(Clone, Copy, Default, PartialEq)]
enum CloudMode {
    #[default]
    Rectangle,
    Polygon,
    Object,
}

#[derive(Default)]
struct RevCloudCmd {
    mode: CloudMode,
    pts: Vec<V2>,
    /// Arc length; 0 until known (from the sheet scale).
    arc: f64,
    asking_arc: bool,
}

impl RevCloudCmd {
    fn arc(&self, doc: &Document) -> f64 {
        if self.arc > 0.0 {
            self.arc
        } else {
            12.0 * doc.ascale()
        }
    }
    fn finish(&mut self, cx: &mut Cx, pts: Vec<V2>) -> Flow {
        if pts.len() < 3 {
            cx.msg("A revision cloud needs at least three corners.");
            return Flow::Continue;
        }
        let arc = self.arc(&cx.tab.doc);
        cx.checkpoint();
        let layer = cx.tab.doc.current_layer.clone();
        cx.doc().add_on(&layer, Shape::RevCloud(RevCloud { pts, arc }));
        Flow::Done
    }
}

impl Command for RevCloudCmd {
    fn name(&self) -> &'static str {
        "REVCLOUD"
    }
    fn prompt(&self, cx: &Cx) -> String {
        if self.asking_arc {
            return format!("Arc length <{}>:", fmt(self.arc(&cx.tab.doc)));
        }
        let opts = format!("[Arc length ({})/Rectangular/Polygonal/Object]", fmt(self.arc(&cx.tab.doc)));
        match (self.mode, self.pts.len()) {
            (CloudMode::Object, _) => "Select a closed polyline, rectangle or circle to turn into a cloud:".into(),
            (CloudMode::Rectangle, 0) => format!("Specify first corner or {opts}:"),
            (CloudMode::Rectangle, _) => "Specify opposite corner:".into(),
            (CloudMode::Polygon, 0) => format!("Specify start point or {opts}:"),
            (CloudMode::Polygon, n) if n < 3 => "Specify next point:".into(),
            (CloudMode::Polygon, _) => "Specify next point or [Close] <close>:".into(),
        }
    }
    fn wants(&self) -> Want {
        if self.asking_arc {
            Want::Value
        } else if self.mode == CloudMode::Object {
            Want::Entity
        } else {
            Want::Point
        }
    }
    fn base(&self) -> Option<V2> {
        self.pts.last().copied()
    }
    fn point(&mut self, cx: &mut Cx, p: V2) -> Flow {
        self.pts.push(p);
        if self.mode == CloudMode::Rectangle && self.pts.len() == 2 {
            let pts = rect_pts(self.pts[0], self.pts[1]);
            return self.finish(cx, pts);
        }
        Flow::Continue
    }
    fn value(&mut self, cx: &mut Cx, s: &str) -> Flow {
        if self.asking_arc {
            match num(s).filter(|v| *v > 0.0) {
                Some(v) => {
                    self.arc = v;
                    self.asking_arc = false;
                }
                None => cx.msg("Enter a positive length."),
            }
            return Flow::Continue;
        }
        match s.to_ascii_uppercase().as_str() {
            "A" | "ARC" => self.asking_arc = true,
            "R" | "RECTANGULAR" => {
                self.mode = CloudMode::Rectangle;
                self.pts.clear();
            }
            "P" | "POLYGONAL" => {
                self.mode = CloudMode::Polygon;
                self.pts.clear();
            }
            "O" | "OBJECT" => self.mode = CloudMode::Object,
            "C" | "CLOSE" if self.mode == CloudMode::Polygon => {
                let pts = std::mem::take(&mut self.pts);
                return self.finish(cx, pts);
            }
            _ => cx.msg(format!("Invalid option: {s}")),
        }
        Flow::Continue
    }
    fn enter(&mut self, cx: &mut Cx) -> Flow {
        if self.asking_arc {
            self.asking_arc = false;
            return Flow::Continue;
        }
        if self.mode == CloudMode::Polygon && self.pts.len() >= 3 {
            let pts = std::mem::take(&mut self.pts);
            return self.finish(cx, pts);
        }
        Flow::Done
    }
    fn entity(&mut self, cx: &mut Cx, id: u64, _at: V2) -> Flow {
        let Some(e) = cx.tab.doc.get(id) else { return Flow::Continue };
        let pts = match &e.shape {
            Shape::Polyline { pts, closed: true } => pts.clone(),
            Shape::Circle { c, r } => (0..24).map(|i| *c + V2::polar(*r, std::f64::consts::TAU * i as f64 / 24.0)).collect(),
            Shape::Ellipse { c, major, ratio, .. } => {
                let minor = major.perp() * *ratio;
                (0..24)
                    .map(|i| {
                        let t = std::f64::consts::TAU * i as f64 / 24.0;
                        *c + *major * t.cos() + minor * t.sin()
                    })
                    .collect()
            }
            _ => {
                cx.msg("Select a closed polyline, rectangle, polygon, circle or ellipse.");
                return Flow::Continue;
            }
        };
        let arc = self.arc(&cx.tab.doc);
        cx.checkpoint();
        let layer = cx.tab.doc.get(id).map(|e| e.layer.clone()).unwrap_or_default();
        cx.doc().remove(&[id]);
        cx.doc().add_on(&layer, Shape::RevCloud(RevCloud { pts, arc }));
        Flow::Done
    }
    fn preview(&self, doc: &Document, c: V2) -> Vec<Shape> {
        let arc = self.arc(doc);
        match (self.mode, self.pts.as_slice()) {
            (CloudMode::Rectangle, [a]) => vec![Shape::RevCloud(RevCloud { pts: rect_pts(*a, c), arc })],
            (CloudMode::Polygon, pts) if !pts.is_empty() => {
                let mut v = pts.to_vec();
                v.push(c);
                if v.len() >= 3 {
                    vec![Shape::RevCloud(RevCloud { pts: v, arc })]
                } else {
                    vec![Shape::Line { a: v[0], b: c }]
                }
            }
            _ => vec![],
        }
    }
}

// ------------------------------------------------------------------ BOM

struct BomCmd {
    table: Option<icad_core::table::Table>,
}

impl Command for BomCmd {
    fn name(&self) -> &'static str {
        "BOM"
    }
    fn prompt(&self, _: &Cx) -> String {
        "Specify insertion point of the bill of materials (top-left corner):".into()
    }
    fn wants(&self) -> Want {
        Want::Point
    }
    fn start(&mut self, cx: &mut Cx) -> Flow {
        let th = 2.5 * cx.tab.doc.ascale();
        let Some(t) = cx.tab.doc.bom_table(V2::ZERO, th) else {
            cx.msg("No parts to list: insert blocks for the parts (with PART_NO, DESCRIPTION, MATERIAL… attributes if you like).");
            return Flow::Done;
        };
        // An existing BOM is refreshed where it is.
        let existing = cx.tab.doc.entities.iter().find(|e| matches!(&e.shape, Shape::Table(x) if x.tag == "BOM")).map(|e| e.id);
        if let Some(id) = existing {
            let items = t.rows().saturating_sub(2);
            cx.checkpoint();
            if let Some(e) = cx.doc().get_mut(id) {
                if let Shape::Table(old) = &mut e.shape {
                    let pos = old.pos;
                    *old = t;
                    old.pos = pos;
                }
            }
            cx.msg(format!("Bill of materials updated: {items} item(s)."));
            return Flow::Done;
        }
        self.table = Some(t);
        Flow::Continue
    }
    fn point(&mut self, cx: &mut Cx, p: V2) -> Flow {
        let Some(mut t) = self.table.take() else { return Flow::Done };
        t.pos = p;
        cx.checkpoint();
        let layer = dim_layer(cx);
        cx.doc().add_on(&layer, Shape::Table(t));
        cx.msg("Bill of materials placed. Run BOM again to refresh it, BALLOON to number the parts.");
        Flow::Done
    }
    fn preview(&self, _: &Document, c: V2) -> Vec<Shape> {
        self.table
            .clone()
            .map(|mut t| {
                t.pos = c;
                vec![Shape::Table(t)]
            })
            .unwrap_or_default()
    }
}

// ------------------------------------------------------------------ BALLOON

#[derive(Default)]
struct BalloonCmd {
    /// Item number and leader tip once a part is picked.
    picked: Option<(usize, V2)>,
}

fn is_balloon(s: &Shape) -> bool {
    matches!(s, Shape::Leader(l) if l.style.as_deref() == Some("Balloon") && l.text.trim().parse::<usize>().is_ok())
}

impl Command for BalloonCmd {
    fn name(&self) -> &'static str {
        "BALLOON"
    }
    fn prompt(&self, _: &Cx) -> String {
        match self.picked {
            None => "Select a part (block reference) or [Auto] <Auto: balloon every BOM item>:".into(),
            Some((n, _)) => format!("Specify balloon location for item {n}:"),
        }
    }
    fn wants(&self) -> Want {
        if self.picked.is_some() {
            Want::Point
        } else {
            Want::Entity
        }
    }
    fn base(&self) -> Option<V2> {
        self.picked.map(|p| p.1)
    }
    fn entity(&mut self, cx: &mut Cx, id: u64, at: V2) -> Flow {
        let bom = cx.tab.doc.bom();
        match bom.iter().find(|it| it.refs.contains(&id)) {
            Some(it) => self.picked = Some((it.item, at)),
            None => cx.msg("That is not a part in the bill of materials (pick a block reference)."),
        }
        Flow::Continue
    }
    fn point(&mut self, cx: &mut Cx, p: V2) -> Flow {
        let Some((n, tip)) = self.picked.take() else { return Flow::Continue };
        cx.checkpoint();
        let layer = dim_layer(cx);
        cx.doc().add_on(&layer, Shape::Leader(Leader { pts: vec![tip, p], text: n.to_string(), style: Some("Balloon".into()) }));
        Flow::Continue
    }
    fn value(&mut self, cx: &mut Cx, s: &str) -> Flow {
        if s.eq_ignore_ascii_case("A") || s.eq_ignore_ascii_case("AUTO") {
            return self.enter(cx);
        }
        cx.msg(format!("Invalid option: {s}"));
        Flow::Continue
    }
    fn enter(&mut self, cx: &mut Cx) -> Flow {
        if self.picked.is_some() {
            self.picked = None;
            return Flow::Continue;
        }
        let gap = 18.0 * cx.tab.doc.ascale();
        let balloons = cx.tab.doc.bom_balloons(gap);
        if balloons.is_empty() {
            cx.msg("No parts to balloon: insert blocks for the parts first.");
            return Flow::Done;
        }
        cx.checkpoint();
        // Automatic balloons replace the previous automatic ones.
        let old: Vec<u64> = cx.tab.doc.entities.iter().filter(|e| is_balloon(&e.shape)).map(|e| e.id).collect();
        cx.doc().remove(&old);
        let layer = dim_layer(cx);
        let n = balloons.len();
        for (_, l) in balloons {
            cx.doc().add_on(&layer, Shape::Leader(l));
        }
        cx.msg(format!("{n} balloon(s) placed, numbered as in the bill of materials."));
        Flow::Done
    }
}

// ------------------------------------------------------------------ DIMARRANGE

/// Move dimension text clear of other text and of the part.
struct ArrangeCmd;

impl Command for ArrangeCmd {
    fn name(&self) -> &'static str {
        "DIMARRANGE"
    }
    fn prompt(&self, _: &Cx) -> String {
        "Select dimensions to tidy <all>:".into()
    }
    fn wants(&self) -> Want {
        Want::Select
    }
    fn selected(&mut self, cx: &mut Cx, ids: Vec<u64>) -> Flow {
        let picked = !ids.is_empty();
        let dims: Vec<u64> = ids.into_iter().filter(|id| matches!(cx.tab.doc.get(*id).map(|e| &e.shape), Some(Shape::Dim(_)))).collect();
        if picked && dims.is_empty() {
            cx.msg("No dimensions in the selection.");
            return Flow::Done;
        }
        cx.checkpoint();
        let n = cx.doc().declutter_dims(&dims);
        cx.msg(format!("DIMARRANGE moved {n} dimension text(s) clear of other annotation and the part."));
        Flow::Done
    }
    fn enter(&mut self, cx: &mut Cx) -> Flow {
        self.selected(cx, vec![])
    }
}
