// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Proteen Das

//! A DWG reader written in Rust (R2000, R2004, R2010, R2013 and R2018 files),
//! so drawings open without an external converter. It reads layers,
//! linetype names, blocks and the 2D drafting entities: lines, circles,
//! arcs, ellipses, points, polylines (light and classic, with bulges),
//! solids, splines, text, multiline text, block references with attributes,
//! attribute definitions and all dimension types (as native dimensions).
//! Paper-space content is left out. Anything else is counted, so the caller
//! can fall back to the converter when it matters.

pub mod bits;
pub mod classes;
pub mod container;
pub mod header;
pub mod objects;
pub mod r2007;

use bits::Ver;
use icad_core::curves::eval_bspline;
use icad_core::document::Document;
use icad_core::entity::*;
use icad_core::geom::*;
use icad_core::prim::Text;
use icad_core::style::*;
use icad_core::{HAlign, VAlign};
use objects::{Common, Ent, P2};
use std::collections::{BTreeMap, HashMap};

/// What the reader left out.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Report {
    pub version: String,
    /// Entities of types not read, by type name, with counts.
    pub skipped: BTreeMap<String, usize>,
    /// Objects that could not be decoded.
    pub damaged: usize,
    pub entities: usize,
    /// Written by ImagineCAD (its extended data was read).
    pub from_imaginecad: bool,
}

impl Report {
    pub fn complete(&self) -> bool {
        self.skipped.is_empty() && self.damaged == 0
    }
}

/// Names of entity types the reader does not convert (to report them).
fn type_name(ty: u16) -> Option<&'static str> {
    Some(match ty {
        4..=6 => return None, // BLOCK, ENDBLK, SEQEND: structure only
        12..=14 => "mesh vertex",
        28 => "3D face",
        29 | 30 => "polyface/mesh",
        32 => "trace",
        33 => "shape",
        34 => return None, // viewport
        37..=39 => "3D solid",
        47 => "multiline",
        _ => return None,
    })
}

fn v(p: P2) -> V2 {
    V2::new(p.0, p.1)
}

pub fn version_name(ver: Ver) -> &'static str {
    match ver {
        Ver::R2000 => "DWG 2000",
        Ver::R2004 => "DWG 2004",
        Ver::R2007 => "DWG 2007",
        Ver::R2010 => "DWG 2010",
        Ver::R2013 => "DWG 2013",
        Ver::R2018 => "DWG 2018",
    }
}

/// Whether the file is a DWG version this reader handles.
pub fn supported(file: &[u8]) -> bool {
    Ver::from_magic(file).is_some()
}

struct Item {
    handle: u64,
    common: Common,
    ent: Ent,
    eed: Vec<(u64, Vec<String>)>,
}

/// ImagineCAD's own data in the drawing.
#[derive(Default)]
struct Extras {
    /// Handle of the IMAGINECAD application id (its extended data).
    app: Option<u64>,
    constraints: String,
    dynblocks: String,
    /// Image definition handle → file.
    images: HashMap<u64, String>,
}

/// Read a DWG file.
pub fn read(file: &[u8]) -> Result<(Document, Report), String> {
    let c = container::read(file)?;
    let mut report = Report { version: version_name(c.ver).into(), ..Default::default() };
    let mut layers: HashMap<u64, objects::LayerRec> = HashMap::new();
    let mut ltypes: HashMap<u64, String> = HashMap::new();
    let mut blocks: HashMap<u64, objects::BlockRec> = HashMap::new();
    let mut items: Vec<Item> = vec![];
    let mut extras = Extras::default();
    let classes = classes::read(&c);
    for &(handle, offset) in &c.map {
        let Some(mut o) = objects::open(&c, offset) else {
            report.damaged += 1;
            continue;
        };
        match o.ty {
            51 => {
                let l = objects::layer(&mut o);
                if !o.d.overrun {
                    layers.insert(handle, l);
                }
            }
            57 => {
                let name = objects::table_common(&mut o);
                ltypes.insert(handle, name);
            }
            67 => {
                let name = objects::table_common(&mut o);
                if name.eq_ignore_ascii_case(crate::dxf::XDATA_APP) {
                    report.from_imaginecad = true;
                    extras.app = Some(handle);
                }
            }
            49 => {
                let b = objects::block_header(&mut o);
                if !o.d.overrun {
                    blocks.insert(handle, b);
                }
            }
            ty if ty >= 500 && classes.get(&ty).is_some_and(|c| c.0 == "IMAGE") => match objects::image(&mut o) {
                Some((common, ent)) => items.push(Item { handle, common, ent, eed: std::mem::take(&mut o.eed) }),
                None => report.damaged += 1,
            },
            ty if ty >= 500 && classes.get(&ty).is_some_and(|c| c.0 == "ARC_DIMENSION") => match objects::arc_dimension(&mut o) {
                Some((common, ent)) => items.push(Item { handle, common, ent, eed: std::mem::take(&mut o.eed) }),
                None => report.damaged += 1,
            },
            ty if ty >= 500 && classes.get(&ty).is_some_and(|c| c.0 == "IMAGEDEF") => {
                if let Some(p) = objects::imagedef(&mut o) {
                    extras.images.insert(handle, p);
                }
            }
            ty if ty >= 500 && classes.get(&ty).is_some_and(|c| c.1) => {
                // Other class-defined entities (multileaders, wipeouts…).
                let name = classes.get(&ty).map(|c| c.0.to_ascii_lowercase()).unwrap_or_default();
                *report.skipped.entry(name).or_default() += 1;
            }
            ty if ty == 79 || (ty >= 500 && classes.get(&ty).is_none_or(|c| c.0 == "XRECORD")) => {
                // Records of ImagineCAD's constraints and dynamic blocks.
                if let Some(strs) = objects::xrecord_strings(&mut o) {
                    let mut ones = strs.iter().filter(|(c, _)| *c == 1).map(|(_, s)| s.as_str());
                    match ones.next().map(str::trim) {
                        Some(k) if k == crate::dxf::CONSTRAINTS_KEY => extras.constraints = ones.collect(),
                        Some(k) if k == crate::dxf::DYNBLOCKS_KEY => extras.dynblocks = ones.collect(),
                        _ => {}
                    }
                }
            }
            ty => {
                if let Some(name) = type_name(ty) {
                    *report.skipped.entry(name.to_string()).or_default() += 1;
                    continue;
                }
                if let Some((common, ent)) = objects::entity(&mut o) {
                    items.push(Item { handle, common, ent, eed: std::mem::take(&mut o.eed) });
                } else if objects::is_entity_read(ty) {
                    report.damaged += 1;
                }
            }
        }
    }
    let vars = header::read(&c);
    let (mut doc, report) = build(c.ver, layers, ltypes, blocks, items, extras, report);
    if let Some(h) = vars {
        // The drawing's own annotation scale and dimension style.
        let ds = &mut doc.dimstyle;
        let s = if h.dimscale > 0.0 { h.dimscale } else { 1.0 };
        ds.text_height = h.dimtxt;
        if h.dimasz > 0.0 {
            ds.arrow_size = h.dimasz;
        }
        ds.ext_offset = h.dimexo;
        ds.ext_beyond = h.dimexe;
        if h.dimgap > 0.0 {
            ds.text_gap = h.dimgap;
        }
        ds.decimals = h.dimdec as usize;
        if !doc.entities.is_empty() {
            doc.sheet.scale = s;
        }
        doc.ltscale = h.ltscale / s;
    }
    Ok((doc, report))
}

fn block_name(name: &str) -> String {
    // Anonymous blocks keep their name without the leading '*'.
    match name.strip_prefix('*') {
        Some(rest) => format!("_{rest}"),
        None => name.to_string(),
    }
}

fn build(
    _ver: Ver,
    layers: HashMap<u64, objects::LayerRec>,
    ltypes: HashMap<u64, String>,
    blocks: HashMap<u64, objects::BlockRec>,
    items: Vec<Item>,
    extras: Extras,
    mut report: Report,
) -> (Document, Report) {
    let mut doc = Document::new();
    let lt_of = |h: u64| ltypes.get(&h).and_then(|n| LineType::from_name(n));
    for l in layers.values() {
        if l.name.is_empty() {
            continue;
        }
        let color = l.color.unsigned_abs().clamp(1, 255) as u8;
        let mut layer =
            Layer::new(&l.name, color, lt_of(l.ltype).unwrap_or(LineType::Continuous), objects::lineweight(l.lineweight).unwrap_or(0.25));
        layer.visible = !(l.off || l.frozen);
        layer.locked = l.locked;
        layer.plot = l.plot || l.name.eq_ignore_ascii_case("0");
        match doc.layer_mut(&l.name) {
            Some(ex) => *ex = layer,
            None => doc.layers.push(layer),
        }
    }
    let layer_name = |h: u64| layers.get(&h).map(|l| l.name.clone()).filter(|n| !n.is_empty()).unwrap_or_else(|| "0".into());

    // Where each entity belongs.
    enum Place {
        Model,
        Paper,
        Block(u64),
        Owned(u64),
    }
    let place = |c: &Common| -> Place {
        match c.entmode {
            2 => Place::Model,
            1 => Place::Paper,
            _ => match blocks.get(&c.owner) {
                Some(b) if b.name.eq_ignore_ascii_case("*Model_Space") => Place::Model,
                Some(b) if b.name.to_ascii_lowercase().starts_with("*paper_space") => Place::Paper,
                Some(_) => Place::Block(c.owner),
                None => Place::Owned(c.owner),
            },
        }
    };

    // Vertices and attributes owned by polylines and references.
    let mut vertices: HashMap<u64, Vec<(u64, P2, f64)>> = HashMap::new();
    let mut attribs: HashMap<u64, Vec<Attrib>> = HashMap::new();
    for it in &items {
        if let Place::Owned(owner) = place(&it.common) {
            match &it.ent {
                Ent::Vertex(p, b) => vertices.entry(owner).or_default().push((it.handle, *p, *b)),
                Ent::Attrib { tag, value, def: false, .. } => {
                    attribs.entry(owner).or_default().push(Attrib { tag: tag.clone(), value: value.clone() })
                }
                _ => {}
            }
        }
    }

    let mut model: Vec<Entity> = vec![];
    let mut in_block: HashMap<u64, Vec<Entity>> = HashMap::new();
    let mut next_id = 1u64;
    // DWG handle → entity id, for the records that refer to entities.
    let mut handle_ids: HashMap<u64, u64> = HashMap::new();
    for it in &items {
        let target = match place(&it.common) {
            Place::Model => None,
            Place::Block(b) => Some(b),
            Place::Paper | Place::Owned(_) => continue,
        };
        // ImagineCAD's exact shape, when it stored one.
        let exact = extras.app.and_then(|app| exact_shape(&it.eed, app));
        let shapes = match exact {
            Some(s) => vec![s],
            None => match &it.ent {
                Ent::Image { pos, u, v: vv, size, def, fade } => match extras.images.get(def) {
                    Some(path) => {
                        let (u, vv) = (v(*u), v(*vv));
                        vec![Shape::Image(icad_core::ImageRef {
                            path: path.clone(),
                            pos: v(*pos),
                            w: u.len() * size.0,
                            h: vv.len() * size.1,
                            fade: *fade,
                        })]
                    }
                    None => vec![],
                },
                _ => shapes_of(it, &blocks, &vertices, &attribs),
            },
        };
        if shapes.is_empty() {
            continue;
        }
        let c = &it.common;
        let color = match c.color.index {
            1..=255 => Color::Aci(c.color.index as u8),
            _ => Color::ByLayer,
        };
        let linetype = match c.ltype_flags {
            2 => LtRef::Lt(LineType::Continuous),
            3 => lt_of(c.ltype).map(LtRef::Lt).unwrap_or_default(),
            _ => LtRef::ByLayer,
        };
        handle_ids.insert(it.handle, next_id);
        for shape in shapes {
            let e =
                Entity { id: next_id, layer: layer_name(c.layer), color, linetype, lineweight: objects::lineweight(c.lineweight), shape };
            next_id += 1;
            match target {
                None => model.push(e),
                Some(b) => in_block.entry(b).or_default().push(e),
            }
        }
    }
    report.entities = model.len();

    // The annotation scale: dimension text is drawn at the style's text
    // height times the scale, so the text in the dimension blocks tells it.
    let mut heights: Vec<f64> = blocks
        .iter()
        .filter(|(_, b)| b.name.to_ascii_lowercase().starts_with("*d"))
        .flat_map(|(h, _)| in_block.get(h).into_iter().flatten())
        .filter_map(|e| match &e.shape {
            Shape::Text(t) => Some(t.height),
            Shape::MText(t) => Some(t.height),
            _ => None,
        })
        .filter(|h| *h > 0.0 && h.is_finite())
        .collect();
    heights.sort_by(f64::total_cmp);
    let dim_scale = heights.get(heights.len() / 2).map(|h| h / doc.dimstyle.text_height);

    // Anonymous blocks nothing refers to (tables, groups…) are left out.
    let used: std::collections::HashSet<String> = model
        .iter()
        .chain(in_block.values().flatten())
        .filter_map(|e| if let Shape::Insert(i) = &e.shape { Some(i.name.clone()) } else { None })
        .collect();

    // Blocks (the model and paper spaces are not blocks; dimension blocks
    // are not needed, the dimensions are native).
    let mut defs: Vec<BlockDef> = blocks
        .iter()
        .filter(|(_, b)| {
            let n = b.name.to_ascii_lowercase();
            !n.is_empty()
                && n != "*model_space"
                && !n.starts_with("*paper_space")
                && !n.starts_with("*d")
                && (!n.starts_with('*') || used.contains(&block_name(&b.name)))
        })
        .map(|(h, b)| BlockDef {
            name: block_name(&b.name),
            base: v(b.base),
            entities: in_block.remove(h).unwrap_or_default(),
            description: String::new(),
            xref: (b.xref && !b.path.is_empty()).then(|| b.path.clone()),
            params: vec![],
        })
        .collect();
    defs.sort_by(|a, b| a.name.cmp(&b.name));
    doc.blocks = defs;
    doc.entities = model;
    doc.next_id = next_id;
    let back = |h: &str| u64::from_str_radix(h.trim(), 16).ok().and_then(|h| handle_ids.get(&h).copied());
    if let Ok(serde_json::Value::Array(list)) = serde_json::from_str::<serde_json::Value>(&extras.dynblocks) {
        for item in list {
            let Some(name) = item.get("block").and_then(|n| n.as_str()).map(String::from) else { continue };
            let mut params = item.get("params").cloned().unwrap_or_default();
            let ok = crate::dxf::map_param_ids(&mut params, &|_| None, &back);
            if let (true, Ok(ps), Some(b)) =
                (ok, serde_json::from_value(params), doc.blocks.iter_mut().find(|b| b.name.eq_ignore_ascii_case(&name)))
            {
                b.params = ps;
            }
        }
    }
    if let Ok(serde_json::Value::Array(list)) = serde_json::from_str::<serde_json::Value>(&extras.constraints) {
        for mut v in list {
            let ok = crate::dxf::map_ids(&mut v, &|h| back(h.as_str()?).map(serde_json::Value::from));
            if let (true, Ok(c)) = (ok, serde_json::from_value::<icad_core::constraints::Constraint>(v)) {
                doc.constraints.push(c);
            }
        }
    }
    // External references named by their references' extended data
    // (converters may drop the block's own flag and path).
    if let Some(app) = extras.app {
        for it in &items {
            let Ent::Insert { block, .. } = &it.ent else { continue };
            let Some(path) = eed_json(&it.eed, app).and_then(|v| v.get("xref")?.as_str().map(String::from)) else { continue };
            let Some(name) = blocks.get(block).map(|b| block_name(&b.name)) else { continue };
            if let Some(b) = doc.blocks.iter_mut().find(|b| b.name == name) {
                b.xref = Some(path);
            }
        }
    }
    // Dimensions follow the geometry they measure.
    doc.associate_all_dims();
    if !doc.entities.is_empty() {
        doc.current_layer = "0".into();
        let ext = doc.extents();
        doc.sheet.fit_to(&ext);
        doc.sheet.visible = false;
        if let Some(s) = dim_scale.filter(|s| *s > 1e-6 && *s < 1e6) {
            doc.sheet.scale = s;
        }
    }
    (doc, report)
}

/// ImagineCAD's extended data of an entity (JSON).
fn eed_json(eed: &[(u64, Vec<String>)], app: u64) -> Option<serde_json::Value> {
    use base64::Engine as _;
    let (_, strings) = eed.iter().find(|(a, _)| *a == app)?;
    let b64: String = strings.iter().map(|s| s.trim()).collect();
    let bytes = base64::engine::general_purpose::STANDARD.decode(b64).ok()?;
    serde_json::from_slice(&bytes).ok()
}

/// The exact shape ImagineCAD keeps in an entity's extended data.
fn exact_shape(eed: &[(u64, Vec<String>)], app: u64) -> Option<Shape> {
    serde_json::from_value(eed_json(eed, app)?).ok()
}

fn text_shape(e: &Ent) -> Option<Text> {
    let Ent::Text { pos, align, height, rotation, h, v: va, text, .. } = e else { return None };
    let ha = match h {
        1 | 4 => HAlign::Center,
        2 => HAlign::Right,
        _ => HAlign::Left,
    };
    let vaa = match va {
        1 => VAlign::Bottom,
        2 => VAlign::Middle,
        3 => VAlign::Top,
        _ => VAlign::Baseline,
    };
    let at = match align {
        Some(a) if ha != HAlign::Left || vaa != VAlign::Baseline => *a,
        _ => *pos,
    };
    Some(Text::new(v(at), height.max(1e-6), crate::dxf::decode_text(text)).aligned(ha, vaa).rotated(norm_angle(*rotation)))
}

fn shapes_of(
    it: &Item,
    blocks: &HashMap<u64, objects::BlockRec>,
    vertices: &HashMap<u64, Vec<(u64, P2, f64)>>,
    attribs: &HashMap<u64, Vec<Attrib>>,
) -> Vec<Shape> {
    let one = |s: Shape| vec![s];
    match &it.ent {
        Ent::Line(a, b) => one(Shape::Line { a: v(*a), b: v(*b) }),
        Ent::Circle(c, r) if *r > 0.0 => one(Shape::Circle { c: v(*c), r: *r }),
        Ent::Arc(c, r, s, e) if *r > 0.0 => one(Shape::Arc { c: v(*c), r: *r, start: norm_angle(*s), end: norm_angle(*e) }),
        Ent::Point(p) => one(Shape::Point { p: v(*p) }),
        Ent::Ellipse(c, m, ratio, s, e) => {
            one(Shape::Ellipse { c: v(*c), major: v(*m), ratio: ratio.clamp(1e-6, 1.0), start: *s, end: *e })
        }
        Ent::Poly(pts, closed) => {
            let verts: Vec<(V2, f64)> = pts.iter().map(|(p, b)| (v(*p), *b)).collect();
            poly(&verts, *closed)
        }
        Ent::PolyHeader { closed } => {
            let mut vs = vertices.get(&it.handle).cloned().unwrap_or_default();
            vs.sort_by_key(|x| x.0);
            let verts: Vec<(V2, f64)> = vs.iter().map(|(_, p, b)| (v(*p), *b)).collect();
            poly(&verts, *closed)
        }
        Ent::Solid(c) => {
            // Corners are stored 1-2-3-4 and drawn 1-2-4-3.
            let mut pts = vec![v(c[0]), v(c[1]), v(c[3]), v(c[2])];
            pts.dedup_by(|a, b| a.dist(*b) < 1e-9);
            if pts.len() < 3 {
                return vec![];
            }
            one(Shape::Hatch(Hatch { loops: vec![pts], pattern: "SOLID".into(), scale: 1.0, angle: 0.0 }))
        }
        Ent::Text { .. } => text_shape(&it.ent).map(Shape::Text).into_iter().collect(),
        Ent::MText { pos, xdir, width, height, attach, spacing, text } => one(Shape::MText(MText {
            pos: v(*pos),
            height: height.max(1e-6),
            width: width.max(0.0),
            rotation: norm_angle(v(*xdir).angle()),
            content: crate::dxf::strip_mtext(&crate::dxf::decode_text(text)),
            attach: (*attach).clamp(1, 9) as u8,
            spacing: *spacing,
            style: None,
        })),
        Ent::Insert { block, pos, scale, rotation, grid } => {
            let Some(b) = blocks.get(block) else { return vec![] };
            let name = block_name(&b.name);
            let mut ins = Insert::new(name, v(*pos));
            ins.sx = scale.0;
            ins.sy = scale.1;
            ins.rotation = norm_angle(*rotation);
            ins.attribs = attribs.get(&it.handle).cloned().unwrap_or_default();
            match grid {
                Some((cols, rows, dx, dy)) if (*cols).max(1) as usize * (*rows).max(1) as usize <= 10_000 => {
                    let mut out = vec![];
                    for r in 0..(*rows).max(1) {
                        for c in 0..(*cols).max(1) {
                            let mut i = ins.clone();
                            i.pos = ins.pos + V2::new(c as f64 * dx, r as f64 * dy).rotate(ins.rotation);
                            out.push(Shape::Insert(i));
                        }
                    }
                    out
                }
                _ => one(Shape::Insert(ins)),
            }
        }
        Ent::Attrib { tag, value, def: true, prompt, text } => {
            let Some(t) = text_shape(text) else { return vec![] };
            one(Shape::AttDef(AttDef { tag: tag.clone(), prompt: prompt.clone(), default: value.clone(), text: t, invisible: false }))
        }
        Ent::Spline { degree, knots, ctrl, fit, closed } => {
            let pts: Vec<V2> = if fit.len() >= 2 {
                fit.iter().map(|p| v(*p)).collect()
            } else {
                let ctrl: Vec<V2> = ctrl.iter().map(|p| v(*p)).collect();
                let degree = (*degree).max(1) as usize;
                if ctrl.len() < 2 || knots.len() < ctrl.len() + degree + 1 {
                    return vec![];
                }
                let (t0, t1) = (knots[degree], knots[knots.len() - 1 - degree]);
                let n = (ctrl.len() * 3).clamp(4, 400);
                (0..=n).map(|i| eval_bspline(&ctrl, knots, degree, t0 + (t1 - t0) * i as f64 / n as f64)).collect()
            };
            one(Shape::Spline { pts, closed: *closed })
        }
        Ent::XLine(p, d, ray) => {
            let dir = v(*d).norm();
            if dir.len() < 0.5 {
                return vec![];
            }
            one(if *ray { Shape::Ray { p: v(*p), dir } } else { Shape::XLine { p: v(*p), dir } })
        }
        // Rows are separated by a line break in DWG, by ^J in DXF.
        Ent::Leader(pts) if pts.len() >= 2 => {
            one(Shape::Leader(Leader { pts: pts.iter().map(|p| v(*p)).collect(), text: String::new(), style: None }))
        }
        Ent::Tolerance(p, text) => {
            one(Shape::Tolerance(icad_core::symbols::Fcf::from_dxf(v(*p), &text.replace("\r\n", "^J").replace('\n', "^J"))))
        }
        Ent::Hatch { loops, pattern, solid, scale, angle } => one(Shape::Hatch(Hatch {
            loops: loops.iter().map(|l| l.iter().map(|p| v(*p)).collect()).collect(),
            pattern: if *solid { "SOLID".into() } else { pattern.clone() },
            scale: *scale,
            angle: *angle,
        })),
        Ent::Dim { kind, pts, rotation, text, xordinate, .. } => {
            dim(*kind, pts, *rotation, text, *xordinate).map(Shape::Dim).into_iter().collect()
        }
        _ => vec![],
    }
}

fn poly(verts: &[(V2, f64)], closed: bool) -> Vec<Shape> {
    if verts.len() < 2 {
        return vec![];
    }
    let pts =
        if verts.iter().any(|x| x.1.abs() > 1e-9) { crate::dxf::bulge_points(verts, closed) } else { verts.iter().map(|x| x.0).collect() };
    vec![Shape::Polyline { pts, closed }]
}

fn dim(kind: u8, pts: &[(u8, P2)], rotation: f64, text: &str, xordinate: bool) -> Option<Dimension> {
    let p = |code: u8| pts.iter().find(|x| x.0 == code).map(|x| v(x.1)).unwrap_or(V2::ZERO);
    let tmid = p(11);
    let mut d = match kind {
        0 | 1 => {
            let (p1, p2, q) = (p(13), p(14), p(10));
            let k = if kind == 1 {
                DimKind::Aligned
            } else {
                let a = norm_angle(rotation);
                if a.abs() < 1e-3 || (a - std::f64::consts::PI).abs() < 1e-3 {
                    DimKind::Horizontal
                } else if (a - std::f64::consts::FRAC_PI_2).abs() < 1e-3 || (a - 3.0 * std::f64::consts::FRAC_PI_2).abs() < 1e-3 {
                    DimKind::Vertical
                } else {
                    DimKind::Aligned
                }
            };
            let mut d = Dimension::new(k, p1, p2, q);
            let dir = d.direction();
            d.loc = q + dir * (tmid - q).dot(dir);
            d
        }
        3 => {
            let (a, b) = (p(10), p(15));
            Dimension::new(DimKind::Diameter, a.mid(b), b, tmid)
        }
        4 => Dimension::new(DimKind::Radius, p(10), p(15), tmid),
        5 => Dimension::angular(p(15), p(13), p(14), p(10)),
        7 => Dimension::arc_length(p(15), p(13), p(14), p(10)),
        6 => {
            let mut d = Dimension::ordinate(p(10), p(13), p(14));
            if xordinate {
                d.kind = DimKind::OrdinateX;
            }
            d
        }
        2 => {
            let (a1, a2, b1, b2) = (p(13), p(14), p(15), p(10));
            let (c, _, _) = line_line(a1, a2, b1, b2)?;
            Dimension::angular(c, if a2.dist(c) > a1.dist(c) { a2 } else { a1 }, if b2.dist(c) > b1.dist(c) { b2 } else { b1 }, p(16))
        }
        _ => return None,
    };
    let t = crate::dxf::decode_text(text);
    d.text = Some(t).filter(|t| !t.is_empty() && t != "<>");
    Some(d)
}
