// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Proteen Das

//! Dynamic blocks: parameters stored with a block definition (stretch,
//! flip, visibility states, lookup tables) whose values each block reference
//! sets, so one definition draws many sizes and variants.

use crate::entity::{BlockDef, Entity, Insert, Shape};
use crate::geom::*;
use serde::{Deserialize, Serialize};
use std::borrow::Cow;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum DynParam {
    /// Points inside `frame` of the listed entities move along `dir` by the
    /// value minus `default` (whole entities inside the frame move as one).
    Stretch {
        name: String,
        base: V2,
        dir: V2,
        default: f64,
        #[serde(default)]
        min: Option<f64>,
        #[serde(default)]
        max: Option<f64>,
        frame: BBox,
        ids: Vec<u64>,
    },
    /// The listed entities mirrored about the line `a`–`b` when flipped.
    Flip { name: String, a: V2, b: V2, ids: Vec<u64> },
    /// Named states, each showing its own entities; entities listed in no
    /// state are always shown.
    Visibility { name: String, states: Vec<(String, Vec<u64>)> },
    /// Rows that set several other parameters at once (e.g. sizes of a part).
    Lookup { name: String, columns: Vec<String>, rows: Vec<(String, Vec<String>)> },
}

impl DynParam {
    pub fn name(&self) -> &str {
        match self {
            DynParam::Stretch { name, .. }
            | DynParam::Flip { name, .. }
            | DynParam::Visibility { name, .. }
            | DynParam::Lookup { name, .. } => name,
        }
    }

    /// The value a reference without its own value uses.
    pub fn default_value(&self) -> String {
        match self {
            DynParam::Stretch { default, .. } => fmt_num(*default),
            DynParam::Flip { .. } => "No".into(),
            DynParam::Visibility { states, .. } => states.first().map(|s| s.0.clone()).unwrap_or_default(),
            DynParam::Lookup { rows, .. } => rows.first().map(|r| r.0.clone()).unwrap_or_default(),
        }
    }

    /// Values offered to choose from (empty for a free number).
    pub fn choices(&self) -> Vec<String> {
        match self {
            DynParam::Stretch { .. } => vec![],
            DynParam::Flip { .. } => vec!["No".into(), "Yes".into()],
            DynParam::Visibility { states, .. } => states.iter().map(|s| s.0.clone()).collect(),
            DynParam::Lookup { rows, .. } => rows.iter().map(|r| r.0.clone()).collect(),
        }
    }
}

impl Insert {
    /// This reference's value of dynamic parameter `name`.
    pub fn dyn_value(&self, name: &str) -> Option<&str> {
        self.dyn_values.iter().find(|(n, _)| n.eq_ignore_ascii_case(name)).map(|(_, v)| v.as_str())
    }

    pub fn set_dyn_value(&mut self, name: &str, value: impl Into<String>) {
        let value = value.into();
        match self.dyn_values.iter_mut().find(|(n, _)| n.eq_ignore_ascii_case(name)) {
            Some(v) => v.1 = value,
            None => self.dyn_values.push((name.to_string(), value)),
        }
    }
}

fn fmt_num(v: f64) -> String {
    let s = format!("{v:.4}");
    s.trim_end_matches('0').trim_end_matches('.').to_string()
}

/// Move the points of `s` that lie in `frame` by `d` (the whole shape when it
/// lies inside).
fn stretch_shape(s: &mut Shape, frame: &BBox, d: V2) {
    let inside = |p: V2| frame.contains(p);
    match s {
        Shape::Line { a, b } => {
            if inside(*a) {
                *a += d;
            }
            if inside(*b) {
                *b += d;
            }
        }
        Shape::Polyline { pts, .. } | Shape::Spline { pts, .. } => pts.iter_mut().filter(|p| inside(**p)).for_each(|p| *p += d),
        Shape::Hatch(h) => h.loops.iter_mut().flatten().filter(|p| inside(**p)).for_each(|p| *p += d),
        other => {
            // Circles, arcs, text…: move when their reference point is inside.
            if other.grips().first().is_some_and(|p| inside(*p)) {
                other.transform(&Xform::translate(d));
            }
        }
    }
}

impl BlockDef {
    /// The block's contents for reference `ins`, with its dynamic parameter
    /// values applied (the definition itself when it has none).
    pub fn entities_for(&self, ins: &Insert) -> Cow<'_, [Entity]> {
        if self.params.is_empty() {
            return Cow::Borrowed(&self.entities);
        }
        // Lookup rows set other parameters first.
        let mut values: Vec<(String, String)> = self
            .params
            .iter()
            .map(|p| (p.name().to_string(), ins.dyn_value(p.name()).map(String::from).unwrap_or_else(|| p.default_value())))
            .collect();
        for p in &self.params {
            if let DynParam::Lookup { name, columns, rows } = p {
                let pick = values.iter().find(|(n, _)| n == name).map(|v| v.1.clone()).unwrap_or_default();
                if let Some((_, row)) = rows.iter().find(|r| r.0 == pick) {
                    for (c, v) in columns.iter().zip(row) {
                        if let Some(slot) = values.iter_mut().find(|(n, _)| n.eq_ignore_ascii_case(c)) {
                            slot.1 = v.clone();
                        }
                    }
                }
            }
        }
        let value = |name: &str| values.iter().find(|(n, _)| n.eq_ignore_ascii_case(name)).map(|v| v.1.as_str()).unwrap_or("");
        let mut out = self.entities.clone();
        for p in &self.params {
            match p {
                DynParam::Stretch { name, dir, default, min, max, frame, ids, .. } => {
                    let mut v = value(name).trim().parse::<f64>().unwrap_or(*default);
                    if let Some(m) = min {
                        v = v.max(*m);
                    }
                    if let Some(m) = max {
                        v = v.min(*m);
                    }
                    let d = dir.norm() * (v - default);
                    if d.len() > 0.0 {
                        for e in out.iter_mut().filter(|e| ids.contains(&e.id)) {
                            stretch_shape(&mut e.shape, frame, d);
                        }
                    }
                }
                DynParam::Flip { name, a, b, ids } => {
                    if value(name).eq_ignore_ascii_case("yes") {
                        let x = Xform::mirror(*a, *b);
                        for e in out.iter_mut().filter(|e| ids.contains(&e.id)) {
                            e.shape.transform(&x);
                        }
                    }
                }
                DynParam::Visibility { name, states } => {
                    let state = value(name);
                    let shown: Vec<u64> = states.iter().find(|s| s.0 == state).map(|s| s.1.clone()).unwrap_or_default();
                    let in_any: Vec<u64> = states.iter().flat_map(|s| s.1.iter().copied()).collect();
                    out.retain(|e| !in_any.contains(&e.id) || shown.contains(&e.id));
                }
                DynParam::Lookup { .. } => {}
            }
        }
        Cow::Owned(out)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::style::{Color, LtRef};

    fn ent(id: u64, shape: Shape) -> Entity {
        Entity { id, layer: "0".into(), color: Color::ByLayer, linetype: LtRef::ByLayer, lineweight: None, shape }
    }

    #[test]
    fn stretch_flip_visibility_lookup() {
        // A 100 × 40 plate with a hole, a notch on the right and two hole states.
        let mut b = BlockDef {
            name: "PLATE".into(),
            base: V2::ZERO,
            entities: vec![
                ent(
                    1,
                    Shape::Polyline { pts: vec![V2::ZERO, V2::new(100.0, 0.0), V2::new(100.0, 40.0), V2::new(0.0, 40.0)], closed: true },
                ),
                ent(2, Shape::Circle { c: V2::new(80.0, 20.0), r: 5.0 }),
                ent(3, Shape::Circle { c: V2::new(20.0, 20.0), r: 8.0 }),
                ent(4, Shape::Line { a: V2::new(90.0, 40.0), b: V2::new(100.0, 30.0) }),
            ],
            description: String::new(),
            xref: None,
            params: vec![],
        };
        b.params = vec![
            DynParam::Stretch {
                name: "Length".into(),
                base: V2::new(0.0, -10.0),
                dir: V2::new(1.0, 0.0),
                default: 100.0,
                min: Some(60.0),
                max: None,
                frame: BBox::from_pts(V2::new(70.0, -1.0), V2::new(101.0, 41.0)),
                ids: vec![1, 2, 4],
            },
            DynParam::Flip { name: "Flip".into(), a: V2::new(0.0, 20.0), b: V2::new(1.0, 20.0), ids: vec![4] },
            DynParam::Visibility { name: "Hole".into(), states: vec![("Small".into(), vec![2]), ("None".into(), vec![])] },
            DynParam::Lookup {
                name: "Size".into(),
                columns: vec!["Length".into(), "Hole".into()],
                rows: vec![("L150".into(), vec!["150".into(), "Small".into()]), ("L200 plain".into(), vec!["200".into(), "None".into()])],
            },
        ];
        let mut ins = Insert::new("PLATE", V2::ZERO);
        // Defaults: unchanged geometry, the small hole shown.
        assert_eq!(b.entities_for(&ins).len(), 4);
        ins.set_dyn_value("Size", "L200 plain");
        let e = b.entities_for(&ins);
        let Shape::Polyline { pts, .. } = &e[0].shape else { panic!() };
        assert_eq!(pts[1], V2::new(200.0, 0.0));
        assert_eq!(pts[0], V2::ZERO);
        // "None" hides the hole listed in "Small"; the big hole (in no state) stays.
        assert!(e.iter().all(|x| x.id != 2) && e.iter().any(|x| x.id == 3));
        ins.set_dyn_value("Size", "");
        ins.set_dyn_value("Length", "30");
        let e = b.entities_for(&ins);
        let Shape::Polyline { pts, .. } = &e[0].shape else { panic!() };
        assert_eq!(pts[1].x, 60.0, "clamped to the minimum");
        ins.set_dyn_value("Flip", "Yes");
        let e = b.entities_for(&ins);
        let Some(Shape::Line { a, .. }) = e.iter().find(|x| x.id == 4).map(|x| &x.shape) else { panic!() };
        assert!((a.y - 0.0).abs() < 1e-9, "{a:?}");
    }
}
