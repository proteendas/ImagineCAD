// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Proteen Das

//! Markup: revision clouds, the bill of materials (BOM) built from the
//! drawing's block references, and balloons that number them.

use crate::document::Document;
use crate::entity::{Leader, Shape};
use crate::geom::*;
use crate::prim::Prim;
use crate::table::{Cell, Table};
use crate::HAlign;
use serde::{Deserialize, Serialize};

/// A closed cloud of outward arcs round a boundary (REVCLOUD).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct RevCloud {
    /// Boundary corners (closed).
    pub pts: Vec<V2>,
    /// Chord length of each arc (drawing units).
    pub arc: f64,
}

/// Included angle of each cloud arc.
const SWEEP: f64 = 2.2;

impl RevCloud {
    /// Arcs (centre, radius, start, end; counter-clockwise) bulging outwards.
    pub fn arcs(&self) -> Vec<(V2, f64, f64, f64)> {
        let n = self.pts.len();
        if n < 2 || self.arc <= 0.0 {
            return vec![];
        }
        // Outward is to the right of travel on a counter-clockwise boundary.
        let ccw = signed_area(&self.pts) >= 0.0;
        let mut out = vec![];
        for i in 0..n {
            let (a, b) = (self.pts[i], self.pts[(i + 1) % n]);
            let len = a.dist(b);
            if len < 1e-9 {
                continue;
            }
            let k = (len / self.arc).round().max(1.0) as usize;
            for j in 0..k {
                let p = a + (b - a) * (j as f64 / k as f64);
                let q = a + (b - a) * ((j + 1) as f64 / k as f64);
                out.push(arc_between(p, q, ccw));
            }
        }
        out
    }

    pub fn prims(&self) -> Vec<Prim> {
        self.arcs().into_iter().map(|(c, r, start, end)| Prim::Arc { c, r, start, end }).collect()
    }

    /// Polyline vertices with DXF bulges (one bulged segment per arc).
    pub fn bulges(&self) -> Vec<(V2, f64)> {
        let ccw = signed_area(&self.pts) >= 0.0;
        let b = (SWEEP / 4.0).tan();
        let mut out = vec![];
        let n = self.pts.len();
        for i in 0..n {
            let (a, e) = (self.pts[i], self.pts[(i + 1) % n]);
            let len = a.dist(e);
            if len < 1e-9 {
                continue;
            }
            let k = (len / self.arc).round().max(1.0) as usize;
            for j in 0..k {
                // Outward on a counter-clockwise boundary is a counter-clockwise arc: positive bulge.
                out.push((a + (e - a) * (j as f64 / k as f64), if ccw { b } else { -b }));
            }
        }
        out
    }
}

/// The arc from `p` to `q` bulging to the outside of the boundary.
fn arc_between(p: V2, q: V2, ccw: bool) -> (V2, f64, f64, f64) {
    let chord = p.dist(q);
    let r = chord / (2.0 * (SWEEP / 2.0).sin());
    let h = r * (SWEEP / 2.0).cos();
    let d = (q - p).norm();
    // Centre on the inside (left of travel on a CCW boundary).
    let inside = if ccw { d.perp() } else { -d.perp() };
    let c = p.mid(q) + inside * h;
    let (ap, aq) = ((p - c).angle(), (q - c).angle());
    // With the centre inside, the outward arc of a CCW boundary runs
    // counter-clockwise from p to q.
    let (start, end) = if ccw { (ap, aq) } else { (aq, ap) };
    (c, r, norm_angle(start), norm_angle(end))
}

pub fn signed_area(pts: &[V2]) -> f64 {
    let n = pts.len();
    (0..n).map(|i| pts[i].cross(pts[(i + 1) % n])).sum::<f64>() / 2.0
}

/// One line of a bill of materials.
#[derive(Clone, Debug, PartialEq)]
pub struct BomItem {
    pub item: usize,
    pub block: String,
    pub qty: usize,
    /// Attribute values (tag, value), in the order of the BOM columns.
    pub fields: Vec<(String, String)>,
    /// Entity ids of the block references counted.
    pub refs: Vec<u64>,
}

/// Attribute tags that describe a part, in BOM column order (others follow).
const PREFERRED: [&str; 6] = ["PART_NO", "PART", "DESCRIPTION", "MATERIAL", "STANDARD", "MASS"];

impl Document {
    /// Block references that are parts: not anonymous, not external references
    /// and not the title block.
    fn part_refs(&self) -> Vec<(u64, &crate::entity::Insert)> {
        let title = self.sheet.title_block.as_deref().unwrap_or("");
        self.entities
            .iter()
            .filter_map(|e| if let Shape::Insert(i) = &e.shape { Some((e.id, i)) } else { None })
            .filter(|(_, i)| {
                !i.name.starts_with('*')
                    && !i.name.eq_ignore_ascii_case(title)
                    && self.block(&i.name).is_some_and(|b| b.xref.is_none())
                    && !self.is_xref_dependent(&i.name)
            })
            .collect()
    }

    /// Bill of materials: one item per block and set of attribute values,
    /// numbered in drawing order.
    pub fn bom(&self) -> Vec<BomItem> {
        let mut items: Vec<BomItem> = vec![];
        for (id, ins) in self.part_refs() {
            let b = self.block(&ins.name).expect("checked");
            let fields: Vec<(String, String)> =
                b.attdefs().map(|a| (a.tag.to_ascii_uppercase(), ins.value_of(&a.tag).unwrap_or(&a.default).to_string())).collect();
            match items.iter_mut().find(|it| it.block.eq_ignore_ascii_case(&ins.name) && it.fields == fields) {
                Some(it) => {
                    it.qty += 1;
                    it.refs.push(id);
                }
                None => items.push(BomItem { item: items.len() + 1, block: ins.name.clone(), qty: 1, fields, refs: vec![id] }),
            }
        }
        items
    }

    /// The BOM as a table (ITEM, QTY, then the attribute columns; the block
    /// name describes parts without attributes).
    pub fn bom_table(&self, pos: V2, text_h: f64) -> Option<Table> {
        let items = self.bom();
        if items.is_empty() {
            return None;
        }
        let mut tags: Vec<String> = vec![];
        for p in PREFERRED {
            if items.iter().any(|it| it.fields.iter().any(|f| f.0 == p)) {
                tags.push(p.to_string());
            }
        }
        for it in &items {
            for (t, _) in &it.fields {
                if !tags.contains(t) {
                    tags.push(t.clone());
                }
            }
        }
        let describe = !tags.iter().any(|t| t == "DESCRIPTION");
        let mut head = vec!["ITEM".to_string(), "QTY".to_string()];
        if describe {
            head.push("DESCRIPTION".into());
        }
        head.extend(tags.iter().map(|t| t.replace('_', " ")));
        let mut rows = vec![head];
        for it in &items {
            let mut r = vec![it.item.to_string(), it.qty.to_string()];
            if describe {
                r.push(it.block.clone());
            }
            for t in &tags {
                r.push(it.fields.iter().find(|f| &f.0 == t).map(|f| f.1.clone()).unwrap_or_default());
            }
            rows.push(r);
        }
        let mut t = Table::from_rows(pos, &rows, text_h);
        t.insert_row(0);
        let cols = t.cols();
        t.cells[0][0] = Cell { text: "BILL OF MATERIALS".into(), align: HAlign::Center, span: cols - 1 };
        t.tag = "BOM".into();
        Some(t)
    }

    /// Balloons for every BOM item: a numbered leader from one of its block
    /// references to a point outside the drawing, spread round it so that no
    /// two balloons or leaders cross.
    pub fn bom_balloons(&self, gap: f64) -> Vec<(usize, Leader)> {
        let items = self.bom();
        let ext = self
            .entities
            .iter()
            .filter(|e| !e.shape.is_annotation() && self.is_visible(e))
            .fold(BBox::empty(), |b, e| b.union(&self.entity_bbox(e)));
        if ext.is_empty() {
            return vec![];
        }
        let centre = ext.center();
        // Leader tip: the point of the part's own geometry nearest its centre.
        let mut tips: Vec<(usize, V2)> = vec![];
        for it in &items {
            let Some(e) = it.refs.first().and_then(|id| self.get(*id)) else { continue };
            let b = self.entity_bbox(e);
            let pts: Vec<V2> = self
                .prims_of(e)
                .iter()
                .flat_map(|p| match p {
                    Prim::Line(a, c) => vec![*a, *c, a.mid(*c)],
                    Prim::Poly(v, _) | Prim::Fill(v) => v.clone(),
                    Prim::Circle(c, r) => vec![*c + V2::new(*r, 0.0), *c + V2::new(0.0, *r), *c - V2::new(*r, 0.0)],
                    Prim::Arc { c, r, start, end } => vec![*c + V2::polar(*r, start + arc_sweep(*start, *end) / 2.0)],
                    _ => vec![],
                })
                .collect();
            let tip = pts.into_iter().min_by(|a, c| a.dist(b.center()).total_cmp(&c.dist(b.center()))).unwrap_or(b.center());
            tips.push((it.item, tip));
        }
        // Balloons round the drawing, each in the direction of its tip, pushed
        // apart so neighbours keep clear of each other.
        tips.sort_by(|a, b| (a.1 - centre).angle().total_cmp(&(b.1 - centre).angle()));
        let n = tips.len();
        let half = V2::new(ext.width() / 2.0 + gap, ext.height() / 2.0 + gap);
        let ring = half.x.min(half.y).max(gap);
        let sep = (1.8 * gap / ring).min(std::f64::consts::TAU / n.max(1) as f64);
        let mut angs: Vec<f64> = tips.iter().map(|t| (t.1 - centre).angle()).collect();
        for _ in 0..100 {
            let mut moved = false;
            for i in 0..n {
                let j = (i + 1) % n;
                if n < 2 || (i == n - 1 && n == 2) {
                    continue;
                }
                let mut d = angs[j] - angs[i];
                if j == 0 {
                    d += std::f64::consts::TAU;
                }
                if d < sep - 1e-9 {
                    let push = (sep - d) / 2.0;
                    angs[i] -= push;
                    angs[j] += push;
                    moved = true;
                }
            }
            if !moved {
                break;
            }
        }
        let mut out = vec![];
        for ((item, tip), a) in tips.into_iter().zip(angs) {
            let dir = V2::polar(1.0, a);
            // Where the ray leaves the drawing's box grown by `gap`.
            let t = (half.x / dir.x.abs().max(1e-9)).min(half.y / dir.y.abs().max(1e-9));
            let at = centre + dir * t;
            out.push((item, Leader { pts: vec![tip, at], text: item.to_string(), style: Some("Balloon".into()) }));
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::entity::{AttDef, BlockDef, Entity, Insert};
    use crate::prim::Text;
    use crate::style::{Color, LtRef};

    #[test]
    fn cloud_arcs_bulge_outwards() {
        let c = RevCloud { pts: vec![V2::new(0.0, 0.0), V2::new(40.0, 0.0), V2::new(40.0, 20.0), V2::new(0.0, 20.0)], arc: 10.0 };
        let arcs = c.arcs();
        assert_eq!(arcs.len(), 12);
        // Bottom edge arcs dip below y = 0; their centres are inside.
        let (cen, r, s, e) = arcs[0];
        assert!(cen.y > 0.0);
        let mid = cen + V2::polar(r, s + arc_sweep(s, e) / 2.0);
        assert!(mid.y < 0.0, "{mid:?}");
        // The same cloud drawn clockwise bulges the same way.
        let mut cw = c.clone();
        cw.pts.reverse();
        let (cen2, r2, s2, e2) = cw.arcs()[0];
        let mid2 = cen2 + V2::polar(r2, s2 + arc_sweep(s2, e2) / 2.0);
        // Reversed, the first edge is the top one: its arc rises above it.
        assert!(mid2.y > 20.0, "{mid2:?}");
        assert_eq!(c.bulges().len(), 12);
    }

    #[test]
    fn bom_counts_parts_and_balloons_each() {
        let mut doc = Document::new();
        let ent =
            |shape: Shape| Entity { id: 0, layer: "0".into(), color: Color::ByLayer, linetype: LtRef::ByLayer, lineweight: None, shape };
        let att = |tag: &str, d: &str| {
            ent(Shape::AttDef(AttDef {
                tag: tag.into(),
                prompt: tag.into(),
                default: d.into(),
                text: Text::new(V2::ZERO, 2.5, ""),
                invisible: true,
            }))
        };
        doc.blocks.push(BlockDef {
            name: "BOLT_M8".into(),
            base: V2::ZERO,
            entities: vec![ent(Shape::Circle { c: V2::ZERO, r: 4.0 }), att("PART_NO", "ISO 4017 M8x30"), att("MATERIAL", "8.8")],
            description: String::new(),
            xref: None,
        });
        doc.blocks.push(BlockDef {
            name: "PLATE".into(),
            base: V2::ZERO,
            entities: vec![ent(Shape::Polyline {
                pts: vec![V2::ZERO, V2::new(100.0, 0.0), V2::new(100.0, 60.0), V2::new(0.0, 60.0)],
                closed: true,
            })],
            description: String::new(),
            xref: None,
        });
        doc.add(Shape::Insert(Insert::new("PLATE", V2::ZERO)));
        for x in [20.0, 80.0] {
            doc.add(Shape::Insert(Insert::new("BOLT_M8", V2::new(x, 30.0))));
        }
        let bom = doc.bom();
        assert_eq!(bom.len(), 2);
        assert_eq!((bom[0].block.as_str(), bom[0].qty), ("PLATE", 1));
        assert_eq!((bom[1].block.as_str(), bom[1].qty), ("BOLT_M8", 2));
        let t = doc.bom_table(V2::ZERO, 2.5).unwrap();
        assert_eq!(t.tag, "BOM");
        let header: Vec<&str> = t.cells[1].iter().map(|c| c.text.as_str()).collect();
        assert_eq!(header, ["ITEM", "QTY", "DESCRIPTION", "PART NO", "MATERIAL"]);
        assert_eq!(t.cells[3][1].text, "2");
        assert_eq!(t.cells[3][3].text, "ISO 4017 M8x30");
        let balloons = doc.bom_balloons(15.0);
        assert_eq!(balloons.len(), 2);
        for (_, l) in &balloons {
            let ext = BBox::from_pts(V2::new(-4.0, 0.0), V2::new(100.0, 60.0));
            assert!(!ext.contains(l.pts[1]), "balloon inside the part: {:?}", l.pts[1]);
        }
    }
}
