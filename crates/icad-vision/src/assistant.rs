// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Proteen Das

//! Conversational drawing assistant. The open drawing is described to the
//! configured AI model as text; the model answers with a short reply and a
//! list of edit operations, which are applied to the document here. The
//! application wraps each answer in a single undo step.

use crate::llm::{self, AiSettings, Turn};
use crate::parts::PartSpec;
use icad_core::document::{layers, Document, PartRecord};
use icad_core::entity::*;
use icad_core::geom::*;
use icad_core::prim::Text;
use icad_core::sheet::{fit_margin, parse_scale, scale_label, PaperSize};
use icad_core::style::*;
use icad_core::symbols::*;
use serde_json::{json, Value};

/// Entities listed to the model in full; larger drawings are summarised.
const MAX_LISTED: usize = 600;

fn n(x: f64) -> String {
    let s = format!("{x:.3}");
    let s = s.trim_end_matches('0').trim_end_matches('.');
    if s == "-0" {
        "0".into()
    } else {
        s.into()
    }
}

fn p(v: V2) -> String {
    format!("({},{})", n(v.x), n(v.y))
}

fn deg(r: f64) -> String {
    n(r.to_degrees())
}

/// One line describing an entity's geometry.
pub fn describe_shape(doc: &Document, s: &Shape) -> String {
    match s {
        Shape::Line { a, b } => format!("line {}-{} length {}", p(*a), p(*b), n(a.dist(*b))),
        Shape::Circle { c, r } => format!("circle center {} r {} (diameter {})", p(*c), n(*r), n(2.0 * r)),
        Shape::Arc { c, r, start, end } => {
            format!("arc center {} r {} from {}° to {}° (counter-clockwise)", p(*c), n(*r), deg(*start), deg(*end))
        }
        Shape::Polyline { pts, closed } => {
            let list: Vec<String> = pts.iter().take(60).map(|q| p(*q)).collect();
            format!(
                "polyline {} {} points: {}{}",
                if *closed { "closed" } else { "open" },
                pts.len(),
                list.join(" "),
                if pts.len() > 60 { " …" } else { "" }
            )
        }
        Shape::Text(t) => format!("text {:?} at {} height {} rotation {}°", t.content, p(t.pos), n(t.height), deg(t.rotation)),
        Shape::MText(t) => format!("multiline text {:?} at {} height {}", t.content, p(t.pos), n(t.height)),
        Shape::Dim(d) => {
            let kind = match d.kind {
                DimKind::Horizontal => "horizontal",
                DimKind::Vertical => "vertical",
                DimKind::Aligned => "aligned",
                DimKind::Radius => "radius",
                DimKind::Diameter => "diameter",
                DimKind::Angular => "angular",
                DimKind::OrdinateX => "ordinate-x",
                DimKind::OrdinateY => "ordinate-y",
                DimKind::ArcLength => "arc-length",
            };
            let shown = d.label(&doc.dimstyle);
            let geo = match d.kind {
                DimKind::Radius | DimKind::Diameter => format!("center {} through {}", p(d.p1), p(d.p2)),
                DimKind::Angular => format!("vertex {} rays to {} and {}", p(d.center.unwrap_or(V2::ZERO)), p(d.p1), p(d.p2)),
                DimKind::OrdinateX | DimKind::OrdinateY => format!("feature {} from datum {}", p(d.p1), p(d.center.unwrap_or(V2::ZERO))),
                DimKind::ArcLength => format!("arc around {} from {} to {}", p(d.center.unwrap_or(V2::ZERO)), p(d.p1), p(d.p2)),
                _ => format!("from {} to {}", p(d.p1), p(d.p2)),
            };
            format!(
                "dimension {kind} {geo} placed at {} measures {} shows {:?}{}",
                p(d.loc),
                n(d.measurement()),
                shown,
                if d.text.is_some() { " (text override)" } else { "" }
            )
        }
        Shape::Ellipse { c, major, ratio, .. } => format!("ellipse center {} major axis {} ratio {}", p(*c), p(*major), n(*ratio)),
        Shape::Spline { pts, closed } => format!("spline {} {} fit points", if *closed { "closed" } else { "open" }, pts.len()),
        Shape::Point { p: q } => format!("point {}", p(*q)),
        Shape::XLine { p: q, dir } => format!("construction line through {} direction {}", p(*q), p(*dir)),
        Shape::Ray { p: q, dir } => format!("ray from {} direction {}", p(*q), p(*dir)),
        Shape::Hatch(h) => format!("hatch {} with {} boundary loop(s)", h.pattern, h.loops.len()),
        Shape::Insert(i) => {
            let atts: Vec<String> = i.attribs.iter().map(|a| format!("{}={:?}", a.tag, a.value)).collect();
            format!(
                "block {:?} at {} scale {}x{} rotation {}°{}",
                i.name,
                p(i.pos),
                n(i.sx),
                n(i.sy),
                deg(i.rotation),
                if atts.is_empty() { String::new() } else { format!(" attributes {}", atts.join(" ")) }
            )
        }
        Shape::AttDef(a) => {
            format!("attribute definition tag {:?} prompt {:?} default {:?} at {}", a.tag, a.prompt, a.default, p(a.text.pos))
        }
        Shape::Leader(l) => format!(
            "leader arrow at {} text {:?}{}",
            p(l.pts.first().copied().unwrap_or(V2::ZERO)),
            l.text,
            l.style.as_ref().map(|s| format!(" style {s}")).unwrap_or_default()
        ),
        Shape::Image(i) => format!("image {:?} at {} size {}x{}", i.path, p(i.pos), n(i.w), n(i.h)),
        Shape::Tolerance(f) => {
            let rows: Vec<String> = f
                .rows
                .iter()
                .map(|r| {
                    let datums: Vec<String> = r.datums.iter().map(|d| format!("{}{}", d.letter, d.material.letter())).collect();
                    format!(
                        "{} {}{}{} datums [{}]",
                        r.symbol.name(),
                        if r.diameter { "Ø" } else { "" },
                        r.tolerance,
                        r.material.letter(),
                        datums.join(",")
                    )
                })
                .collect();
            format!("geometric tolerance at {}: {}", p(f.pos), rows.join(" / "))
        }
        Shape::Table(t) => {
            let rows: Vec<String> =
                t.cells.iter().take(40).map(|r| r.iter().map(|c| c.text.replace('\n', " ")).collect::<Vec<_>>().join(" | ")).collect();
            format!("table at {} ({}x{} cells, text {}): {}", p(t.pos), t.rows(), t.cols(), n(t.text_height), rows.join(" / "))
        }
        Shape::Region(r) => {
            format!("region area {} with {} loop(s), bounds {} to {}", n(r.area()), r.loops.len(), p(r.bbox().min), p(r.bbox().max))
        }
        Shape::Datum(d) => format!("datum feature symbol {:?} at {} on feature {}", d.letter, p(d.pos), p(d.tip)),
        Shape::RevCloud(c) => format!("revision cloud round {}", c.pts.iter().map(|q| p(*q)).collect::<Vec<_>>().join(" ")),
        Shape::Surface(s) => format!("surface texture symbol at {} {:?} {:?} method {:?}", p(s.pos), s.process, s.requirement, s.method),
        Shape::Weld(w) => format!(
            "weld symbol arrow at {} arrow side {} {}{} other side {} {}{} tail {:?}",
            p(w.tip),
            w.arrow_side.kind.name(),
            w.arrow_side.size,
            w.arrow_side.length,
            w.other_side.kind.name(),
            w.other_side.size,
            w.other_side.length,
            w.tail
        ),
    }
}

fn gdt_symbol(s: &str) -> Option<GdtSymbol> {
    let k = s.to_ascii_lowercase().replace([' ', '_', '-'], "");
    GdtSymbol::ALL.into_iter().find(|g| {
        let name = g.name().to_ascii_lowercase().replace([' ', '_', '-', '(', ')', '/'], "");
        name.starts_with(&k) || format!("{g:?}").to_ascii_lowercase() == k
    })
}

fn material(v: &Value) -> Material {
    v.as_str().map(Material::from_letter).unwrap_or_default()
}

/// "A", "B(M)", "BM" or {"letter": "B", "material": "M"}.
fn datum_ref(v: &Value) -> Option<DatumRef> {
    match v {
        Value::String(s) => {
            let s = s.trim().replace(['(', ')'], "");
            let (letter, m) =
                if s.len() > 1 && matches!(s.chars().last(), Some('M' | 'L' | 'S')) { s.split_at(s.len() - 1) } else { (s.as_str(), "") };
            Some(DatumRef { letter: letter.to_string(), material: Material::from_letter(m) })
        }
        Value::Object(_) => Some(DatumRef { letter: v["letter"].as_str()?.to_string(), material: material(&v["material"]) }),
        _ => None,
    }
}

fn weld_side(v: &Value) -> WeldSide {
    let kind = v["kind"].as_str().map(|k| k.to_ascii_lowercase().replace([' ', '_', '-'], "")).and_then(|k| {
        WeldType::ALL.into_iter().find(|t| {
            let n = t.name().to_ascii_lowercase().replace([' ', '_', '-', '/'], "");
            n.contains(&k) || format!("{t:?}").to_ascii_lowercase() == k
        })
    });
    WeldSide {
        kind: kind.unwrap_or_default(),
        size: v["size"].as_str().unwrap_or("").to_string(),
        length: v["length"].as_str().unwrap_or("").to_string(),
    }
}

/// A text description of the drawing for the model.
pub fn describe(doc: &Document, selection: &[u64]) -> String {
    let mut out = String::new();
    let sh = &doc.sheet;
    let tb = &sh.title;
    out += &format!(
        "Units: {} (all coordinates in drawing units, y up, angles in degrees counter-clockwise).\n",
        if tb.units.is_empty() { "mm" } else { &tb.units }
    );
    out += &format!(
        "Sheet: {} {}, scale {} (annotation text {} drawing units high for 3.5 mm on paper).\n",
        sh.paper.name(),
        if sh.landscape { "landscape" } else { "portrait" },
        scale_label(sh.scale),
        n(3.5 * sh.scale)
    );
    out += &format!(
        "Title block: company={:?} title={:?} drawing_no={:?} revision={:?} drawn_by={:?} checked_by={:?} approved_by={:?} date={:?} material={:?} finish={:?} sheet_no={:?} tolerance={:?}\n",
        tb.company, tb.title, tb.drawing_no, tb.revision, tb.drawn_by, tb.checked_by, tb.approved_by, tb.date, tb.material, tb.finish, tb.sheet_no, tb.tolerance
    );
    out += &format!(
        "Dimension style: text height {} mm, arrow {} mm, {} decimals.\n",
        n(doc.dimstyle.text_height),
        n(doc.dimstyle.arrow_size),
        doc.dimstyle.decimals
    );
    out += "Layers:";
    for l in &doc.layers {
        out += &format!(
            " {}(color {}, {}, {} mm{}{})",
            l.name,
            l.color,
            l.linetype.name(),
            n(l.lineweight),
            if l.visible { "" } else { ", off" },
            if l.locked { ", locked" } else { "" }
        );
    }
    out += &format!("\nCurrent layer: {}\n", doc.current_layer);
    let ext = doc.extents();
    if !ext.is_empty() {
        out += &format!("Drawing extents: {} to {}\n", p(ext.min), p(ext.max));
    }
    if let Some(part) = &doc.part {
        out += &format!("Parametric part (regenerate it with the \"part\" operation to change its dimensions): {}\n", part.spec);
    }
    if !doc.blocks.is_empty() {
        out += &format!("Blocks: {}\n", doc.blocks.iter().map(|b| b.name.as_str()).collect::<Vec<_>>().join(", "));
    }
    out += &format!(
        "Selected entity ids: {}\n",
        if selection.is_empty() { "none".to_string() } else { selection.iter().map(|i| i.to_string()).collect::<Vec<_>>().join(", ") }
    );
    let total = doc.entities.len();
    out += &format!("Entities ({total}): id | layer | geometry\n");
    let mut order: Vec<&Entity> = doc.entities.iter().filter(|e| selection.contains(&e.id)).collect();
    order.extend(doc.entities.iter().filter(|e| !selection.contains(&e.id)));
    for e in order.iter().take(MAX_LISTED) {
        let mut extra = String::new();
        if let Color::Aci(c) = e.color {
            extra += &format!(" color {c}");
        }
        if let LtRef::Lt(lt) = e.linetype {
            extra += &format!(" linetype {}", lt.name());
        }
        out += &format!("{} | {} | {}{}\n", e.id, e.layer, describe_shape(doc, &e.shape), extra);
    }
    if total > MAX_LISTED {
        out += &format!("… {} more entities not listed.\n", total - MAX_LISTED);
    }
    out
}

/// Instructions for the model.
pub fn system_prompt() -> String {
    r#"You are the drawing assistant inside ImagineCAD, a 2D CAD program for mechanical drawings. The user asks for changes to the open drawing in plain language; you make them by answering with edit operations. You can also answer questions about the drawing.

Answer with ONE JSON object and nothing else:
{"reply": "one or two sentences for the user saying what you changed or answering the question", "actions": [ ...operations... ]}

Use "actions": [] for questions, or when the request is unclear (then ask in "reply"). Never invent entity ids; use the ids listed in the drawing description. Coordinates are drawing units (usually mm), y points up, angles are degrees counter-clockwise. Points are [x, y].

Operations:
{"op": "add", "layer": "OUTLINE", "shape": SHAPE}                  add an entity (layer optional: current layer)
{"op": "update", "id": 12, "shape": SHAPE}                          replace an entity's geometry, keeping its layer and properties
{"op": "delete", "ids": [12, 13]}
{"op": "move", "ids": [...], "dx": 10, "dy": 0}
{"op": "copy", "ids": [...], "dx": 10, "dy": 0, "count": 1}        count copies, each offset again
{"op": "rotate", "ids": [...], "center": [x, y], "angle": 90}
{"op": "scale", "ids": [...], "center": [x, y], "factor": 2}
{"op": "mirror", "ids": [...], "a": [x, y], "b": [x, y], "keep": true}   mirror about the line a-b; keep the originals or not
{"op": "set", "ids": [...], "layer": "HIDDEN", "color": 1 | "bylayer", "linetype": "Center" | "bylayer", "text": "new text", "height": 5}
     (any subset; "text" replaces text content, leader text, or a dimension's displayed text where "<>" stands for the measured value)
{"op": "layer", "name": "WELDS", "color": 3, "linetype": "Continuous", "lineweight": 0.35, "visible": true, "locked": false, "current": true}
{"op": "title", "fields": {"company": "...", "title": "...", "drawing_no": "...", "revision": "...", "drawn_by": "...", "checked_by": "...", "approved_by": "...", "date": "...", "material": "...", "finish": "...", "sheet_no": "...", "tolerance": "..."}}
{"op": "sheet", "paper": "A3", "landscape": true, "scale": "1:10"}   (also "fit": true to choose paper and scale automatically)
{"op": "dimstyle", "text_height": 3.5, "arrow_size": 3, "decimals": 1}
{"op": "part", "part": PART}       regenerate the parametric part (all its geometry and dimensions) from new parameters
{"op": "select", "ids": [...]}      highlight entities for the user
{"op": "zoom"}                      zoom to the whole drawing

SHAPE is one of:
{"type": "line", "a": [x, y], "b": [x, y]}
{"type": "circle", "c": [x, y], "r": 10}
{"type": "arc", "c": [x, y], "r": 10, "start": 0, "end": 90}
{"type": "polyline", "pts": [[x, y], ...], "closed": true}
{"type": "rectangle", "a": [x, y], "b": [x, y]}
{"type": "polygon", "c": [x, y], "r": 20, "sides": 6, "rotation": 0}
{"type": "text", "at": [x, y], "text": "NOTE", "height": 3.5, "rotation": 0}
{"type": "dim", "kind": "horizontal" | "vertical" | "aligned", "p1": [x, y], "p2": [x, y], "at": [x, y], "text": "optional, <> = measured value"}
{"type": "dim", "kind": "radius" | "diameter", "c": [x, y], "r": 10, "at": [x, y]}
{"type": "dim", "kind": "angular", "center": [x, y], "p1": [x, y], "p2": [x, y], "at": [x, y]}
{"type": "dim", "kind": "ordinate", "datum": [0, 0], "feature": [x, y], "at": [leader end x, y], "axis": "x" | "y"}
{"type": "dim", "kind": "arc_length", "c": [x, y], "r": 10, "start": 0, "end": 90, "at": [x, y]}
{"type": "leader", "pts": [[tip x, tip y], [x, y]], "text": "2x M6"}
{"type": "ellipse", "c": [x, y], "major": [dx, dy], "ratio": 0.5}
{"type": "point", "p": [x, y]}
{"type": "hatch", "loops": [[[x, y], ...]], "pattern": "ANSI31", "scale": 1, "angle": 0}
{"type": "tolerance", "at": [x, y], "rows": [{"symbol": "position", "diameter": true, "tolerance": "0.05", "material": "M", "datums": ["A", "B(M)"]}], "leader": [[feature x, y]]}
     (symbols: position, straightness, flatness, circularity, cylindricity, profile of a line, profile of a surface, angularity, perpendicularity, parallelism, concentricity, symmetry, circular run-out, total run-out)
{"type": "table", "at": [top-left x, y], "rows": [["ITEM", "PART", "QTY"], ["1", "Shaft", "1"]], "text_height": 2.5}
{"type": "leader", "pts": [[tip], [x, y]], "text": "3", "style": "Balloon"}     (multileader styles: Standard, Balloon, or the drawing's own)
{"type": "datum", "letter": "A", "at": [x, y], "tip": [point on the feature]}
{"type": "surface", "at": [point on the surface], "rotation": 0, "process": "removal" | "any" | "no_removal", "requirement": "Ra 1.6", "method": "ground", "lay": "=", "all_around": false}
{"type": "weld", "tip": [joint x, y], "elbow": [x, y], "arrow_side": {"kind": "fillet", "size": "a5", "length": "100"}, "other_side": {"kind": "none"}, "all_around": false, "field": false, "tail": "111"}
     (weld kinds: fillet, square butt, v butt, bevel butt, u butt, j butt, plug, spot, seam)

PART (same form as the drawing description shows):
{"type": "shaft", "total_length": 2836, "segments": [{"length": 155, "diameter": 160}, ...], "keyways": [{"start": 25, "length": 105, "width": 0}], "chamfers": [{"at": 2, "size": 2.5}]}
   (segments left to right; keyway start from the left end, width 0 = nominal; chamfer "at" = boundary index, 0 = left end, k = between segment k-1 and k, n = right end)
{"type": "plate", "width": 200, "height": 100, "holes": [{"x": 20, "y": 20, "diameter": 10}]}

Rules:
- When the drawing has a parametric part and the user changes its sizes (lengths, diameters, keyways, chamfers, holes), use one "part" operation with the complete new part; it redraws geometry and dimensions consistently.
- Keep dimensions consistent: when you move or resize geometry by other means, update or add the matching dimensions.
- Dimension lines go on layer DIMENSIONS, visible outlines on OUTLINE, hidden edges on HIDDEN, centre lines on CENTER, notes on TEXT.
- New note text should be about the annotation height given in the drawing description.
- Do exactly what was asked; do not restyle or move other things."#
        .to_string()
}

/// What the model answered.
#[derive(Clone, Debug)]
pub struct Answer {
    pub reply: String,
    pub actions: Vec<Value>,
    pub switched_model: Option<String>,
}

/// Split a model answer into the reply and the operations.
pub fn parse_answer(text: &str) -> (String, Vec<Value>) {
    match llm::parse_reply(text) {
        Some((v, _)) => {
            let reply = v["reply"].as_str().or_else(|| v["message"].as_str()).unwrap_or("").trim().to_string();
            let actions = v["actions"].as_array().or_else(|| v["operations"].as_array()).cloned().unwrap_or_default();
            (reply, actions)
        }
        None => (text.trim().to_string(), vec![]),
    }
}

/// Ask the model. `history` holds the conversation so far, ending with the
/// user's new message; the current drawing is attached to that message.
pub fn ask(s: &AiSettings, history: &[Turn], doc: &Document, selection: &[u64]) -> Result<Answer, String> {
    let mut turns = history.to_vec();
    if let Some(last) = turns.last_mut().filter(|t| t.user) {
        last.text = format!("Current drawing:\n{}\nRequest: {}", describe(doc, selection), last.text);
    }
    let a = llm::chat(s, &system_prompt(), &turns)?;
    let (reply, actions) = parse_answer(&a.text);
    Ok(Answer { reply, actions, switched_model: a.switched_model })
}

/// Outcome of applying operations.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Report {
    pub applied: usize,
    pub errors: Vec<String>,
    pub added: Vec<u64>,
    pub select: Vec<u64>,
    pub zoom: bool,
}

fn pt(v: &Value) -> Option<V2> {
    match v {
        Value::Array(a) if a.len() >= 2 => Some(V2::new(a[0].as_f64()?, a[1].as_f64()?)),
        Value::Object(o) => Some(V2::new(o.get("x")?.as_f64()?, o.get("y")?.as_f64()?)),
        _ => None,
    }
}

fn get_pt(v: &Value, keys: &[&str]) -> Result<V2, String> {
    keys.iter().find_map(|k| v.get(*k).and_then(pt)).ok_or_else(|| format!("missing point \"{}\"", keys[0]))
}

fn get_num(v: &Value, keys: &[&str]) -> Option<f64> {
    keys.iter().find_map(|k| v.get(*k).and_then(|x| x.as_f64().or_else(|| x.as_str().and_then(|s| s.trim().parse().ok()))))
}

fn ids(v: &Value) -> Vec<u64> {
    match v.get("ids").or_else(|| v.get("id")) {
        Some(Value::Array(a)) => a.iter().filter_map(|x| x.as_u64()).collect(),
        Some(x) => x.as_u64().into_iter().collect(),
        None => vec![],
    }
}

/// Build a shape from the assistant's JSON form (or ImagineCAD's own serialisation).
pub fn shape_from_json(v: &Value, doc: &Document) -> Result<Shape, String> {
    let ty = v["type"].as_str().unwrap_or("").to_ascii_lowercase();
    let shape = match ty.as_str() {
        "line" => Shape::Line { a: get_pt(v, &["a", "p1", "start"])?, b: get_pt(v, &["b", "p2", "end"])? },
        "circle" => {
            let r = get_num(v, &["r", "radius"]).or_else(|| get_num(v, &["diameter", "d"]).map(|d| d / 2.0)).ok_or("circle needs r")?;
            Shape::Circle { c: get_pt(v, &["c", "center"])?, r }
        }
        "arc" => Shape::Arc {
            c: get_pt(v, &["c", "center"])?,
            r: get_num(v, &["r", "radius"]).ok_or("arc needs r")?,
            start: get_num(v, &["start", "start_angle"]).ok_or("arc needs start")?.to_radians(),
            end: get_num(v, &["end", "end_angle"]).ok_or("arc needs end")?.to_radians(),
        },
        "polyline" | "pline" => {
            let pts: Vec<V2> =
                v["pts"].as_array().or_else(|| v["points"].as_array()).ok_or("polyline needs pts")?.iter().filter_map(pt).collect();
            if pts.len() < 2 {
                return Err("polyline needs at least 2 points".into());
            }
            Shape::Polyline { pts, closed: v["closed"].as_bool().unwrap_or(false) }
        }
        "rectangle" | "rect" => {
            let (a, b) = (get_pt(v, &["a", "p1", "corner1"])?, get_pt(v, &["b", "p2", "corner2"])?);
            Shape::Polyline { pts: vec![a, V2::new(b.x, a.y), b, V2::new(a.x, b.y)], closed: true }
        }
        "polygon" => {
            let c = get_pt(v, &["c", "center"])?;
            let r = get_num(v, &["r", "radius"]).ok_or("polygon needs r")?;
            let sides = get_num(v, &["sides", "n"]).unwrap_or(6.0).round().clamp(3.0, 1024.0) as usize;
            let rot = get_num(v, &["rotation"]).unwrap_or(0.0).to_radians();
            let pts = (0..sides).map(|i| c + V2::polar(r, rot + i as f64 * std::f64::consts::TAU / sides as f64)).collect();
            Shape::Polyline { pts, closed: true }
        }
        "text" | "note" => {
            let h = get_num(v, &["height", "h"]).unwrap_or(3.5 * doc.sheet.scale);
            let content = v["text"].as_str().or_else(|| v["content"].as_str()).ok_or("text needs text")?;
            Shape::Text(
                Text::new(get_pt(v, &["at", "pos", "position"])?, h, content)
                    .rotated(get_num(v, &["rotation"]).unwrap_or(0.0).to_radians()),
            )
        }
        "dim" | "dimension" => {
            let kind = v["kind"].as_str().unwrap_or("aligned").to_ascii_lowercase();
            let at = get_pt(v, &["at", "loc", "location"])?;
            let mut d = match kind.as_str() {
                "horizontal" | "vertical" | "aligned" | "linear" => {
                    let k = match kind.as_str() {
                        "horizontal" => DimKind::Horizontal,
                        "vertical" => DimKind::Vertical,
                        _ => DimKind::Aligned,
                    };
                    Dimension::new(k, get_pt(v, &["p1", "a"])?, get_pt(v, &["p2", "b"])?, at)
                }
                "radius" | "diameter" => {
                    let c = get_pt(v, &["c", "center"])?;
                    let r = get_num(v, &["r", "radius"])
                        .or_else(|| get_num(v, &["diameter"]).map(|d| d / 2.0))
                        .ok_or("radial dimension needs r")?;
                    let u = (at - c).norm();
                    let u = if u == V2::ZERO { V2::new(1.0, 0.0) } else { u };
                    Dimension::new(if kind == "radius" { DimKind::Radius } else { DimKind::Diameter }, c, c + u * r, at)
                }
                "angular" | "angle" => {
                    Dimension::angular(get_pt(v, &["center", "c", "vertex"])?, get_pt(v, &["p1", "a"])?, get_pt(v, &["p2", "b"])?, at)
                }
                "ordinate" => {
                    let datum = get_pt(v, &["datum", "origin"]).unwrap_or(V2::ZERO);
                    let mut d = Dimension::ordinate(datum, get_pt(v, &["feature", "p1", "p"])?, at);
                    match v["axis"].as_str().map(|a| a.to_ascii_lowercase()) {
                        Some(a) if a == "x" => d.kind = DimKind::OrdinateX,
                        Some(a) if a == "y" => d.kind = DimKind::OrdinateY,
                        _ => {}
                    }
                    d
                }
                "arc_length" | "arclength" | "arc" => {
                    let c = get_pt(v, &["c", "center"])?;
                    let r = get_num(v, &["r", "radius"]).ok_or("arc length dimension needs r")?;
                    let s = get_num(v, &["start"]).ok_or("arc length dimension needs start")?.to_radians();
                    let e = get_num(v, &["end"]).ok_or("arc length dimension needs end")?.to_radians();
                    Dimension::arc_length(c, c + V2::polar(r, s), c + V2::polar(r, e), at)
                }
                other => return Err(format!("unknown dimension kind \"{other}\"")),
            };
            d.text = v["text"].as_str().filter(|t| !t.trim().is_empty()).map(String::from);
            Shape::Dim(d)
        }
        "leader" => {
            let pts: Vec<V2> =
                v["pts"].as_array().or_else(|| v["points"].as_array()).ok_or("leader needs pts")?.iter().filter_map(pt).collect();
            if pts.len() < 2 {
                return Err("leader needs 2 points".into());
            }
            Shape::Leader(Leader { pts, text: v["text"].as_str().unwrap_or("").to_string(), style: v["style"].as_str().map(String::from) })
        }
        "tolerance" | "fcf" | "gdt" => {
            let rows = v["rows"]
                .as_array()
                .ok_or("tolerance needs rows")?
                .iter()
                .map(|r| {
                    Ok(FcfRow {
                        symbol: r["symbol"].as_str().and_then(gdt_symbol).ok_or(format!("unknown tolerance symbol {}", r["symbol"]))?,
                        diameter: r["diameter"].as_bool().unwrap_or(false),
                        tolerance: r["tolerance"].as_str().map(String::from).or_else(|| r["tolerance"].as_f64().map(n)).unwrap_or_default(),
                        material: material(&r["material"]),
                        datums: r["datums"].as_array().map(|a| a.iter().filter_map(datum_ref).take(3).collect()).unwrap_or_default(),
                    })
                })
                .collect::<Result<Vec<_>, String>>()?;
            let leader = v["leader"].as_array().map(|a| a.iter().filter_map(pt).collect()).unwrap_or_default();
            Shape::Tolerance(Fcf { pos: get_pt(v, &["at", "pos"])?, rows, leader })
        }
        "table" => {
            let rows: Vec<Vec<String>> = v["rows"]
                .as_array()
                .ok_or("table needs rows")?
                .iter()
                .map(|r| {
                    r.as_array()
                        .map(|c| c.iter().map(|x| x.as_str().map(String::from).unwrap_or_else(|| x.to_string())).collect())
                        .unwrap_or_default()
                })
                .collect();
            let h = get_num(v, &["text_height", "height"]).unwrap_or(2.5 * doc.sheet.scale);
            Shape::Table(icad_core::table::Table::from_rows(get_pt(v, &["at", "pos"])?, &rows, h))
        }
        "datum" => Shape::Datum(DatumFeature {
            letter: v["letter"].as_str().unwrap_or("A").to_string(),
            pos: get_pt(v, &["at", "pos"])?,
            tip: get_pt(v, &["tip", "feature"])?,
        }),
        "surface" | "surface_finish" | "roughness" => Shape::Surface(SurfaceFinish {
            pos: get_pt(v, &["at", "pos", "tip"])?,
            rotation: get_num(v, &["rotation"]).unwrap_or(0.0).to_radians(),
            process: match v["process"].as_str().unwrap_or("removal").to_ascii_lowercase().as_str() {
                "any" => SurfaceProcess::Any,
                "no_removal" | "noremoval" | "none" => SurfaceProcess::NoRemoval,
                _ => SurfaceProcess::Removal,
            },
            requirement: v["requirement"].as_str().unwrap_or("").to_string(),
            method: v["method"].as_str().unwrap_or("").to_string(),
            lay: v["lay"].as_str().unwrap_or("").to_string(),
            all_around: v["all_around"].as_bool().unwrap_or(false),
        }),
        "weld" => Shape::Weld(WeldSymbol {
            tip: get_pt(v, &["tip", "arrow"])?,
            elbow: get_pt(v, &["elbow", "at"])?,
            arrow_side: weld_side(&v["arrow_side"]),
            other_side: weld_side(&v["other_side"]),
            all_around: v["all_around"].as_bool().unwrap_or(false),
            field: v["field"].as_bool().unwrap_or(false),
            tail: v["tail"].as_str().unwrap_or("").to_string(),
        }),
        "ellipse" => Shape::Ellipse {
            c: get_pt(v, &["c", "center"])?,
            major: get_pt(v, &["major", "major_axis"])?,
            ratio: get_num(v, &["ratio"]).unwrap_or(0.5).clamp(1e-6, 1.0),
            start: 0.0,
            end: std::f64::consts::TAU,
        },
        "point" => Shape::Point { p: get_pt(v, &["p", "at", "pos"])? },
        "hatch" => {
            let loops: Vec<Vec<V2>> = v["loops"]
                .as_array()
                .ok_or("hatch needs loops")?
                .iter()
                .filter_map(|l| l.as_array().map(|a| a.iter().filter_map(pt).collect::<Vec<_>>()))
                .filter(|l| l.len() >= 3)
                .collect();
            if loops.is_empty() {
                return Err("hatch needs a closed loop of 3+ points".into());
            }
            Shape::Hatch(Hatch {
                loops,
                pattern: v["pattern"].as_str().unwrap_or("ANSI31").to_ascii_uppercase(),
                scale: get_num(v, &["scale"]).unwrap_or(1.0),
                angle: get_num(v, &["angle"]).unwrap_or(0.0).to_radians(),
            })
        }
        _ => serde_json::from_value::<Shape>(v.clone()).map_err(|_| format!("unknown shape type \"{}\"", v["type"]))?,
    };
    Ok(shape)
}

fn existing(doc: &Document, ids: &[u64], errors: &mut Vec<String>) -> Vec<u64> {
    let (ok, missing): (Vec<u64>, Vec<u64>) = ids.iter().partition(|i| doc.get(**i).is_some());
    if !missing.is_empty() {
        errors.push(format!("no entities with id {missing:?}"));
    }
    ok
}

fn transform(doc: &mut Document, ids: &[u64], x: &Xform) {
    for id in ids {
        if let Some(e) = doc.get_mut(*id) {
            e.shape.transform(x);
        }
    }
}

fn copy_of(doc: &mut Document, ids: &[u64], x: &Xform) -> Vec<u64> {
    let protos: Vec<Entity> = ids.iter().filter_map(|i| doc.get(*i).cloned()).collect();
    protos
        .iter()
        .map(|e| {
            let mut s = e.shape.clone();
            s.transform(x);
            doc.add_like(e, s)
        })
        .collect()
}

fn ensure_layer(doc: &mut Document, name: &str) {
    if doc.layer(name).is_none() {
        let (color, lt, lw) = match name {
            layers::OUTLINE => (7, LineType::Continuous, 0.5),
            layers::HIDDEN => (2, LineType::Hidden, 0.25),
            layers::CENTER => (1, LineType::Center, 0.18),
            layers::DIMENSIONS => (4, LineType::Continuous, 0.18),
            layers::TEXT => (3, LineType::Continuous, 0.25),
            _ => (7, LineType::Continuous, 0.25),
        };
        doc.ensure_layer(Layer::new(name, color, lt, lw));
    }
}

/// Where a regenerated part is anchored: the left end on the centre line for
/// a shaft, the lower-left corner of the outline otherwise.
fn part_anchor(spec: &PartSpec, b: &BBox) -> V2 {
    match spec {
        PartSpec::Shaft { .. } => V2::new(b.min.x, (b.min.y + b.max.y) / 2.0),
        _ => b.min,
    }
}

fn outline_bbox(doc: &Document, ids: &[u64]) -> BBox {
    ids.iter()
        .filter_map(|i| doc.get(*i))
        .filter(|e| e.layer == layers::OUTLINE && matches!(e.shape, Shape::Polyline { closed: true, .. }))
        .fold(BBox::empty(), |b, e| b.union(&doc.entity_bbox(e)))
}

/// Regenerate the parametric part from `spec_json`, keeping it where it was.
pub fn rebuild_part(doc: &mut Document, spec_json: &Value) -> Result<Vec<u64>, String> {
    let spec = PartSpec::from_json(&json!({ "part": spec_json })).ok_or("the part description is incomplete")?;
    let shapes = spec.build();
    let new_outline = shapes.iter().filter(|(l, s)| l == layers::OUTLINE && matches!(s, Shape::Polyline { closed: true, .. })).fold(
        BBox::empty(),
        |b, (_, s)| {
            b.union(&BBox::of(match s {
                Shape::Polyline { pts, .. } => pts.iter().copied(),
                _ => unreachable!(),
            }))
        },
    );
    let mut shift = V2::ZERO;
    if let Some(old) = doc.part.take() {
        let ob = outline_bbox(doc, &old.ids);
        if !ob.is_empty() && !new_outline.is_empty() {
            shift = part_anchor(&spec, &ob) - part_anchor(&spec, &new_outline);
        }
        let gone: Vec<u64> = old.ids.iter().copied().filter(|i| doc.get(*i).is_some()).collect();
        doc.remove(&gone);
    }
    let x = Xform::translate(shift);
    let mut ids = vec![];
    for (layer, mut s) in shapes {
        ensure_layer(doc, &layer);
        s.transform(&x);
        ids.push(doc.add_on(&layer, s));
    }
    doc.part = Some(PartRecord { spec: spec.to_json(), ids: ids.clone() });
    doc.associate_all_dims();
    // Choose a larger sheet when the part no longer fits.
    let geo = ids
        .iter()
        .filter_map(|i| doc.get(*i))
        .filter(|e| !e.shape.is_annotation())
        .fold(BBox::empty(), |b, e| b.union(&doc.entity_bbox(e)));
    let sh = &doc.sheet;
    let avail = sh.drawable_mm();
    let room = BBox::from_pts(sh.origin + avail.min * sh.scale, sh.origin + avail.max * sh.scale);
    if !geo.is_empty() && !(room.contains(geo.min) && room.contains(geo.max)) {
        doc.sheet.fit_to(&fit_margin(&geo));
    }
    Ok(ids)
}

fn parse_color(v: &Value) -> Option<Color> {
    match v {
        Value::Number(x) => x.as_u64().filter(|c| (1..=255).contains(c)).map(|c| Color::Aci(c as u8)),
        Value::String(s) if s.eq_ignore_ascii_case("bylayer") => Some(Color::ByLayer),
        Value::String(s) => ACI_NAMES
            .iter()
            .find(|(_, name)| name.eq_ignore_ascii_case(s.trim()))
            .map(|(c, _)| Color::Aci(*c))
            .or_else(|| s.parse().ok().map(Color::Aci)),
        _ => None,
    }
}

/// Apply one operation.
fn apply_one(doc: &mut Document, a: &Value, r: &mut Report) -> Result<(), String> {
    let op = a["op"].as_str().or_else(|| a["action"].as_str()).unwrap_or("").to_ascii_lowercase();
    match op.as_str() {
        "add" | "create" | "draw" => {
            let mut shape = shape_from_json(a.get("shape").unwrap_or(a), doc)?;
            // New dimensions follow the geometry they measure.
            if let Shape::Dim(d) = &mut shape {
                doc.associate_dim(d);
            }
            let layer = a["layer"].as_str().map(String::from).unwrap_or_else(|| doc.current_layer.clone());
            ensure_layer(doc, &layer);
            r.added.push(doc.add_on(&layer, shape));
        }
        "update" | "replace" | "modify" => {
            let id = a["id"].as_u64().ok_or("update needs an id")?;
            let shape = shape_from_json(&a["shape"], doc)?;
            doc.get_mut(id).ok_or(format!("no entity with id {id}"))?.shape = shape;
        }
        "delete" | "erase" | "remove" => {
            let ids = existing(doc, &ids(a), &mut r.errors);
            doc.remove(&ids);
        }
        "move" | "translate" => {
            let ids = existing(doc, &ids(a), &mut r.errors);
            let d = V2::new(get_num(a, &["dx"]).unwrap_or(0.0), get_num(a, &["dy"]).unwrap_or(0.0));
            transform(doc, &ids, &Xform::translate(d));
        }
        "copy" | "array" => {
            let ids = existing(doc, &ids(a), &mut r.errors);
            let d = V2::new(get_num(a, &["dx"]).unwrap_or(0.0), get_num(a, &["dy"]).unwrap_or(0.0));
            let count = get_num(a, &["count"]).unwrap_or(1.0).round().clamp(1.0, 1000.0) as usize;
            for k in 1..=count {
                let new = copy_of(doc, &ids, &Xform::translate(d * k as f64));
                r.added.extend(new);
            }
        }
        "rotate" => {
            let ids = existing(doc, &ids(a), &mut r.errors);
            let c = get_pt(a, &["center", "c", "base"])?;
            transform(doc, &ids, &Xform::rotate(c, get_num(a, &["angle", "deg"]).ok_or("rotate needs angle")?.to_radians()));
        }
        "scale" => {
            let ids = existing(doc, &ids(a), &mut r.errors);
            let c = get_pt(a, &["center", "c", "base"])?;
            let k = get_num(a, &["factor", "k"]).filter(|k| *k > 0.0).ok_or("scale needs a positive factor")?;
            transform(doc, &ids, &Xform::scale(c, k));
        }
        "mirror" => {
            let ids = existing(doc, &ids(a), &mut r.errors);
            let x = Xform::mirror(get_pt(a, &["a", "p1"])?, get_pt(a, &["b", "p2"])?);
            if a["keep"].as_bool().unwrap_or(true) {
                let new = copy_of(doc, &ids, &x);
                r.added.extend(new);
            } else {
                transform(doc, &ids, &x);
            }
        }
        "set" | "properties" | "change" => {
            let ids = existing(doc, &ids(a), &mut r.errors);
            if let Some(l) = a["layer"].as_str() {
                ensure_layer(doc, l);
            }
            let color = a.get("color").and_then(parse_color);
            let lt = a["linetype"].as_str().map(|s| {
                if s.eq_ignore_ascii_case("bylayer") {
                    Some(LtRef::ByLayer)
                } else {
                    LineType::from_name(s).map(LtRef::Lt)
                }
            });
            for id in ids {
                let e = doc.get_mut(id).ok_or(format!("no entity with id {id}"))?;
                if let Some(l) = a["layer"].as_str() {
                    e.layer = l.to_string();
                }
                if let Some(c) = color {
                    e.color = c;
                }
                match lt {
                    Some(Some(l)) => e.linetype = l,
                    Some(None) => return Err(format!("unknown linetype {}", a["linetype"])),
                    None => {}
                }
                if let Some(w) = get_num(a, &["lineweight"]) {
                    e.lineweight = Some(w);
                }
                if let Some(t) = a["text"].as_str() {
                    match &mut e.shape {
                        Shape::Text(x) => x.content = t.into(),
                        Shape::MText(x) => x.content = t.into(),
                        Shape::Leader(x) => x.text = t.into(),
                        Shape::Dim(d) => d.text = (!t.trim().is_empty() && t.trim() != "<>").then(|| t.to_string()),
                        _ => return Err(format!("entity {id} has no text")),
                    }
                }
                if let Some(h) = get_num(a, &["height"]) {
                    match &mut e.shape {
                        Shape::Text(x) => x.height = h,
                        Shape::MText(x) => x.height = h,
                        _ => {}
                    }
                }
            }
        }
        "layer" => {
            let name = a["name"].as_str().ok_or("layer needs a name")?.to_string();
            ensure_layer(doc, &name);
            let l = doc.layer_mut(&name).expect("just ensured");
            if let Some(Color::Aci(c)) = a.get("color").and_then(parse_color) {
                l.color = c;
            }
            if let Some(lt) = a["linetype"].as_str().and_then(LineType::from_name) {
                l.linetype = lt;
            }
            if let Some(w) = get_num(a, &["lineweight"]) {
                l.lineweight = w;
            }
            if let Some(v) = a["visible"].as_bool() {
                l.visible = v;
            }
            if let Some(v) = a["locked"].as_bool() {
                l.locked = v;
            }
            if a["current"].as_bool() == Some(true) {
                doc.current_layer = name;
            }
        }
        "title" | "titleblock" => {
            let f = a.get("fields").unwrap_or(a);
            let tb = &mut doc.sheet.title;
            for (k, field) in [
                ("company", &mut tb.company),
                ("title", &mut tb.title),
                ("drawing_no", &mut tb.drawing_no),
                ("revision", &mut tb.revision),
                ("drawn_by", &mut tb.drawn_by),
                ("checked_by", &mut tb.checked_by),
                ("approved_by", &mut tb.approved_by),
                ("date", &mut tb.date),
                ("material", &mut tb.material),
                ("finish", &mut tb.finish),
                ("sheet_no", &mut tb.sheet_no),
                ("units", &mut tb.units),
                ("tolerance", &mut tb.tolerance),
            ] {
                if let Some(v) = f[k].as_str() {
                    *field = v.to_string();
                }
            }
            doc.sheet.visible = true;
        }
        "sheet" | "page" => {
            if a["fit"].as_bool() == Some(true) {
                let geo = doc.entities.iter().filter(|e| !e.shape.is_annotation()).fold(BBox::empty(), |b, e| b.union(&doc.entity_bbox(e)));
                if !geo.is_empty() {
                    doc.sheet.fit_to(&fit_margin(&geo));
                }
            }
            if let Some(name) = a["paper"].as_str() {
                doc.sheet.paper = PaperSize::ALL
                    .into_iter()
                    .find(|p| p.name().eq_ignore_ascii_case(name.trim()))
                    .ok_or(format!("unknown paper size {name}"))?;
            }
            if let Some(l) = a["landscape"].as_bool() {
                doc.sheet.landscape = l;
            }
            if let Some(s) = a["scale"].as_str().map(String::from).or_else(|| a["scale"].as_f64().map(|x| x.to_string())) {
                doc.sheet.scale = parse_scale(&s).ok_or(format!("cannot read scale {s}"))?;
            }
            let geo = doc.extents();
            if !geo.is_empty() {
                doc.sheet.center_on(&geo);
            }
            doc.sheet.visible = true;
        }
        "dimstyle" => {
            if let Some(h) = get_num(a, &["text_height"]) {
                doc.dimstyle.text_height = h;
            }
            if let Some(s) = get_num(a, &["arrow_size"]) {
                doc.dimstyle.arrow_size = s;
            }
            if let Some(d) = get_num(a, &["decimals"]) {
                doc.dimstyle.decimals = d.round().clamp(0.0, 8.0) as usize;
            }
        }
        "part" | "rebuild" => {
            let spec = a.get("part").or_else(|| a.get("spec")).ok_or("part needs the part description")?;
            let ids = rebuild_part(doc, spec)?;
            r.added.extend(ids);
            r.zoom = true;
        }
        "select" | "highlight" => r.select = existing(doc, &ids(a), &mut r.errors),
        "zoom" | "zoom_extents" => r.zoom = true,
        "" => return Err("operation without \"op\"".into()),
        other => return Err(format!("unknown operation \"{other}\"")),
    }
    Ok(())
}

/// Apply the operations in order. Failing operations are reported and skipped.
pub fn apply(doc: &mut Document, actions: &[Value]) -> Report {
    let mut r = Report::default();
    for a in actions {
        match apply_one(doc, a, &mut r) {
            Ok(()) => r.applied += 1,
            Err(e) => r.errors.push(format!("{}: {e}", a["op"].as_str().unwrap_or("?"))),
        }
    }
    r.added.retain(|i| doc.get(*i).is_some());
    r
}

#[cfg(test)]
mod tests {
    use super::*;

    fn shaft() -> Value {
        json!({"type": "shaft", "total_length": 600,
            "segments": [{"length": 100, "diameter": 40}, {"length": 400, "diameter": 50}, {"length": 100, "diameter": 40}],
            "keyways": [], "chamfers": []})
    }

    #[test]
    fn answer_is_split_into_reply_and_actions() {
        let (reply, actions) = parse_answer("```json\n{\"reply\": \"Done.\", \"actions\": [{\"op\": \"zoom\"}]}\n```");
        assert_eq!(reply, "Done.");
        assert_eq!(actions.len(), 1);
        let (reply, actions) = parse_answer("The shaft is 2836 mm long.");
        assert_eq!(reply, "The shaft is 2836 mm long.");
        assert!(actions.is_empty());
    }

    #[test]
    fn edits_apply_to_the_document() {
        let mut doc = Document::new();
        let r = apply(
            &mut doc,
            &[
                json!({"op": "add", "layer": "OUTLINE", "shape": {"type": "rectangle", "a": [0, 0], "b": [100, 50]}}),
                json!({"op": "add", "shape": {"type": "circle", "c": [50, 25], "r": 10}}),
                json!({"op": "title", "fields": {"company": "ACME", "material": "C45"}}),
                json!({"op": "nonsense"}),
            ],
        );
        assert_eq!(r.applied, 3);
        assert_eq!(r.errors.len(), 1);
        assert_eq!(doc.sheet.title.company, "ACME");
        let circle = r.added[1];
        let r = apply(
            &mut doc,
            &[
                json!({"op": "move", "ids": [circle], "dx": 10, "dy": 5}),
                json!({"op": "set", "ids": [circle], "layer": "HIDDEN", "color": "red"}),
                json!({"op": "copy", "ids": [circle], "dx": 30, "dy": 0, "count": 2}),
            ],
        );
        assert_eq!(r.applied, 3, "{:?}", r.errors);
        let e = doc.get(circle).unwrap();
        assert_eq!(e.shape, Shape::Circle { c: V2::new(60.0, 30.0), r: 10.0 });
        assert_eq!((e.layer.as_str(), e.color), ("HIDDEN", Color::Aci(1)));
        assert_eq!(doc.entities.len(), 4);
        let text = describe(&doc, &[circle]);
        assert!(text.contains(&format!("{circle} | HIDDEN | circle center (60,30) r 10")), "{text}");
        let r = apply(&mut doc, &[json!({"op": "delete", "ids": [circle, 999]})]);
        assert_eq!(r.errors.len(), 1);
        assert!(doc.get(circle).is_none());
    }

    #[test]
    fn part_is_regenerated_in_place() {
        let mut doc = Document::new();
        let ids = rebuild_part(&mut doc, &shaft()).unwrap();
        // The user moves the part, then asks for a longer middle segment.
        apply(&mut doc, &[json!({"op": "move", "ids": ids, "dx": 1000, "dy": 200})]);
        let mut longer = shaft();
        longer["segments"][1]["length"] = json!(500);
        longer["total_length"] = json!(700);
        let r = apply(&mut doc, &[json!({"op": "part", "part": longer})]);
        assert!(r.errors.is_empty(), "{:?}", r.errors);
        assert!(ids.iter().all(|i| doc.get(*i).is_none()), "old geometry removed");
        let rec = doc.part.clone().unwrap();
        let b = outline_bbox(&doc, &rec.ids);
        assert!((b.min.x - 1000.0).abs() < 1e-9 && (b.width() - 700.0).abs() < 1e-9, "{b:?}");
        assert!(((b.min.y + b.max.y) / 2.0 - 200.0).abs() < 1e-9);
        assert_eq!(rec.spec["segments"][1]["length"], 500.0);
        assert!(describe(&doc, &[]).contains("Parametric part"));
    }
}
