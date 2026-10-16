// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Proteen Das

//! DXF reader and writer.
//!
//! * **DXF 2000 (AC1015)**: full fidelity: LWPOLYLINE, ELLIPSE, SPLINE,
//!   MTEXT, HATCH, XLINE/RAY, block INSERTs, lineweights and real DIMENSION
//!   entities, with the handle/owner structure the format requires.
//! * **R12 (AC1009)**: maximum compatibility; entities newer than R12 are
//!   written as equivalent lines/polylines/solids/text.
//!
//! The reader understands R12 through current DXF for the entity set
//! ImagineCAD supports, including blocks and hatches.

use icad_core::curves::{eval_bspline, spline_bspline};
use icad_core::document::Document;
use icad_core::entity::*;
use icad_core::geom::*;
use icad_core::hatch::pattern_families;
use icad_core::prim::*;
use icad_core::style::*;
use std::fmt::Write as _;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum DxfVersion {
    R12,
    #[default]
    R2000,
}

#[derive(Clone, Debug)]
pub struct DxfOptions {
    /// Include the sheet border and title block (layer `TITLEBLOCK`).
    pub include_sheet: bool,
    /// Write dimensions as plain lines/text instead of DIMENSION entities.
    pub explode_dimensions: bool,
    pub version: DxfVersion,
}

impl Default for DxfOptions {
    fn default() -> Self {
        Self { include_sheet: true, explode_dimensions: false, version: DxfVersion::R2000 }
    }
}

#[derive(Clone, Copy, Default)]
struct Style {
    color: Option<i32>,
    lt: Option<&'static str>,
    /// Lineweight in 1/100 mm.
    lw: Option<i32>,
}

struct W {
    s: String,
    v: DxfVersion,
    next: u64,
}

impl W {
    fn new2000(&self) -> bool {
        self.v == DxfVersion::R2000
    }
    fn g(&mut self, code: i32, v: impl std::fmt::Display) {
        let _ = write!(self.s, "{code:>3}\n{v}\n");
    }
    fn f(&mut self, code: i32, v: f64) {
        let v = if v.is_finite() { v } else { 0.0 };
        let mut t = format!("{:.10}", v);
        while t.ends_with('0') {
            t.pop();
        }
        if t.ends_with('.') {
            t.push('0');
        }
        if t == "-0.0" {
            t = "0.0".into();
        }
        self.g(code, t);
    }
    fn pt(&mut self, base: i32, p: V2) {
        self.f(base, p.x);
        self.f(base + 10, p.y);
        self.f(base + 20, 0.0);
    }
    fn alloc(&mut self) -> u64 {
        let h = self.next;
        self.next += 1;
        h
    }
    /// Entity header. In DXF 2000 also handle, owner and the AcDbEntity subclass.
    fn ent(&mut self, kind: &str, layer: &str, owner: u64, st: Style, subclass: &str) {
        self.g(0, kind);
        if self.new2000() {
            let h = self.alloc();
            self.g(5, format!("{h:X}"));
            self.g(330, format!("{owner:X}"));
            self.g(100, "AcDbEntity");
        }
        self.g(8, layer);
        if let Some(lt) = st.lt {
            self.g(6, lt);
        }
        if let Some(c) = st.color {
            self.g(62, c);
        }
        if self.new2000() {
            if let Some(lw) = st.lw {
                self.g(370, lw);
            }
            if !subclass.is_empty() {
                self.g(100, subclass);
            }
        }
    }
    fn sub(&mut self, name: &str) {
        if self.new2000() {
            self.g(100, name);
        }
    }
    /// ImagineCAD extended data: the exact shape, so it reads back losslessly.
    fn xdata(&mut self, shape: &Shape) {
        if let Ok(json) = serde_json::to_string(shape) {
            self.xdata_json(&json);
        }
    }
    fn xdata_json(&mut self, json: &str) {
        use base64::Engine as _;
        let b64 = base64::engine::general_purpose::STANDARD.encode(json);
        self.g(1001, XDATA_APP);
        for chunk in b64.as_bytes().chunks(250) {
            self.g(1000, std::str::from_utf8(chunk).unwrap_or(""));
        }
    }
}

/// Named-dictionary key of the XRECORD holding ImagineCAD's constraints.
const CONSTRAINTS_KEY: &str = "IMAGINECAD_CONSTRAINTS";

/// Replace every `"id": n` in `v` using `map` (false when one is missing).
fn map_ids(v: &mut serde_json::Value, map: &dyn Fn(&serde_json::Value) -> Option<serde_json::Value>) -> bool {
    match v {
        serde_json::Value::Object(o) => {
            if let Some(id) = o.get("id").cloned() {
                if o.contains_key("k") {
                    match map(&id) {
                        Some(n) => {
                            o.insert("id".into(), n);
                        }
                        None => return false,
                    }
                }
            }
            o.values_mut().all(|x| map_ids(x, map))
        }
        serde_json::Value::Array(a) => a.iter_mut().all(|x| map_ids(x, map)),
        _ => true,
    }
}

/// The constraints as JSON with entity ids replaced by DXF handles.
fn constraints_json(doc: &Document, handles: &[(u64, u64)]) -> Option<String> {
    if doc.constraints.is_empty() {
        return None;
    }
    let mut kept = vec![];
    for c in &doc.constraints {
        let mut v = serde_json::to_value(c).ok()?;
        let ok = map_ids(&mut v, &|id| {
            let id = id.as_u64()?;
            handles.iter().find(|(e, _)| *e == id).map(|(_, h)| serde_json::Value::String(format!("{h:X}")))
        });
        if ok {
            kept.push(v);
        }
    }
    (!kept.is_empty()).then(|| serde_json::Value::Array(kept).to_string())
}

/// Registered application name of ImagineCAD's extended data.
const XDATA_APP: &str = "IMAGINECAD";

/// Shapes DXF 2000 has no entity for: written as an anonymous block of their
/// graphics plus extended data holding the exact shape.
fn needs_proxy(s: &Shape) -> bool {
    match s {
        Shape::Dim(d) => d.kind == DimKind::ArcLength,
        Shape::Tolerance(f) => !f.leader.is_empty(),
        Shape::Datum(_) | Shape::Surface(_) | Shape::Weld(_) | Shape::Table(_) | Shape::Region(_) => true,
        Shape::Leader(l) => l.style.is_some(),
        _ => false,
    }
}

/// ImagineCAD extended data of a record, decoded.
fn xdata_bytes(r: &Rec) -> Option<Vec<u8>> {
    use base64::Engine as _;
    let start = r.codes.iter().position(|(c, v)| *c == 1001 && v.trim() == XDATA_APP)?;
    let b64: String =
        r.codes[start + 1..].iter().take_while(|(c, _)| *c != 1001).filter(|(c, _)| *c == 1000).map(|(_, v)| v.trim()).collect();
    base64::engine::general_purpose::STANDARD.decode(b64).ok()
}

/// The exact shape stored in ImagineCAD extended data, if present.
fn xdata_shape(r: &Rec) -> Option<Shape> {
    serde_json::from_slice(&xdata_bytes(r)?).ok()
}

/// The external reference file named in an INSERT's extended data
/// (converters may drop the external flag and path of the block itself).
fn xdata_xref(r: &Rec) -> Option<String> {
    let v: serde_json::Value = serde_json::from_slice(&xdata_bytes(r)?).ok()?;
    v.get("xref")?.as_str().map(String::from)
}

/// Encode text for DXF: standard %% control codes for common symbols, `\U+XXXX` for the rest.
pub fn encode_text(s: &str) -> String {
    let mut o = String::new();
    for c in s.chars() {
        match c {
            'Ø' | 'ø' | '⌀' | '∅' => o.push_str("%%c"),
            '°' => o.push_str("%%d"),
            '±' => o.push_str("%%p"),
            c if (c as u32) < 128 => o.push(c),
            c => {
                let _ = write!(o, "\\U+{:04X}", c as u32);
            }
        }
    }
    o
}

pub fn decode_text(s: &str) -> String {
    let mut o = s.replace("%%c", "Ø").replace("%%C", "Ø").replace("%%d", "°").replace("%%D", "°");
    o = o.replace("%%p", "±").replace("%%P", "±").replace("%%u", "").replace("%%U", "");
    let mut out = String::new();
    let mut rest = o.as_str();
    while let Some(i) = rest.find("\\U+") {
        out.push_str(&rest[..i]);
        let hex = rest.get(i + 3..i + 7).unwrap_or("");
        match u32::from_str_radix(hex, 16).ok().and_then(char::from_u32) {
            Some(c) if hex.len() == 4 => {
                out.push(c);
                rest = &rest[i + 7..];
            }
            _ => {
                out.push_str("\\U+");
                rest = &rest[i + 3..];
            }
        }
    }
    out.push_str(rest);
    out
}

fn write_prim(w: &mut W, p: &Prim, layer: &str, owner: u64, st: Style) {
    match p {
        Prim::Line(a, b) => {
            w.ent("LINE", layer, owner, st, "AcDbLine");
            w.pt(10, *a);
            w.pt(11, *b);
        }
        Prim::Poly(pts, closed) => write_polyline(w, pts, *closed, layer, owner, st),
        Prim::Circle(c, r) => {
            w.ent("CIRCLE", layer, owner, st, "AcDbCircle");
            w.pt(10, *c);
            w.f(40, *r);
        }
        Prim::Arc { c, r, start, end } => {
            w.ent("ARC", layer, owner, st, "AcDbCircle");
            w.pt(10, *c);
            w.f(40, *r);
            w.sub("AcDbArc");
            w.f(50, deg(*start));
            w.f(51, deg(*end));
        }
        Prim::Fill(pts) if pts.len() >= 3 => {
            w.ent("SOLID", layer, owner, Style { lt: None, ..st }, "AcDbTrace");
            // SOLID vertex order is 1-2-4-3.
            let p4 = if pts.len() > 3 { pts[3] } else { pts[2] };
            w.pt(10, pts[0]);
            w.pt(11, pts[1]);
            w.pt(12, p4);
            w.pt(13, pts[2]);
        }
        Prim::Fill(_) | Prim::Image(_) => {}
        Prim::Text(t) => write_text(w, t, layer, owner, st),
    }
}

fn write_polyline(w: &mut W, pts: &[V2], closed: bool, layer: &str, owner: u64, st: Style) {
    if w.new2000() {
        w.ent("LWPOLYLINE", layer, owner, st, "AcDbPolyline");
        w.g(90, pts.len());
        w.g(70, if closed { 1 } else { 0 });
        w.f(43, 0.0);
        for p in pts {
            w.f(10, p.x);
            w.f(20, p.y);
        }
        return;
    }
    w.ent("POLYLINE", layer, owner, st, "");
    w.g(66, 1);
    w.pt(10, V2::ZERO);
    w.g(70, if closed { 1 } else { 0 });
    for p in pts {
        w.g(0, "VERTEX");
        w.g(8, layer);
        w.pt(10, *p);
    }
    w.g(0, "SEQEND");
    w.g(8, layer);
}

fn write_text(w: &mut W, t: &Text, layer: &str, owner: u64, st: Style) {
    w.ent("TEXT", layer, owner, Style { lt: None, ..st }, "AcDbText");
    let v = text_groups(w, t, &t.content);
    w.sub("AcDbText");
    if v != 0 {
        w.g(73, v);
    }
}

/// Attribute (ATTRIB in an INSERT, or ATTDEF in a block): text groups, then
/// the attribute subclass with tag, flags and vertical alignment.
#[allow(clippy::too_many_arguments)]
fn write_attribute(
    w: &mut W,
    def: bool,
    t: &Text,
    tag: &str,
    value: &str,
    prompt: &str,
    invisible: bool,
    layer: &str,
    owner: u64,
    st: Style,
) {
    w.ent(if def { "ATTDEF" } else { "ATTRIB" }, layer, owner, Style { lt: None, ..st }, "AcDbText");
    let v = text_groups(w, t, value);
    if def {
        w.sub("AcDbAttributeDefinition");
        w.g(3, encode_text(prompt));
    } else {
        w.sub("AcDbAttribute");
    }
    w.g(2, tag);
    w.g(70, if invisible { 1 } else { 0 });
    if v != 0 {
        w.g(74, v);
    }
}

/// Common TEXT groups; returns the vertical alignment code.
fn text_groups(w: &mut W, t: &Text, content: &str) -> i32 {
    w.pt(10, t.pos);
    w.f(40, t.height);
    w.g(1, encode_text(content));
    if t.rotation.abs() > 1e-9 {
        w.f(50, deg(t.rotation));
    }
    if let Some(s) = &t.style {
        w.g(7, s);
    }
    let h = match t.halign {
        HAlign::Left => 0,
        HAlign::Center => 1,
        HAlign::Right => 2,
    };
    let v = match t.valign {
        VAlign::Baseline => 0,
        VAlign::Bottom => 1,
        VAlign::Middle => 2,
        VAlign::Top => 3,
    };
    if h != 0 {
        w.g(72, h);
    }
    if h != 0 || v != 0 {
        w.pt(11, t.pos);
    }
    v
}

fn style_of(e: &Entity) -> Style {
    Style {
        color: match e.color {
            Color::ByLayer => None,
            Color::Aci(c) => Some(c as i32),
        },
        lt: match e.linetype {
            LtRef::ByLayer => None,
            LtRef::Lt(l) => Some(l.name()),
        },
        lw: e.lineweight.map(|l| (l * 100.0).round() as i32),
    }
}

/// Write one entity. `dim_no` numbers the anonymous dimension blocks.
/// Running numbers of the anonymous blocks written so far.
#[derive(Default)]
struct Counters {
    dim: usize,
    proxy: usize,
}

fn write_entity(w: &mut W, doc: &Document, e: &Entity, owner: u64, explode_dims: bool, n: &mut Counters) {
    let st = style_of(e);
    let layer = e.layer.as_str();
    let modern = w.new2000();
    match &e.shape {
        // Block contents (explode_dims) draw proxies as plain graphics.
        s if modern && !explode_dims && needs_proxy(s) => {
            n.proxy += 1;
            w.ent("INSERT", layer, owner, st, "AcDbBlockReference");
            w.g(2, format!("*U{}", n.proxy));
            w.pt(10, V2::ZERO);
            w.xdata(s);
        }
        Shape::Dim(d) if !explode_dims => {
            n.dim += 1;
            write_dimension(w, doc, d, layer, owner, st, n.dim);
        }
        Shape::Tolerance(f) if modern && !explode_dims => {
            w.ent("TOLERANCE", layer, owner, st, "AcDbFcf");
            w.g(3, "Standard");
            w.pt(10, f.pos);
            w.g(1, f.to_dxf());
            w.pt(11, V2::new(1.0, 0.0));
            w.xdata(&e.shape);
        }
        Shape::Point { p } => {
            w.ent("POINT", layer, owner, st, "AcDbPoint");
            w.pt(10, *p);
        }
        Shape::Insert(ins) if doc.block(&ins.name).is_some() => {
            let b = doc.block(&ins.name).unwrap();
            let has_atts = b.attdefs().next().is_some();
            let handle = w.next;
            w.ent("INSERT", layer, owner, st, "AcDbBlockReference");
            if has_atts {
                w.g(66, 1);
            }
            w.g(2, &ins.name);
            w.pt(10, ins.pos);
            w.f(41, ins.sx);
            w.f(42, ins.sy);
            w.f(43, 1.0);
            w.f(50, deg(ins.rotation));
            if has_atts {
                let x = ins.xform(b.base);
                for a in b.attdefs() {
                    let value = ins.value_of(&a.tag).unwrap_or(&a.default).to_string();
                    let t = match Prim::Text(a.text.clone()).transformed(&x) {
                        Prim::Text(t) => t,
                        _ => a.text.clone(),
                    };
                    write_attribute(w, false, &t, &a.tag, &value, "", a.invisible, layer, handle, st);
                }
                w.ent("SEQEND", layer, handle, Style::default(), "");
            }
            if let Some(path) = &b.xref {
                w.xdata_json(&serde_json::json!({ "xref": path }).to_string());
            }
        }
        // Fields: the current value, with the field codes kept as extended data.
        Shape::Text(t) if modern && t.content.contains("{{") => {
            let shown = Text { content: icad_core::fields::resolve(doc, &t.content), ..t.clone() };
            write_text(w, &shown, layer, owner, st);
            w.xdata(&e.shape);
        }
        Shape::AttDef(a) => write_attribute(w, true, &a.text, &a.tag, &a.default, &a.prompt, a.invisible, layer, owner, st),
        Shape::Ellipse { c, major, ratio, start, end } if modern => {
            w.ent("ELLIPSE", layer, owner, st, "AcDbEllipse");
            w.pt(10, *c);
            w.pt(11, *major);
            w.f(210, 0.0);
            w.f(220, 0.0);
            w.f(230, 1.0);
            w.f(40, *ratio);
            let full = icad_core::curves::is_full_ellipse(*start, *end);
            w.f(41, if full { 0.0 } else { *start });
            w.f(42, if full { std::f64::consts::TAU } else { *end });
        }
        Shape::Spline { pts, closed } if modern && pts.len() >= 2 => {
            let (ctrl, knots) = spline_bspline(pts, *closed);
            w.ent("SPLINE", layer, owner, st, "AcDbSpline");
            w.f(210, 0.0);
            w.f(220, 0.0);
            w.f(230, 1.0);
            w.g(70, 8);
            w.g(71, 3);
            w.g(72, knots.len());
            w.g(73, ctrl.len());
            w.g(74, 0);
            w.f(42, 1e-10);
            w.f(43, 1e-10);
            for k in &knots {
                w.f(40, *k);
            }
            for p in &ctrl {
                w.pt(10, *p);
            }
        }
        Shape::XLine { p, dir } | Shape::Ray { p, dir } if modern => {
            let (kind, sub) = if matches!(e.shape, Shape::XLine { .. }) { ("XLINE", "AcDbXline") } else { ("RAY", "AcDbRay") };
            w.ent(kind, layer, owner, st, sub);
            w.pt(10, *p);
            w.pt(11, dir.norm());
        }
        Shape::MText(m) if modern => {
            w.ent("MTEXT", layer, owner, Style { lt: None, ..st }, "AcDbMText");
            w.pt(10, m.pos);
            w.f(40, m.height);
            w.f(41, m.width.max(0.0));
            w.g(71, m.attach.clamp(1, 9));
            w.g(72, 1);
            let text = encode_text(&icad_core::fields::resolve(doc, &m.content)).replace('\n', "\\P");
            let chunks: Vec<String> = text.chars().collect::<Vec<_>>().chunks(250).map(|c| c.iter().collect()).collect();
            for (i, c) in chunks.iter().enumerate() {
                w.g(if i + 1 == chunks.len() { 1 } else { 3 }, c);
            }
            if chunks.is_empty() {
                w.g(1, "");
            }
            w.g(7, m.style.as_deref().unwrap_or("Standard"));
            w.pt(11, V2::polar(1.0, m.rotation));
            w.f(44, m.spacing);
            if m.content.contains("{{") {
                w.xdata(&e.shape);
            }
        }
        Shape::Hatch(h) if modern => write_hatch(w, doc, h, layer, owner, st),
        // A revision cloud is a closed polyline of bulged segments, as other
        // programs draw it; the extended data keeps it a cloud for ImagineCAD.
        Shape::RevCloud(c) if modern && c.pts.len() >= 2 => {
            let v = c.bulges();
            w.ent("LWPOLYLINE", layer, owner, st, "AcDbPolyline");
            w.g(90, v.len());
            w.g(70, 1);
            w.f(43, 0.0);
            for (p, b) in &v {
                w.f(10, p.x);
                w.f(20, p.y);
                w.f(42, *b);
            }
            w.xdata(&e.shape);
        }
        _ => {
            for p in doc.prims_of(e) {
                write_prim(w, &p, layer, owner, st);
            }
        }
    }
}

fn write_hatch(w: &mut W, doc: &Document, h: &Hatch, layer: &str, owner: u64, st: Style) {
    w.ent("HATCH", layer, owner, Style { lt: None, ..st }, "AcDbHatch");
    w.pt(10, V2::ZERO);
    w.f(210, 0.0);
    w.f(220, 0.0);
    w.f(230, 1.0);
    w.g(2, h.pattern.to_ascii_uppercase());
    w.g(70, if h.is_solid() { 1 } else { 0 });
    w.g(71, 0);
    let loops: Vec<&Vec<V2>> = h.loops.iter().filter(|l| l.len() >= 3).collect();
    w.g(91, loops.len());
    for (i, l) in loops.iter().enumerate() {
        w.g(92, if i == 0 { 3 } else { 2 });
        w.g(72, 0);
        w.g(73, 1);
        w.g(93, l.len());
        for p in l.iter() {
            w.f(10, p.x);
            w.f(20, p.y);
        }
        w.g(97, 0);
    }
    w.g(75, 0);
    w.g(76, 1);
    if !h.is_solid() {
        let scale = h.scale * doc.ascale();
        w.f(52, deg(h.angle));
        w.f(41, scale);
        w.g(77, 0);
        let fams = pattern_families(&h.pattern);
        w.g(78, fams.len());
        for f in fams {
            let a = rad(f.angle) + h.angle;
            let n = V2::polar(1.0, a).perp();
            let base = n * (f.shift * scale);
            let off = n * (f.spacing * scale);
            w.f(53, deg(a));
            w.f(43, base.x);
            w.f(44, base.y);
            w.f(45, off.x);
            w.f(46, off.y);
            w.g(79, 0);
        }
    }
    w.g(98, 0);
}

fn write_dimension(w: &mut W, doc: &Document, d: &Dimension, layer: &str, owner: u64, st: Style, n: usize) {
    let ds = &doc.dimstyle;
    let th = ds.text_height * doc.ascale();
    w.ent("DIMENSION", layer, owner, st, "AcDbDimension");
    w.g(2, format!("*D{n}"));
    let text = d.text.clone().unwrap_or_default();
    match d.kind {
        DimKind::Horizontal | DimKind::Vertical | DimKind::Aligned => {
            let (_, q2) = d.dim_line_points();
            let dir = d.direction();
            let mut ang = dir.angle();
            if ang > std::f64::consts::FRAC_PI_2 + 1e-6 || ang <= -std::f64::consts::FRAC_PI_2 + 1e-6 {
                ang += std::f64::consts::PI;
            }
            let up = V2::polar(1.0, ang + std::f64::consts::FRAC_PI_2);
            w.pt(10, q2);
            w.pt(11, d.loc + up * (ds.text_gap * doc.ascale() + th / 2.0));
            let ty = if d.kind == DimKind::Aligned { 1 } else { 0 };
            w.g(70, 32 | 128 | ty);
            w.g(1, encode_text(&text));
            if w.new2000() {
                w.g(3, "Standard");
                w.g(100, "AcDbAlignedDimension");
            }
            w.pt(13, d.p1);
            w.pt(14, d.p2);
            if d.kind != DimKind::Aligned {
                w.f(50, if d.kind == DimKind::Vertical { 90.0 } else { 0.0 });
                w.sub("AcDbRotatedDimension");
            }
        }
        DimKind::Radius | DimKind::Diameter => {
            let r = d.p1.dist(d.p2);
            let mut u = (d.loc - d.p1).norm();
            if u == V2::ZERO {
                u = (d.p2 - d.p1).norm();
            }
            let tip = d.p1 + u * r;
            if d.kind == DimKind::Diameter {
                w.pt(10, d.p1 - u * r);
            } else {
                w.pt(10, d.p1);
            }
            w.pt(11, d.loc);
            w.g(70, 32 | 128 | if d.kind == DimKind::Diameter { 3 } else { 4 });
            w.g(1, encode_text(&text));
            if w.new2000() {
                w.g(3, "Standard");
                w.g(100, if d.kind == DimKind::Diameter { "AcDbDiametricDimension" } else { "AcDbRadialDimension" });
            }
            w.pt(15, tip);
            w.f(40, (d.loc.dist(d.p1) - r).max(0.0));
        }
        DimKind::OrdinateX | DimKind::OrdinateY => {
            w.pt(10, d.center.unwrap_or(V2::ZERO));
            w.pt(11, d.loc);
            w.g(70, 32 | 128 | 6 | if d.kind == DimKind::OrdinateX { 64 } else { 0 });
            w.g(1, encode_text(&text));
            if w.new2000() {
                w.g(3, "Standard");
                w.g(100, "AcDbOrdinateDimension");
            }
            w.pt(13, d.p1);
            w.pt(14, d.loc);
        }
        // Written as a proxy (needs_proxy); reaching here means R12 or a block.
        DimKind::ArcLength => {}
        DimKind::Angular => {
            let c = d.center.unwrap_or(V2::ZERO);
            w.pt(10, d.loc);
            w.pt(11, d.loc);
            w.g(70, 32 | 128 | 5);
            w.g(1, encode_text(&text));
            if w.new2000() {
                w.g(3, "Standard");
                w.g(100, "AcDb3PointAngularDimension");
            }
            w.pt(13, d.p1);
            w.pt(14, d.p2);
            w.pt(15, c);
        }
    }
}

fn lt_entry(w: &mut W, name: &str, desc: &str, pat: &[f64], table: u64) {
    w.g(0, "LTYPE");
    if w.new2000() {
        let h = w.alloc();
        w.g(5, format!("{h:X}"));
        w.g(330, format!("{table:X}"));
        w.g(100, "AcDbSymbolTableRecord");
        w.g(100, "AcDbLinetypeTableRecord");
    }
    w.g(2, name);
    w.g(70, 0);
    w.g(3, desc);
    w.g(72, 65);
    w.g(73, pat.len());
    w.f(40, pat.iter().map(|x| x.abs()).sum::<f64>());
    for x in pat {
        w.f(49, *x);
        if w.new2000() {
            w.g(74, 0);
        }
    }
}

/// Conventional fixed handles of the symbol tables (what DWG-derived files use).
fn table_handle(name: &str) -> u64 {
    match name {
        "BLOCK_RECORD" => 0x1,
        "LAYER" => 0x2,
        "STYLE" => 0x3,
        "LTYPE" => 0x5,
        "VIEW" => 0x6,
        "UCS" => 0x7,
        "VPORT" => 0x8,
        "APPID" => 0x9,
        _ => 0xA, // DIMSTYLE
    }
}

fn table_start(w: &mut W, name: &str, count: usize) -> u64 {
    w.g(0, "TABLE");
    w.g(2, name);
    let h = if w.new2000() {
        let h = table_handle(name);
        w.g(5, format!("{h:X}"));
        w.g(330, 0);
        w.g(100, "AcDbSymbolTable");
        h
    } else {
        0
    };
    w.g(70, count);
    h
}

fn record(w: &mut W, kind: &str, table: u64, subclass: &str) -> u64 {
    w.g(0, kind);
    let h = if w.new2000() {
        let h = w.alloc();
        w.g(if kind == "DIMSTYLE" { 105 } else { 5 }, format!("{h:X}"));
        w.g(330, format!("{table:X}"));
        w.g(100, "AcDbSymbolTableRecord");
        w.g(100, subclass);
        h
    } else {
        0
    };
    h
}

/// Serialise a document to DXF text.
pub fn write_dxf(doc: &Document, opts: &DxfOptions) -> String {
    let mut w = W { s: String::with_capacity(128 * 1024), v: opts.version, next: 0x20 };
    let modern = w.new2000();
    let ds = &doc.dimstyle;
    let sc = doc.ascale();
    let ext = if opts.include_sheet && doc.sheet.visible { doc.extents().union(&doc.sheet.bounds()) } else { doc.extents() };
    let ext = if ext.is_empty() { BBox::from_pts(V2::ZERO, V2::new(100.0, 100.0)) } else { ext };

    // Layers of external references come with the referenced file.
    let mut layers: Vec<Layer> = doc.layers.iter().filter(|l| !doc.is_xref_dependent(&l.name)).cloned().collect();
    if opts.include_sheet && doc.sheet.visible && doc.layer("TITLEBLOCK").is_none() {
        layers.push(Layer::new("TITLEBLOCK", 7, LineType::Continuous, 0.35));
    }
    let dims: Vec<&Entity> = if opts.explode_dimensions {
        vec![]
    } else {
        doc.entities.iter().filter(|e| matches!(e.shape, Shape::Dim(_)) && !needs_proxy(&e.shape)).collect()
    };
    let proxies: Vec<&Entity> = if modern { doc.entities.iter().filter(|e| needs_proxy(&e.shape)).collect() } else { vec![] };

    // ---------------- TABLES (written first so handles exist; HEADER prepended later)
    w.g(0, "SECTION");
    w.g(2, "TABLES");
    if modern {
        let t = table_start(&mut w, "VPORT", 1);
        record(&mut w, "VPORT", t, "AcDbViewportTableRecord");
        w.g(2, "*ACTIVE");
        w.g(70, 0);
        w.f(10, 0.0);
        w.f(20, 0.0);
        w.f(11, 1.0);
        w.f(21, 1.0);
        w.pt(12, ext.center());
        w.f(40, ext.height().max(1.0) * 1.1);
        w.f(41, (ext.width() / ext.height().max(1e-9)).clamp(0.2, 5.0));
        w.g(0, "ENDTAB");
    }
    let ltypes: Vec<LineType> = LineType::ALL.to_vec();
    let t = table_start(&mut w, "LTYPE", ltypes.len() + if modern { 2 } else { 0 });
    if modern {
        lt_entry(&mut w, "ByBlock", "", &[], t);
        lt_entry(&mut w, "ByLayer", "", &[], t);
    }
    for lt in ltypes {
        lt_entry(&mut w, lt.name(), lt.description(), lt.pattern(), t);
    }
    w.g(0, "ENDTAB");

    let t = table_start(&mut w, "LAYER", layers.len());
    for l in &layers {
        record(&mut w, "LAYER", t, "AcDbLayerTableRecord");
        w.g(2, &l.name);
        w.g(70, if l.locked { 4 } else { 0 });
        let c = l.color.max(1) as i32;
        w.g(62, if l.visible { c } else { -c });
        w.g(6, l.linetype.name());
        if modern {
            w.g(290, if l.plot { 1 } else { 0 });
            w.g(370, (l.lineweight * 100.0).round() as i32);
        }
    }
    w.g(0, "ENDTAB");

    let mut styles = doc.text_styles.clone();
    if !styles.iter().any(|s| s.name.eq_ignore_ascii_case("Standard")) {
        styles.insert(0, icad_core::fonts::TextStyle::standard());
    }
    let t = table_start(&mut w, "STYLE", styles.len());
    for st in &styles {
        record(&mut w, "STYLE", t, "AcDbTextStyleTableRecord");
        let name = if !modern && st.name.eq_ignore_ascii_case("Standard") { "STANDARD".to_string() } else { st.name.clone() };
        w.g(2, name);
        w.g(70, 0);
        w.f(40, st.height);
        w.f(41, st.width_factor);
        w.f(50, st.oblique);
        w.g(71, 0);
        w.f(42, 2.5);
        // Other programs find the font by its file name.
        let file = st.file.rsplit_once('#').map(|x| x.0).unwrap_or(&st.file);
        let base = std::path::Path::new(file).file_name().and_then(|f| f.to_str()).unwrap_or("");
        w.g(3, if base.is_empty() { "txt" } else { base });
        w.g(4, "");
    }
    w.g(0, "ENDTAB");

    let mut msp_br = 0;
    let mut psp_br = 0;
    let mut block_brs: Vec<(String, u64)> = vec![];
    if modern {
        for name in ["VIEW", "UCS"] {
            table_start(&mut w, name, 0);
            w.g(0, "ENDTAB");
        }
        let t = table_start(&mut w, "APPID", 2);
        for app in ["ACAD", XDATA_APP] {
            record(&mut w, "APPID", t, "AcDbRegAppTableRecord");
            w.g(2, app);
            w.g(70, 0);
        }
        w.g(0, "ENDTAB");

        let t = table_start(&mut w, "DIMSTYLE", 1);
        w.g(100, "AcDbDimStyleTable");
        record(&mut w, "DIMSTYLE", t, "AcDbDimStyleTableRecord");
        w.g(2, "Standard");
        w.g(70, 0);
        w.f(40, sc);
        w.f(41, ds.arrow_size);
        w.f(42, ds.ext_offset);
        w.f(44, ds.ext_beyond);
        w.f(140, ds.text_height);
        w.f(147, ds.text_gap);
        w.g(271, ds.decimals);
        w.g(0, "ENDTAB");

        let names: Vec<String> = ["*Model_Space".to_string(), "*Paper_Space".to_string()]
            .into_iter()
            .chain((1..=dims.len()).map(|i| format!("*D{i}")))
            .chain((1..=proxies.len()).map(|i| format!("*U{i}")))
            .chain(doc.blocks.iter().filter(|b| !doc.is_xref_dependent(&b.name)).map(|b| b.name.clone()))
            .collect();
        let t = table_start(&mut w, "BLOCK_RECORD", names.len());
        for n in names {
            let h = record(&mut w, "BLOCK_RECORD", t, "AcDbBlockTableRecord");
            w.g(2, &n);
            match n.as_str() {
                "*Model_Space" => msp_br = h,
                "*Paper_Space" => psp_br = h,
                _ => block_brs.push((n, h)),
            }
        }
        w.g(0, "ENDTAB");
    }
    w.g(0, "ENDSEC");

    // ---------------- BLOCKS
    w.g(0, "SECTION");
    w.g(2, "BLOCKS");
    let br_of = |name: &str, brs: &[(String, u64)]| brs.iter().find(|(n, _)| n == name).map(|(_, h)| *h).unwrap_or(0);
    let block_begin = |w: &mut W, name: &str, br: u64, base: V2, flags: i32, paper: bool, path: &str| {
        w.g(0, "BLOCK");
        if w.new2000() {
            let h = w.alloc();
            w.g(5, format!("{h:X}"));
            w.g(330, format!("{br:X}"));
            w.g(100, "AcDbEntity");
            if paper {
                w.g(67, 1);
            }
        }
        w.g(8, "0");
        w.sub("AcDbBlockBegin");
        w.g(2, name);
        w.g(70, flags);
        w.pt(10, base);
        w.g(3, name);
        w.g(1, path);
    };
    let block_end = |w: &mut W, br: u64, paper: bool| {
        w.g(0, "ENDBLK");
        if w.new2000() {
            let h = w.alloc();
            w.g(5, format!("{h:X}"));
            w.g(330, format!("{br:X}"));
            w.g(100, "AcDbEntity");
            if paper {
                w.g(67, 1);
            }
        }
        w.g(8, "0");
        w.sub("AcDbBlockEnd");
    };
    if modern {
        block_begin(&mut w, "*Model_Space", msp_br, V2::ZERO, 0, false, "");
        block_end(&mut w, msp_br, false);
        block_begin(&mut w, "*Paper_Space", psp_br, V2::ZERO, 0, true, "");
        block_end(&mut w, psp_br, true);
    }
    for (i, e) in dims.iter().enumerate() {
        let name = format!("*D{}", i + 1);
        let br = br_of(&name, &block_brs);
        block_begin(&mut w, &name, br, V2::ZERO, 1, false, "");
        for p in doc.prims_of(e) {
            write_prim(&mut w, &p, "0", br, Style { color: Some(0), lt: None, lw: None });
        }
        block_end(&mut w, br, false);
    }
    for (i, e) in proxies.iter().enumerate() {
        let name = format!("*U{}", i + 1);
        let br = br_of(&name, &block_brs);
        block_begin(&mut w, &name, br, V2::ZERO, 1, false, "");
        for p in doc.prims_of(e) {
            write_prim(&mut w, &p, "0", br, Style { color: Some(0), lt: None, lw: None });
        }
        block_end(&mut w, br, false);
    }
    let mut nested = Counters::default();
    for b in doc.blocks.iter().filter(|b| !doc.is_xref_dependent(&b.name)) {
        let br = br_of(&b.name, &block_brs);
        // External references are written empty with their file (flag 4).
        match &b.xref {
            Some(path) => block_begin(&mut w, &b.name, br, b.base, 4, false, path),
            None => {
                block_begin(&mut w, &b.name, br, b.base, 0, false, "");
                for e in &b.entities {
                    write_entity(&mut w, doc, e, br, true, &mut nested);
                }
            }
        }
        block_end(&mut w, br, false);
    }
    w.g(0, "ENDSEC");

    // ---------------- ENTITIES
    w.g(0, "SECTION");
    w.g(2, "ENTITIES");
    let mut counters = Counters::default();
    // Raster images (DXF 2000): IMAGE entities pointing at IMAGEDEF objects.
    struct ImgDef {
        path: String,
        handle: u64,
        px: (u32, u32),
        reactors: Vec<(u64, u64)>,
    }
    let mut img_defs: Vec<ImgDef> = vec![];
    let mut handles: Vec<(u64, u64)> = vec![];
    for e in &doc.entities {
        if let Shape::Image(img) = &e.shape {
            if !modern {
                continue;
            }
            let px = image::image_dimensions(&img.path).unwrap_or((1000, 1000));
            let di = match img_defs.iter().position(|d| d.path == img.path) {
                Some(i) => i,
                None => {
                    let handle = w.alloc();
                    img_defs.push(ImgDef { path: img.path.clone(), handle, px, reactors: vec![] });
                    img_defs.len() - 1
                }
            };
            let reactor = w.alloc();
            let ih = w.next;
            let def = &mut img_defs[di];
            def.reactors.push((reactor, ih));
            w.ent("IMAGE", &e.layer, msp_br, style_of(e), "AcDbRasterImage");
            w.g(90, 0);
            w.pt(10, img.pos);
            w.pt(11, V2::new(img.w / def.px.0 as f64, 0.0));
            w.pt(12, V2::new(0.0, img.h / def.px.1 as f64));
            w.f(13, def.px.0 as f64);
            w.f(23, def.px.1 as f64);
            w.g(340, format!("{:X}", def.handle));
            w.g(70, 7);
            w.g(280, 0);
            w.g(281, 50);
            w.g(282, 50);
            w.g(283, img.fade);
            w.g(360, format!("{reactor:X}"));
            w.g(71, 1);
            w.g(91, 2);
            w.f(14, -0.5);
            w.f(24, -0.5);
            w.f(14, def.px.0 as f64 - 0.5);
            w.f(24, def.px.1 as f64 - 0.5);
            continue;
        }
        // The handle this entity gets, for constraints that refer to it.
        handles.push((e.id, w.next));
        write_entity(&mut w, doc, e, msp_br, opts.explode_dimensions, &mut counters);
    }
    if opts.include_sheet && doc.sheet.visible {
        for (p, lw) in doc.sheet_prims() {
            let st = Style { lw: Some((lw * 100.0).round() as i32), ..Style::default() };
            write_prim(&mut w, &p, "TITLEBLOCK", msp_br, st);
        }
    }
    w.g(0, "ENDSEC");

    // ---------------- OBJECTS (DXF 2000)
    if modern {
        let (root, group) = (0xC, 0xD);
        w.g(0, "SECTION");
        w.g(2, "OBJECTS");
        w.g(0, "DICTIONARY");
        w.g(5, format!("{root:X}"));
        w.g(330, 0);
        w.g(100, "AcDbDictionary");
        w.g(281, 1);
        w.g(3, "ACAD_GROUP");
        w.g(350, format!("{group:X}"));
        let img_dict = if img_defs.is_empty() { 0 } else { w.alloc() };
        if img_dict != 0 {
            w.g(3, "ACAD_IMAGE_DICT");
            w.g(350, format!("{img_dict:X}"));
        }
        // Constraints: an XRECORD of JSON with entities named by their handles.
        let constraints = constraints_json(doc, &handles);
        let cons_rec = if constraints.is_some() { w.alloc() } else { 0 };
        if cons_rec != 0 {
            w.g(3, CONSTRAINTS_KEY);
            w.g(350, format!("{cons_rec:X}"));
        }
        w.g(0, "DICTIONARY");
        w.g(5, format!("{group:X}"));
        w.g(330, format!("{root:X}"));
        w.g(100, "AcDbDictionary");
        w.g(281, 1);
        if let Some(json) = &constraints {
            w.g(0, "XRECORD");
            w.g(5, format!("{cons_rec:X}"));
            w.g(330, format!("{root:X}"));
            w.g(100, "AcDbXrecord");
            w.g(280, 1);
            w.g(1, CONSTRAINTS_KEY);
            for chunk in json.as_bytes().chunks(240) {
                w.g(1, std::str::from_utf8(chunk).unwrap_or(""));
            }
        }
        if img_dict != 0 {
            w.g(0, "DICTIONARY");
            w.g(5, format!("{img_dict:X}"));
            w.g(330, format!("{root:X}"));
            w.g(100, "AcDbDictionary");
            w.g(281, 1);
            for (i, d) in img_defs.iter().enumerate() {
                let stem = std::path::Path::new(&d.path).file_stem().and_then(|s| s.to_str()).unwrap_or("image");
                w.g(3, format!("{stem}_{}", i + 1));
                w.g(350, format!("{:X}", d.handle));
            }
            for d in &img_defs {
                w.g(0, "IMAGEDEF");
                w.g(5, format!("{:X}", d.handle));
                w.g(102, "{ACAD_REACTORS");
                w.g(330, format!("{img_dict:X}"));
                for (r, _) in &d.reactors {
                    w.g(330, format!("{r:X}"));
                }
                w.g(102, "}");
                w.g(330, format!("{img_dict:X}"));
                w.g(100, "AcDbRasterImageDef");
                w.g(90, 0);
                w.g(1, &d.path);
                w.f(10, d.px.0 as f64);
                w.f(20, d.px.1 as f64);
                w.f(11, 1.0);
                w.f(21, 1.0);
                w.g(280, 1);
                w.g(281, 0);
                for (r, image) in &d.reactors {
                    w.g(0, "IMAGEDEF_REACTOR");
                    w.g(5, format!("{r:X}"));
                    w.g(330, format!("{image:X}"));
                    w.g(100, "AcDbRasterImageDefReactor");
                    w.g(90, 2);
                    w.g(330, format!("{image:X}"));
                }
            }
        }
        w.g(0, "ENDSEC");
    }
    w.g(0, "EOF");

    // ---------------- HEADER (+ CLASSES) prepended.
    let mut h = W { s: String::new(), v: opts.version, next: 0 };
    h.g(0, "SECTION");
    h.g(2, "HEADER");
    h.g(9, "$ACADVER");
    h.g(1, if modern { "AC1015" } else { "AC1009" });
    if modern {
        h.g(9, "$HANDSEED");
        h.g(5, format!("{:X}", w.next + 1));
        h.g(9, "$INSUNITS");
        h.g(70, 4);
        h.g(9, "$MEASUREMENT");
        h.g(70, 1);
        h.g(9, "$LWDISPLAY");
        h.g(290, 1);
    }
    h.g(9, "$EXTMIN");
    h.pt(10, ext.min);
    h.g(9, "$EXTMAX");
    h.pt(10, ext.max);
    h.g(9, "$LIMMIN");
    h.f(10, ext.min.x);
    h.f(20, ext.min.y);
    h.g(9, "$LIMMAX");
    h.f(10, ext.max.x);
    h.f(20, ext.max.y);
    h.g(9, "$LTSCALE");
    h.f(40, doc.ltscale * sc);
    h.g(9, "$DIMSCALE");
    h.f(40, sc);
    h.g(9, "$DIMTXT");
    h.f(40, ds.text_height);
    h.g(9, "$DIMASZ");
    h.f(40, ds.arrow_size);
    h.g(9, "$DIMEXO");
    h.f(40, ds.ext_offset);
    h.g(9, "$DIMEXE");
    h.f(40, ds.ext_beyond);
    h.g(9, "$DIMGAP");
    h.f(40, ds.text_gap);
    h.g(9, "$DIMDEC");
    h.g(70, ds.decimals);
    h.g(9, "$CLAYER");
    h.g(8, &doc.current_layer);
    h.g(0, "ENDSEC");
    // Raster image classes (only when there are images: an empty CLASSES
    // section upsets some readers).
    if modern && !img_defs.is_empty() {
        h.g(0, "SECTION");
        h.g(2, "CLASSES");
        for (name, cpp, flags, is_entity) in [
            ("IMAGE", "AcDbRasterImage", 127, 1),
            ("IMAGEDEF", "AcDbRasterImageDef", 0, 0),
            ("IMAGEDEF_REACTOR", "AcDbRasterImageDefReactor", 1, 0),
        ] {
            h.g(0, "CLASS");
            h.g(1, name);
            h.g(2, cpp);
            h.g(3, "ISM");
            h.g(90, flags);
            h.g(280, 0);
            h.g(281, is_entity);
        }
        h.g(0, "ENDSEC");
    }
    h.s + &w.s
}

// ---------------------------------------------------------------- reading

struct Pair {
    code: i32,
    val: String,
}

fn pairs(src: &str) -> Vec<Pair> {
    let mut lines = src.lines();
    let mut out = Vec::new();
    while let (Some(c), Some(v)) = (lines.next(), lines.next()) {
        if let Ok(code) = c.trim().parse::<i32>() {
            out.push(Pair { code, val: v.trim_end_matches('\r').to_string() });
        }
    }
    out
}

/// Collected group codes of one record.
#[derive(Default, Clone)]
struct Rec {
    kind: String,
    codes: Vec<(i32, String)>,
}

impl Rec {
    fn s(&self, c: i32) -> Option<&str> {
        self.codes.iter().find(|(k, _)| *k == c).map(|(_, v)| v.trim())
    }
    fn f(&self, c: i32) -> f64 {
        self.s(c).and_then(|v| v.parse().ok()).unwrap_or(0.0)
    }
    fn f_or(&self, c: i32, d: f64) -> f64 {
        self.s(c).and_then(|v| v.parse().ok()).unwrap_or(d)
    }
    fn i(&self, c: i32) -> i64 {
        self.s(c).and_then(|v| v.parse().ok()).unwrap_or(0)
    }
    fn has(&self, c: i32) -> bool {
        self.s(c).is_some()
    }
    fn p(&self, base: i32) -> V2 {
        V2::new(self.f(base), self.f(base + 10))
    }
    fn all(&self, c: i32) -> Vec<f64> {
        self.codes.iter().filter(|(k, _)| *k == c).filter_map(|(_, v)| v.trim().parse().ok()).collect()
    }
}

/// Sequential reader over a record's group codes (for HATCH boundaries etc.).
struct Cur<'a> {
    c: &'a [(i32, String)],
    i: usize,
}

impl Cur<'_> {
    /// Advance to the next occurrence of `code` and return its value.
    fn next(&mut self, code: i32) -> Option<f64> {
        while self.i < self.c.len() {
            let (k, v) = &self.c[self.i];
            self.i += 1;
            if *k == code {
                return v.trim().parse().ok();
            }
        }
        None
    }
    /// Read `code` only if it is the very next pair.
    fn peek_take(&mut self, code: i32) -> Option<f64> {
        if self.i < self.c.len() && self.c[self.i].0 == code {
            self.i += 1;
            return self.c[self.i - 1].1.trim().parse().ok();
        }
        None
    }
}

/// Polyline vertices with bulges, tessellated.
fn bulge_points(verts: &[(V2, f64)], closed: bool) -> Vec<V2> {
    let mut out = vec![];
    let n = verts.len();
    for i in 0..n {
        let (p, b) = verts[i];
        out.push(p);
        let last = i + 1 == n;
        if last && !closed {
            break;
        }
        let q = verts[(i + 1) % n].0;
        if b.abs() > 1e-9 {
            let theta = 4.0 * b.atan();
            let l = p.dist(q);
            let m = p.mid(q);
            let nrm = (q - p).norm().perp();
            let c = m + nrm * (l / 2.0 / (theta / 2.0).tan());
            let a0 = (p - c).angle();
            let r = c.dist(p);
            let steps = ((theta.abs() / 0.1).ceil() as usize).clamp(2, 128);
            for k in 1..steps {
                out.push(c + V2::polar(r, a0 + theta * k as f64 / steps as f64));
            }
        }
    }
    out
}

fn read_hatch(r: &Rec) -> Option<Shape> {
    let mut cur = Cur { c: &r.codes, i: 0 };
    let n = cur.next(91)? as usize;
    let mut loops = vec![];
    for _ in 0..n.min(10_000) {
        let flags = cur.next(92)? as i64;
        let mut pts: Vec<V2> = vec![];
        if flags & 2 != 0 {
            let bulge = cur.next(72)? != 0.0;
            let closed = cur.next(73).unwrap_or(1.0) != 0.0;
            let nv = cur.next(93)? as usize;
            let mut verts = vec![];
            for _ in 0..nv {
                let x = cur.next(10)?;
                let y = cur.next(20)?;
                let b = if bulge { cur.peek_take(42).unwrap_or(0.0) } else { 0.0 };
                verts.push((V2::new(x, y), b));
            }
            pts = bulge_points(&verts, closed);
        } else {
            let ne = cur.next(93)? as usize;
            for _ in 0..ne {
                let ty = cur.next(72)? as i64;
                match ty {
                    1 => {
                        let a = V2::new(cur.next(10)?, cur.next(20)?);
                        let b = V2::new(cur.next(11)?, cur.next(21)?);
                        if pts.last().is_none_or(|l| l.dist(a) > 1e-9) {
                            pts.push(a);
                        }
                        pts.push(b);
                    }
                    2 => {
                        let c = V2::new(cur.next(10)?, cur.next(20)?);
                        let rr = cur.next(40)?;
                        let (s, e) = (rad(cur.next(50)?), rad(cur.next(51)?));
                        let ccw = cur.next(73).unwrap_or(1.0) != 0.0;
                        let arc = if ccw { arc_points(c, rr, s, e, rr * 0.002) } else { arc_points(c, rr, -s, -e, rr * 0.002) };
                        let arc: Vec<V2> = if ccw { arc } else { arc.into_iter().map(|p| V2::new(p.x, 2.0 * c.y - p.y)).collect() };
                        pts.extend(arc);
                    }
                    3 => {
                        let c = V2::new(cur.next(10)?, cur.next(20)?);
                        let major = V2::new(cur.next(11)?, cur.next(21)?);
                        let ratio = cur.next(40)?;
                        let (s, e) = (rad(cur.next(50)?), rad(cur.next(51)?));
                        let ccw = cur.next(73).unwrap_or(1.0) != 0.0;
                        let mut ps = icad_core::curves::ellipse_points(c, major, ratio, s, e, major.len() * 0.002);
                        if !ccw {
                            ps.reverse();
                        }
                        pts.extend(ps);
                    }
                    4 => {
                        let degree = cur.next(94)? as usize;
                        let nk = {
                            let _ = cur.next(73);
                            let _ = cur.next(74);
                            cur.next(95)? as usize
                        };
                        let nc = cur.next(96)? as usize;
                        let knots: Vec<f64> = (0..nk).filter_map(|_| cur.next(40)).collect();
                        let ctrl: Vec<V2> = (0..nc).filter_map(|_| Some(V2::new(cur.next(10)?, cur.next(20)?))).collect();
                        if ctrl.len() >= 2 && knots.len() >= ctrl.len() {
                            let (t0, t1) = (knots[degree.min(knots.len() - 1)], knots[knots.len() - 1 - degree.min(knots.len() - 1)]);
                            for k in 0..=(nc * 8) {
                                let t = t0 + (t1 - t0) * k as f64 / (nc * 8) as f64;
                                pts.push(eval_bspline(&ctrl, &knots, degree, t));
                            }
                        }
                    }
                    _ => {}
                }
            }
        }
        // Skip source-object references.
        if let Some(ns) = cur.next(97) {
            for _ in 0..ns as usize {
                let _ = cur.peek_take(330);
            }
        }
        if pts.len() >= 3 {
            loops.push(pts);
        }
    }
    if loops.is_empty() {
        return None;
    }
    let pattern = r.s(2).unwrap_or("SOLID").to_string();
    let solid = r.i(70) & 1 != 0;
    Some(Shape::Hatch(Hatch {
        loops,
        pattern: if solid { "SOLID".into() } else { pattern },
        scale: r.f_or(41, 1.0).max(1e-6),
        angle: rad(r.f(52)),
    }))
}

fn read_shape(recs: &[Rec], k: &mut usize, blocks: &[String], warn: &mut std::collections::BTreeSet<String>) -> Option<Shape> {
    let r = &recs[*k];
    if let Some(s) = xdata_shape(r) {
        return Some(s);
    }
    match r.kind.as_str() {
        "LINE" => Some(Shape::Line { a: r.p(10), b: r.p(11) }),
        "CIRCLE" => Some(Shape::Circle { c: r.p(10), r: r.f(40) }),
        "ARC" => Some(Shape::Arc { c: r.p(10), r: r.f(40), start: norm_angle(rad(r.f(50))), end: norm_angle(rad(r.f(51))) }),
        "LWPOLYLINE" => {
            let mut verts: Vec<(V2, f64)> = vec![];
            let mut x = None;
            for (c, v) in &r.codes {
                let val: f64 = v.trim().parse().unwrap_or(0.0);
                match *c {
                    10 => x = Some(val),
                    20 => {
                        if let Some(xx) = x.take() {
                            verts.push((V2::new(xx, val), 0.0));
                        }
                    }
                    42 => {
                        if let Some(l) = verts.last_mut() {
                            l.1 = val;
                        }
                    }
                    _ => {}
                }
            }
            let closed = r.i(70) & 1 != 0;
            let pts =
                if verts.iter().any(|v| v.1.abs() > 1e-9) { bulge_points(&verts, closed) } else { verts.iter().map(|v| v.0).collect() };
            Some(Shape::Polyline { pts, closed })
        }
        "POLYLINE" => {
            let closed = r.i(70) & 1 != 0;
            let mut verts = vec![];
            let mut j = *k + 1;
            while j < recs.len() && recs[j].kind == "VERTEX" {
                verts.push((recs[j].p(10), recs[j].f(42)));
                j += 1;
            }
            if j < recs.len() && recs[j].kind == "SEQEND" {
                j += 1;
            }
            *k = j - 1;
            Some(Shape::Polyline { pts: bulge_points(&verts, closed), closed })
        }
        "TEXT" | "ATTRIB" => Some(Shape::Text(read_text(r))),
        "ATTDEF" => Some(Shape::AttDef(AttDef {
            tag: r.s(2).unwrap_or("").to_string(),
            prompt: decode_text(r.s(3).unwrap_or("")),
            default: decode_text(r.s(1).unwrap_or("")),
            text: read_text(r),
            invisible: r.i(70) & 1 != 0,
        })),
        "MTEXT" => {
            let mut content: String = r.codes.iter().filter(|(c, _)| *c == 3).map(|(_, v)| v.as_str()).collect::<Vec<_>>().join("");
            content.push_str(r.s(1).unwrap_or(""));
            let content = strip_mtext(&decode_text(&content));
            let rot = if r.has(11) { r.p(11).angle() } else { r.f(50) };
            Some(Shape::MText(MText {
                pos: r.p(10),
                height: r.f(40).max(1e-6),
                width: r.f(41).max(0.0),
                rotation: norm_angle(rot),
                content,
                attach: r.i(71).clamp(1, 9) as u8,
                spacing: r.f_or(44, 1.0),
                style: r.s(7).filter(|s| !s.is_empty()).map(String::from),
            }))
        }
        "DIMENSION" => read_dimension(r),
        "ELLIPSE" => Some(Shape::Ellipse {
            c: r.p(10),
            major: r.p(11),
            ratio: r.f_or(40, 1.0).clamp(1e-6, 1.0),
            start: r.f_or(41, 0.0),
            end: r.f_or(42, std::f64::consts::TAU),
        }),
        "SPLINE" => {
            let closed = r.i(70) & 1 != 0;
            let fx = r.all(11);
            let fy = r.all(21);
            let pts: Vec<V2> = if fx.len() >= 2 {
                fx.iter().zip(fy.iter()).map(|(x, y)| V2::new(*x, *y)).collect()
            } else {
                let ctrl: Vec<V2> = r.all(10).iter().zip(r.all(20).iter()).map(|(x, y)| V2::new(*x, *y)).collect();
                let knots = r.all(40);
                let degree = r.i(71).max(1) as usize;
                if ctrl.len() < 2 || knots.len() < ctrl.len() {
                    return None;
                }
                let (t0, t1) = (knots[degree.min(knots.len() - 1)], knots[knots.len() - 1 - degree.min(knots.len() - 1)]);
                let n = (ctrl.len() * 3).clamp(4, 400);
                (0..=n).map(|i| eval_bspline(&ctrl, &knots, degree, t0 + (t1 - t0) * i as f64 / n as f64)).collect()
            };
            Some(Shape::Spline { pts, closed })
        }
        "POINT" => Some(Shape::Point { p: r.p(10) }),
        "IMAGE" => {
            let path = r.s(1)?.to_string();
            let (u, v) = (r.p(11), r.p(12));
            let (nx, ny) = (r.f(13), r.f(23));
            Some(Shape::Image(ImageRef { path, pos: r.p(10), w: u.len() * nx, h: v.len() * ny, fade: r.i(283).clamp(0, 100) as u8 }))
        }
        "TOLERANCE" => Some(Shape::Tolerance(icad_core::symbols::Fcf::from_dxf(r.p(10), r.s(1).unwrap_or("")))),
        "XLINE" => Some(Shape::XLine { p: r.p(10), dir: r.p(11).norm() }),
        "RAY" => Some(Shape::Ray { p: r.p(10), dir: r.p(11).norm() }),
        "HATCH" => read_hatch(r),
        "LEADER" => {
            let pts: Vec<V2> = r.all(10).iter().zip(r.all(20).iter()).map(|(x, y)| V2::new(*x, *y)).collect();
            (pts.len() >= 2).then_some(Shape::Leader(Leader { pts, text: String::new(), style: None }))
        }
        "SOLID" | "TRACE" => {
            let (a, b, c, d) = (r.p(10), r.p(11), r.p(12), r.p(13));
            let mut pts = vec![a, b, d, c];
            pts.dedup_by(|x, y| x.dist(*y) < 1e-12);
            (pts.len() >= 3).then_some(Shape::Hatch(Hatch { loops: vec![pts], pattern: "SOLID".into(), scale: 1.0, angle: 0.0 }))
        }
        "INSERT" => {
            let name = r.s(2).unwrap_or("").to_string();
            if !blocks.iter().any(|b| b.eq_ignore_ascii_case(&name)) {
                warn.insert(format!("INSERT of missing block {name}"));
                return None;
            }
            // Attribute values follow the INSERT up to SEQEND.
            let mut attribs = vec![];
            if r.i(66) == 1 {
                let mut j = *k + 1;
                while j < recs.len() && recs[j].kind == "ATTRIB" {
                    attribs.push(Attrib { tag: recs[j].s(2).unwrap_or("").to_string(), value: decode_text(recs[j].s(1).unwrap_or("")) });
                    j += 1;
                }
                if j < recs.len() && recs[j].kind == "SEQEND" {
                    j += 1;
                }
                *k = j - 1;
            }
            Some(Shape::Insert(Insert {
                name,
                pos: r.p(10),
                sx: r.f_or(41, 1.0),
                sy: r.f_or(42, 1.0),
                rotation: norm_angle(rad(r.f(50))),
                attribs,
            }))
        }
        "VERTEX" | "SEQEND" | "VIEWPORT" => None,
        other => {
            warn.insert(other.to_string());
            None
        }
    }
}

/// TEXT, ATTRIB and ATTDEF share the text groups; attributes keep the
/// vertical alignment in 74 (73 is their field length).
fn read_text(r: &Rec) -> Text {
    let h = match r.i(72) {
        1 | 4 => HAlign::Center,
        2 => HAlign::Right,
        _ => HAlign::Left,
    };
    let v = match r.i(if r.kind == "TEXT" { 73 } else { 74 }) {
        1 => VAlign::Bottom,
        2 => VAlign::Middle,
        3 => VAlign::Top,
        _ => VAlign::Baseline,
    };
    let pos = if (h != HAlign::Left || v != VAlign::Baseline) && r.has(11) { r.p(11) } else { r.p(10) };
    let mut t = Text::new(pos, r.f(40).max(1e-6), decode_text(r.s(1).unwrap_or(""))).aligned(h, v).rotated(norm_angle(rad(r.f(50))));
    t.style = r.s(7).filter(|s| !s.is_empty()).map(String::from);
    t
}

fn apply_props(e: &mut Entity, r: &Rec) {
    let c = r.i(62);
    if (1..=255).contains(&c) {
        e.color = Color::Aci(c as u8);
    }
    if let Some(lt) = r.s(6) {
        if !lt.eq_ignore_ascii_case("BYLAYER") {
            if let Some(l) = LineType::from_name(lt) {
                e.linetype = LtRef::Lt(l);
            }
        }
    }
    let lw = r.i(370);
    if lw > 0 {
        e.lineweight = Some(lw as f64 / 100.0);
    }
}

/// Parse a DXF file (R12 through current versions) into a new document.
pub fn read_dxf(src: &str) -> Result<Document, String> {
    let ps = pairs(src);
    if ps.is_empty() {
        return Err("Not a DXF file".into());
    }
    let mut doc = Document::new();
    let mut warn = std::collections::BTreeSet::new();

    let mut i = 0;
    let mut section = String::new();
    let mut table_layers: Vec<Layer> = Vec::new();
    let mut table_styles: Vec<icad_core::fonts::TextStyle> = Vec::new();
    let mut image_files: Vec<(String, String)> = Vec::new();
    let mut xref_paths: Vec<(String, String)> = Vec::new();
    let mut insert_xrefs: Vec<(String, String)> = Vec::new();
    let mut recs: Vec<Rec> = Vec::new();
    let mut block_recs: Vec<(String, V2, Vec<Rec>)> = Vec::new();
    let mut vars: Vec<(String, String)> = Vec::new();
    let mut constraints_json = String::new();
    let mut handle_ids: Vec<(String, u64)> = Vec::new();
    while i < ps.len() {
        let p = &ps[i];
        if section == "HEADER" && p.code == 9 {
            if let Some(v) = ps.get(i + 1) {
                vars.push((p.val.clone(), v.val.clone()));
            }
            i += 2;
            continue;
        }
        if p.code == 0 && p.val == "SECTION" {
            section = ps.get(i + 1).map(|x| x.val.clone()).unwrap_or_default();
            i += 2;
            continue;
        }
        if p.code == 0 && p.val == "ENDSEC" {
            section.clear();
            i += 1;
            continue;
        }
        if p.code == 0 && matches!(section.as_str(), "ENTITIES" | "TABLES" | "BLOCKS" | "OBJECTS") {
            let mut r = Rec { kind: p.val.clone(), codes: vec![] };
            i += 1;
            while i < ps.len() && ps[i].code != 0 {
                r.codes.push((ps[i].code, ps[i].val.clone()));
                i += 1;
            }
            match section.as_str() {
                "TABLES" if r.kind == "STYLE" => {
                    if let Some(name) = r.s(2).filter(|n| !n.is_empty()) {
                        let file = r.s(3).unwrap_or("").to_string();
                        // Shape (.shx) fonts are drawn with the standard font.
                        let ttf = file.to_ascii_lowercase().ends_with(".ttf") || file.to_ascii_lowercase().ends_with(".otf");
                        let stem = std::path::Path::new(&file).file_stem().and_then(|s| s.to_str()).unwrap_or("").to_string();
                        table_styles.push(icad_core::fonts::TextStyle {
                            name: name.to_string(),
                            file: if ttf { file } else { String::new() },
                            font: if ttf { stem } else { String::new() },
                            width_factor: r.f_or(41, 1.0),
                            oblique: r.f(50),
                            height: r.f(40),
                        });
                    }
                }
                "TABLES" if r.kind == "LAYER" => {
                    if let Some(name) = r.s(2) {
                        let c = r.i(62);
                        let mut l = Layer::new(
                            name,
                            (c.unsigned_abs().clamp(1, 255)) as u8,
                            r.s(6).and_then(LineType::from_name).unwrap_or_default(),
                            0.25,
                        );
                        let lw = r.i(370);
                        if lw > 0 {
                            l.lineweight = lw as f64 / 100.0;
                        }
                        l.visible = c >= 0;
                        l.locked = r.i(70) & 4 != 0;
                        if r.has(290) {
                            l.plot = r.i(290) != 0;
                        }
                        table_layers.push(l);
                    }
                }
                "BLOCKS" => match r.kind.as_str() {
                    "BLOCK" => {
                        let name = r.s(2).unwrap_or("").to_string();
                        if r.i(70) & 4 != 0 {
                            xref_paths.push((name.clone(), r.s(1).unwrap_or("").to_string()));
                        }
                        block_recs.push((name, r.p(10), vec![]))
                    }
                    "ENDBLK" => {}
                    _ => {
                        if let Some(b) = block_recs.last_mut() {
                            b.2.push(r);
                        }
                    }
                },
                "ENTITIES" => recs.push(r),
                "OBJECTS" if r.kind == "XRECORD" && r.codes.iter().find(|c| c.0 == 1).is_some_and(|c| c.1.trim() == CONSTRAINTS_KEY) => {
                    constraints_json = r.codes.iter().filter(|c| c.0 == 1).skip(1).map(|c| c.1.as_str()).collect();
                }
                "OBJECTS" if r.kind == "IMAGEDEF" => {
                    if let (Some(h), Some(path)) = (r.s(5), r.s(1)) {
                        image_files.push((h.to_ascii_uppercase(), path.to_string()));
                    }
                }
                _ => {}
            }
            continue;
        }
        i += 1;
    }

    for st in table_styles {
        match doc.text_styles.iter_mut().find(|s| s.name.eq_ignore_ascii_case(&st.name)) {
            Some(s) => *s = st,
            None => doc.text_styles.push(st),
        }
    }
    for l in table_layers {
        if let Some(ex) = doc.layer_mut(&l.name) {
            *ex = l;
        } else {
            doc.layers.push(l);
        }
    }

    // Named (non-anonymous) blocks become block definitions.
    let block_names: Vec<String> = block_recs.iter().filter(|b| !b.0.starts_with('*') && !b.0.is_empty()).map(|b| b.0.clone()).collect();
    for (name, base, brecs) in &block_recs {
        if name.starts_with('*') || name.is_empty() {
            continue;
        }
        let mut ents = vec![];
        let mut k = 0;
        while k < brecs.len() {
            if let Some(shape) = read_shape(brecs, &mut k, &block_names, &mut warn) {
                let layer = brecs[k].s(8).unwrap_or("0").to_string();
                let id = doc.alloc_id();
                let mut e = Entity { id, layer, color: Color::ByLayer, linetype: LtRef::ByLayer, lineweight: None, shape };
                apply_props(&mut e, &brecs[k]);
                ents.push(e);
            }
            k += 1;
        }
        doc.define_block(BlockDef { name: name.clone(), base: *base, entities: ents, description: String::new(), xref: None });
    }

    // Images name their file through the IMAGEDEF object.
    for r in recs.iter_mut().filter(|r| r.kind == "IMAGE") {
        let def = r.s(340).map(|h| h.to_ascii_uppercase());
        if let Some(path) = def.and_then(|h| image_files.iter().find(|(k, _)| *k == h)).map(|(_, p)| p.clone()) {
            r.codes.push((1, path));
        }
    }
    // External references: the application loads their contents from the file.
    for (name, path) in xref_paths {
        if let Some(b) = doc.blocks.iter_mut().find(|b| b.name.eq_ignore_ascii_case(&name)) {
            b.xref = Some(path);
            b.entities.clear();
        }
    }
    let mut k = 0;
    while k < recs.len() {
        let layer = recs[k].s(8).unwrap_or("0").to_string();
        if let (Some(path), Some(name)) = (xdata_xref(&recs[k]), recs[k].s(2)) {
            insert_xrefs.push((name.to_string(), path));
        }
        let handle = recs[k].s(5).map(|h| h.trim().to_ascii_uppercase());
        if let Some(shape) = read_shape(&recs, &mut k, &block_names, &mut warn) {
            if doc.layer(&layer).is_none() {
                doc.layers.push(Layer::new(&layer, 7, LineType::Continuous, 0.25));
            }
            let id = doc.add_on(&layer, shape);
            if let Some(h) = handle {
                handle_ids.push((h, id));
            }
            let e = doc.get_mut(id).unwrap();
            apply_props(e, &recs[k]);
        }
        k += 1;
    }
    for (name, path) in insert_xrefs {
        if let Some(b) = doc.blocks.iter_mut().find(|b| b.name.eq_ignore_ascii_case(&name) && b.xref.is_none()) {
            b.xref = Some(path);
            b.entities.clear();
        }
    }
    // Layers referenced only inside blocks.
    let missing: Vec<String> =
        doc.blocks.iter().flat_map(|b| b.entities.iter().map(|e| e.layer.clone())).filter(|l| doc.layer(l).is_none()).collect();
    for l in missing {
        if doc.layer(&l).is_none() {
            doc.layers.push(Layer::new(&l, 7, LineType::Continuous, 0.25));
        }
    }
    // Constraints, their entities found by handle.
    if !constraints_json.is_empty() {
        if let Ok(serde_json::Value::Array(items)) = serde_json::from_str::<serde_json::Value>(&constraints_json) {
            for mut v in items {
                let ok = map_ids(&mut v, &|h| {
                    let h = h.as_str()?.trim().to_ascii_uppercase();
                    handle_ids.iter().find(|(k, _)| *k == h).map(|(_, id)| serde_json::Value::from(*id))
                });
                if let (true, Ok(c)) = (ok, serde_json::from_value::<icad_core::constraints::Constraint>(v)) {
                    doc.constraints.push(c);
                }
            }
        }
    }
    // Dimensions follow the geometry they measure.
    doc.associate_all_dims();
    // Annotation scale and dimension style from the header.
    let var = |name: &str| vars.iter().find(|(n, _)| n.eq_ignore_ascii_case(name)).and_then(|(_, v)| v.trim().parse::<f64>().ok());
    let dimscale = var("$DIMSCALE").filter(|v| *v > 0.0);
    for (name, field) in [
        ("$DIMTXT", &mut doc.dimstyle.text_height),
        ("$DIMASZ", &mut doc.dimstyle.arrow_size),
        ("$DIMEXO", &mut doc.dimstyle.ext_offset),
        ("$DIMEXE", &mut doc.dimstyle.ext_beyond),
        ("$DIMGAP", &mut doc.dimstyle.text_gap),
    ] {
        if let Some(v) = var(name).filter(|v| *v > 0.0) {
            *field = v;
        }
    }
    if let Some(v) = var("$DIMDEC").filter(|v| (0.0..=8.0).contains(v)) {
        doc.dimstyle.decimals = v as usize;
    }
    if !doc.entities.is_empty() {
        doc.current_layer = "0".into();
        // Fit the paper to the drawing itself, not to an exported border.
        let ext = doc
            .entities
            .iter()
            .filter(|e| !e.layer.eq_ignore_ascii_case("TITLEBLOCK"))
            .fold(icad_core::geom::BBox::empty(), |b, e| b.union(&doc.entity_bbox(e)));
        let ext = if ext.is_empty() { doc.extents() } else { ext };
        doc.sheet.fit_to(&ext);
        doc.sheet.visible = false;
    }
    // Keep the drawing's own annotation scale: text and arrows stay the size
    // they were drawn at.
    if let Some(s) = dimscale {
        doc.sheet.scale = s;
        if let Some(lt) = var("$LTSCALE").filter(|v| *v > 0.0) {
            doc.ltscale = lt / s;
        }
    }
    if !warn.is_empty() {
        eprintln!("DXF import: skipped {warn:?}");
    }
    Ok(doc)
}

fn read_dimension(r: &Rec) -> Option<Shape> {
    let ty = r.i(70) & 7;
    let text = r.s(1).map(decode_text).filter(|t| !t.is_empty() && t != "<>");
    let tmid = r.p(11);
    let mut d = match ty {
        0 | 1 => {
            let p1 = r.p(13);
            let p2 = r.p(14);
            let q = r.p(10);
            let kind = if ty == 1 {
                DimKind::Aligned
            } else {
                let a = norm_angle(rad(r.f(50)));
                if a.abs() < 1e-3 || (a - std::f64::consts::PI).abs() < 1e-3 {
                    DimKind::Horizontal
                } else if (a - std::f64::consts::FRAC_PI_2).abs() < 1e-3 || (a - 3.0 * std::f64::consts::FRAC_PI_2).abs() < 1e-3 {
                    DimKind::Vertical
                } else {
                    DimKind::Aligned
                }
            };
            let mut d = Dimension::new(kind, p1, p2, q);
            let dir = d.direction();
            d.loc = q + dir * (tmid - q).dot(dir);
            d
        }
        3 => {
            let a = r.p(10);
            let b = r.p(15);
            Dimension::new(DimKind::Diameter, a.mid(b), b, tmid)
        }
        4 => Dimension::new(DimKind::Radius, r.p(10), r.p(15), tmid),
        5 => Dimension::angular(r.p(15), r.p(13), r.p(14), r.p(10)),
        6 => {
            // Flag 64 marks X ordinates; without it (some converters drop it) the
            // axis follows the leader direction.
            let mut d = Dimension::ordinate(r.p(10), r.p(13), r.p(14));
            if r.i(70) & 64 != 0 {
                d.kind = DimKind::OrdinateX;
            }
            d
        }
        2 => {
            // Two-line angular: lines (13→14) and (15→10); arc point 16.
            let (a1, a2, b1, b2) = (r.p(13), r.p(14), r.p(15), r.p(10));
            let (c, _, _) = line_line(a1, a2, b1, b2)?;
            Dimension::angular(c, if a2.dist(c) > a1.dist(c) { a2 } else { a1 }, if b2.dist(c) > b1.dist(c) { b2 } else { b1 }, r.p(16))
        }
        _ => return None,
    };
    d.text = text;
    Some(Shape::Dim(d))
}

fn strip_mtext(s: &str) -> String {
    let mut out = String::new();
    let mut chars = s.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            '{' | '}' => {}
            '\\' => match chars.next() {
                Some('P') => out.push('\n'),
                Some('~') => out.push(' '),
                Some('\\') => out.push('\\'),
                Some(_) => {
                    for d in chars.by_ref() {
                        if d == ';' {
                            break;
                        }
                    }
                }
                None => {}
            },
            c => out.push(c),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> Document {
        let mut d = Document::new();
        d.add(Shape::Line { a: V2::new(0., 0.), b: V2::new(100., 0.) });
        d.add(Shape::Circle { c: V2::new(50., 25.), r: 10. });
        d.add(Shape::Arc { c: V2::new(0., 0.), r: 5., start: 0.0, end: 1.5 });
        d.add(Shape::Polyline { pts: vec![V2::new(0., 0.), V2::new(10., 0.), V2::new(10., 10.)], closed: true });
        d.add(Shape::Text(Text::new(V2::new(1., 2.), 3.5, "Ø20 ±0.1")));
        d.add(Shape::Dim(Dimension::new(DimKind::Horizontal, V2::new(0., 0.), V2::new(100., 0.), V2::new(50., -15.))));
        d.add(Shape::Dim(Dimension::angular(V2::ZERO, V2::new(10., 0.), V2::new(0., 10.), V2::new(7., 7.))));
        d.add(Shape::Ellipse { c: V2::new(80., 60.), major: V2::new(20., 0.), ratio: 0.5, start: 0.0, end: std::f64::consts::TAU });
        d.add(Shape::Spline { pts: vec![V2::new(0., 50.), V2::new(10., 60.), V2::new(20., 50.), V2::new(30., 60.)], closed: false });
        d.add(Shape::Point { p: V2::new(5., 5.) });
        d.add(Shape::MText(MText::new(V2::new(0., 100.), 3.5, 60.0, "NOTES:\n1. Break all edges")));
        d.add(Shape::Hatch(Hatch {
            loops: vec![vec![V2::new(0., 0.), V2::new(10., 0.), V2::new(10., 10.), V2::new(0., 10.)]],
            pattern: "ANSI31".into(),
            scale: 1.0,
            angle: 0.0,
        }));
        d.define_block(BlockDef {
            name: "WASHER".into(),
            base: V2::ZERO,
            entities: vec![Entity {
                id: 999,
                layer: "0".into(),
                color: Color::ByLayer,
                linetype: LtRef::ByLayer,
                lineweight: None,
                shape: Shape::Circle { c: V2::ZERO, r: 4.0 },
            }],
            description: String::new(),
            xref: None,
        });
        d.add(Shape::Insert(Insert::new("WASHER", V2::new(120., 20.))));
        d
    }

    #[test]
    fn constraints_round_trip() {
        use icad_core::constraints::{Constraint, DimConstraint, DimKind, PtRef, SegRef};
        let mut d = Document::new();
        let a = d.add(Shape::Line { a: V2::new(0.0, 0.0), b: V2::new(100.0, 0.0) });
        let b = d.add(Shape::Line { a: V2::new(100.0, 0.0), b: V2::new(100.0, 40.0) });
        let c = d.add(Shape::Circle { c: V2::new(50.0, 20.0), r: 10.0 });
        d.add_constraint(Constraint::Horizontal(SegRef::line(a))).unwrap();
        d.add_constraint(Constraint::Coincident(PtRef { id: a, k: 1 }, PtRef { id: b, k: 0 })).unwrap();
        d.add_constraint(Constraint::Perpendicular(SegRef::line(a), SegRef::line(b))).unwrap();
        let dim = DimConstraint {
            name: "d1".into(),
            kind: DimKind::Diameter,
            a: PtRef { id: c, k: 0 },
            b: PtRef { id: c, k: 0 },
            s: None,
            expr: "20".into(),
            value: 20.0,
            loc: V2::new(70.0, 30.0),
        };
        d.add_constraint(Constraint::Dim(dim)).unwrap();
        let r = read_dxf(&write_dxf(&d, &DxfOptions { include_sheet: false, ..Default::default() })).unwrap();
        assert_eq!(r.constraints.len(), 4, "{:?}", r.constraints);
        // The references point at the re-read entities of the same kind.
        let Constraint::Coincident(p, q) = &r.constraints[1] else { panic!() };
        assert!(matches!(r.get(p.id).unwrap().shape, Shape::Line { .. }) && p.k == 1 && q.k == 0 && p.id != q.id);
        let Constraint::Dim(dd) = &r.constraints[3] else { panic!() };
        assert!(matches!(r.get(dd.a.id).unwrap().shape, Shape::Circle { .. }) && dd.name == "d1");
    }

    #[test]
    fn annotation_scale_survives_export_with_sheet() {
        let mut d = Document::new();
        d.add(Shape::Line { a: V2::new(0.0, 0.0), b: V2::new(2800.0, 0.0) });
        d.add(Shape::Line { a: V2::new(0.0, 0.0), b: V2::new(0.0, 180.0) });
        d.sheet.scale = 20.0;
        d.sheet.visible = true;
        d.dimstyle.text_height = 2.5;
        // The exported border is much bigger than the part; it must not change the scale.
        let r = read_dxf(&write_dxf(&d, &DxfOptions::default())).unwrap();
        assert_eq!(r.sheet.scale, 20.0);
        assert_eq!(r.dimstyle.text_height, 2.5);
    }

    #[test]
    fn round_trip_r2000() {
        let d = sample();
        let s = write_dxf(&d, &DxfOptions::default());
        assert!(s.contains("AC1015") && s.contains("ELLIPSE") && s.contains("SPLINE") && s.contains("HATCH") && s.contains("MTEXT"));
        let r = read_dxf(&s).unwrap();
        let kinds: Vec<&str> = r.entities.iter().map(|e| e.shape.type_name()).collect();
        for k in [
            "Line",
            "Circle",
            "Arc",
            "Polyline",
            "Text",
            "Rotated Dimension",
            "Angular Dimension",
            "Ellipse",
            "Spline",
            "Point",
            "MText",
            "Hatch",
            "Block Reference",
        ] {
            assert!(kinds.contains(&k), "missing {k} in {kinds:?}");
        }
        assert!(r.block("WASHER").is_some());
        let ang = r
            .entities
            .iter()
            .find_map(|e| if let Shape::Dim(d) = &e.shape { (d.kind == DimKind::Angular).then_some(d) } else { None })
            .unwrap();
        assert!((ang.measurement() - 90.0).abs() < 1e-6);
        // Spline passes through the same fit points after B-spline round trip.
        let sp = r.entities.iter().find_map(|e| if let Shape::Spline { pts, .. } = &e.shape { Some(pts.clone()) } else { None }).unwrap();
        assert!(sp.iter().any(|p| p.dist(V2::new(10., 60.)) < 1e-6));
    }

    /// Writes sample files for external validation when ICAD_DXF_OUT is set.
    #[test]
    fn dump_samples_for_validation() {
        if let Ok(dir) = std::env::var("ICAD_DXF_OUT") {
            let d = sample();
            std::fs::write(format!("{dir}/sample2000.dxf"), write_dxf(&d, &DxfOptions::default())).unwrap();
            std::fs::write(format!("{dir}/sample12.dxf"), write_dxf(&d, &DxfOptions { version: DxfVersion::R12, ..Default::default() }))
                .unwrap();
        }
    }

    #[test]
    fn round_trip_r12() {
        let d = sample();
        let s = write_dxf(&d, &DxfOptions { version: DxfVersion::R12, ..Default::default() });
        assert!(s.contains("AC1009") && s.contains("%%c20 %%p0.1") && !s.contains("LWPOLYLINE"));
        let r = read_dxf(&s).unwrap();
        let kinds: Vec<&str> = r.entities.iter().map(|e| e.shape.type_name()).collect();
        assert!(kinds.contains(&"Rotated Dimension") && kinds.contains(&"Block Reference"));
        let dim = r.entities.iter().find_map(|e| if let Shape::Dim(d) = &e.shape { Some(d) } else { None }).unwrap();
        assert!((dim.measurement() - 100.0).abs() < 1e-9);
    }

    #[test]
    fn ordinate_and_arc_length_round_trip() {
        let mut d = Document::new();
        let arc = Shape::Arc { c: V2::new(50., 0.), r: 20., start: 0.0, end: std::f64::consts::FRAC_PI_2 };
        d.add(arc);
        let ord = Dimension::ordinate(V2::new(0., 0.), V2::new(70., 0.), V2::new(70., 30.));
        let al = Dimension::arc_length(V2::new(50., 0.), V2::new(70., 0.), V2::new(50., 20.), V2::new(70., 20.));
        d.add(Shape::Dim(ord.clone()));
        d.add(Shape::Dim(al.clone()));
        let s = write_dxf(&d, &DxfOptions::default());
        assert!(s.contains("AcDbOrdinateDimension") && s.contains("IMAGINECAD") && s.contains("*U1"));
        let r = read_dxf(&s).unwrap();
        let dims: Vec<&Dimension> = r.entities.iter().filter_map(|e| if let Shape::Dim(d) = &e.shape { Some(d) } else { None }).collect();
        assert_eq!(dims.len(), 2, "{:?}", r.entities.iter().map(|e| e.shape.type_name()).collect::<Vec<_>>());
        let o = dims.iter().find(|d| d.is_ordinate()).unwrap();
        assert_eq!((o.kind, o.measurement()), (DimKind::OrdinateX, 70.0));
        let a = dims.iter().find(|d| d.kind == DimKind::ArcLength).unwrap();
        assert!((a.measurement() - al.measurement()).abs() < 1e-9);
        // Re-associated with the arc on reading.
        assert!(a.assoc.is_some_and(|x| x.curve.is_some()));
        assert!(r.blocks.iter().all(|b| !b.name.starts_with('*')));
    }

    fn symbols() -> Vec<Shape> {
        use icad_core::symbols::*;
        let row = FcfRow {
            symbol: GdtSymbol::Perpendicularity,
            diameter: false,
            tolerance: "0.02".into(),
            material: Material::None,
            datums: vec![DatumRef { letter: "A".into(), material: Material::None }],
        };
        vec![
            Shape::Tolerance(Fcf { pos: V2::new(10., 10.), rows: vec![row.clone()], leader: vec![] }),
            Shape::Tolerance(Fcf { pos: V2::new(10., 40.), rows: vec![row], leader: vec![V2::new(0., 30.)] }),
            Shape::Datum(DatumFeature { letter: "B".into(), pos: V2::new(50., 20.), tip: V2::new(50., 0.) }),
            Shape::RevCloud(icad_core::markup::RevCloud {
                pts: vec![V2::new(0., 0.), V2::new(30., 0.), V2::new(30., 20.), V2::new(0., 20.)],
                arc: 8.0,
            }),
            Shape::Surface(SurfaceFinish {
                pos: V2::new(70., 0.),
                rotation: 0.3,
                process: SurfaceProcess::NoRemoval,
                requirement: "Rz 6.3".into(),
                method: "cast".into(),
                lay: "⊥".into(),
                all_around: true,
            }),
            Shape::Weld(WeldSymbol {
                tip: V2::new(90., 0.),
                elbow: V2::new(100., 15.),
                arrow_side: WeldSide { kind: WeldType::Fillet, size: "a4".into(), length: "3x50 (100)".into() },
                other_side: WeldSide { kind: WeldType::VButt, size: String::new(), length: String::new() },
                all_around: false,
                field: true,
                tail: "135".into(),
            }),
        ]
    }

    #[test]
    fn symbols_round_trip() {
        let mut d = Document::new();
        for s in symbols() {
            d.add(s);
        }
        let s = write_dxf(&d, &DxfOptions { include_sheet: false, ..Default::default() });
        assert!(s.contains("AcDbFcf") && s.contains("{\\Fgdt;b}%%v0.02%%vA"));
        let r = read_dxf(&s).unwrap();
        let back: Vec<Shape> = r.entities.iter().map(|e| e.shape.clone()).collect();
        assert_eq!(back, symbols());
        // A plain TOLERANCE from other software is read from its GDT codes.
        let plain = s.replace("1001\nIMAGINECAD", "1001\nOTHERAPP");
        let r = read_dxf(&plain).unwrap();
        assert!(r.entities.iter().any(|e| matches!(&e.shape, Shape::Tolerance(f) if f.rows[0].tolerance == "0.02")));
        // R12 explodes them into plain graphics.
        let r12 = read_dxf(&write_dxf(&d, &DxfOptions { version: DxfVersion::R12, include_sheet: false, ..Default::default() })).unwrap();
        assert!(r12.entities.len() > 20);
    }

    pub fn attributed() -> Document {
        let mut d = Document::new();
        let ent = |shape| Entity { id: 0, layer: "0".into(), color: Color::ByLayer, linetype: LtRef::ByLayer, lineweight: None, shape };
        d.define_block(BlockDef {
            name: "BALLOON".into(),
            base: V2::ZERO,
            entities: vec![
                ent(Shape::Circle { c: V2::ZERO, r: 5.0 }),
                ent(Shape::AttDef(AttDef {
                    tag: "ITEM".into(),
                    prompt: "Item number".into(),
                    default: "1".into(),
                    text: Text::new(V2::ZERO, 3.5, "").aligned(HAlign::Center, VAlign::Middle),
                    invisible: false,
                })),
                ent(Shape::AttDef(AttDef {
                    tag: "PART".into(),
                    prompt: "Part number".into(),
                    default: String::new(),
                    text: Text::new(V2::new(0., -8.), 2.5, ""),
                    invisible: true,
                })),
            ],
            description: String::new(),
            xref: None,
        });
        let mut ins = Insert::new("BALLOON", V2::new(50., 20.));
        ins.set_value("ITEM", "12");
        ins.set_value("PART", "ICP-0042");
        d.add(Shape::Insert(ins));
        d
    }

    #[test]
    fn attributes_round_trip() {
        let d = attributed();
        let s = write_dxf(&d, &DxfOptions { include_sheet: false, ..Default::default() });
        assert!(s.contains("ATTDEF") && s.contains("ATTRIB") && s.contains("SEQEND"));
        let r = read_dxf(&s).unwrap();
        let b = r.block("BALLOON").unwrap();
        let defs: Vec<&AttDef> = b.attdefs().collect();
        assert_eq!(defs.len(), 2);
        assert_eq!((defs[0].tag.as_str(), defs[0].prompt.as_str(), defs[0].default.as_str()), ("ITEM", "Item number", "1"));
        assert!(defs[1].invisible);
        let Shape::Insert(ins) = &r.entities[0].shape else { panic!("{:?}", r.entities[0].shape) };
        assert_eq!((ins.value_of("item"), ins.value_of("PART")), (Some("12"), Some("ICP-0042")));
        assert_eq!(r.entities.len(), 1, "attributes are not separate entities");
    }

    #[test]
    fn tables_and_styled_leaders_round_trip() {
        use icad_core::table::*;
        let mut d = Document::new();
        let mut t = Table::from_rows(V2::new(10., 50.), &[vec!["A".into(), "B".into()], vec!["1".into(), "{{TITLE}}".into()]], 2.5);
        t.cells[0][0].span = 1;
        d.add(Shape::Table(t.clone()));
        let l = Leader { pts: vec![V2::ZERO, V2::new(5., 5.)], text: "3".into(), style: Some("Balloon".into()) };
        d.add(Shape::Leader(l.clone()));
        let s = write_dxf(&d, &DxfOptions { include_sheet: false, ..Default::default() });
        assert!(s.contains("UNTITLED PART"), "fields are written with their values");
        let r = read_dxf(&s).unwrap();
        assert_eq!(r.entities.iter().map(|e| e.shape.clone()).collect::<Vec<_>>(), vec![Shape::Table(t), Shape::Leader(l)]);
    }

    #[test]
    fn text_styles_round_trip() {
        let mut d = Document::new();
        d.text_styles.push(icad_core::fonts::TextStyle {
            name: "NOTES".into(),
            file: "/Library/Fonts/Arial Narrow.ttf".into(),
            font: "Arial Narrow".into(),
            width_factor: 0.8,
            oblique: 15.0,
            height: 0.0,
        });
        let mut t = Text::new(V2::ZERO, 3.5, "SEE NOTE 2");
        t.style = Some("NOTES".into());
        d.add(Shape::Text(t));
        let mut m = MText::new(V2::new(0., 20.), 2.5, 50.0, "REMOVE BURRS");
        m.style = Some("NOTES".into());
        d.add(Shape::MText(m));
        let s = write_dxf(&d, &DxfOptions { include_sheet: false, ..Default::default() });
        assert!(s.contains("Arial Narrow.ttf"));
        let r = read_dxf(&s).unwrap();
        let st = r.text_style("NOTES").unwrap();
        assert_eq!((st.file.as_str(), st.font.as_str(), st.width_factor, st.oblique), ("Arial Narrow.ttf", "Arial Narrow", 0.8, 15.0));
        assert!(r.entities.iter().all(|e| match &e.shape {
            Shape::Text(t) => t.style.as_deref() == Some("NOTES"),
            Shape::MText(m) => m.style.as_deref() == Some("NOTES"),
            _ => false,
        }));
    }

    #[test]
    fn external_references_round_trip() {
        let mut part = Document::new();
        part.ensure_layer(Layer::new("WELDS", 3, LineType::Continuous, 0.35));
        part.add_on("WELDS", Shape::Circle { c: V2::ZERO, r: 5.0 });
        let mut d = Document::new();
        d.attach_xref("BASE", "/parts/base.dwg", &part);
        d.add(Shape::Insert(Insert::new("BASE", V2::new(10., 10.))));
        let s = write_dxf(&d, &DxfOptions { include_sheet: false, ..Default::default() });
        assert!(!s.contains("BASE|WELDS"), "dependent layers come with the file");
        let r = read_dxf(&s).unwrap();
        let b = r.block("BASE").unwrap();
        assert_eq!(b.xref.as_deref(), Some("/parts/base.dwg"));
        assert!(b.entities.is_empty());
        assert!(matches!(&r.entities[0].shape, Shape::Insert(i) if i.name == "BASE"));
    }

    #[test]
    fn bulged_polyline_becomes_arc_points() {
        // Semicircle from (0,0) to (10,0) with bulge 1.
        let pts = bulge_points(&[(V2::new(0., 0.), 1.0), (V2::new(10., 0.), 0.0)], false);
        assert!(pts.len() > 10);
        for p in &pts {
            assert!((p.dist(V2::new(5., 0.)) - 5.0).abs() < 1e-9);
        }
    }
}
