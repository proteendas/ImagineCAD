// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Proteen Das

//! Dimension commands added in 0.3: ordinate, arc length, quick dimension and
//! (dis)association.

use crate::commands::{fmt, parse_point, Command, Cx, Flow, Want};
use icad_core::entity::*;
use icad_core::geom::*;
use icad_core::Document;

pub fn create(name: &str) -> Option<Box<dyn Command>> {
    let c: Box<dyn Command> = match name {
        "DIMORDINATE" | "DOR" => Box::new(OrdinateCmd::default()),
        "DIMARC" | "DAR" => Box::new(ArcLenCmd::default()),
        "QDIM" => Box::new(QDimCmd::default()),
        "DIMREASSOCIATE" | "DRE" => Box::new(AssocCmd { on: true }),
        "DIMDISASSOCIATE" | "DDA" => Box::new(AssocCmd { on: false }),
        _ => return None,
    };
    Some(c)
}

// ------------------------------------------------------------------ ordinate

#[derive(Default)]
struct OrdinateCmd {
    feature: Option<V2>,
    force: Option<DimKind>,
    setting_datum: bool,
}

impl OrdinateCmd {
    fn make(&self, cx_datum: V2, end: V2) -> Option<Dimension> {
        let f = self.feature?;
        let mut d = Dimension::ordinate(cx_datum, f, end);
        if let Some(k) = self.force {
            d.kind = k;
        }
        Some(d)
    }
}

impl Command for OrdinateCmd {
    fn name(&self) -> &'static str {
        "DIMORDINATE"
    }
    fn prompt(&self, cx: &Cx) -> String {
        if self.setting_datum {
            return format!("Specify datum origin <{},{}>:", fmt(cx.mem.ordinate_datum.x), fmt(cx.mem.ordinate_datum.y));
        }
        match self.feature {
            None => "Specify feature location:".into(),
            Some(_) => "Specify leader endpoint or [Xdatum/Ydatum/Datum]:".into(),
        }
    }
    fn wants(&self) -> Want {
        Want::Point
    }
    fn base(&self) -> Option<V2> {
        self.feature
    }
    fn point(&mut self, cx: &mut Cx, p: V2) -> Flow {
        if self.setting_datum {
            cx.mem.ordinate_datum = p;
            self.setting_datum = false;
            cx.msg(format!("Ordinate datum set to {},{}.", fmt(p.x), fmt(p.y)));
            return Flow::Continue;
        }
        if self.feature.is_none() {
            self.feature = Some(p);
            return Flow::Continue;
        }
        if let Some(d) = self.make(cx.mem.ordinate_datum, p) {
            cx.checkpoint();
            let m = d.measurement();
            cx.add_dim(d);
            cx.msg(format!("Dimension text = {}", cx.tab.doc.dimstyle.format(m)));
        }
        Flow::Done
    }
    fn value(&mut self, cx: &mut Cx, s: &str) -> Flow {
        match s.to_ascii_uppercase().as_str() {
            "X" | "XDATUM" => self.force = Some(DimKind::OrdinateX),
            "Y" | "YDATUM" => self.force = Some(DimKind::OrdinateY),
            "D" | "DATUM" => self.setting_datum = true,
            _ if self.setting_datum => match parse_point(s, None) {
                Some(p) => return self.point(cx, p),
                None => cx.msg("Enter the datum as x,y."),
            },
            _ => cx.msg(format!("Invalid input: {s}")),
        }
        Flow::Continue
    }
    fn enter(&mut self, _: &mut Cx) -> Flow {
        if self.setting_datum {
            self.setting_datum = false;
            return Flow::Continue;
        }
        Flow::Done
    }
    fn preview(&self, doc: &Document, c: V2) -> Vec<Shape> {
        let _ = doc;
        self.make(V2::ZERO, c).map(|d| vec![Shape::Dim(d)]).unwrap_or_default()
    }
}

// ------------------------------------------------------------------ arc length

#[derive(Default)]
struct ArcLenCmd {
    arc: Option<(V2, f64, f64, f64)>,
}

impl ArcLenCmd {
    fn make(&self, at: V2) -> Option<Dimension> {
        let (c, r, s, e) = self.arc?;
        Some(Dimension::arc_length(c, c + V2::polar(r, s), c + V2::polar(r, e), at))
    }
}

impl Command for ArcLenCmd {
    fn name(&self) -> &'static str {
        "DIMARC"
    }
    fn prompt(&self, _: &Cx) -> String {
        match self.arc {
            None => "Select arc segment:".into(),
            Some(_) => "Specify arc length dimension location:".into(),
        }
    }
    fn wants(&self) -> Want {
        if self.arc.is_none() {
            Want::Entity
        } else {
            Want::Point
        }
    }
    fn entity(&mut self, cx: &mut Cx, id: u64, _: V2) -> Flow {
        match cx.shape(id) {
            Some(Shape::Arc { c, r, start, end }) => self.arc = Some((c, r, start, end)),
            _ => cx.msg("Select an arc."),
        }
        Flow::Continue
    }
    fn point(&mut self, cx: &mut Cx, p: V2) -> Flow {
        if let Some(d) = self.make(p) {
            cx.checkpoint();
            let m = d.measurement();
            cx.add_dim(d);
            cx.msg(format!("Dimension text = {}", cx.tab.doc.dimstyle.format(m)));
        }
        Flow::Done
    }
    fn preview(&self, _: &Document, c: V2) -> Vec<Shape> {
        self.make(c).map(|d| vec![Shape::Dim(d)]).unwrap_or_default()
    }
}

// ------------------------------------------------------------------ QDIM

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum QMode {
    #[default]
    Continuous,
    Staggered,
    Baseline,
    Ordinate,
    Radius,
    Diameter,
}

#[derive(Default)]
struct QDimCmd {
    ids: Vec<u64>,
}

/// Feature points of the selection (ends, centres), and circles/arcs.
fn features(doc: &Document, ids: &[u64]) -> (Vec<V2>, Vec<(V2, f64)>) {
    let mut pts = vec![];
    let mut circles = vec![];
    for id in ids {
        let Some(e) = doc.get(*id) else { continue };
        match &e.shape {
            Shape::Circle { c, r } | Shape::Arc { c, r, .. } => circles.push((*c, *r)),
            _ => {}
        }
        if e.shape.is_annotation() {
            continue;
        }
        for (k, p) in e.shape.snap_points() {
            if matches!(k, SnapKind::End | SnapKind::Center) {
                pts.push(p);
            }
        }
    }
    (pts, circles)
}

/// Distinct stations along one axis, each with the feature point nearest to
/// the dimension line.
fn stations(pts: &[V2], horizontal: bool, line: f64, tol: f64) -> Vec<V2> {
    let key = |p: &V2| if horizontal { p.x } else { p.y };
    let across = |p: &V2| if horizontal { p.y } else { p.x };
    let mut sorted = pts.to_vec();
    sorted.sort_by(|a, b| key(a).total_cmp(&key(b)));
    let mut out: Vec<V2> = vec![];
    for p in sorted {
        match out.last_mut() {
            Some(q) if (key(q) - key(&p)).abs() <= tol => {
                if (across(&p) - line).abs() < (across(q) - line).abs() {
                    *q = p;
                }
            }
            _ => out.push(p),
        }
    }
    out
}

/// The dimensions QDIM creates for the selection with the line at `at`.
pub fn quick_dims(doc: &Document, ids: &[u64], mode: QMode, at: V2, datum: V2) -> Vec<Dimension> {
    let (pts, circles) = features(doc, ids);
    let scale = doc.ascale();
    let step = doc.dimstyle.text_height * scale * 2.2;
    if matches!(mode, QMode::Radius | QMode::Diameter) {
        let mut seen: Vec<(V2, f64)> = vec![];
        let mut out = vec![];
        for (c, r) in circles {
            if seen.iter().any(|(sc, sr)| sc.dist(c) < 1e-9 && (sr - r).abs() < 1e-9) {
                continue;
            }
            seen.push((c, r));
            let u = (at - c).norm();
            let u = if u == V2::ZERO { V2::new(1.0, 1.0).norm() } else { u };
            let kind = if mode == QMode::Radius { DimKind::Radius } else { DimKind::Diameter };
            out.push(Dimension::new(kind, c, c + u * r, c + u * (r + step * 1.5)));
        }
        return out;
    }
    if pts.len() < 2 && mode != QMode::Ordinate {
        return vec![];
    }
    let b = BBox::of(pts.iter().copied());
    // Above/below the geometry → horizontal dimensions; beside it → vertical.
    let outside_y = (at.y - b.max.y).max(b.min.y - at.y);
    let outside_x = (at.x - b.max.x).max(b.min.x - at.x);
    let horizontal = outside_y >= outside_x;
    let line = if horizontal { at.y } else { at.x };
    let tol = (b.width().max(b.height()) * 1e-6).max(1e-9);
    let st = stations(&pts, horizontal, line, tol);
    let away = if horizontal {
        if at.y >= b.center().y {
            1.0
        } else {
            -1.0
        }
    } else if at.x >= b.center().x {
        1.0
    } else {
        -1.0
    };
    let place = |p1: V2, p2: V2, offset: f64| {
        let l = line + away * offset;
        if horizontal {
            Dimension::new(DimKind::Horizontal, p1, p2, V2::new(p1.mid(p2).x, l))
        } else {
            Dimension::new(DimKind::Vertical, p1, p2, V2::new(l, p1.mid(p2).y))
        }
    };
    match mode {
        QMode::Continuous => st.windows(2).map(|w| place(w[0], w[1], 0.0)).collect(),
        QMode::Staggered => st.windows(2).enumerate().map(|(i, w)| place(w[0], w[1], if i % 2 == 0 { 0.0 } else { step })).collect(),
        QMode::Baseline => st.iter().skip(1).enumerate().map(|(i, p)| place(st[0], *p, i as f64 * step)).collect(),
        QMode::Ordinate => st
            .iter()
            .map(|p| {
                let end = if horizontal { V2::new(p.x, line) } else { V2::new(line, p.y) };
                Dimension::ordinate(datum, *p, end)
            })
            .collect(),
        QMode::Radius | QMode::Diameter => unreachable!(),
    }
}

impl Command for QDimCmd {
    fn name(&self) -> &'static str {
        "QDIM"
    }
    fn start(&mut self, cx: &mut Cx) -> Flow {
        self.ids = cx.take_selection();
        Flow::Continue
    }
    fn prompt(&self, cx: &Cx) -> String {
        if self.ids.is_empty() {
            "Select geometry to dimension:".into()
        } else {
            format!(
                "Specify dimension line position, or [Continuous/Staggered/Baseline/Ordinate/Radius/Diameter] <{:?}>:",
                cx.mem.qdim_mode
            )
        }
    }
    fn wants(&self) -> Want {
        if self.ids.is_empty() {
            Want::Select
        } else {
            Want::Point
        }
    }
    fn selected(&mut self, cx: &mut Cx, ids: Vec<u64>) -> Flow {
        if ids.is_empty() {
            return Flow::Done;
        }
        self.ids = ids.into_iter().filter(|id| cx.tab.doc.get(*id).is_some_and(|e| !matches!(e.shape, Shape::Dim(_)))).collect();
        Flow::Continue
    }
    fn value(&mut self, cx: &mut Cx, s: &str) -> Flow {
        let m = match s.to_ascii_uppercase().as_str() {
            "C" | "CONTINUOUS" => QMode::Continuous,
            "S" | "STAGGERED" => QMode::Staggered,
            "B" | "BASELINE" => QMode::Baseline,
            "O" | "ORDINATE" => QMode::Ordinate,
            "R" | "RADIUS" => QMode::Radius,
            "D" | "DIAMETER" => QMode::Diameter,
            _ => {
                cx.msg(format!("Invalid input: {s}"));
                return Flow::Continue;
            }
        };
        cx.mem.qdim_mode = m;
        Flow::Continue
    }
    fn point(&mut self, cx: &mut Cx, p: V2) -> Flow {
        let dims = quick_dims(&cx.tab.doc, &self.ids, cx.mem.qdim_mode, p, cx.mem.ordinate_datum);
        if dims.is_empty() {
            cx.msg("Nothing to dimension in the selection.");
            return Flow::Done;
        }
        cx.checkpoint();
        let n = dims.len();
        for d in dims {
            cx.add_dim(d);
        }
        cx.msg(format!("{n} dimension(s) created."));
        Flow::Done
    }
    fn preview(&self, doc: &Document, c: V2) -> Vec<Shape> {
        // The mode is not known here; preview the default continuous chain.
        quick_dims(doc, &self.ids, QMode::Continuous, c, V2::ZERO).into_iter().map(Shape::Dim).collect()
    }
}

// ------------------------------------------------------------------ association

struct AssocCmd {
    on: bool,
}

impl AssocCmd {
    fn run(&self, cx: &mut Cx, ids: Vec<u64>) -> Flow {
        let dims: Vec<u64> = ids.into_iter().filter(|id| cx.tab.doc.get(*id).is_some_and(|e| matches!(e.shape, Shape::Dim(_)))).collect();
        if dims.is_empty() {
            cx.msg("No dimensions selected.");
            return Flow::Done;
        }
        cx.checkpoint();
        let mut n = 0;
        for id in dims {
            let Some(Shape::Dim(mut d)) = cx.shape(id) else { continue };
            d.assoc = None;
            if self.on {
                cx.tab.doc.associate_dim(&mut d);
            }
            n += usize::from(d.assoc.is_some() == self.on);
            if let Some(e) = cx.doc().get_mut(id) {
                e.shape = Shape::Dim(d);
            }
        }
        cx.msg(if self.on { format!("{n} dimension(s) associated.") } else { format!("{n} dimension(s) disassociated.") });
        Flow::Done
    }
}

impl Command for AssocCmd {
    fn name(&self) -> &'static str {
        if self.on {
            "DIMREASSOCIATE"
        } else {
            "DIMDISASSOCIATE"
        }
    }
    fn start(&mut self, cx: &mut Cx) -> Flow {
        let ids = cx.take_selection();
        if ids.is_empty() {
            Flow::Continue
        } else {
            self.run(cx, ids)
        }
    }
    fn prompt(&self, _: &Cx) -> String {
        "Select dimensions:".into()
    }
    fn wants(&self) -> Want {
        Want::Select
    }
    fn selected(&mut self, cx: &mut Cx, ids: Vec<u64>) -> Flow {
        self.run(cx, ids)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn quick_dimension_chain_and_baseline() {
        let mut doc = Document::new();
        let pts = [V2::new(0.0, 0.0), V2::new(40.0, 0.0), V2::new(40.0, 20.0), V2::new(100.0, 20.0), V2::new(100.0, 0.0)];
        let id = doc.add(Shape::Polyline { pts: pts.to_vec(), closed: false });
        let c = doc.add(Shape::Circle { c: V2::new(70.0, 10.0), r: 5.0 });
        let below = V2::new(50.0, -20.0);
        let chain = quick_dims(&doc, &[id, c], QMode::Continuous, below, V2::ZERO);
        let values: Vec<f64> = chain.iter().map(|d| d.measurement()).collect();
        assert_eq!(values, vec![40.0, 30.0, 30.0]);
        assert!(chain.iter().all(|d| d.kind == DimKind::Horizontal && d.loc.y == -20.0));
        let base = quick_dims(&doc, &[id, c], QMode::Baseline, below, V2::ZERO);
        assert_eq!(base.iter().map(|d| d.measurement()).collect::<Vec<_>>(), vec![40.0, 70.0, 100.0]);
        assert!(base[1].loc.y < base[0].loc.y, "baseline rows step away from the part");
        let side = quick_dims(&doc, &[id], QMode::Continuous, V2::new(130.0, 10.0), V2::ZERO);
        assert!(side.iter().all(|d| d.kind == DimKind::Vertical));
        let dia = quick_dims(&doc, &[id, c], QMode::Diameter, V2::new(90.0, 30.0), V2::ZERO);
        assert_eq!(dia.len(), 1);
        assert_eq!(dia[0].measurement(), 10.0);
        let ord = quick_dims(&doc, &[id], QMode::Ordinate, below, V2::ZERO);
        assert_eq!(ord.iter().map(|d| d.measurement()).collect::<Vec<_>>(), vec![0.0, 40.0, 100.0]);
    }
}
