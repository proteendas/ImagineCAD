// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Proteen Das

//! Additional drafting commands: curves, text, hatching, blocks, advanced
//! dimensions, modify and inquiry tools.

use crate::commands::{dim_layer, fmt, num, rect_pts, Command, Cx, Flow, Want};
use icad_core::document::layers;
use icad_core::entity::*;
use icad_core::geom::*;
use icad_core::hatch;
use icad_core::ops;
use icad_core::prim::ImageRef;
use icad_core::Document;

pub fn create(name: &str) -> Option<Box<dyn Command>> {
    let c: Box<dyn Command> = match name {
        "ELLIPSE" | "EL" => Box::new(EllipseCmd::default()),
        "SPLINE" | "SPL" => Box::new(SplineCmd::default()),
        "POINT" | "PO" => Box::new(PointCmd),
        "XLINE" | "XL" => Box::new(XLineCmd { ray: false, p: None, fixed: None }),
        "RAY" => Box::new(XLineCmd { ray: true, p: None, fixed: None }),
        "MTEXT" | "MT" | "T" => Box::new(MTextCmd::default()),
        "LEADER" | "LE" | "QLEADER" => Box::new(LeaderCmd::default()),
        "MLEADER" | "MLD" => Box::new(LeaderCmd { multi: true, ..Default::default() }),
        "DIMANGULAR" | "DAN" => Box::new(DimAngCmd::default()),
        "DIMBASELINE" | "DBA" => Box::new(DimChainCmd { baseline: true }),
        "DIMCONTINUE" | "DCO" => Box::new(DimChainCmd { baseline: false }),
        "HATCH" | "H" | "BH" | "BHATCH" => Box::new(HatchCmd::default()),
        "BLOCK" | "B" | "BMAKE" => Box::new(BlockCmd::default()),
        "INSERT" | "I" | "DDINSERT" => Box::new(InsertCmd::default()),
        "STRETCH" | "S" => Box::new(StretchCmd::default()),
        "BREAK" | "BR" => Box::new(BreakCmd { at_point: false, target: None, first: None, pick_first: false }),
        "BREAKATPOINT" | "BAP" => Box::new(BreakCmd { at_point: true, target: None, first: None, pick_first: false }),
        "JOIN" | "J" => Box::new(JoinCmd),
        "LENGTHEN" | "LEN" => Box::new(LengthenCmd { setting: false }),
        "ARRAYPOLAR" | "APO" => Box::new(PolarArrayCmd::default()),
        "DIVIDE" | "DIV" => Box::new(DivideCmd { measure: false, target: None }),
        "MEASURE" | "ME" => Box::new(DivideCmd { measure: true, target: None }),
        "AREA" | "AA" => Box::new(AreaCmd::default()),
        "ID" => Box::new(IdCmd),
        "LIST" | "LI" | "LS" => Box::new(ListCmd),
        "OVERKILL" => Box::new(OverkillCmd),
        "LAYOFF" | "LAYFRZ" => Box::new(LayerPick { op: LayOp::Off }),
        "LAYLCK" => Box::new(LayerPick { op: LayOp::Lock }),
        "LAYULK" => Box::new(LayerPick { op: LayOp::Unlock }),
        "LAYMCUR" => Box::new(LayerPick { op: LayOp::MakeCurrent }),
        "LAYISO" => Box::new(LayerPick { op: LayOp::Isolate }),
        other => return crate::commands_dim::create(other),
    };
    Some(c)
}

// ------------------------------------------------------------------ curves

#[derive(Default)]
struct EllipseCmd {
    center_mode: bool,
    pts: Vec<V2>,
}

impl EllipseCmd {
    fn axis(&self) -> Option<(V2, V2)> {
        match (self.center_mode, self.pts.len()) {
            (false, 2) => Some((self.pts[0].mid(self.pts[1]), (self.pts[1] - self.pts[0]) / 2.0)),
            (true, 2) => Some((self.pts[0], self.pts[1] - self.pts[0])),
            _ => None,
        }
    }
    fn shape(&self, dist: f64) -> Option<Shape> {
        let (c, major) = self.axis()?;
        let a = major.len();
        if a < 1e-12 || dist < 1e-12 {
            return None;
        }
        let (major, ratio) = if dist <= a { (major, dist / a) } else { (major.norm().perp() * dist, a / dist) };
        Some(Shape::Ellipse { c, major, ratio, start: 0.0, end: std::f64::consts::TAU })
    }
    fn finish(&mut self, cx: &mut Cx, d: f64) -> Flow {
        if let Some(s) = self.shape(d) {
            cx.checkpoint();
            cx.doc().add(s);
        }
        Flow::Done
    }
}

impl Command for EllipseCmd {
    fn name(&self) -> &'static str {
        "ELLIPSE"
    }
    fn prompt(&self, _: &Cx) -> String {
        match (self.center_mode, self.pts.len()) {
            (false, 0) => "Specify axis endpoint of ellipse or [Center]:".into(),
            (false, 1) => "Specify other endpoint of axis:".into(),
            (true, 0) => "Specify center of ellipse:".into(),
            (true, 1) => "Specify endpoint of axis:".into(),
            _ => "Specify distance to other axis:".into(),
        }
    }
    fn wants(&self) -> Want {
        Want::Point
    }
    fn base(&self) -> Option<V2> {
        match self.axis() {
            Some((c, _)) => Some(c),
            None => self.pts.last().copied(),
        }
    }
    fn point(&mut self, cx: &mut Cx, p: V2) -> Flow {
        if self.pts.len() < 2 {
            self.pts.push(p);
            return Flow::Continue;
        }
        let (c, major) = self.axis().unwrap();
        let d = (p - c).dot(major.norm().perp()).abs();
        self.finish(cx, d)
    }
    fn value(&mut self, cx: &mut Cx, s: &str) -> Flow {
        if self.pts.is_empty() && s.eq_ignore_ascii_case("C") {
            self.center_mode = true;
            return Flow::Continue;
        }
        if self.pts.len() == 2 {
            if let Some(d) = num(s).filter(|d| *d > 0.0) {
                return self.finish(cx, d);
            }
        }
        cx.msg(format!("Invalid input: {s}"));
        Flow::Continue
    }
    fn preview(&self, _: &Document, cur: V2) -> Vec<Shape> {
        match self.axis() {
            Some((c, major)) => self.shape((cur - c).dot(major.norm().perp()).abs()).into_iter().collect(),
            None => self.pts.last().map(|a| vec![Shape::Line { a: *a, b: cur }]).unwrap_or_default(),
        }
    }
}

#[derive(Default)]
struct SplineCmd {
    pts: Vec<V2>,
}

impl SplineCmd {
    fn finish(&mut self, cx: &mut Cx, closed: bool) -> Flow {
        if self.pts.len() >= 2 {
            cx.checkpoint();
            cx.doc().add(Shape::Spline { pts: std::mem::take(&mut self.pts), closed });
        }
        Flow::Done
    }
}

impl Command for SplineCmd {
    fn name(&self) -> &'static str {
        "SPLINE"
    }
    fn prompt(&self, _: &Cx) -> String {
        match self.pts.len() {
            0 => "Specify first fit point:".into(),
            1 | 2 => "Enter next fit point or [Undo]:".into(),
            _ => "Enter next fit point or [Close/Undo] <finish>:".into(),
        }
    }
    fn wants(&self) -> Want {
        Want::Point
    }
    fn base(&self) -> Option<V2> {
        self.pts.last().copied()
    }
    fn point(&mut self, _: &mut Cx, p: V2) -> Flow {
        self.pts.push(p);
        Flow::Continue
    }
    fn enter(&mut self, cx: &mut Cx) -> Flow {
        self.finish(cx, false)
    }
    fn value(&mut self, cx: &mut Cx, s: &str) -> Flow {
        match s.to_ascii_uppercase().as_str() {
            "C" | "CLOSE" if self.pts.len() >= 3 => self.finish(cx, true),
            "U" | "UNDO" => {
                self.pts.pop();
                Flow::Continue
            }
            _ => {
                cx.msg(format!("Invalid option: {s}"));
                Flow::Continue
            }
        }
    }
    fn preview(&self, _: &Document, c: V2) -> Vec<Shape> {
        if self.pts.is_empty() {
            return vec![];
        }
        let mut p = self.pts.clone();
        p.push(c);
        vec![Shape::Spline { pts: p, closed: false }]
    }
}

struct PointCmd;

impl Command for PointCmd {
    fn name(&self) -> &'static str {
        "POINT"
    }
    fn prompt(&self, _: &Cx) -> String {
        "Specify a point <exit>:".into()
    }
    fn wants(&self) -> Want {
        Want::Point
    }
    fn point(&mut self, cx: &mut Cx, p: V2) -> Flow {
        cx.checkpoint();
        cx.doc().add(Shape::Point { p });
        Flow::Continue
    }
}

struct XLineCmd {
    ray: bool,
    p: Option<V2>,
    /// Fixed direction (H/V option).
    fixed: Option<V2>,
}

impl Command for XLineCmd {
    fn name(&self) -> &'static str {
        if self.ray {
            "RAY"
        } else {
            "XLINE"
        }
    }
    fn prompt(&self, _: &Cx) -> String {
        match (self.p, self.ray, self.fixed) {
            (None, true, _) => "Specify start point:".into(),
            (None, false, None) => "Specify a point or [Hor/Ver]:".into(),
            (None, false, Some(_)) | (Some(_), false, Some(_)) => "Specify through point <exit>:".into(),
            (Some(_), ..) => "Specify through point <exit>:".into(),
        }
    }
    fn wants(&self) -> Want {
        Want::Point
    }
    fn base(&self) -> Option<V2> {
        self.p
    }
    fn point(&mut self, cx: &mut Cx, p: V2) -> Flow {
        let layer = construction_layer(cx);
        if let Some(dir) = self.fixed {
            cx.checkpoint();
            cx.doc().add_on(&layer, Shape::XLine { p, dir });
            return Flow::Continue;
        }
        match self.p {
            None => self.p = Some(p),
            Some(o) if o.dist(p) > 1e-12 => {
                cx.checkpoint();
                let dir = (p - o).norm();
                let s = if self.ray { Shape::Ray { p: o, dir } } else { Shape::XLine { p: o, dir } };
                cx.doc().add_on(&layer, s);
            }
            _ => {}
        }
        Flow::Continue
    }
    fn value(&mut self, cx: &mut Cx, s: &str) -> Flow {
        match s.to_ascii_uppercase().as_str() {
            "H" | "HOR" if !self.ray => self.fixed = Some(V2::new(1.0, 0.0)),
            "V" | "VER" if !self.ray => self.fixed = Some(V2::new(0.0, 1.0)),
            _ => cx.msg(format!("Invalid option: {s}")),
        }
        Flow::Continue
    }
    fn preview(&self, _: &Document, c: V2) -> Vec<Shape> {
        if let Some(dir) = self.fixed {
            return vec![Shape::XLine { p: c, dir }];
        }
        match self.p {
            Some(o) if o.dist(c) > 1e-12 => {
                let dir = (c - o).norm();
                vec![if self.ray { Shape::Ray { p: o, dir } } else { Shape::XLine { p: o, dir } }]
            }
            _ => vec![],
        }
    }
}

fn construction_layer(cx: &Cx) -> String {
    if cx.tab.doc.layer(layers::CONSTRUCTION).is_some() {
        layers::CONSTRUCTION.into()
    } else {
        cx.tab.doc.current_layer.clone()
    }
}

// ------------------------------------------------------------------- text

#[derive(Default)]
struct MTextCmd {
    a: Option<V2>,
    b: Option<V2>,
}

impl Command for MTextCmd {
    fn name(&self) -> &'static str {
        "MTEXT"
    }
    fn prompt(&self, _: &Cx) -> String {
        match (self.a, self.b) {
            (None, _) => "Specify first corner:".into(),
            (Some(_), None) => "Specify opposite corner (sets the wrap width):".into(),
            _ => "Enter text (use \\P for a new line; double-click later to edit):".into(),
        }
    }
    fn wants(&self) -> Want {
        if self.b.is_some() {
            Want::Text
        } else {
            Want::Point
        }
    }
    fn base(&self) -> Option<V2> {
        self.a
    }
    fn point(&mut self, _: &mut Cx, p: V2) -> Flow {
        if self.a.is_none() {
            self.a = Some(p);
        } else {
            self.b = Some(p);
        }
        Flow::Continue
    }
    fn value(&mut self, cx: &mut Cx, s: &str) -> Flow {
        let (Some(a), Some(b)) = (self.a, self.b) else { return Flow::Continue };
        if !s.is_empty() {
            let h = cx.mem.text_h.unwrap_or(cx.tab.doc.dimstyle.text_height * cx.tab.doc.ascale());
            let pos = V2::new(a.x.min(b.x), a.y.max(b.y));
            let content = s.replace("\\P", "\n").replace("\\p", "\n");
            cx.checkpoint();
            let layer = if cx.tab.doc.layer(layers::TEXT).is_some() { layers::TEXT.to_string() } else { cx.tab.doc.current_layer.clone() };
            let (style, fixed) = cx.text_style();
            let mut m = MText::new(pos, fixed.unwrap_or(h), (a.x - b.x).abs(), content);
            m.style = Some(style);
            cx.doc().add_on(&layer, Shape::MText(m));
        }
        Flow::Done
    }
    fn enter(&mut self, _: &mut Cx) -> Flow {
        Flow::Done
    }
    fn preview(&self, _: &Document, c: V2) -> Vec<Shape> {
        match (self.a, self.b) {
            (Some(a), None) => vec![Shape::Polyline { pts: rect_pts(a, c), closed: true }],
            _ => vec![],
        }
    }
}

#[derive(Default)]
struct LeaderCmd {
    pts: Vec<V2>,
    texting: bool,
    /// MLEADER: use the drawing's current multileader style.
    multi: bool,
    style: Option<String>,
}

impl Command for LeaderCmd {
    fn name(&self) -> &'static str {
        if self.multi {
            "MLEADER"
        } else {
            "LEADER"
        }
    }
    fn start(&mut self, cx: &mut Cx) -> Flow {
        if self.multi {
            self.style = Some(cx.tab.doc.mleader_style.clone());
        }
        Flow::Continue
    }
    fn prompt(&self, _: &Cx) -> String {
        if self.texting {
            "Enter annotation text (\\P for a new line):".into()
        } else if self.pts.is_empty() {
            "Specify leader arrowhead location:".into()
        } else {
            "Specify next point <annotation>:".into()
        }
    }
    fn wants(&self) -> Want {
        if self.texting {
            Want::Text
        } else {
            Want::Point
        }
    }
    fn base(&self) -> Option<V2> {
        self.pts.last().copied()
    }
    fn point(&mut self, _: &mut Cx, p: V2) -> Flow {
        self.pts.push(p);
        Flow::Continue
    }
    fn enter(&mut self, cx: &mut Cx) -> Flow {
        if !self.texting && self.pts.len() >= 2 {
            self.texting = true;
            return Flow::Continue;
        }
        if self.texting {
            return self.value(cx, "");
        }
        Flow::Done
    }
    fn value(&mut self, cx: &mut Cx, s: &str) -> Flow {
        if !self.texting {
            cx.msg(format!("Invalid input: {s}"));
            return Flow::Continue;
        }
        cx.checkpoint();
        let layer = dim_layer(cx);
        cx.doc().add_on(
            &layer,
            Shape::Leader(Leader { pts: std::mem::take(&mut self.pts), text: s.replace("\\P", "\n"), style: self.style.clone() }),
        );
        Flow::Done
    }
    fn preview(&self, _: &Document, c: V2) -> Vec<Shape> {
        if self.pts.is_empty() || self.texting {
            return vec![];
        }
        let mut p = self.pts.clone();
        p.push(c);
        vec![Shape::Leader(Leader { pts: p, text: String::new(), style: self.style.clone() })]
    }
}

// ------------------------------------------------------------- dimensions

#[derive(Default)]
struct DimAngCmd {
    first: Option<(V2, V2, V2)>, // line a, b, pick
    dim: Option<(V2, V2, V2)>,   // vertex, p1, p2
}

impl DimAngCmd {
    fn make(&self, loc: V2) -> Option<Dimension> {
        let (c, p1, p2) = self.dim?;
        Some(Dimension::angular(c, p1, p2, loc))
    }
}

impl Command for DimAngCmd {
    fn name(&self) -> &'static str {
        "DIMANGULAR"
    }
    fn prompt(&self, _: &Cx) -> String {
        match (self.first, self.dim) {
            (_, Some(_)) => "Specify dimension arc line location:".into(),
            (None, _) => "Select arc, circle or first line:".into(),
            (Some(_), None) => "Select second line:".into(),
        }
    }
    fn wants(&self) -> Want {
        if self.dim.is_some() {
            Want::Point
        } else {
            Want::Entity
        }
    }
    fn entity(&mut self, cx: &mut Cx, id: u64, at: V2) -> Flow {
        match (cx.shape(id), self.first) {
            (Some(Shape::Arc { c, r, start, end }), None) => {
                self.dim = Some((c, c + V2::polar(r, start), c + V2::polar(r, end)));
            }
            (Some(Shape::Line { a, b }), None) => self.first = Some((a, b, at)),
            (Some(Shape::Line { a, b }), Some((a1, b1, at1))) => match line_line(a1, b1, a, b) {
                Some((c, _, _)) => {
                    let on = |p: V2, a: V2, b: V2| a.lerp(b, project_t(p, a, b));
                    let (q1, q2) = (on(at1, a1, b1), on(at, a, b));
                    let far = |q: V2, a: V2, b: V2| {
                        if q.dist(c) > 1e-9 {
                            q
                        } else if a.dist(c) > b.dist(c) {
                            a
                        } else {
                            b
                        }
                    };
                    self.dim = Some((c, far(q1, a1, b1), far(q2, a, b)));
                }
                None => cx.msg("Lines are parallel."),
            },
            _ => cx.msg("Select a line or an arc."),
        }
        Flow::Continue
    }
    fn point(&mut self, cx: &mut Cx, p: V2) -> Flow {
        if let Some(d) = self.make(p) {
            cx.checkpoint();
            let m = d.measurement();
            cx.add_dim(d);
            cx.msg(format!("Dimension text = {}°", fmt(m)));
        }
        Flow::Done
    }
    fn preview(&self, _: &Document, c: V2) -> Vec<Shape> {
        self.make(c).map(|d| vec![Shape::Dim(d)]).unwrap_or_default()
    }
}

/// DIMBASELINE / DIMCONTINUE from the last linear or angular dimension.
struct DimChainCmd {
    baseline: bool,
}

impl DimChainCmd {
    fn base_dim(&self, cx: &Cx) -> Option<Dimension> {
        match cx.mem.last_dim.and_then(|id| cx.tab.doc.get(id)).map(|e| e.shape.clone()) {
            Some(Shape::Dim(d)) if !matches!(d.kind, DimKind::Radius | DimKind::Diameter) => Some(d),
            _ => None,
        }
    }
    fn next(&self, cx: &Cx, prev: &Dimension, p: V2) -> Dimension {
        let spacing = 2.4 * cx.tab.doc.dimstyle.text_height * cx.tab.doc.ascale();
        let mut d = prev.clone();
        d.text = None;
        if prev.kind == DimKind::Angular {
            let c = prev.center.unwrap_or(V2::ZERO);
            let r = prev.loc.dist(c);
            let (first, second) = if self.baseline { (prev.p1, p) } else { (prev.p2, p) };
            d.p1 = first;
            d.p2 = second;
            let (_, s, e) = Dimension::angular(c, first, second, prev.loc).sector();
            let mid = s + arc_sweep(s, e) / 2.0;
            d.loc = c + V2::polar(if self.baseline { r + spacing } else { r }, mid);
            return d;
        }
        if self.baseline {
            d.p2 = p;
            let (q1, _) = prev.dim_line_points();
            let away = (q1 - prev.p1).norm();
            let away = if away == V2::ZERO { prev.direction().perp() } else { away };
            d.loc = prev.loc + away * spacing;
        } else {
            d.p1 = prev.p2;
            d.p2 = p;
        }
        let dir = d.direction();
        let (a, b) = (d.loc + dir * (d.p1 - d.loc).dot(dir), d.loc + dir * (d.p2 - d.loc).dot(dir));
        d.loc = a.mid(b);
        d
    }
}

impl Command for DimChainCmd {
    fn name(&self) -> &'static str {
        if self.baseline {
            "DIMBASELINE"
        } else {
            "DIMCONTINUE"
        }
    }
    fn start(&mut self, cx: &mut Cx) -> Flow {
        if self.base_dim(cx).is_none() {
            cx.msg("Create a linear, aligned or angular dimension first.");
            return Flow::Done;
        }
        Flow::Continue
    }
    fn prompt(&self, _: &Cx) -> String {
        "Specify a second extension line origin <exit>:".into()
    }
    fn wants(&self) -> Want {
        Want::Point
    }
    fn point(&mut self, cx: &mut Cx, p: V2) -> Flow {
        let Some(prev) = self.base_dim(cx) else { return Flow::Done };
        let d = self.next(cx, &prev, p);
        cx.checkpoint();
        cx.add_dim(d);
        Flow::Continue
    }
    fn preview(&self, _doc: &Document, _c: V2) -> Vec<Shape> {
        vec![]
    }
}

// ----------------------------------------------------------------- hatch

#[derive(Default)]
struct HatchCmd {
    setting: Option<char>,
    selecting: bool,
    made: usize,
}

impl Command for HatchCmd {
    fn name(&self) -> &'static str {
        "HATCH"
    }
    fn prompt(&self, cx: &Cx) -> String {
        match self.setting {
            Some('P') => {
                format!("Pattern name ({}) <{}>:", hatch::PATTERNS.iter().map(|p| p.0).collect::<Vec<_>>().join(", "), cx.mem.hatch_pattern)
            }
            Some('S') => format!("Pattern scale <{}>:", fmt(cx.mem.hatch_scale)),
            Some('A') => format!("Pattern angle <{}>:", fmt(cx.mem.hatch_angle)),
            _ if self.selecting => "Select closed objects to hatch:".into(),
            _ => format!(
                "Pick internal point or [Pattern/Scale/Angle/selectObjects] ({} scale {} angle {}°):",
                cx.mem.hatch_pattern,
                fmt(cx.mem.hatch_scale),
                fmt(cx.mem.hatch_angle)
            ),
        }
    }
    fn wants(&self) -> Want {
        if self.setting.is_some() {
            Want::Value
        } else if self.selecting {
            Want::Select
        } else {
            Want::Point
        }
    }
    fn point(&mut self, cx: &mut Cx, p: V2) -> Flow {
        let segs = cx.tab.doc.boundary_segments();
        match hatch::find_boundary(&segs, p) {
            Some(loops) => {
                self.add(cx, loops);
            }
            None => cx.msg("No closed boundary found around that point."),
        }
        Flow::Continue
    }
    fn value(&mut self, cx: &mut Cx, s: &str) -> Flow {
        if let Some(k) = self.setting.take() {
            match k {
                'P' => {
                    let up = s.trim().to_ascii_uppercase();
                    if hatch::PATTERNS.iter().any(|p| p.0 == up) {
                        cx.mem.hatch_pattern = up;
                    } else {
                        cx.msg(format!("Unknown pattern {s}"));
                    }
                }
                'S' => match num(s).filter(|v| *v > 0.0) {
                    Some(v) => cx.mem.hatch_scale = v,
                    None => cx.msg("Scale must be positive."),
                },
                _ => match num(s) {
                    Some(v) => cx.mem.hatch_angle = v,
                    None => cx.msg("Angle must be a number."),
                },
            }
            return Flow::Continue;
        }
        match s.to_ascii_uppercase().as_str() {
            "P" | "PATTERN" => self.setting = Some('P'),
            "S" | "SCALE" => self.setting = Some('S'),
            "A" | "ANGLE" => self.setting = Some('A'),
            "O" | "SELECT" | "SELECTOBJECTS" => self.selecting = true,
            _ => cx.msg(format!("Invalid option: {s}")),
        }
        Flow::Continue
    }
    fn selected(&mut self, cx: &mut Cx, ids: Vec<u64>) -> Flow {
        let loops: Vec<Vec<V2>> = ids.iter().filter_map(|id| cx.tab.doc.get(*id)).filter_map(|e| e.shape.closed_loop()).collect();
        if loops.is_empty() {
            cx.msg("No closed objects selected (circles, closed polylines, ellipses, closed splines).");
        } else {
            self.add(cx, loops);
        }
        self.selecting = false;
        Flow::Continue
    }
    fn enter(&mut self, _: &mut Cx) -> Flow {
        if self.setting.take().is_some() {
            return Flow::Continue;
        }
        Flow::Done
    }
}

impl HatchCmd {
    fn add(&mut self, cx: &mut Cx, loops: Vec<Vec<V2>>) {
        cx.checkpoint();
        let h = Hatch { loops, pattern: cx.mem.hatch_pattern.clone(), scale: cx.mem.hatch_scale, angle: rad(cx.mem.hatch_angle) };
        let area = h.area();
        let layer = if cx.tab.doc.layer(layers::HATCH).is_some() { layers::HATCH.to_string() } else { cx.tab.doc.current_layer.clone() };
        cx.doc().add_on(&layer, Shape::Hatch(h));
        self.made += 1;
        cx.msg(format!("Hatch added (area {}).", fmt(area)));
    }
}

// ------------------------------------------------------------------ blocks

#[derive(Default)]
struct BlockCmd {
    name: Option<String>,
    base: Option<V2>,
    ids: Vec<u64>,
}

impl BlockCmd {
    fn define(&mut self, cx: &mut Cx) -> Flow {
        let (Some(name), Some(base)) = (self.name.clone(), self.base) else { return Flow::Done };
        let ents: Vec<Entity> = self.ids.iter().filter_map(|id| cx.tab.doc.get(*id).cloned()).collect();
        if ents.is_empty() {
            cx.msg("No objects selected.");
            return Flow::Done;
        }
        cx.checkpoint();
        let redefined = cx.tab.doc.block(&name).is_some();
        let layer = ents[0].layer.clone();
        cx.doc().define_block(BlockDef { name: name.clone(), base, entities: ents, description: String::new() });
        cx.doc().remove(&self.ids);
        cx.doc().add_on(&layer, Shape::Insert(Insert::new(name.clone(), base)));
        cx.mem.last_block = name.clone();
        cx.msg(format!("Block {name} {} with {} object(s).", if redefined { "redefined" } else { "created" }, self.ids.len()));
        Flow::Done
    }
}

impl Command for BlockCmd {
    fn name(&self) -> &'static str {
        "BLOCK"
    }
    fn start(&mut self, cx: &mut Cx) -> Flow {
        self.ids = cx.take_selection();
        Flow::Continue
    }
    fn prompt(&self, _: &Cx) -> String {
        match (&self.name, self.base) {
            (None, _) => "Enter block name:".into(),
            (Some(_), None) => "Specify insertion base point:".into(),
            _ => "Select objects:".into(),
        }
    }
    fn wants(&self) -> Want {
        match (&self.name, self.base) {
            (None, _) => Want::Value,
            (Some(_), None) => Want::Point,
            _ => Want::Select,
        }
    }
    fn value(&mut self, cx: &mut Cx, s: &str) -> Flow {
        let n = s.trim();
        if self.name.is_none() && !n.is_empty() && !n.starts_with('*') {
            self.name = Some(n.to_string());
        } else {
            cx.msg("Enter a valid block name.");
        }
        Flow::Continue
    }
    fn point(&mut self, cx: &mut Cx, p: V2) -> Flow {
        self.base = Some(p);
        if !self.ids.is_empty() {
            return self.define(cx);
        }
        Flow::Continue
    }
    fn selected(&mut self, cx: &mut Cx, ids: Vec<u64>) -> Flow {
        self.ids = ids;
        self.define(cx)
    }
}

#[derive(Default)]
struct InsertCmd {
    name: Option<String>,
    pos: Option<V2>,
    scale: Option<f64>,
}

impl Command for InsertCmd {
    fn name(&self) -> &'static str {
        "INSERT"
    }
    fn prompt(&self, cx: &Cx) -> String {
        match (&self.name, self.pos, self.scale) {
            (None, ..) => {
                let names: Vec<&str> = cx.tab.doc.blocks.iter().map(|b| b.name.as_str()).take(12).collect();
                if names.is_empty() {
                    "No blocks defined (use BLOCK). Enter block name:".into()
                } else {
                    format!("Enter block name ({}) <{}>:", names.join(", "), cx.mem.last_block)
                }
            }
            (Some(_), None, _) => "Specify insertion point:".into(),
            (Some(_), Some(_), None) => "Enter scale factor <1>:".into(),
            _ => "Specify rotation angle <0>:".into(),
        }
    }
    fn wants(&self) -> Want {
        match (&self.name, self.pos, self.scale) {
            (Some(_), None, _) => Want::Point,
            (Some(_), Some(_), Some(_)) => Want::Point,
            _ => Want::Value,
        }
    }
    fn base(&self) -> Option<V2> {
        if self.scale.is_some() {
            self.pos
        } else {
            None
        }
    }
    fn value(&mut self, cx: &mut Cx, s: &str) -> Flow {
        if self.name.is_none() {
            match cx.tab.doc.block(s.trim()) {
                Some(b) => self.name = Some(b.name.clone()),
                None => cx.msg(format!("Block {s} not found.")),
            }
            return Flow::Continue;
        }
        if self.pos.is_some() && self.scale.is_none() {
            match num(s).filter(|v| *v != 0.0) {
                Some(v) => self.scale = Some(v),
                None => cx.msg("Scale must be a non-zero number."),
            }
            return Flow::Continue;
        }
        if self.scale.is_some() {
            if let Some(a) = num(s) {
                return self.place(cx, rad(a));
            }
        }
        cx.msg(format!("Invalid input: {s}"));
        Flow::Continue
    }
    fn enter(&mut self, cx: &mut Cx) -> Flow {
        if self.name.is_none() {
            let last = cx.mem.last_block.clone();
            if cx.tab.doc.block(&last).is_some() {
                self.name = Some(last);
                return Flow::Continue;
            }
            return Flow::Done;
        }
        if self.pos.is_some() && self.scale.is_none() {
            self.scale = Some(1.0);
            return Flow::Continue;
        }
        if self.scale.is_some() {
            return self.place(cx, 0.0);
        }
        Flow::Done
    }
    fn point(&mut self, cx: &mut Cx, p: V2) -> Flow {
        if self.pos.is_none() {
            self.pos = Some(p);
            return Flow::Continue;
        }
        let a = (p - self.pos.unwrap()).angle();
        self.place(cx, a)
    }
    fn preview(&self, _: &Document, c: V2) -> Vec<Shape> {
        let Some(name) = &self.name else { return vec![] };
        let mut ins = Insert::new(name.clone(), self.pos.unwrap_or(c));
        if let Some(k) = self.scale {
            ins.sx = k;
            ins.sy = k;
            ins.rotation = (c - ins.pos).angle();
        }
        vec![Shape::Insert(ins)]
    }
}

impl InsertCmd {
    fn place(&mut self, cx: &mut Cx, rot: f64) -> Flow {
        let (Some(name), Some(pos)) = (self.name.clone(), self.pos) else { return Flow::Done };
        let k = self.scale.unwrap_or(1.0);
        cx.checkpoint();
        let mut ins = Insert { name: name.clone(), pos, sx: k, sy: k, rotation: norm_angle(rot), attribs: vec![] };
        let defs: Vec<(String, String)> =
            cx.tab.doc.block(&name).map(|b| b.attdefs().map(|a| (a.tag.clone(), a.default.clone())).collect()).unwrap_or_default();
        for (tag, default) in &defs {
            ins.set_value(tag, default.clone());
        }
        let id = cx.doc().add(Shape::Insert(ins));
        // Ask for the attribute values right away.
        if !defs.is_empty() {
            cx.mem.pending_attedit = Some(id);
        }
        cx.mem.last_block = name;
        Flow::Done
    }
}

/// Place a raster image (path chosen by the application).
pub struct ImagePlaceCmd {
    pub path: String,
    pub aspect: f64,
    pub pos: Option<V2>,
}

impl Command for ImagePlaceCmd {
    fn name(&self) -> &'static str {
        "IMAGEATTACH"
    }
    fn prompt(&self, _: &Cx) -> String {
        if self.pos.is_none() {
            "Specify insertion point (lower-left):".into()
        } else {
            "Specify width (or opposite corner):".into()
        }
    }
    fn wants(&self) -> Want {
        Want::Point
    }
    fn base(&self) -> Option<V2> {
        self.pos
    }
    fn point(&mut self, cx: &mut Cx, p: V2) -> Flow {
        match self.pos {
            None => {
                self.pos = Some(p);
                Flow::Continue
            }
            Some(o) => self.place(cx, (p.x - o.x).abs().max(1e-6)),
        }
    }
    fn value(&mut self, cx: &mut Cx, s: &str) -> Flow {
        match (self.pos, num(s)) {
            (Some(_), Some(w)) if w > 0.0 => self.place(cx, w),
            _ => {
                cx.msg(format!("Invalid input: {s}"));
                Flow::Continue
            }
        }
    }
    fn preview(&self, _: &Document, c: V2) -> Vec<Shape> {
        let Some(o) = self.pos else { return vec![] };
        let w = (c.x - o.x).abs().max(1e-6);
        vec![Shape::Polyline { pts: rect_pts(o, o + V2::new(w, w * self.aspect)), closed: true }]
    }
}

impl ImagePlaceCmd {
    fn place(&mut self, cx: &mut Cx, w: f64) -> Flow {
        let pos = self.pos.unwrap();
        cx.checkpoint();
        cx.doc().add(Shape::Image(ImageRef { path: self.path.clone(), pos, w, h: w * self.aspect, fade: 0 }));
        Flow::Done
    }
}

// ----------------------------------------------------------------- modify

#[derive(Default)]
struct StretchCmd {
    ids: Vec<u64>,
    window: Option<BBox>,
    base: Option<V2>,
}

impl Command for StretchCmd {
    fn name(&self) -> &'static str {
        "STRETCH"
    }
    fn start(&mut self, cx: &mut Cx) -> Flow {
        self.ids = cx.take_selection();
        self.window = cx.tab.last_window.take();
        Flow::Continue
    }
    fn prompt(&self, _: &Cx) -> String {
        if self.ids.is_empty() {
            "Select objects to stretch by crossing-window (drag right → left):".into()
        } else if self.base.is_none() {
            "Specify base point:".into()
        } else {
            "Specify second point:".into()
        }
    }
    fn wants(&self) -> Want {
        if self.ids.is_empty() {
            Want::Select
        } else {
            Want::Point
        }
    }
    fn base(&self) -> Option<V2> {
        self.base
    }
    fn selected(&mut self, cx: &mut Cx, ids: Vec<u64>) -> Flow {
        if ids.is_empty() {
            return Flow::Done;
        }
        self.ids = ids;
        self.window = cx.tab.last_window.take();
        Flow::Continue
    }
    fn point(&mut self, cx: &mut Cx, p: V2) -> Flow {
        let Some(b) = self.base else {
            self.base = Some(p);
            return Flow::Continue;
        };
        cx.checkpoint();
        let d = p - b;
        for id in self.ids.clone() {
            if let Some(e) = cx.doc().get_mut(id) {
                match self.window {
                    Some(w) => ops::stretch(&mut e.shape, &w, d),
                    None => e.shape.transform(&Xform::translate(d)),
                }
            }
        }
        Flow::Done
    }
    fn preview(&self, doc: &Document, c: V2) -> Vec<Shape> {
        let Some(b) = self.base else { return vec![] };
        self.ids
            .iter()
            .filter_map(|id| doc.get(*id))
            .map(|e| {
                let mut s = e.shape.clone();
                match self.window {
                    Some(w) => ops::stretch(&mut s, &w, c - b),
                    None => s.transform(&Xform::translate(c - b)),
                }
                s
            })
            .collect()
    }
}

struct BreakCmd {
    at_point: bool,
    target: Option<(u64, V2)>,
    first: Option<V2>,
    pick_first: bool,
}

impl Command for BreakCmd {
    fn name(&self) -> &'static str {
        if self.at_point {
            "BREAKATPOINT"
        } else {
            "BREAK"
        }
    }
    fn prompt(&self, _: &Cx) -> String {
        match (self.target, self.at_point, self.pick_first) {
            (None, ..) => "Select object:".into(),
            (Some(_), true, _) => "Specify break point:".into(),
            (Some(_), false, true) => "Specify first break point:".into(),
            (Some(_), false, false) => "Specify second break point or [First point]:".into(),
        }
    }
    fn wants(&self) -> Want {
        if self.target.is_none() {
            Want::Entity
        } else {
            Want::Point
        }
    }
    fn entity(&mut self, cx: &mut Cx, id: u64, at: V2) -> Flow {
        match cx.shape(id) {
            Some(Shape::Line { .. } | Shape::Arc { .. } | Shape::Circle { .. } | Shape::Polyline { .. }) => {
                self.target = Some((id, at));
                self.first = cx.shape(id).and_then(|s| s.nearest(at));
            }
            _ => cx.msg("Select a line, arc, circle or polyline."),
        }
        Flow::Continue
    }
    fn value(&mut self, cx: &mut Cx, s: &str) -> Flow {
        if s.eq_ignore_ascii_case("F") && !self.at_point {
            self.pick_first = true;
        } else {
            cx.msg(format!("Invalid option: {s}"));
        }
        Flow::Continue
    }
    fn point(&mut self, cx: &mut Cx, p: V2) -> Flow {
        let Some((id, _)) = self.target else { return Flow::Continue };
        let Some(e) = cx.tab.doc.get(id).cloned() else { return Flow::Done };
        let p = e.shape.nearest(p).unwrap_or(p);
        if self.pick_first {
            self.first = Some(p);
            self.pick_first = false;
            return Flow::Continue;
        }
        let first = if self.at_point { p } else { self.first.unwrap_or(p) };
        match ops::break_between(&e.shape, first, p) {
            Some(parts) => {
                cx.checkpoint();
                cx.doc().remove(&[id]);
                for s in parts {
                    cx.doc().add_like(&e, s);
                }
            }
            None => cx.msg("That object cannot be broken at a single point."),
        }
        Flow::Done
    }
}

struct JoinCmd;

impl Command for JoinCmd {
    fn name(&self) -> &'static str {
        "JOIN"
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
        "Select objects to join (lines, polylines, arcs):".into()
    }
    fn wants(&self) -> Want {
        Want::Select
    }
    fn selected(&mut self, cx: &mut Cx, ids: Vec<u64>) -> Flow {
        let ents: Vec<Entity> = ids
            .iter()
            .filter_map(|id| cx.tab.doc.get(*id).cloned())
            .filter(|e| matches!(e.shape, Shape::Line { .. } | Shape::Arc { .. } | Shape::Polyline { closed: false, .. }))
            .collect();
        if ents.len() < 2 {
            cx.msg("Select at least two lines, arcs or open polylines.");
            return Flow::Done;
        }
        let shapes: Vec<Shape> = ents.iter().map(|e| e.shape.clone()).collect();
        let ext = cx.tab.doc.extents();
        let tol = (ext.width().max(ext.height()) * 1e-7).max(1e-7);
        let joined = ops::join(&shapes, tol);
        cx.checkpoint();
        let rm: Vec<u64> = ents.iter().map(|e| e.id).collect();
        cx.doc().remove(&rm);
        for s in &joined {
            cx.doc().add_like(&ents[0], s.clone());
        }
        cx.msg(format!("{} objects joined into {}.", ents.len(), joined.len()));
        Flow::Done
    }
}

struct LengthenCmd {
    setting: bool,
}

impl Command for LengthenCmd {
    fn name(&self) -> &'static str {
        "LENGTHEN"
    }
    fn prompt(&self, cx: &Cx) -> String {
        if self.setting {
            format!("Enter delta length (negative shortens) <{}>:", fmt(cx.mem.lengthen))
        } else {
            format!("Select object to change (near the end) or [DElta] (delta = {}):", fmt(cx.mem.lengthen))
        }
    }
    fn wants(&self) -> Want {
        if self.setting {
            Want::Value
        } else {
            Want::Entity
        }
    }
    fn value(&mut self, cx: &mut Cx, s: &str) -> Flow {
        if self.setting {
            match num(s) {
                Some(v) if v != 0.0 => {
                    cx.mem.lengthen = v;
                    self.setting = false;
                }
                _ => cx.msg("Enter a non-zero number."),
            }
        } else if s.to_ascii_uppercase().starts_with("DE") {
            self.setting = true;
        } else {
            cx.msg(format!("Invalid option: {s}"));
        }
        Flow::Continue
    }
    fn enter(&mut self, _: &mut Cx) -> Flow {
        if self.setting {
            self.setting = false;
            return Flow::Continue;
        }
        Flow::Done
    }
    fn entity(&mut self, cx: &mut Cx, id: u64, at: V2) -> Flow {
        let Some(e) = cx.tab.doc.get(id).cloned() else { return Flow::Continue };
        match ops::lengthen(&e.shape, at, cx.mem.lengthen) {
            Some(s) => {
                cx.checkpoint();
                if let Some(m) = cx.doc().get_mut(id) {
                    m.shape = s;
                }
            }
            None => cx.msg("Cannot lengthen that object by this amount."),
        }
        Flow::Continue
    }
}

#[derive(Default)]
struct PolarArrayCmd {
    ids: Vec<u64>,
    center: Option<V2>,
    count: Option<u32>,
}

impl PolarArrayCmd {
    fn run(&mut self, cx: &mut Cx, fill_deg: f64) -> Flow {
        let (Some(c), Some(n)) = (self.center, self.count) else { return Flow::Done };
        let full = (fill_deg.abs() - 360.0).abs() < 1e-9;
        let step = if full { fill_deg / n as f64 } else { fill_deg / (n.max(2) - 1) as f64 };
        cx.checkpoint();
        for id in self.ids.clone() {
            let Some(e) = cx.tab.doc.get(id).cloned() else { continue };
            for k in 1..n {
                let mut s = e.shape.clone();
                s.transform(&Xform::rotate(c, rad(step * k as f64)));
                cx.doc().add_like(&e, s);
            }
        }
        cx.msg(format!("Polar array: {n} items over {}°.", fmt(fill_deg)));
        Flow::Done
    }
}

impl Command for PolarArrayCmd {
    fn name(&self) -> &'static str {
        "ARRAYPOLAR"
    }
    fn start(&mut self, cx: &mut Cx) -> Flow {
        self.ids = cx.take_selection();
        Flow::Continue
    }
    fn prompt(&self, cx: &Cx) -> String {
        match (self.ids.is_empty(), self.center, self.count) {
            (true, ..) => "Select objects:".into(),
            (false, None, _) => "Specify center point of array:".into(),
            (false, Some(_), None) => format!("Enter number of items <{}>:", cx.mem.polar_count),
            _ => "Specify the angle to fill (+=ccw, -=cw) <360>:".into(),
        }
    }
    fn wants(&self) -> Want {
        match (self.ids.is_empty(), self.center) {
            (true, _) => Want::Select,
            (false, None) => Want::Point,
            _ => Want::Value,
        }
    }
    fn selected(&mut self, _: &mut Cx, ids: Vec<u64>) -> Flow {
        if ids.is_empty() {
            return Flow::Done;
        }
        self.ids = ids;
        Flow::Continue
    }
    fn point(&mut self, _: &mut Cx, p: V2) -> Flow {
        self.center = Some(p);
        Flow::Continue
    }
    fn value(&mut self, cx: &mut Cx, s: &str) -> Flow {
        if self.count.is_none() {
            match num(s).filter(|v| *v >= 2.0) {
                Some(v) => {
                    self.count = Some(v as u32);
                    cx.mem.polar_count = v as u32;
                }
                None => cx.msg("Enter at least 2 items."),
            }
            return Flow::Continue;
        }
        match num(s).filter(|v| *v != 0.0) {
            Some(a) => self.run(cx, a.clamp(-360.0, 360.0)),
            None => {
                cx.msg("Enter a non-zero angle.");
                Flow::Continue
            }
        }
    }
    fn enter(&mut self, cx: &mut Cx) -> Flow {
        if self.center.is_some() && self.count.is_none() {
            self.count = Some(cx.mem.polar_count);
            return Flow::Continue;
        }
        if self.count.is_some() {
            return self.run(cx, 360.0);
        }
        Flow::Done
    }
}

struct DivideCmd {
    measure: bool,
    target: Option<u64>,
}

impl Command for DivideCmd {
    fn name(&self) -> &'static str {
        if self.measure {
            "MEASURE"
        } else {
            "DIVIDE"
        }
    }
    fn prompt(&self, cx: &Cx) -> String {
        match (self.target, self.measure) {
            (None, _) => format!("Select object to {}:", if self.measure { "measure" } else { "divide" }),
            (Some(_), false) => format!("Enter the number of segments <{}>:", cx.mem.divide_n),
            (Some(_), true) => format!("Specify length of segment <{}>:", fmt(cx.mem.measure_len)),
        }
    }
    fn wants(&self) -> Want {
        if self.target.is_none() {
            Want::Entity
        } else {
            Want::Value
        }
    }
    fn entity(&mut self, cx: &mut Cx, id: u64, _: V2) -> Flow {
        if cx.shape(id).and_then(|s| s.length()).is_some() {
            self.target = Some(id);
        } else {
            cx.msg("Select a line, arc, circle, polyline, ellipse or spline.");
        }
        Flow::Continue
    }
    fn value(&mut self, cx: &mut Cx, s: &str) -> Flow {
        match num(s).filter(|v| *v > 0.0) {
            Some(v) => self.run(cx, v),
            None => {
                cx.msg("Enter a positive number.");
                Flow::Continue
            }
        }
    }
    fn enter(&mut self, cx: &mut Cx) -> Flow {
        if self.target.is_some() {
            let v = if self.measure { cx.mem.measure_len } else { cx.mem.divide_n as f64 };
            return self.run(cx, v);
        }
        Flow::Done
    }
}

impl DivideCmd {
    fn run(&mut self, cx: &mut Cx, v: f64) -> Flow {
        let Some(shape) = self.target.and_then(|id| cx.shape(id)) else { return Flow::Done };
        let pts = if self.measure {
            cx.mem.measure_len = v;
            ops::measure(&shape, v)
        } else {
            cx.mem.divide_n = (v as u32).max(2);
            ops::divide(&shape, (v as usize).max(2))
        };
        cx.checkpoint();
        for p in &pts {
            cx.doc().add(Shape::Point { p: *p });
        }
        cx.msg(format!("{} point(s) placed.", pts.len()));
        Flow::Done
    }
}

// ---------------------------------------------------------------- inquiry

#[derive(Default)]
struct AreaCmd {
    pts: Vec<V2>,
    object: bool,
}

impl Command for AreaCmd {
    fn name(&self) -> &'static str {
        "AREA"
    }
    fn prompt(&self, _: &Cx) -> String {
        if self.object {
            "Select object:".into()
        } else if self.pts.is_empty() {
            "Specify first corner point or [Object]:".into()
        } else {
            "Specify next point or press Enter for total:".into()
        }
    }
    fn wants(&self) -> Want {
        if self.object {
            Want::Entity
        } else {
            Want::Point
        }
    }
    fn base(&self) -> Option<V2> {
        self.pts.last().copied()
    }
    fn point(&mut self, _: &mut Cx, p: V2) -> Flow {
        self.pts.push(p);
        Flow::Continue
    }
    fn value(&mut self, cx: &mut Cx, s: &str) -> Flow {
        if self.pts.is_empty() && s.eq_ignore_ascii_case("O") {
            self.object = true;
        } else {
            cx.msg(format!("Invalid option: {s}"));
        }
        Flow::Continue
    }
    fn entity(&mut self, cx: &mut Cx, id: u64, _: V2) -> Flow {
        let Some(s) = cx.shape(id) else { return Flow::Done };
        match s.area() {
            Some(a) => cx.msg(format!("Area = {},  Perimeter = {}", fmt(a), s.length().map(fmt).unwrap_or_else(|| "—".into()))),
            None => cx.msg("That object does not enclose an area."),
        }
        Flow::Done
    }
    fn enter(&mut self, cx: &mut Cx) -> Flow {
        if self.pts.len() >= 3 {
            let a = hatch::signed_area(&self.pts).abs();
            let per: f64 = icad_core::prim::poly_segments(&self.pts, true).map(|(a, b)| a.dist(b)).sum();
            cx.msg(format!("Area = {},  Perimeter = {}", fmt(a), fmt(per)));
        }
        Flow::Done
    }
    fn preview(&self, _: &Document, c: V2) -> Vec<Shape> {
        if self.pts.is_empty() {
            return vec![];
        }
        let mut p = self.pts.clone();
        p.push(c);
        vec![Shape::Polyline { pts: p, closed: true }]
    }
}

struct IdCmd;

impl Command for IdCmd {
    fn name(&self) -> &'static str {
        "ID"
    }
    fn prompt(&self, _: &Cx) -> String {
        "Specify point:".into()
    }
    fn wants(&self) -> Want {
        Want::Point
    }
    fn point(&mut self, cx: &mut Cx, p: V2) -> Flow {
        cx.msg(format!("X = {}     Y = {}     Z = 0", fmt(p.x), fmt(p.y)));
        Flow::Done
    }
}

struct ListCmd;

impl Command for ListCmd {
    fn name(&self) -> &'static str {
        "LIST"
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
        "Select objects:".into()
    }
    fn wants(&self) -> Want {
        Want::Select
    }
    fn selected(&mut self, cx: &mut Cx, ids: Vec<u64>) -> Flow {
        for id in ids {
            let Some(e) = cx.tab.doc.get(id).cloned() else { continue };
            let mut line = format!("{} (handle {:X}) on layer {}", e.shape.type_name(), e.id, e.layer);
            if let Some(l) = e.shape.length() {
                line += &format!(", length {}", fmt(l));
            }
            if let Some(a) = e.shape.area() {
                line += &format!(", area {}", fmt(a));
            }
            match &e.shape {
                Shape::Line { a, b } => line += &format!(", from {},{} to {},{}", fmt(a.x), fmt(a.y), fmt(b.x), fmt(b.y)),
                Shape::Circle { c, r } => line += &format!(", center {},{} radius {}", fmt(c.x), fmt(c.y), fmt(*r)),
                Shape::Insert(i) => line += &format!(", block {} at {},{}", i.name, fmt(i.pos.x), fmt(i.pos.y)),
                Shape::Dim(d) => line += &format!(", measurement {}", cx.tab.doc.dimstyle.format(d.measurement())),
                _ => {}
            }
            cx.msg(line);
        }
        Flow::Done
    }
}

struct OverkillCmd;

impl Command for OverkillCmd {
    fn name(&self) -> &'static str {
        "OVERKILL"
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
        "Select objects to clean up (or type ALL):".into()
    }
    fn wants(&self) -> Want {
        Want::Select
    }
    fn selected(&mut self, cx: &mut Cx, ids: Vec<u64>) -> Flow {
        let ents: Vec<Entity> = ids.iter().filter_map(|id| cx.tab.doc.get(*id).cloned()).collect();
        let shapes: Vec<Shape> = ents.iter().map(|e| e.shape.clone()).collect();
        let ext = cx.tab.doc.extents();
        let dups = ops::duplicates(&shapes, (ext.width().max(ext.height()) * 1e-8).max(1e-8));
        if dups.is_empty() {
            cx.msg("No duplicate objects found.");
            return Flow::Done;
        }
        cx.checkpoint();
        let rm: Vec<u64> = dups.iter().map(|i| ents[*i].id).collect();
        cx.doc().remove(&rm);
        cx.msg(format!("{} duplicate object(s) deleted.", rm.len()));
        Flow::Done
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum LayOp {
    Off,
    Lock,
    Unlock,
    MakeCurrent,
    Isolate,
}

struct LayerPick {
    op: LayOp,
}

impl Command for LayerPick {
    fn name(&self) -> &'static str {
        match self.op {
            LayOp::Off => "LAYOFF",
            LayOp::Lock => "LAYLCK",
            LayOp::Unlock => "LAYULK",
            LayOp::MakeCurrent => "LAYMCUR",
            LayOp::Isolate => "LAYISO",
        }
    }
    fn start(&mut self, cx: &mut Cx) -> Flow {
        let ids = cx.take_selection();
        if self.op == LayOp::Isolate && !ids.is_empty() {
            return self.selected(cx, ids);
        }
        Flow::Continue
    }
    fn prompt(&self, _: &Cx) -> String {
        match self.op {
            LayOp::Isolate => "Select objects on the layer(s) to isolate:".into(),
            LayOp::Unlock => "Select an object on the layer to unlock (locked layers can be picked):".into(),
            _ => "Select an object on the layer:".into(),
        }
    }
    fn wants(&self) -> Want {
        if self.op == LayOp::Isolate {
            Want::Select
        } else {
            Want::Entity
        }
    }
    fn entity(&mut self, cx: &mut Cx, id: u64, _: V2) -> Flow {
        let Some(name) = cx.tab.doc.get(id).map(|e| e.layer.clone()) else { return Flow::Done };
        cx.checkpoint();
        match self.op {
            LayOp::MakeCurrent => {
                cx.doc().current_layer = name.clone();
                cx.msg(format!("{name} is now the current layer."));
            }
            op => {
                if op == LayOp::Off && name.eq_ignore_ascii_case(&cx.tab.doc.current_layer) {
                    cx.msg("The current layer cannot be turned off.");
                    return Flow::Done;
                }
                if let Some(l) = cx.doc().layer_mut(&name) {
                    match op {
                        LayOp::Off => l.visible = false,
                        LayOp::Lock => l.locked = true,
                        _ => l.locked = false,
                    }
                }
                cx.msg(format!("Layer {name} updated."));
            }
        }
        Flow::Done
    }
    fn selected(&mut self, cx: &mut Cx, ids: Vec<u64>) -> Flow {
        let keep: std::collections::HashSet<String> =
            ids.iter().filter_map(|id| cx.tab.doc.get(*id)).map(|e| e.layer.to_ascii_uppercase()).collect();
        if keep.is_empty() {
            return Flow::Done;
        }
        cx.checkpoint();
        cx.mem.layiso_backup = cx.tab.doc.layers.iter().map(|l| (l.name.clone(), l.visible)).collect();
        let first = cx.tab.doc.layers.iter().find(|l| keep.contains(&l.name.to_ascii_uppercase())).map(|l| l.name.clone());
        if let Some(f) = first {
            cx.doc().current_layer = f;
        }
        for l in cx.doc().layers.iter_mut() {
            l.visible = keep.contains(&l.name.to_ascii_uppercase());
        }
        cx.msg(format!("{} layer(s) isolated. LAYUNISO restores.", keep.len()));
        Flow::Done
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::commands::{Memory, ViewReq};
    use crate::tab::Tab;

    fn run(tab: &mut Tab, mem: &mut Memory, name: &str, inputs: &[&str]) {
        let mut log = vec![];
        let mut view: Option<ViewReq> = None;
        let mut c = create(name).expect("command");
        let mut cx = Cx { tab, log: &mut log, mem, view_req: &mut view };
        let mut flow = c.start(&mut cx);
        for inp in inputs {
            if flow == Flow::Done {
                break;
            }
            flow = if inp.is_empty() {
                if c.wants() == Want::Select {
                    let ids = std::mem::take(&mut cx.tab.selection);
                    c.selected(&mut cx, ids)
                } else {
                    c.enter(&mut cx)
                }
            } else if let Some(rest) = inp.strip_prefix("pick:") {
                let p = crate::commands::parse_point(rest, None).unwrap();
                let id = cx.tab.doc.pick(p, 0.5).expect("entity under pick");
                c.entity(&mut cx, id, p)
            } else if c.wants() == Want::Point {
                match crate::commands::parse_point(inp, c.base()) {
                    Some(p) => c.point(&mut cx, p),
                    None => c.value(&mut cx, inp),
                }
            } else {
                c.value(&mut cx, inp)
            };
        }
    }

    #[test]
    fn hatch_block_insert_and_angular() {
        let mut tab = Tab::new(Document::new(), "t");
        let mut mem = Memory::default();
        crate::commands::create("REC").unwrap();
        tab.doc.add(Shape::Polyline { pts: rect_pts(V2::new(0.0, 0.0), V2::new(100.0, 50.0)), closed: true });
        tab.doc.add(Shape::Circle { c: V2::new(50.0, 25.0), r: 10.0 });
        run(&mut tab, &mut mem, "H", &["S", "2", "10,10", ""]);
        let h = tab.doc.entities.iter().find_map(|e| if let Shape::Hatch(h) = &e.shape { Some(h.clone()) } else { None }).unwrap();
        assert!((h.area() - (5000.0 - std::f64::consts::PI * 100.0)).abs() < 1.0, "{}", h.area());
        assert_eq!(h.scale, 2.0);

        // BLOCK from the circle, then INSERT it scaled.
        let circle = tab.doc.entities.iter().find(|e| matches!(e.shape, Shape::Circle { .. })).unwrap().id;
        tab.selection = vec![circle];
        run(&mut tab, &mut mem, "B", &["HOLE", "50,25"]);
        assert!(tab.doc.block("HOLE").is_some());
        run(&mut tab, &mut mem, "I", &["HOLE", "200,0", "2", "0"]);
        let ins: Vec<&Insert> =
            tab.doc.entities.iter().filter_map(|e| if let Shape::Insert(i) = &e.shape { Some(i) } else { None }).collect();
        assert_eq!(ins.len(), 2);
        assert!(ins.iter().any(|i| i.sx == 2.0 && i.pos == V2::new(200.0, 0.0)));

        // DIMANGULAR between two lines → 90°.
        tab.doc.add(Shape::Line { a: V2::new(300.0, 0.0), b: V2::new(400.0, 0.0) });
        tab.doc.add(Shape::Line { a: V2::new(300.0, 0.0), b: V2::new(300.0, 100.0) });
        run(&mut tab, &mut mem, "DAN", &["pick:350,0", "pick:300,50", "330,30"]);
        let d = tab.doc.entities.iter().find_map(|e| if let Shape::Dim(d) = &e.shape { Some(d.clone()) } else { None }).unwrap();
        assert!((d.measurement() - 90.0).abs() < 1e-9);
    }

    #[test]
    fn ellipse_spline_break_join_polar() {
        let mut tab = Tab::new(Document::new(), "t");
        let mut mem = Memory::default();
        run(&mut tab, &mut mem, "EL", &["0,0", "40,0", "10"]);
        let Shape::Ellipse { c, major, ratio, .. } = tab.doc.entities[0].shape.clone() else { panic!() };
        assert!((c - V2::new(20.0, 0.0)).len() < 1e-9 && (major.len() - 20.0).abs() < 1e-9 && (ratio - 0.5).abs() < 1e-9);
        run(&mut tab, &mut mem, "SPL", &["0,10", "10,20", "20,10", ""]);
        assert!(matches!(tab.doc.entities[1].shape, Shape::Spline { ref pts, .. } if pts.len() == 3));
        tab.doc.add(Shape::Line { a: V2::new(0.0, 100.0), b: V2::new(100.0, 100.0) });
        run(&mut tab, &mut mem, "BR", &["pick:20,100", "40,100"]);
        let lines = tab.doc.entities.iter().filter(|e| matches!(e.shape, Shape::Line { .. })).count();
        assert_eq!(lines, 2);
        tab.selection = tab.doc.entities.iter().filter(|e| matches!(e.shape, Shape::Line { .. })).map(|e| e.id).collect();
        run(&mut tab, &mut mem, "J", &[]);
        // The pieces are collinear but not touching, so they stay two lines.
        assert_eq!(tab.doc.entities.iter().filter(|e| matches!(e.shape, Shape::Line { .. })).count(), 2);
        let first_line = tab.doc.entities.iter().find(|e| matches!(e.shape, Shape::Line { .. })).unwrap().id;
        tab.selection = vec![first_line];
        run(&mut tab, &mut mem, "APO", &["0,0", "4", ""]);
        assert_eq!(tab.doc.entities.iter().filter(|e| matches!(e.shape, Shape::Line { .. })).count(), 5);
    }
}
