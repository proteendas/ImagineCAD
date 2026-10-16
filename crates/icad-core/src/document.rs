// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Proteen Das

//! The drawing document: entities, layers, blocks, dimension style and sheet.

use crate::entity::*;
use crate::geom::*;
use crate::prim::Prim;
use crate::sheet::Sheet;
use crate::style::*;
use serde::{Deserialize, Serialize};

pub const FILE_VERSION: u32 = 2;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Document {
    #[serde(default)]
    pub version: u32,
    pub entities: Vec<Entity>,
    pub layers: Vec<Layer>,
    pub current_layer: String,
    pub dimstyle: DimStyle,
    pub sheet: Sheet,
    /// Global linetype scale (LTSCALE).
    #[serde(default = "one")]
    pub ltscale: f64,
    pub next_id: u64,
    /// Block definitions.
    #[serde(default)]
    pub blocks: Vec<BlockDef>,
    /// Parametric part the drawing was generated from, if any.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub part: Option<PartRecord>,
    /// Multileader styles and the current one.
    #[serde(default = "default_mleader_styles")]
    pub mleader_styles: Vec<crate::table::MLeaderStyle>,
    #[serde(default = "standard")]
    pub mleader_style: String,
    /// Paper-space layouts besides the main sheet.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub layouts: Vec<crate::layout::Layout>,
    /// Text styles and the current one.
    #[serde(default = "default_text_styles")]
    pub text_styles: Vec<crate::fonts::TextStyle>,
    #[serde(default = "standard")]
    pub text_style: String,
    /// File name for the FILENAME field (set by the application; not saved).
    #[serde(skip)]
    pub file_name: String,
}

/// A parametric part (shaft, plate) and the entities generated from it, so the
/// drawing can be regenerated when its parameters change.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct PartRecord {
    /// The part description (`{"type": "shaft", "segments": [...]}` …).
    pub spec: serde_json::Value,
    /// Entities generated from it.
    pub ids: Vec<u64>,
}

fn one() -> f64 {
    1.0
}

fn default_mleader_styles() -> Vec<crate::table::MLeaderStyle> {
    vec![crate::table::MLeaderStyle::standard(), crate::table::MLeaderStyle::balloon()]
}

fn default_text_styles() -> Vec<crate::fonts::TextStyle> {
    vec![crate::fonts::TextStyle::standard()]
}

fn standard() -> String {
    "Standard".into()
}

/// Layers created by the mechanical drawing template.
pub mod layers {
    pub const OUTLINE: &str = "OUTLINE";
    pub const HIDDEN: &str = "HIDDEN";
    pub const CENTER: &str = "CENTER";
    pub const DIMENSIONS: &str = "DIMENSIONS";
    pub const TEXT: &str = "TEXT";
    pub const HATCH: &str = "HATCH";
    pub const CONSTRUCTION: &str = "CONSTRUCTION";
    pub const AI_REVIEW: &str = "AI-REVIEW";
    pub const AI_SKETCH: &str = "AI-SKETCH-ANNOTATION";
    pub const AI_IMAGE: &str = "AI-SKETCH-IMAGE";
}

/// A primitive with its resolved display properties.
pub struct RenderItem {
    pub prim: Prim,
    pub color: u8,
    pub linetype: LineType,
    pub lineweight: f64,
}

impl Default for Document {
    fn default() -> Self {
        Self::new()
    }
}

impl Document {
    /// A new drawing from the mechanical template.
    pub fn new() -> Self {
        let mut construction = Layer::new(layers::CONSTRUCTION, 8, LineType::Continuous, 0.13);
        construction.plot = false;
        let layers = vec![
            Layer::new("0", 7, LineType::Continuous, 0.25),
            Layer::new(layers::OUTLINE, 7, LineType::Continuous, 0.5),
            Layer::new(layers::HIDDEN, 2, LineType::Hidden, 0.25),
            Layer::new(layers::CENTER, 1, LineType::Center, 0.18),
            Layer::new(layers::DIMENSIONS, 4, LineType::Continuous, 0.18),
            Layer::new(layers::TEXT, 3, LineType::Continuous, 0.25),
            Layer::new(layers::HATCH, 8, LineType::Continuous, 0.18),
            construction,
        ];
        Self {
            version: FILE_VERSION,
            entities: vec![],
            layers,
            current_layer: layers::OUTLINE.into(),
            dimstyle: DimStyle::default(),
            sheet: Sheet::default(),
            ltscale: 1.0,
            next_id: 1,
            blocks: vec![],
            part: None,
            mleader_styles: default_mleader_styles(),
            mleader_style: standard(),
            layouts: vec![],
            text_styles: default_text_styles(),
            text_style: standard(),
            file_name: String::new(),
        }
    }

    pub fn to_json(&self) -> String {
        serde_json::to_string_pretty(self).unwrap_or_default()
    }

    pub fn from_json(s: &str) -> Result<Self, String> {
        let mut d: Document = serde_json::from_str(s).map_err(|e| e.to_string())?;
        let max_block_id = d.blocks.iter().flat_map(|b| b.entities.iter()).map(|e| e.id + 1).max().unwrap_or(1);
        d.next_id = d.next_id.max(d.entities.iter().map(|e| e.id + 1).max().unwrap_or(1)).max(max_block_id);
        if d.layers.is_empty() {
            d.layers.push(Layer::new("0", 7, LineType::Continuous, 0.25));
        }
        if d.layer(&d.current_layer).is_none() {
            d.current_layer = d.layers[0].name.clone();
        }
        d.version = FILE_VERSION;
        Ok(d)
    }

    /// Annotation scale multiplier (text heights, arrow sizes…).
    pub fn ascale(&self) -> f64 {
        self.sheet.scale
    }

    pub fn ctx(&self) -> Ctx<'_> {
        Ctx { ds: &self.dimstyle, scale: self.ascale(), blocks: &self.blocks, depth: 0, mstyles: &self.mleader_styles }
    }

    pub fn alloc_id(&mut self) -> u64 {
        let id = self.next_id;
        self.next_id += 1;
        id
    }

    pub fn add(&mut self, shape: Shape) -> u64 {
        let layer = self.current_layer.clone();
        self.add_on(&layer, shape)
    }

    pub fn add_on(&mut self, layer: &str, shape: Shape) -> u64 {
        let id = self.alloc_id();
        self.entities.push(Entity {
            id,
            layer: layer.to_string(),
            color: Color::ByLayer,
            linetype: LtRef::ByLayer,
            lineweight: None,
            shape,
        });
        id
    }

    /// Add with properties copied from another entity (used by modify commands).
    pub fn add_like(&mut self, proto: &Entity, shape: Shape) -> u64 {
        let id = self.alloc_id();
        self.entities.push(Entity { id, shape, ..proto.clone() });
        id
    }

    pub fn get(&self, id: u64) -> Option<&Entity> {
        self.entities.iter().find(|e| e.id == id)
    }

    pub fn get_mut(&mut self, id: u64) -> Option<&mut Entity> {
        self.entities.iter_mut().find(|e| e.id == id)
    }

    pub fn remove(&mut self, ids: &[u64]) {
        self.entities.retain(|e| !ids.contains(&e.id));
    }

    pub fn layer(&self, name: &str) -> Option<&Layer> {
        self.layers.iter().find(|l| l.name.eq_ignore_ascii_case(name))
    }

    pub fn layer_mut(&mut self, name: &str) -> Option<&mut Layer> {
        self.layers.iter_mut().find(|l| l.name.eq_ignore_ascii_case(name))
    }

    pub fn ensure_layer(&mut self, layer: Layer) {
        if self.layer(&layer.name).is_none() {
            self.layers.push(layer);
        }
    }

    pub fn block(&self, name: &str) -> Option<&BlockDef> {
        self.blocks.iter().find(|b| b.name.eq_ignore_ascii_case(name))
    }

    /// Define (or redefine) a block.
    pub fn define_block(&mut self, def: BlockDef) {
        self.blocks.retain(|b| !b.name.eq_ignore_ascii_case(&def.name));
        self.blocks.push(def);
    }

    pub fn is_visible(&self, e: &Entity) -> bool {
        self.layer(&e.layer).is_none_or(|l| l.visible)
    }

    pub fn is_locked(&self, e: &Entity) -> bool {
        self.layer(&e.layer).is_some_and(|l| l.locked)
    }

    pub fn color_of(&self, e: &Entity) -> u8 {
        match e.color {
            Color::Aci(c) => c,
            Color::ByLayer => self.layer(&e.layer).map_or(7, |l| l.color),
        }
    }

    pub fn linetype_of(&self, e: &Entity) -> LineType {
        match e.linetype {
            LtRef::Lt(l) => l,
            LtRef::ByLayer => self.layer(&e.layer).map_or(LineType::Continuous, |l| l.linetype),
        }
    }

    pub fn lineweight_of(&self, e: &Entity) -> f64 {
        e.lineweight.unwrap_or_else(|| self.layer(&e.layer).map_or(0.25, |l| l.lineweight))
    }

    pub fn prims_of(&self, e: &Entity) -> Vec<Prim> {
        e.shape.prims(&self.ctx()).into_iter().map(|p| self.resolve_prim(p)).collect()
    }

    /// Text with its style applied and `{{FIELD}}` placeholders replaced by
    /// their current values.
    fn resolve_prim(&self, p: Prim) -> Prim {
        match p {
            Prim::Text(mut t) => {
                if t.content.contains("{{") {
                    t.content = crate::fields::resolve(self, &t.content);
                }
                if let Some(st) = t.style.as_deref().and_then(|n| self.text_style(n)) {
                    t.font = st.file.clone();
                    t.width_factor = st.width_factor;
                    t.oblique = st.oblique.to_radians();
                }
                Prim::Text(t)
            }
            p => p,
        }
    }

    /// Text style by name (case-insensitive).
    pub fn text_style(&self, name: &str) -> Option<&crate::fonts::TextStyle> {
        self.text_styles.iter().find(|s| s.name.eq_ignore_ascii_case(name))
    }

    /// Border, zones and title block: the built-in title block, or the block
    /// chosen in `sheet.title_block` with its attributes filled from the
    /// title fields (each attribute tag names a field: COMPANY, TITLE, SCALE…).
    pub fn sheet_prims(&self) -> Vec<(Prim, f64)> {
        self.sheet_prims_for(&self.sheet)
    }

    /// Border and title block of any sheet of this drawing (main or layout).
    pub fn sheet_prims_for(&self, sh: &crate::sheet::Sheet) -> Vec<(Prim, f64)> {
        let Some(b) = sh.title_block.as_deref().and_then(|n| self.block(n)) else { return sh.prims() };
        let mut out = sh.prims_with(false);
        let mut ins = Insert::new(b.name.clone(), sh.title_anchor());
        ins.sx = sh.scale;
        ins.sy = sh.scale;
        for a in b.attdefs() {
            if let Some(v) = sh.field(&a.tag) {
                ins.set_value(&a.tag, v);
            }
        }
        let x = ins.xform(b.base);
        let ctx = self.ctx();
        for e in b.entities.iter().filter(|e| !matches!(e.shape, Shape::AttDef(_))) {
            let lw = e.lineweight.unwrap_or(0.25);
            out.extend(e.shape.prims(&ctx).into_iter().map(|p| (self.resolve_prim(p.transformed(&x)), lw)));
        }
        out.extend(ins.attribute_texts(b).into_iter().map(|t| (self.resolve_prim(Prim::Text(t).transformed(&x)), 0.25)));
        out
    }

    /// Primitives with resolved colour/linetype/lineweight. Objects inside a
    /// block that sit on layer `0` with ByLayer properties take the insert's
    /// properties (the usual CAD convention).
    pub fn render_items(&self, e: &Entity) -> Vec<RenderItem> {
        let mut out = vec![];
        self.collect_items(e, None, Xform::identity(), 0, &mut out);
        out
    }

    fn collect_items(&self, e: &Entity, parent: Option<&Entity>, x: Xform, depth: u8, out: &mut Vec<RenderItem>) {
        let inherit = parent.is_some() && e.layer == "0";
        let src = if inherit { parent.unwrap() } else { e };
        let color = match (e.color, inherit) {
            (Color::Aci(c), _) => c,
            (Color::ByLayer, _) => self.color_of(src),
        };
        let linetype = match (e.linetype, inherit) {
            (LtRef::Lt(l), _) => l,
            (LtRef::ByLayer, _) => self.linetype_of(src),
        };
        let lineweight = e.lineweight.unwrap_or_else(|| self.lineweight_of(src));
        if let Shape::Insert(ins) = &e.shape {
            if depth > 8 {
                return;
            }
            if let Some(b) = self.block(&ins.name) {
                let bx = compose(&x, &ins.xform(b.base));
                // Nested objects inherit from this insert (resolved properties).
                let mut proxy = e.clone();
                proxy.color = Color::Aci(color);
                proxy.linetype = LtRef::Lt(linetype);
                proxy.lineweight = Some(lineweight);
                for sub in &b.entities {
                    if self.layer(&sub.layer).is_some_and(|l| !l.visible) {
                        continue;
                    }
                    // Attribute definitions show this reference's value.
                    if let Shape::AttDef(a) = &sub.shape {
                        if a.invisible {
                            continue;
                        }
                        let mut t = a.text.clone();
                        t.content = ins.value_of(&a.tag).unwrap_or(&a.default).to_string();
                        let shown = Entity { shape: Shape::Text(t), ..sub.clone() };
                        self.collect_items(&shown, Some(&proxy), bx, depth + 1, out);
                        continue;
                    }
                    self.collect_items(sub, Some(&proxy), bx, depth + 1, out);
                }
                return;
            }
        }
        let ctx = self.ctx();
        for p in e.shape.prims(&ctx) {
            out.push(RenderItem { prim: self.resolve_prim(p.transformed(&x)), color, linetype, lineweight });
        }
    }

    pub fn entity_bbox(&self, e: &Entity) -> BBox {
        e.shape.bbox(&self.ctx())
    }

    /// Extents of visible entities (excluding the sheet and construction lines).
    pub fn extents(&self) -> BBox {
        self.entities
            .iter()
            .filter(|e| self.is_visible(e) && !e.shape.is_construction())
            .fold(BBox::empty(), |b, e| b.union(&self.entity_bbox(e)))
    }

    /// Entity closest to `p` within `tol`.
    pub fn pick(&self, p: V2, tol: f64) -> Option<u64> {
        let probe = BBox::from_pts(p, p).expand(tol);
        let ctx = self.ctx();
        self.entities
            .iter()
            .rev()
            .filter(|e| self.is_visible(e) && self.entity_bbox(e).expand(tol).intersects(&probe))
            .map(|e| (e.id, e.shape.distance(p, &ctx)))
            .filter(|(_, d)| *d <= tol)
            .min_by(|a, b| a.1.total_cmp(&b.1))
            .map(|(id, _)| id)
    }

    /// Every entity within `tol` of `p` (for selection cycling).
    pub fn pick_all(&self, p: V2, tol: f64) -> Vec<u64> {
        let ctx = self.ctx();
        let mut v: Vec<(u64, f64)> = self
            .entities
            .iter()
            .filter(|e| self.is_visible(e))
            .map(|e| (e.id, e.shape.distance(p, &ctx)))
            .filter(|(_, d)| *d <= tol)
            .collect();
        v.sort_by(|a, b| a.1.total_cmp(&b.1));
        v.into_iter().map(|x| x.0).collect()
    }

    /// Window (fully inside) or crossing selection.
    pub fn select_box(&self, bx: &BBox, crossing: bool) -> Vec<u64> {
        let ctx = self.ctx();
        self.entities
            .iter()
            .filter(|e| self.is_visible(e))
            .filter(|e| {
                let b = self.entity_bbox(e);
                if crossing {
                    bx.contains_box(&b) || e.shape.crosses(bx, &ctx)
                } else {
                    bx.contains_box(&b)
                }
            })
            .map(|e| e.id)
            .collect()
    }

    /// Curves of an entity (block references are expanded).
    pub fn curves_of(&self, e: &Entity) -> Vec<Curve> {
        self.curves_rec(&e.shape, Xform::identity(), 0)
    }

    fn curves_rec(&self, s: &Shape, x: Xform, depth: u8) -> Vec<Curve> {
        if let Shape::Insert(ins) = s {
            if depth > 8 {
                return vec![];
            }
            let Some(b) = self.block(&ins.name) else { return vec![] };
            let bx = compose(&x, &ins.xform(b.base));
            return b.entities.iter().flat_map(|e| self.curves_rec(&e.shape, bx, depth + 1)).collect();
        }
        s.curves().iter().flat_map(|c| c.transformed(&x)).collect()
    }

    /// Snap points of an entity (block references are expanded).
    pub fn snap_points_of(&self, e: &Entity) -> Vec<(SnapKind, V2)> {
        let mut v = e.shape.snap_points();
        if let Shape::Insert(ins) = &e.shape {
            if let Some(b) = self.block(&ins.name) {
                let x = ins.xform(b.base);
                for sub in &b.entities {
                    v.extend(sub.shape.snap_points().into_iter().map(|(k, p)| (k, x.apply(p))));
                }
            }
        }
        v
    }

    /// All analytic curves except those of `skip`.
    pub fn curves_except(&self, skip: u64) -> Vec<Curve> {
        self.entities.iter().filter(|e| e.id != skip && self.is_visible(e)).flat_map(|e| self.curves_of(e)).collect()
    }

    /// Explode an entity into new entities (not yet added). Block references
    /// expand into their contents; layer `0` contents take the insert's layer.
    pub fn explode_entity(&self, e: &Entity) -> Option<Vec<Entity>> {
        if let Shape::Insert(ins) = &e.shape {
            let b = self.block(&ins.name)?;
            let x = ins.xform(b.base);
            // Attribute definitions become plain text holding this reference's values.
            return Some(
                b.entities
                    .iter()
                    .filter_map(|sub| {
                        let mut n = sub.clone();
                        if let Shape::AttDef(a) = &sub.shape {
                            if a.invisible {
                                return None;
                            }
                            let mut t = a.text.clone();
                            t.content = ins.value_of(&a.tag).unwrap_or(&a.default).to_string();
                            n.shape = Shape::Text(t);
                        }
                        n.shape.transform(&x);
                        if n.layer == "0" {
                            n.layer = e.layer.clone();
                        }
                        Some(n)
                    })
                    .collect(),
            );
        }
        let parts = e.shape.explode(&self.ctx())?;
        Some(parts.into_iter().map(|s| Entity { shape: s, ..e.clone() }).collect())
    }

    /// Segments of all visible geometry (curves tessellated) for hatch boundary detection.
    pub fn boundary_segments(&self) -> Vec<(V2, V2)> {
        let mut out = vec![];
        let ctx = self.ctx();
        for e in &self.entities {
            if !self.is_visible(e) || matches!(e.shape, Shape::Hatch(_) | Shape::Dim(_) | Shape::Text(_) | Shape::MText(_)) {
                continue;
            }
            for item in self.render_items(e) {
                match item.prim {
                    Prim::Line(a, b) => out.push((a, b)),
                    Prim::Poly(p, closed) => out.extend(crate::prim::poly_segments(&p, closed)),
                    Prim::Circle(c, r) => {
                        let pts = arc_points(c, r, 0.0, std::f64::consts::TAU, r * 0.002);
                        out.extend(crate::prim::poly_segments(&pts, false));
                    }
                    Prim::Arc { c, r, start, end } => {
                        let pts = arc_points(c, r, start, end, r * 0.002);
                        out.extend(crate::prim::poly_segments(&pts, false));
                    }
                    _ => {}
                }
            }
            let _ = &ctx;
        }
        out
    }

    /// Remove unused layers (except 0 and the current layer) and unused blocks.
    /// Returns (layers removed, blocks removed).
    pub fn purge(&mut self) -> (usize, usize) {
        let mut used_blocks: std::collections::HashSet<String> = Default::default();
        let mut stack: Vec<String> = self
            .entities
            .iter()
            .filter_map(|e| if let Shape::Insert(i) = &e.shape { Some(i.name.to_ascii_uppercase()) } else { None })
            .collect();
        while let Some(n) = stack.pop() {
            if used_blocks.insert(n.clone()) {
                if let Some(b) = self.block(&n) {
                    for e in &b.entities {
                        if let Shape::Insert(i) = &e.shape {
                            stack.push(i.name.to_ascii_uppercase());
                        }
                    }
                }
            }
        }
        let nb = self.blocks.len();
        self.blocks.retain(|b| used_blocks.contains(&b.name.to_ascii_uppercase()));
        let used_layers: std::collections::HashSet<String> =
            self.entities.iter().chain(self.blocks.iter().flat_map(|b| b.entities.iter())).map(|e| e.layer.to_ascii_uppercase()).collect();
        let cur = self.current_layer.to_ascii_uppercase();
        let nl = self.layers.len();
        self.layers.retain(|l| {
            let n = l.name.to_ascii_uppercase();
            n == "0" || n == cur || used_layers.contains(&n)
        });
        (nl - self.layers.len(), nb - self.blocks.len())
    }
}

/// `a ∘ b` (apply `b` first).
pub fn compose(a: &Xform, b: &Xform) -> Xform {
    let m = [
        a.m[0] * b.m[0] + a.m[1] * b.m[2],
        a.m[0] * b.m[1] + a.m[1] * b.m[3],
        a.m[2] * b.m[0] + a.m[3] * b.m[2],
        a.m[2] * b.m[1] + a.m[3] * b.m[3],
    ];
    Xform { m, t: a.apply(b.t) }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn custom_title_block_is_filled_from_the_title_fields() {
        let mut d = Document::new();
        d.sheet.title.company = "ACME".into();
        d.sheet.scale = 5.0;
        let builtin = d.sheet_prims();
        let entities = d
            .sheet
            .title_block_template()
            .into_iter()
            .map(|(shape, lw)| Entity {
                id: 0,
                layer: "0".into(),
                color: Color::ByLayer,
                linetype: LtRef::ByLayer,
                lineweight: Some(lw),
                shape,
            })
            .collect::<Vec<_>>();
        assert!(entities.iter().filter(|e| matches!(e.shape, Shape::AttDef(_))).count() >= 14);
        d.define_block(BlockDef { name: "TB".into(), base: V2::ZERO, entities, description: String::new(), xref: None });
        d.sheet.title_block = Some("TB".into());
        let custom = d.sheet_prims();
        let texts = |v: &[(Prim, f64)]| {
            let mut t: Vec<(String, i64, i64)> = v
                .iter()
                .filter_map(|(p, _)| {
                    if let Prim::Text(t) = p {
                        Some((t.content.clone(), t.pos.x.round() as i64, t.pos.y.round() as i64))
                    } else {
                        None
                    }
                })
                .collect();
            t.sort();
            t
        };
        // Same texts in the same places as the built-in title block.
        assert_eq!(texts(&custom), texts(&builtin));
        assert!(texts(&custom).iter().any(|t| t.0 == "ACME") && texts(&custom).iter().any(|t| t.0 == "1:5"));
    }

    #[test]
    fn block_insert_renders_transformed_and_inherits() {
        let mut d = Document::new();
        let def = BlockDef {
            name: "BOLT".into(),
            base: V2::ZERO,
            entities: vec![Entity {
                id: 100,
                layer: "0".into(),
                color: Color::ByLayer,
                linetype: LtRef::ByLayer,
                lineweight: None,
                shape: Shape::Line { a: V2::new(0.0, 0.0), b: V2::new(10.0, 0.0) },
            }],
            description: String::new(),
            xref: None,
        };
        d.define_block(def);
        let mut ins = Insert::new("BOLT", V2::new(5.0, 5.0));
        ins.rotation = std::f64::consts::FRAC_PI_2;
        ins.sx = 2.0;
        ins.sy = 2.0;
        let id = d.add_on(layers::CENTER, Shape::Insert(ins));
        let items = d.render_items(d.get(id).unwrap());
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].color, 1, "layer-0 content takes the insert's layer colour");
        let Prim::Line(a, b) = items[0].prim else { panic!() };
        assert!((a - V2::new(5.0, 5.0)).len() < 1e-9 && (b - V2::new(5.0, 25.0)).len() < 1e-9);
        // Snaps reach into the block.
        assert!(d.snap_points_of(d.get(id).unwrap()).iter().any(|(_, p)| (*p - V2::new(5.0, 25.0)).len() < 1e-9));
        // Explode produces model-space geometry on the insert's layer.
        let parts = d.explode_entity(d.get(id).unwrap()).unwrap();
        assert_eq!(parts[0].layer, layers::CENTER);
        assert_eq!(d.purge().1, 0);
    }
}
