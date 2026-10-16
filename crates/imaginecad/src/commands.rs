// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Proteen Das

//! Command-line driven command state machines.
//!
//! Each command declares what input it wants next (a point, a selection, an
//! entity pick or a typed value) and reacts to it. The canvas and the command
//! line feed it; the command mutates the document through [`Cx`].

use crate::tab::Tab;
use icad_core::entity::*;
use icad_core::geom::*;
use icad_core::ops;
use icad_core::prim::{HAlign, Text, VAlign};
use icad_core::Document;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Want {
    Point,
    Select,
    Entity,
    Value,
    /// Free text (space is a character, not Enter).
    Text,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Flow {
    Continue,
    Done,
}

#[derive(Clone)]
pub enum ViewReq {
    Extents,
    Window(BBox),
    Sheet,
    Previous,
}

/// Values remembered between command invocations (like CAD system variables).
#[derive(Clone, Debug)]
pub struct Memory {
    pub offset: f64,
    pub fillet_r: f64,
    pub chamfer_d: f64,
    pub text_h: Option<f64>,
    pub polygon_sides: u32,
    pub rows: u32,
    pub cols: u32,
    /// Last linear/angular dimension created (for DIMBASELINE / DIMCONTINUE).
    pub last_dim: Option<u64>,
    pub hatch_pattern: String,
    pub hatch_scale: f64,
    pub hatch_angle: f64,
    /// Objects removed by the last ERASE (for OOPS).
    pub last_erased: Vec<icad_core::Entity>,
    pub polar_count: u32,
    pub lengthen: f64,
    pub divide_n: u32,
    pub measure_len: f64,
    pub last_block: String,
    /// Layer visibility before LAYISO.
    pub layiso_backup: Vec<(String, bool)>,
    /// Origin of ordinate dimensions.
    pub ordinate_datum: V2,
    pub qdim_mode: crate::commands_dim::QMode,
    /// Block reference whose attributes should be edited next (after INSERT).
    pub pending_attedit: Option<u64>,
}

impl Default for Memory {
    fn default() -> Self {
        Self {
            offset: 10.0,
            fillet_r: 0.0,
            chamfer_d: 5.0,
            text_h: None,
            polygon_sides: 6,
            rows: 2,
            cols: 3,
            last_dim: None,
            hatch_pattern: "ANSI31".into(),
            hatch_scale: 1.0,
            hatch_angle: 0.0,
            last_erased: vec![],
            polar_count: 6,
            lengthen: 5.0,
            divide_n: 4,
            measure_len: 10.0,
            last_block: String::new(),
            layiso_backup: vec![],
            ordinate_datum: V2::ZERO,
            qdim_mode: Default::default(),
            pending_attedit: None,
        }
    }
}

pub struct Cx<'a> {
    pub tab: &'a mut Tab,
    pub log: &'a mut Vec<String>,
    pub mem: &'a mut Memory,
    pub view_req: &'a mut Option<ViewReq>,
}

impl Cx<'_> {
    pub fn doc(&mut self) -> &mut Document {
        &mut self.tab.doc
    }
    pub fn checkpoint(&mut self) {
        self.tab.checkpoint();
    }
    pub fn msg(&mut self, s: impl Into<String>) {
        self.log.push(s.into());
    }
    pub fn shape(&self, id: u64) -> Option<Shape> {
        self.tab.doc.get(id).map(|e| e.shape.clone())
    }
    /// Take the pre-selection (noun-verb) if any.
    pub fn take_selection(&mut self) -> Vec<u64> {
        std::mem::take(&mut self.tab.selection)
    }
    /// Add a dimension on the dimension layer, associated with the geometry
    /// it measures; it becomes the base for DIMBASELINE/DIMCONTINUE.
    pub fn add_dim(&mut self, mut d: Dimension) -> u64 {
        let layer = dim_layer(self);
        self.tab.doc.associate_dim(&mut d);
        let id = self.tab.doc.add_on(&layer, Shape::Dim(d));
        self.mem.last_dim = Some(id);
        id
    }
}

pub trait Command {
    fn name(&self) -> &'static str;
    fn prompt(&self, cx: &Cx) -> String;
    fn wants(&self) -> Want;
    /// Reference point for rubber-banding, ortho and `@` relative input.
    fn base(&self) -> Option<V2> {
        None
    }
    fn start(&mut self, _cx: &mut Cx) -> Flow {
        Flow::Continue
    }
    fn point(&mut self, _cx: &mut Cx, _p: V2) -> Flow {
        Flow::Continue
    }
    fn value(&mut self, cx: &mut Cx, s: &str) -> Flow {
        cx.msg(format!("Invalid input: {s}"));
        Flow::Continue
    }
    fn enter(&mut self, _cx: &mut Cx) -> Flow {
        Flow::Done
    }
    fn entity(&mut self, _cx: &mut Cx, _id: u64, _at: V2) -> Flow {
        Flow::Continue
    }
    fn selected(&mut self, _cx: &mut Cx, _ids: Vec<u64>) -> Flow {
        Flow::Continue
    }
    fn preview(&self, _doc: &Document, _cursor: V2) -> Vec<Shape> {
        vec![]
    }
}

pub fn num(s: &str) -> Option<f64> {
    s.trim().parse::<f64>().ok().filter(|v| v.is_finite())
}

/// Parse typed coordinates: `x,y`, `@dx,dy`, `@dist<angle`, `dist<angle`.
pub fn parse_point(s: &str, last: Option<V2>) -> Option<V2> {
    let s = s.trim();
    let (rel, body) = match s.strip_prefix('@') {
        Some(b) => (true, b),
        None => (false, s),
    };
    let origin = if rel { last.unwrap_or(V2::ZERO) } else { V2::ZERO };
    if let Some((d, a)) = body.split_once('<') {
        let d = num(d)?;
        let a = num(a)?;
        return Some(origin + V2::polar(d, rad(a)));
    }
    let parts: Vec<&str> = body.split(',').collect();
    if parts.len() == 2 || parts.len() == 3 {
        let x = num(parts[0])?;
        let y = num(parts[1])?;
        return Some(origin + V2::new(x, y));
    }
    None
}

/// Every command name and alias, for autocomplete.
pub const COMMANDS: &[(&str, &str)] = &[
    ("LINE", "L"),
    ("PLINE", "PL"),
    ("CIRCLE", "C"),
    ("ARC", "A"),
    ("RECTANG", "REC"),
    ("POLYGON", "POL"),
    ("TEXT", "DT"),
    ("DIMLINEAR", "DLI"),
    ("DIMALIGNED", "DAL"),
    ("DIMRADIUS", "DRA"),
    ("DIMDIAMETER", "DDI"),
    ("CENTERMARK", "CM"),
    ("MOVE", "M"),
    ("COPY", "CO"),
    ("ROTATE", "RO"),
    ("SCALE", "SC"),
    ("MIRROR", "MI"),
    ("OFFSET", "O"),
    ("TRIM", "TR"),
    ("EXTEND", "EX"),
    ("FILLET", "F"),
    ("CHAMFER", "CHA"),
    ("ERASE", "E"),
    ("EXPLODE", "X"),
    ("ARRAYRECT", "AR"),
    ("MATCHPROP", "MA"),
    ("DIST", "DI"),
    ("ZOOM", "Z"),
    ("ELLIPSE", "EL"),
    ("SPLINE", "SPL"),
    ("POINT", "PO"),
    ("XLINE", "XL"),
    ("RAY", ""),
    ("MTEXT", "MT"),
    ("LEADER", "LE"),
    ("DIMANGULAR", "DAN"),
    ("DIMBASELINE", "DBA"),
    ("DIMCONTINUE", "DCO"),
    ("HATCH", "H"),
    ("BLOCK", "B"),
    ("INSERT", "I"),
    ("STRETCH", "S"),
    ("BREAK", "BR"),
    ("BREAKATPOINT", "BAP"),
    ("JOIN", "J"),
    ("LENGTHEN", "LEN"),
    ("ARRAYPOLAR", "APO"),
    ("DIVIDE", "DIV"),
    ("MEASURE", "ME"),
    ("AREA", "AA"),
    ("ID", ""),
    ("LIST", "LI"),
    ("OVERKILL", ""),
    ("PURGE", "PU"),
    ("OOPS", ""),
    ("LAYISO", ""),
    ("LAYUNISO", ""),
    ("LAYOFF", ""),
    ("LAYON", ""),
    ("LAYLCK", ""),
    ("LAYULK", ""),
    ("LAYMCUR", ""),
    ("QSELECT", ""),
    ("TEXTEDIT", "ED"),
    ("IMAGEATTACH", "IAT"),
    ("BLOCKS", ""),
    ("UNITS", "UN"),
    ("LAYER", "LA"),
    ("PROPERTIES", "PR"),
    ("UNDO", "U"),
    ("REDO", ""),
    ("NEW", ""),
    ("OPEN", ""),
    ("SAVE", ""),
    ("SAVEAS", ""),
    ("EXPORT", "EXP"),
    ("PLOT", ""),
    ("PAGESETUP", ""),
    ("TITLEBLOCK", "TB"),
    ("DIMSTYLE", "D"),
    ("IMAGETOCAD", "AI"),
    ("AISETTINGS", ""),
    ("SELECTALL", ""),
    ("REGEN", "RE"),
    ("HELP", ""),
    ("AICHAT", "CHAT"),
    ("DIMORDINATE", "DOR"),
    ("DIMARC", "DAR"),
    ("QDIM", ""),
    ("DIMREASSOCIATE", "DRE"),
    ("DIMDISASSOCIATE", "DDA"),
    ("TOLERANCE", "TOL"),
    ("DATUM", ""),
    ("SURFFINISH", "SF"),
    ("WELD", ""),
    ("ATTDEF", "ATT"),
    ("ATTEDIT", "ATE"),
    ("ATTEXPORT", ""),
    ("TABLE", ""),
    ("TABLEDIT", ""),
    ("MLEADER", "MLD"),
    ("MLEADERSTYLE", "MLS"),
];

pub fn create(name: &str) -> Option<Box<dyn Command>> {
    let c: Box<dyn Command> = match name.to_ascii_uppercase().as_str() {
        "LINE" | "L" => Box::new(LineCmd::default()),
        "PLINE" | "PL" => Box::new(PlineCmd::default()),
        "CIRCLE" | "C" => Box::new(CircleCmd::default()),
        "ARC" | "A" => Box::new(ArcCmd::default()),
        "RECTANG" | "REC" | "RECTANGLE" => Box::new(RectCmd::default()),
        "POLYGON" | "POL" => Box::new(PolygonCmd::default()),
        "TEXT" | "DT" | "DTEXT" | "T" | "MTEXT" | "MT" => Box::new(TextCmd::default()),
        "DIMLINEAR" | "DLI" | "DIM" => Box::new(DimLinCmd::default()),
        "DIMALIGNED" | "DAL" => Box::new(DimAlignCmd::default()),
        "DIMRADIUS" | "DRA" => Box::new(DimRadCmd { diameter: false, target: None }),
        "DIMDIAMETER" | "DDI" => Box::new(DimRadCmd { diameter: true, target: None }),
        "CENTERMARK" | "CM" => Box::new(CenterMarkCmd),
        "MOVE" | "M" => Box::new(XformCmd::new(XKind::Move)),
        "COPY" | "CO" | "CP" => Box::new(XformCmd::new(XKind::Copy)),
        "ROTATE" | "RO" => Box::new(XformCmd::new(XKind::Rotate)),
        "SCALE" | "SC" => Box::new(XformCmd::new(XKind::Scale)),
        "MIRROR" | "MI" => Box::new(XformCmd::new(XKind::Mirror)),
        "ERASE" | "E" => Box::new(SelCmd { explode: false }),
        "EXPLODE" | "X" => Box::new(SelCmd { explode: true }),
        "OFFSET" | "O" => Box::new(OffsetCmd::default()),
        "TRIM" | "TR" => Box::new(TrimCmd { extend: false }),
        "EXTEND" | "EX" => Box::new(TrimCmd { extend: true }),
        "FILLET" | "F" => Box::new(CornerCmd::new(false)),
        "CHAMFER" | "CHA" => Box::new(CornerCmd::new(true)),
        "ARRAYRECT" | "ARRAY" | "AR" => Box::new(ArrayCmd::default()),
        "MATCHPROP" | "MA" => Box::new(MatchCmd::default()),
        "DIST" | "DI" => Box::new(DistCmd::default()),
        "ZOOM" | "Z" => Box::new(ZoomCmd::default()),
        other => return crate::commands_ext::create(other),
    };
    Some(c)
}

// ------------------------------------------------------------------ draw

#[derive(Default)]
struct LineCmd {
    pts: Vec<V2>,
    added: Vec<u64>,
}

impl Command for LineCmd {
    fn name(&self) -> &'static str {
        "LINE"
    }
    fn prompt(&self, _: &Cx) -> String {
        match self.pts.len() {
            0 => "Specify first point:".into(),
            1 | 2 => "Specify next point or [Undo]:".into(),
            _ => "Specify next point or [Close/Undo]:".into(),
        }
    }
    fn wants(&self) -> Want {
        Want::Point
    }
    fn base(&self) -> Option<V2> {
        self.pts.last().copied()
    }
    fn point(&mut self, cx: &mut Cx, p: V2) -> Flow {
        if let Some(&last) = self.pts.last() {
            if last.dist(p) < 1e-12 {
                return Flow::Continue;
            }
            cx.checkpoint();
            let id = cx.doc().add(Shape::Line { a: last, b: p });
            self.added.push(id);
        }
        self.pts.push(p);
        Flow::Continue
    }
    fn value(&mut self, cx: &mut Cx, s: &str) -> Flow {
        match s.to_ascii_uppercase().as_str() {
            "C" | "CLOSE" if self.pts.len() >= 3 => {
                cx.checkpoint();
                let (a, b) = (*self.pts.last().unwrap(), self.pts[0]);
                cx.doc().add(Shape::Line { a, b });
                Flow::Done
            }
            "U" | "UNDO" => {
                if let Some(id) = self.added.pop() {
                    cx.doc().remove(&[id]);
                }
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
        self.pts.last().map(|a| vec![Shape::Line { a: *a, b: c }]).unwrap_or_default()
    }
}

#[derive(Default)]
struct PlineCmd {
    pts: Vec<V2>,
}

impl PlineCmd {
    fn finish(&mut self, cx: &mut Cx, closed: bool) -> Flow {
        if self.pts.len() >= 2 {
            cx.checkpoint();
            cx.doc().add(Shape::Polyline { pts: std::mem::take(&mut self.pts), closed });
        }
        Flow::Done
    }
}

impl Command for PlineCmd {
    fn name(&self) -> &'static str {
        "PLINE"
    }
    fn prompt(&self, _: &Cx) -> String {
        if self.pts.is_empty() {
            "Specify start point:".into()
        } else {
            "Specify next point or [Close/Undo]:".into()
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
    fn enter(&mut self, cx: &mut Cx) -> Flow {
        self.finish(cx, false)
    }
    fn preview(&self, _: &Document, c: V2) -> Vec<Shape> {
        if self.pts.is_empty() {
            return vec![];
        }
        let mut p = self.pts.clone();
        p.push(c);
        vec![Shape::Polyline { pts: p, closed: false }]
    }
}

#[derive(Default)]
struct CircleCmd {
    center: Option<V2>,
    diameter: bool,
    three: Option<Vec<V2>>,
    two: Option<Vec<V2>>,
}

impl Command for CircleCmd {
    fn name(&self) -> &'static str {
        "CIRCLE"
    }
    fn prompt(&self, _: &Cx) -> String {
        if let Some(p) = &self.three {
            return format!("Specify point {} on circle:", p.len() + 1);
        }
        if let Some(p) = &self.two {
            return format!("Specify {} end point of circle's diameter:", if p.is_empty() { "first" } else { "second" });
        }
        match (self.center, self.diameter) {
            (None, _) => "Specify center point for circle or [3P/2P]:".into(),
            (Some(_), false) => "Specify radius of circle or [Diameter]:".into(),
            (Some(_), true) => "Specify diameter of circle:".into(),
        }
    }
    fn wants(&self) -> Want {
        Want::Point
    }
    fn base(&self) -> Option<V2> {
        self.center
            .or_else(|| self.three.as_ref().and_then(|p| p.last().copied()))
            .or_else(|| self.two.as_ref().and_then(|p| p.last().copied()))
    }
    fn point(&mut self, cx: &mut Cx, p: V2) -> Flow {
        if let Some(pts) = &mut self.three {
            pts.push(p);
            if pts.len() == 3 {
                if let Some((c, r)) = circle_3p(pts[0], pts[1], pts[2]) {
                    cx.checkpoint();
                    cx.doc().add(Shape::Circle { c, r });
                }
                return Flow::Done;
            }
            return Flow::Continue;
        }
        if let Some(pts) = &mut self.two {
            pts.push(p);
            if pts.len() == 2 {
                let (c, r) = (pts[0].mid(pts[1]), pts[0].dist(pts[1]) / 2.0);
                if r > 0.0 {
                    cx.checkpoint();
                    cx.doc().add(Shape::Circle { c, r });
                }
                return Flow::Done;
            }
            return Flow::Continue;
        }
        match self.center {
            None => {
                self.center = Some(p);
                Flow::Continue
            }
            Some(c) => {
                let d = c.dist(p);
                self.make(cx, if self.diameter { d / 2.0 } else { d })
            }
        }
    }
    fn value(&mut self, cx: &mut Cx, s: &str) -> Flow {
        let u = s.to_ascii_uppercase();
        if self.center.is_none() && u == "3P" {
            self.three = Some(vec![]);
            return Flow::Continue;
        }
        if self.center.is_none() && u == "2P" {
            self.two = Some(vec![]);
            return Flow::Continue;
        }
        if u == "D" || u == "DIAMETER" {
            self.diameter = true;
            return Flow::Continue;
        }
        if let (Some(_), Some(v)) = (self.center, num(s)) {
            return self.make(cx, if self.diameter { v / 2.0 } else { v });
        }
        cx.msg(format!("Invalid input: {s}"));
        Flow::Continue
    }
    fn enter(&mut self, _: &mut Cx) -> Flow {
        Flow::Done
    }
    fn preview(&self, _: &Document, cur: V2) -> Vec<Shape> {
        if let Some(p) = &self.three {
            if p.len() == 2 {
                if let Some((c, r)) = circle_3p(p[0], p[1], cur) {
                    return vec![Shape::Circle { c, r }];
                }
            }
            return vec![];
        }
        if let Some(p) = &self.two {
            if p.len() == 1 {
                return vec![Shape::Circle { c: p[0].mid(cur), r: p[0].dist(cur) / 2.0 }];
            }
            return vec![];
        }
        match self.center {
            Some(c) => {
                let d = c.dist(cur);
                vec![Shape::Circle { c, r: if self.diameter { d / 2.0 } else { d } }, Shape::Line { a: c, b: cur }]
            }
            None => vec![],
        }
    }
}

impl CircleCmd {
    fn make(&mut self, cx: &mut Cx, r: f64) -> Flow {
        if r <= 0.0 {
            cx.msg("Value must be positive and nonzero.");
            return Flow::Continue;
        }
        cx.checkpoint();
        let c = self.center.unwrap();
        cx.doc().add(Shape::Circle { c, r });
        Flow::Done
    }
}

#[derive(Default)]
struct ArcCmd {
    pts: Vec<V2>,
    center_mode: bool,
}

impl Command for ArcCmd {
    fn name(&self) -> &'static str {
        "ARC"
    }
    fn prompt(&self, _: &Cx) -> String {
        match (self.center_mode, self.pts.len()) {
            (false, 0) => "Specify start point of arc or [Center]:".into(),
            (false, 1) => "Specify second point of arc:".into(),
            (false, _) => "Specify end point of arc:".into(),
            (true, 0) => "Specify center point of arc:".into(),
            (true, 1) => "Specify start point of arc:".into(),
            (true, _) => "Specify end point of arc (counter-clockwise):".into(),
        }
    }
    fn wants(&self) -> Want {
        Want::Point
    }
    fn base(&self) -> Option<V2> {
        self.pts.last().copied()
    }
    fn point(&mut self, cx: &mut Cx, p: V2) -> Flow {
        self.pts.push(p);
        if self.pts.len() < 3 {
            return Flow::Continue;
        }
        if let Some(Shape::Arc { c, r, start, end }) = self.arc(p) {
            cx.checkpoint();
            cx.doc().add(Shape::Arc { c, r, start, end });
        }
        Flow::Done
    }
    fn value(&mut self, cx: &mut Cx, s: &str) -> Flow {
        if self.pts.is_empty() && s.eq_ignore_ascii_case("C") {
            self.center_mode = true;
        } else {
            cx.msg(format!("Invalid option: {s}"));
        }
        Flow::Continue
    }
    fn preview(&self, _: &Document, cur: V2) -> Vec<Shape> {
        match self.pts.len() {
            1 => vec![Shape::Line { a: self.pts[0], b: cur }],
            2 => self.arc(cur).into_iter().collect(),
            _ => vec![],
        }
    }
}

impl ArcCmd {
    fn arc(&self, last: V2) -> Option<Shape> {
        if self.center_mode {
            let c = self.pts[0];
            let r = c.dist(self.pts[1]);
            Some(Shape::Arc { c, r, start: norm_angle((self.pts[1] - c).angle()), end: norm_angle((last - c).angle()) })
        } else {
            let (c, r, start, end) = arc_3p(self.pts[0], self.pts[1], last)?;
            Some(Shape::Arc { c, r, start, end })
        }
    }
}

pub fn rect_pts(a: V2, b: V2) -> Vec<V2> {
    vec![a, V2::new(b.x, a.y), b, V2::new(a.x, b.y)]
}

#[derive(Default)]
struct RectCmd {
    first: Option<V2>,
}

impl Command for RectCmd {
    fn name(&self) -> &'static str {
        "RECTANG"
    }
    fn prompt(&self, _: &Cx) -> String {
        if self.first.is_none() {
            "Specify first corner point:".into()
        } else {
            "Specify other corner point (e.g. @100,50):".into()
        }
    }
    fn wants(&self) -> Want {
        Want::Point
    }
    fn base(&self) -> Option<V2> {
        self.first
    }
    fn point(&mut self, cx: &mut Cx, p: V2) -> Flow {
        match self.first {
            None => {
                self.first = Some(p);
                Flow::Continue
            }
            Some(a) => {
                if (a.x - p.x).abs() > 1e-12 && (a.y - p.y).abs() > 1e-12 {
                    cx.checkpoint();
                    cx.doc().add(Shape::Polyline { pts: rect_pts(a, p), closed: true });
                }
                Flow::Done
            }
        }
    }
    fn preview(&self, _: &Document, c: V2) -> Vec<Shape> {
        self.first.map(|a| vec![Shape::Polyline { pts: rect_pts(a, c), closed: true }]).unwrap_or_default()
    }
}

#[derive(Default)]
struct PolygonCmd {
    sides: Option<u32>,
    center: Option<V2>,
}

impl PolygonCmd {
    fn pts(&self, c: V2, vertex: V2) -> Vec<V2> {
        let n = self.sides.unwrap_or(6).max(3);
        let r = c.dist(vertex);
        let a0 = (vertex - c).angle();
        (0..n).map(|i| c + V2::polar(r, a0 + std::f64::consts::TAU * i as f64 / n as f64)).collect()
    }
}

impl Command for PolygonCmd {
    fn name(&self) -> &'static str {
        "POLYGON"
    }
    fn prompt(&self, cx: &Cx) -> String {
        match (self.sides, self.center) {
            (None, _) => format!("Enter number of sides <{}>:", cx.mem.polygon_sides),
            (Some(_), None) => "Specify center of polygon:".into(),
            _ => "Specify vertex (inscribed) or radius:".into(),
        }
    }
    fn wants(&self) -> Want {
        if self.sides.is_none() {
            Want::Value
        } else {
            Want::Point
        }
    }
    fn base(&self) -> Option<V2> {
        self.center
    }
    fn enter(&mut self, cx: &mut Cx) -> Flow {
        if self.sides.is_none() {
            self.sides = Some(cx.mem.polygon_sides);
            return Flow::Continue;
        }
        Flow::Done
    }
    fn value(&mut self, cx: &mut Cx, s: &str) -> Flow {
        match (self.sides, self.center, num(s)) {
            (None, _, Some(v)) if (3.0..=1024.0).contains(&v) => {
                self.sides = Some(v as u32);
                cx.mem.polygon_sides = v as u32;
            }
            (Some(_), Some(c), Some(r)) if r > 0.0 => {
                let pts = self.pts(c, c + V2::new(0.0, r));
                cx.checkpoint();
                cx.doc().add(Shape::Polyline { pts, closed: true });
                return Flow::Done;
            }
            _ => cx.msg(format!("Invalid input: {s}")),
        }
        Flow::Continue
    }
    fn point(&mut self, cx: &mut Cx, p: V2) -> Flow {
        match self.center {
            None => {
                self.center = Some(p);
                Flow::Continue
            }
            Some(c) => {
                if c.dist(p) > 0.0 {
                    let pts = self.pts(c, p);
                    cx.checkpoint();
                    cx.doc().add(Shape::Polyline { pts, closed: true });
                }
                Flow::Done
            }
        }
    }
    fn preview(&self, _: &Document, cur: V2) -> Vec<Shape> {
        self.center.map(|c| vec![Shape::Polyline { pts: self.pts(c, cur), closed: true }]).unwrap_or_default()
    }
}

#[derive(Default)]
struct TextCmd {
    pos: Option<V2>,
    height: Option<f64>,
    rot: Option<f64>,
}

impl Command for TextCmd {
    fn name(&self) -> &'static str {
        "TEXT"
    }
    fn prompt(&self, cx: &Cx) -> String {
        let def_h = cx.mem.text_h.unwrap_or(cx.tab.doc.dimstyle.text_height * cx.tab.doc.ascale());
        match (self.pos, self.height, self.rot) {
            (None, ..) => "Specify start point of text:".into(),
            (Some(_), None, _) => format!("Specify height <{}>:", fmt(def_h)),
            (Some(_), Some(_), None) => "Specify rotation angle of text <0>:".into(),
            _ => "Enter text:".into(),
        }
    }
    fn wants(&self) -> Want {
        match (self.pos, self.height, self.rot) {
            (None, ..) => Want::Point,
            (Some(_), Some(_), Some(_)) => Want::Text,
            _ => Want::Value,
        }
    }
    fn base(&self) -> Option<V2> {
        self.pos
    }
    fn point(&mut self, _: &mut Cx, p: V2) -> Flow {
        if self.pos.is_none() {
            self.pos = Some(p);
        }
        Flow::Continue
    }
    fn enter(&mut self, cx: &mut Cx) -> Flow {
        if self.pos.is_some() && self.height.is_none() {
            self.height = Some(cx.mem.text_h.unwrap_or(cx.tab.doc.dimstyle.text_height * cx.tab.doc.ascale()));
            return Flow::Continue;
        }
        if self.height.is_some() && self.rot.is_none() {
            self.rot = Some(0.0);
            return Flow::Continue;
        }
        Flow::Done
    }
    fn value(&mut self, cx: &mut Cx, s: &str) -> Flow {
        if self.pos.is_some() && self.height.is_none() {
            match num(s) {
                Some(h) if h > 0.0 => {
                    self.height = Some(h);
                    cx.mem.text_h = Some(h);
                }
                _ => cx.msg("Height must be a positive number."),
            }
            return Flow::Continue;
        }
        if self.height.is_some() && self.rot.is_none() {
            match num(s) {
                Some(a) => self.rot = Some(rad(a)),
                None => cx.msg("Rotation must be a number (degrees)."),
            }
            return Flow::Continue;
        }
        if let (Some(p), Some(h), Some(r)) = (self.pos, self.height, self.rot) {
            if !s.is_empty() {
                cx.checkpoint();
                cx.doc().add(Shape::Text(Text::new(p, h, s).rotated(r)));
            }
            return Flow::Done;
        }
        Flow::Continue
    }
}

// ------------------------------------------------------------- dimensions

fn auto_dim_kind(p1: V2, p2: V2, c: V2) -> DimKind {
    let inx = c.x > p1.x.min(p2.x) && c.x < p1.x.max(p2.x);
    let iny = c.y > p1.y.min(p2.y) && c.y < p1.y.max(p2.y);
    if inx && !iny {
        DimKind::Horizontal
    } else if iny && !inx {
        DimKind::Vertical
    } else if (p2.x - p1.x).abs() >= (p2.y - p1.y).abs() {
        DimKind::Horizontal
    } else {
        DimKind::Vertical
    }
}

#[derive(Default)]
struct DimLinCmd {
    p1: Option<V2>,
    p2: Option<V2>,
    force: Option<DimKind>,
    picking: bool,
}

impl DimLinCmd {
    fn dim(&self, c: V2) -> Option<Dimension> {
        let (p1, p2) = (self.p1?, self.p2?);
        let kind = self.force.unwrap_or_else(|| auto_dim_kind(p1, p2, c));
        let mid = p1.mid(p2);
        let loc = if kind == DimKind::Horizontal { V2::new(mid.x, c.y) } else { V2::new(c.x, mid.y) };
        Some(Dimension::new(kind, p1, p2, loc))
    }
}

impl Command for DimLinCmd {
    fn name(&self) -> &'static str {
        "DIMLINEAR"
    }
    fn prompt(&self, _: &Cx) -> String {
        if self.picking {
            return "Select object to dimension:".into();
        }
        match (self.p1, self.p2) {
            (None, _) => "Specify first extension line origin or <select object>:".into(),
            (Some(_), None) => "Specify second extension line origin:".into(),
            _ => "Specify dimension line location or [Horizontal/Vertical]:".into(),
        }
    }
    fn wants(&self) -> Want {
        if self.picking {
            Want::Entity
        } else {
            Want::Point
        }
    }
    fn base(&self) -> Option<V2> {
        if self.p2.is_none() {
            self.p1
        } else {
            None
        }
    }
    fn enter(&mut self, _: &mut Cx) -> Flow {
        if self.p1.is_none() && !self.picking {
            self.picking = true;
            return Flow::Continue;
        }
        Flow::Done
    }
    fn entity(&mut self, cx: &mut Cx, id: u64, _: V2) -> Flow {
        match cx.shape(id) {
            Some(Shape::Line { a, b }) => {
                self.p1 = Some(a);
                self.p2 = Some(b);
                self.picking = false;
            }
            Some(Shape::Arc { c, r, start, end }) => {
                self.p1 = Some(c + V2::polar(r, start));
                self.p2 = Some(c + V2::polar(r, end));
                self.picking = false;
            }
            Some(Shape::Circle { c, r }) => {
                self.p1 = Some(c - V2::new(r, 0.0));
                self.p2 = Some(c + V2::new(r, 0.0));
                self.picking = false;
            }
            _ => cx.msg("Select a line, arc or circle."),
        }
        Flow::Continue
    }
    fn point(&mut self, cx: &mut Cx, p: V2) -> Flow {
        if self.p1.is_none() {
            self.p1 = Some(p);
            return Flow::Continue;
        }
        if self.p2.is_none() {
            self.p2 = Some(p);
            return Flow::Continue;
        }
        if let Some(d) = self.dim(p) {
            cx.checkpoint();
            let m = d.measurement();
            cx.add_dim(d);
            cx.msg(format!("Dimension text = {}", cx.tab.doc.dimstyle.format(m)));
        }
        Flow::Done
    }
    fn value(&mut self, cx: &mut Cx, s: &str) -> Flow {
        match s.to_ascii_uppercase().as_str() {
            "H" => self.force = Some(DimKind::Horizontal),
            "V" => self.force = Some(DimKind::Vertical),
            _ => cx.msg(format!("Invalid option: {s}")),
        }
        Flow::Continue
    }
    fn preview(&self, _: &Document, c: V2) -> Vec<Shape> {
        match (self.p1, self.p2) {
            (Some(a), None) => vec![Shape::Line { a, b: c }],
            _ => self.dim(c).map(|d| vec![Shape::Dim(d)]).unwrap_or_default(),
        }
    }
}

pub fn dim_layer(cx: &Cx) -> String {
    let d = icad_core::document::layers::DIMENSIONS;
    if cx.tab.doc.layer(d).is_some() {
        d.to_string()
    } else {
        cx.tab.doc.current_layer.clone()
    }
}

#[derive(Default)]
struct DimAlignCmd {
    p1: Option<V2>,
    p2: Option<V2>,
}

impl DimAlignCmd {
    fn dim(&self, c: V2) -> Option<Dimension> {
        let (p1, p2) = (self.p1?, self.p2?);
        let n = (p2 - p1).norm().perp();
        let loc = p1.mid(p2) + n * (c - p1).dot(n);
        Some(Dimension::new(DimKind::Aligned, p1, p2, loc))
    }
}

impl Command for DimAlignCmd {
    fn name(&self) -> &'static str {
        "DIMALIGNED"
    }
    fn prompt(&self, _: &Cx) -> String {
        match (self.p1, self.p2) {
            (None, _) => "Specify first extension line origin:".into(),
            (Some(_), None) => "Specify second extension line origin:".into(),
            _ => "Specify dimension line location:".into(),
        }
    }
    fn wants(&self) -> Want {
        Want::Point
    }
    fn base(&self) -> Option<V2> {
        if self.p2.is_none() {
            self.p1
        } else {
            None
        }
    }
    fn point(&mut self, cx: &mut Cx, p: V2) -> Flow {
        if self.p1.is_none() {
            self.p1 = Some(p);
            return Flow::Continue;
        }
        if self.p2.is_none() {
            self.p2 = Some(p);
            return Flow::Continue;
        }
        if let Some(d) = self.dim(p) {
            cx.checkpoint();
            cx.add_dim(d);
        }
        Flow::Done
    }
    fn preview(&self, _: &Document, c: V2) -> Vec<Shape> {
        match (self.p1, self.p2) {
            (Some(a), None) => vec![Shape::Line { a, b: c }],
            _ => self.dim(c).map(|d| vec![Shape::Dim(d)]).unwrap_or_default(),
        }
    }
}

struct DimRadCmd {
    diameter: bool,
    target: Option<(V2, f64)>,
}

impl DimRadCmd {
    fn dim(&self, cur: V2) -> Option<Dimension> {
        let (c, r) = self.target?;
        let mut u = (cur - c).norm();
        if u == V2::ZERO {
            u = V2::new(1.0, 0.0);
        }
        let kind = if self.diameter { DimKind::Diameter } else { DimKind::Radius };
        Some(Dimension::new(kind, c, c + u * r, cur))
    }
}

impl Command for DimRadCmd {
    fn name(&self) -> &'static str {
        if self.diameter {
            "DIMDIAMETER"
        } else {
            "DIMRADIUS"
        }
    }
    fn prompt(&self, _: &Cx) -> String {
        if self.target.is_none() {
            "Select arc or circle:".into()
        } else {
            "Specify dimension line location:".into()
        }
    }
    fn wants(&self) -> Want {
        if self.target.is_none() {
            Want::Entity
        } else {
            Want::Point
        }
    }
    fn entity(&mut self, cx: &mut Cx, id: u64, _: V2) -> Flow {
        match cx.shape(id) {
            Some(Shape::Circle { c, r } | Shape::Arc { c, r, .. }) => self.target = Some((c, r)),
            _ => cx.msg("Object selected is not a circle or arc."),
        }
        Flow::Continue
    }
    fn point(&mut self, cx: &mut Cx, p: V2) -> Flow {
        if let Some(d) = self.dim(p) {
            cx.checkpoint();
            cx.add_dim(d);
        }
        Flow::Done
    }
    fn preview(&self, _: &Document, c: V2) -> Vec<Shape> {
        self.dim(c).map(|d| vec![Shape::Dim(d)]).unwrap_or_default()
    }
}

struct CenterMarkCmd;

impl Command for CenterMarkCmd {
    fn name(&self) -> &'static str {
        "CENTERMARK"
    }
    fn prompt(&self, _: &Cx) -> String {
        "Select circle or arc to add center mark <exit>:".into()
    }
    fn wants(&self) -> Want {
        Want::Entity
    }
    fn entity(&mut self, cx: &mut Cx, id: u64, _: V2) -> Flow {
        if let Some(Shape::Circle { c, r } | Shape::Arc { c, r, .. }) = cx.shape(id) {
            cx.checkpoint();
            let layer = if cx.tab.doc.layer(icad_core::document::layers::CENTER).is_some() {
                icad_core::document::layers::CENTER.to_string()
            } else {
                cx.tab.doc.current_layer.clone()
            };
            let e = r * 1.15 + 2.0 * cx.tab.doc.ascale();
            cx.doc().add_on(&layer, Shape::Line { a: c - V2::new(e, 0.0), b: c + V2::new(e, 0.0) });
            cx.doc().add_on(&layer, Shape::Line { a: c - V2::new(0.0, e), b: c + V2::new(0.0, e) });
        } else {
            cx.msg("Select a circle or arc.");
        }
        Flow::Continue
    }
}

// ------------------------------------------------------------- modify

#[derive(Clone, Copy, PartialEq, Eq)]
enum XKind {
    Move,
    Copy,
    Rotate,
    Scale,
    Mirror,
}

struct XformCmd {
    kind: XKind,
    ids: Vec<u64>,
    base: Option<V2>,
    p2: Option<V2>,
    asked_erase: bool,
}

impl XformCmd {
    fn new(kind: XKind) -> Self {
        Self { kind, ids: vec![], base: None, p2: None, asked_erase: false }
    }
    fn xform(&self, p: V2) -> Option<Xform> {
        let b = self.base?;
        Some(match self.kind {
            XKind::Move | XKind::Copy => Xform::translate(p - b),
            XKind::Rotate => Xform::rotate(b, (p - b).angle()),
            XKind::Scale => Xform::scale(b, b.dist(p).max(1e-9)),
            XKind::Mirror => Xform::mirror(b, p),
        })
    }
    fn apply(&mut self, cx: &mut Cx, x: Xform, erase_source: bool) {
        cx.checkpoint();
        let ids = self.ids.clone();
        for id in ids {
            let Some(e) = cx.tab.doc.get(id).cloned() else { continue };
            let mut s = e.shape.clone();
            s.transform(&x);
            match self.kind {
                XKind::Copy | XKind::Mirror => {
                    cx.doc().add_like(&e, s);
                    if erase_source {
                        cx.doc().remove(&[id]);
                    }
                }
                _ => {
                    if let Some(m) = cx.doc().get_mut(id) {
                        m.shape = s;
                    }
                }
            }
        }
    }
}

impl Command for XformCmd {
    fn name(&self) -> &'static str {
        match self.kind {
            XKind::Move => "MOVE",
            XKind::Copy => "COPY",
            XKind::Rotate => "ROTATE",
            XKind::Scale => "SCALE",
            XKind::Mirror => "MIRROR",
        }
    }
    fn start(&mut self, cx: &mut Cx) -> Flow {
        self.ids = cx.take_selection();
        if !self.ids.is_empty() {
            cx.msg(format!("{} found", self.ids.len()));
        }
        Flow::Continue
    }
    fn prompt(&self, _: &Cx) -> String {
        if self.ids.is_empty() {
            return "Select objects:".into();
        }
        if self.base.is_none() {
            return if self.kind == XKind::Mirror { "Specify first point of mirror line:".into() } else { "Specify base point:".into() };
        }
        match self.kind {
            XKind::Move => "Specify second point:".into(),
            XKind::Copy => "Specify second point or [Exit]:".into(),
            XKind::Rotate => "Specify rotation angle:".into(),
            XKind::Scale => "Specify scale factor:".into(),
            XKind::Mirror if self.p2.is_none() => "Specify second point of mirror line:".into(),
            XKind::Mirror => "Erase source objects? [Yes/No] <No>:".into(),
        }
    }
    fn wants(&self) -> Want {
        if self.ids.is_empty() {
            Want::Select
        } else if self.p2.is_some() {
            Want::Value
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
        cx.msg(format!("{} found", ids.len()));
        self.ids = ids;
        Flow::Continue
    }
    fn point(&mut self, cx: &mut Cx, p: V2) -> Flow {
        if self.base.is_none() {
            self.base = Some(p);
            return Flow::Continue;
        }
        if self.kind == XKind::Mirror {
            self.p2 = Some(p);
            self.asked_erase = true;
            return Flow::Continue;
        }
        if let Some(x) = self.xform(p) {
            self.apply(cx, x, false);
        }
        if self.kind == XKind::Copy {
            Flow::Continue
        } else {
            Flow::Done
        }
    }
    fn value(&mut self, cx: &mut Cx, s: &str) -> Flow {
        let up = s.to_ascii_uppercase();
        let b = match self.base {
            Some(b) => b,
            None => {
                cx.msg(format!("Invalid input: {s}"));
                return Flow::Continue;
            }
        };
        match self.kind {
            XKind::Rotate => {
                if let Some(a) = num(s) {
                    self.apply(cx, Xform::rotate(b, rad(a)), false);
                    return Flow::Done;
                }
            }
            XKind::Scale => {
                if let Some(k) = num(s).filter(|k| *k > 0.0) {
                    self.apply(cx, Xform::scale(b, k), false);
                    return Flow::Done;
                }
            }
            XKind::Copy if up == "E" || up == "EXIT" => return Flow::Done,
            XKind::Mirror if self.p2.is_some() => {
                let erase = up.starts_with('Y');
                let x = Xform::mirror(b, self.p2.unwrap());
                self.apply(cx, x, erase);
                return Flow::Done;
            }
            _ => {}
        }
        cx.msg(format!("Invalid input: {s}"));
        Flow::Continue
    }
    fn enter(&mut self, cx: &mut Cx) -> Flow {
        if self.kind == XKind::Mirror && self.p2.is_some() {
            let x = Xform::mirror(self.base.unwrap(), self.p2.unwrap());
            self.apply(cx, x, false);
        }
        Flow::Done
    }
    fn preview(&self, doc: &Document, cur: V2) -> Vec<Shape> {
        let p = match (self.kind, self.p2) {
            (XKind::Mirror, Some(p2)) => p2,
            _ => cur,
        };
        let Some(x) = self.xform(p) else { return vec![] };
        let mut out: Vec<Shape> = self
            .ids
            .iter()
            .filter_map(|id| doc.get(*id))
            .map(|e| {
                let mut s = e.shape.clone();
                s.transform(&x);
                s
            })
            .collect();
        if let Some(b) = self.base {
            out.push(Shape::Line { a: b, b: p });
        }
        out
    }
}

/// ERASE / EXPLODE.
struct SelCmd {
    explode: bool,
}

impl SelCmd {
    fn run(&self, cx: &mut Cx, ids: Vec<u64>) -> Flow {
        if ids.is_empty() {
            return Flow::Done;
        }
        cx.checkpoint();
        if self.explode {
            let mut n = 0;
            for id in ids {
                let Some(e) = cx.tab.doc.get(id).cloned() else { continue };
                if let Some(parts) = cx.tab.doc.explode_entity(&e) {
                    cx.doc().remove(&[id]);
                    for mut p in parts {
                        p.id = cx.doc().alloc_id();
                        cx.doc().entities.push(p);
                    }
                    n += 1;
                }
            }
            cx.msg(format!("{n} object(s) exploded"));
        } else {
            let n = ids.len();
            cx.mem.last_erased = ids.iter().filter_map(|id| cx.tab.doc.get(*id).cloned()).collect();
            cx.doc().remove(&ids);
            cx.msg(format!("{n} object(s) erased"));
        }
        Flow::Done
    }
}

impl Command for SelCmd {
    fn name(&self) -> &'static str {
        if self.explode {
            "EXPLODE"
        } else {
            "ERASE"
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
        "Select objects:".into()
    }
    fn wants(&self) -> Want {
        Want::Select
    }
    fn selected(&mut self, cx: &mut Cx, ids: Vec<u64>) -> Flow {
        self.run(cx, ids)
    }
}

#[derive(Default)]
struct OffsetCmd {
    dist: Option<f64>,
    target: Option<u64>,
}

impl Command for OffsetCmd {
    fn name(&self) -> &'static str {
        "OFFSET"
    }
    fn prompt(&self, cx: &Cx) -> String {
        match (self.dist, self.target) {
            (None, _) => format!("Specify offset distance <{}>:", fmt(cx.mem.offset)),
            (Some(_), None) => "Select object to offset or <exit>:".into(),
            _ => "Specify point on side to offset:".into(),
        }
    }
    fn wants(&self) -> Want {
        match (self.dist, self.target) {
            (None, _) => Want::Value,
            (Some(_), None) => Want::Entity,
            _ => Want::Point,
        }
    }
    fn enter(&mut self, cx: &mut Cx) -> Flow {
        if self.dist.is_none() {
            self.dist = Some(cx.mem.offset);
            return Flow::Continue;
        }
        Flow::Done
    }
    fn value(&mut self, cx: &mut Cx, s: &str) -> Flow {
        match num(s) {
            Some(d) if d > 0.0 && self.dist.is_none() => {
                self.dist = Some(d);
                cx.mem.offset = d;
            }
            _ => cx.msg(format!("Invalid input: {s}")),
        }
        Flow::Continue
    }
    fn entity(&mut self, cx: &mut Cx, id: u64, _: V2) -> Flow {
        match cx.shape(id) {
            Some(Shape::Line { .. } | Shape::Circle { .. } | Shape::Arc { .. } | Shape::Polyline { .. }) => self.target = Some(id),
            _ => cx.msg("Cannot offset that object."),
        }
        Flow::Continue
    }
    fn point(&mut self, cx: &mut Cx, p: V2) -> Flow {
        let (Some(id), Some(d)) = (self.target, self.dist) else { return Flow::Continue };
        if let Some(e) = cx.tab.doc.get(id).cloned() {
            match ops::offset(&e.shape, d, p) {
                Some(s) => {
                    cx.checkpoint();
                    cx.doc().add_like(&e, s);
                }
                None => cx.msg("Cannot offset to that side (radius would vanish)."),
            }
        }
        self.target = None;
        Flow::Continue
    }
    fn preview(&self, doc: &Document, cur: V2) -> Vec<Shape> {
        let (Some(id), Some(d)) = (self.target, self.dist) else { return vec![] };
        doc.get(id).and_then(|e| ops::offset(&e.shape, d, cur)).into_iter().collect()
    }
}

struct TrimCmd {
    extend: bool,
}

impl Command for TrimCmd {
    fn name(&self) -> &'static str {
        if self.extend {
            "EXTEND"
        } else {
            "TRIM"
        }
    }
    fn prompt(&self, _: &Cx) -> String {
        if self.extend {
            "Select object to extend (near the end to extend) or [Undo] <exit>:".into()
        } else {
            "Select object to trim (the part to remove) or [Undo] <exit>:".into()
        }
    }
    fn wants(&self) -> Want {
        Want::Entity
    }
    fn value(&mut self, cx: &mut Cx, s: &str) -> Flow {
        if s.eq_ignore_ascii_case("U") {
            cx.tab.undo();
        } else {
            cx.msg(format!("Invalid option: {s}"));
        }
        Flow::Continue
    }
    fn entity(&mut self, cx: &mut Cx, id: u64, at: V2) -> Flow {
        let Some(e) = cx.tab.doc.get(id).cloned() else { return Flow::Continue };
        let others = cx.tab.doc.curves_except(id);
        if self.extend {
            match ops::extend(&e.shape, at, &others) {
                Some(s) => {
                    cx.checkpoint();
                    if let Some(m) = cx.doc().get_mut(id) {
                        m.shape = s;
                    }
                }
                None => cx.msg("Object does not intersect an edge."),
            }
        } else {
            match ops::trim(&e.shape, at, &others) {
                Some(parts) => {
                    cx.checkpoint();
                    cx.doc().remove(&[id]);
                    for p in parts {
                        cx.doc().add_like(&e, p);
                    }
                }
                None => cx.msg("Cannot trim that object."),
            }
        }
        Flow::Continue
    }
}

struct CornerCmd {
    chamfer: bool,
    first: Option<(u64, V2)>,
    setting: bool,
}

impl CornerCmd {
    fn new(chamfer: bool) -> Self {
        Self { chamfer, first: None, setting: false }
    }
}

impl Command for CornerCmd {
    fn name(&self) -> &'static str {
        if self.chamfer {
            "CHAMFER"
        } else {
            "FILLET"
        }
    }
    fn prompt(&self, cx: &Cx) -> String {
        if self.setting {
            return if self.chamfer {
                format!("Specify chamfer distance <{}>:", fmt(cx.mem.chamfer_d))
            } else {
                format!("Specify fillet radius <{}>:", fmt(cx.mem.fillet_r))
            };
        }
        match (self.first, self.chamfer) {
            (None, false) => format!("Current radius = {}. Select first line or [Radius]:", fmt(cx.mem.fillet_r)),
            (None, true) => format!("Distance = {}. Select first line or [Distance]:", fmt(cx.mem.chamfer_d)),
            (Some(_), _) => "Select second line:".into(),
        }
    }
    fn wants(&self) -> Want {
        if self.setting {
            Want::Value
        } else {
            Want::Entity
        }
    }
    fn enter(&mut self, _: &mut Cx) -> Flow {
        if self.setting {
            self.setting = false;
            return Flow::Continue;
        }
        Flow::Done
    }
    fn value(&mut self, cx: &mut Cx, s: &str) -> Flow {
        let up = s.to_ascii_uppercase();
        if !self.setting && (up == "R" || up == "D") {
            self.setting = true;
            return Flow::Continue;
        }
        if self.setting {
            if let Some(v) = num(s).filter(|v| *v >= 0.0) {
                if self.chamfer {
                    cx.mem.chamfer_d = v
                } else {
                    cx.mem.fillet_r = v
                }
                self.setting = false;
                return Flow::Continue;
            }
        }
        cx.msg(format!("Invalid input: {s}"));
        Flow::Continue
    }
    fn entity(&mut self, cx: &mut Cx, id: u64, at: V2) -> Flow {
        let Some(Shape::Line { a, b }) = cx.shape(id) else {
            cx.msg("Select a line (explode polylines first).");
            return Flow::Continue;
        };
        let Some((id1, at1)) = self.first else {
            self.first = Some((id, at));
            return Flow::Continue;
        };
        if id1 == id {
            cx.msg("Select a different line.");
            return Flow::Continue;
        }
        let Some(Shape::Line { a: a1, b: b1 }) = cx.shape(id1) else { return Flow::Done };
        let r = if self.chamfer { cx.mem.chamfer_d } else { cx.mem.fillet_r };
        match ops::corner((a1, b1), at1, (a, b), at, r, self.chamfer) {
            Ok(res) => {
                cx.checkpoint();
                let e1 = cx.tab.doc.get(id1).cloned().unwrap();
                if let Some(m) = cx.doc().get_mut(id1) {
                    m.shape = res.first;
                }
                if let Some(m) = cx.doc().get_mut(id) {
                    m.shape = res.second;
                }
                if let Some(j) = res.joint {
                    cx.doc().add_like(&e1, j);
                }
            }
            Err(e) => cx.msg(e),
        }
        Flow::Done
    }
}

#[derive(Default)]
struct ArrayCmd {
    ids: Vec<u64>,
    rows: Option<u32>,
    cols: Option<u32>,
    dy: Option<f64>,
}

impl ArrayCmd {
    fn finish(&mut self, cx: &mut Cx, dx: f64) -> Flow {
        let (rows, cols, dy) = (self.rows.unwrap_or(1), self.cols.unwrap_or(1), self.dy.unwrap_or(0.0));
        cx.checkpoint();
        for id in self.ids.clone() {
            let Some(e) = cx.tab.doc.get(id).cloned() else { continue };
            for r in 0..rows {
                for c in 0..cols {
                    if r == 0 && c == 0 {
                        continue;
                    }
                    let mut s = e.shape.clone();
                    s.transform(&Xform::translate(V2::new(dx * c as f64, dy * r as f64)));
                    cx.doc().add_like(&e, s);
                }
            }
        }
        Flow::Done
    }
    fn sel_box(&self, doc: &Document) -> BBox {
        self.ids.iter().filter_map(|i| doc.get(*i)).fold(BBox::empty(), |b, e| b.union(&doc.entity_bbox(e)))
    }
}

impl Command for ArrayCmd {
    fn name(&self) -> &'static str {
        "ARRAYRECT"
    }
    fn start(&mut self, cx: &mut Cx) -> Flow {
        self.ids = cx.take_selection();
        Flow::Continue
    }
    fn prompt(&self, cx: &Cx) -> String {
        let b = self.sel_box(&cx.tab.doc);
        if self.ids.is_empty() {
            "Select objects:".into()
        } else if self.rows.is_none() {
            format!("Enter number of rows <{}>:", cx.mem.rows)
        } else if self.cols.is_none() {
            format!("Enter number of columns <{}>:", cx.mem.cols)
        } else if self.dy.is_none() {
            format!("Specify distance between rows <{}>:", fmt(b.height() * 1.5))
        } else {
            format!("Specify distance between columns <{}>:", fmt(b.width() * 1.5))
        }
    }
    fn wants(&self) -> Want {
        if self.ids.is_empty() {
            Want::Select
        } else {
            Want::Value
        }
    }
    fn selected(&mut self, _: &mut Cx, ids: Vec<u64>) -> Flow {
        if ids.is_empty() {
            return Flow::Done;
        }
        self.ids = ids;
        Flow::Continue
    }
    fn enter(&mut self, cx: &mut Cx) -> Flow {
        let b = self.sel_box(&cx.tab.doc);
        if self.rows.is_none() {
            self.rows = Some(cx.mem.rows);
        } else if self.cols.is_none() {
            self.cols = Some(cx.mem.cols);
        } else if self.dy.is_none() {
            self.dy = Some(b.height() * 1.5);
        } else {
            return self.finish(cx, b.width() * 1.5);
        }
        Flow::Continue
    }
    fn value(&mut self, cx: &mut Cx, s: &str) -> Flow {
        let Some(v) = num(s) else {
            cx.msg(format!("Invalid input: {s}"));
            return Flow::Continue;
        };
        if self.rows.is_none() {
            self.rows = Some((v as u32).max(1));
            cx.mem.rows = (v as u32).max(1);
        } else if self.cols.is_none() {
            self.cols = Some((v as u32).max(1));
            cx.mem.cols = (v as u32).max(1);
        } else if self.dy.is_none() {
            self.dy = Some(v);
        } else {
            return self.finish(cx, v);
        }
        Flow::Continue
    }
}

#[derive(Default)]
struct MatchCmd {
    source: Option<u64>,
}

impl Command for MatchCmd {
    fn name(&self) -> &'static str {
        "MATCHPROP"
    }
    fn prompt(&self, _: &Cx) -> String {
        if self.source.is_none() {
            "Select source object:".into()
        } else {
            "Select destination object(s) <exit>:".into()
        }
    }
    fn wants(&self) -> Want {
        Want::Entity
    }
    fn entity(&mut self, cx: &mut Cx, id: u64, _: V2) -> Flow {
        match self.source {
            None => self.source = Some(id),
            Some(src) => {
                let Some(s) = cx.tab.doc.get(src).cloned() else { return Flow::Done };
                cx.checkpoint();
                if let Some(d) = cx.doc().get_mut(id) {
                    d.layer = s.layer.clone();
                    d.color = s.color;
                    d.linetype = s.linetype;
                }
            }
        }
        Flow::Continue
    }
}

#[derive(Default)]
struct DistCmd {
    p1: Option<V2>,
}

impl Command for DistCmd {
    fn name(&self) -> &'static str {
        "DIST"
    }
    fn prompt(&self, _: &Cx) -> String {
        if self.p1.is_none() {
            "Specify first point:".into()
        } else {
            "Specify second point:".into()
        }
    }
    fn wants(&self) -> Want {
        Want::Point
    }
    fn base(&self) -> Option<V2> {
        self.p1
    }
    fn point(&mut self, cx: &mut Cx, p: V2) -> Flow {
        match self.p1 {
            None => {
                self.p1 = Some(p);
                Flow::Continue
            }
            Some(a) => {
                let d = p - a;
                cx.msg(format!(
                    "Distance = {},  Angle in XY Plane = {}°,  Delta X = {},  Delta Y = {}",
                    fmt(d.len()),
                    fmt(deg(norm_angle(d.angle()))),
                    fmt(d.x),
                    fmt(d.y)
                ));
                Flow::Done
            }
        }
    }
    fn preview(&self, _: &Document, c: V2) -> Vec<Shape> {
        self.p1.map(|a| vec![Shape::Line { a, b: c }]).unwrap_or_default()
    }
}

#[derive(Default)]
struct ZoomCmd {
    first: Option<V2>,
}

impl Command for ZoomCmd {
    fn name(&self) -> &'static str {
        "ZOOM"
    }
    fn prompt(&self, _: &Cx) -> String {
        if self.first.is_none() {
            "Specify corner of window or [All/Extents/Previous/Window] <Extents>:".into()
        } else {
            "Specify opposite corner:".into()
        }
    }
    fn wants(&self) -> Want {
        Want::Point
    }
    fn point(&mut self, cx: &mut Cx, p: V2) -> Flow {
        match self.first {
            None => {
                self.first = Some(p);
                Flow::Continue
            }
            Some(a) => {
                *cx.view_req = Some(ViewReq::Window(BBox::from_pts(a, p)));
                Flow::Done
            }
        }
    }
    fn value(&mut self, cx: &mut Cx, s: &str) -> Flow {
        match s.to_ascii_uppercase().as_str() {
            "E" | "EXTENTS" => {
                *cx.view_req = Some(ViewReq::Extents);
                Flow::Done
            }
            "A" | "ALL" => {
                *cx.view_req = Some(ViewReq::Sheet);
                Flow::Done
            }
            "W" | "WINDOW" => Flow::Continue,
            "P" | "PREVIOUS" => {
                *cx.view_req = Some(ViewReq::Previous);
                Flow::Done
            }
            _ => {
                cx.msg(format!("Invalid option: {s}"));
                Flow::Continue
            }
        }
    }
    fn enter(&mut self, cx: &mut Cx) -> Flow {
        *cx.view_req = Some(ViewReq::Extents);
        Flow::Done
    }
    fn preview(&self, _: &Document, c: V2) -> Vec<Shape> {
        self.first.map(|a| vec![Shape::Polyline { pts: rect_pts(a, c), closed: true }]).unwrap_or_default()
    }
}

/// Paste the internal clipboard at a picked insertion point.
pub struct PasteCmd {
    pub items: Vec<Entity>,
}

impl PasteCmd {
    fn origin(&self) -> V2 {
        let mut b = BBox::empty();
        for e in &self.items {
            for g in e.shape.grips() {
                b.add(g);
            }
        }
        if b.is_empty() {
            V2::ZERO
        } else {
            b.min
        }
    }
}

impl Command for PasteCmd {
    fn name(&self) -> &'static str {
        "PASTECLIP"
    }
    fn prompt(&self, _: &Cx) -> String {
        "Specify insertion point:".into()
    }
    fn wants(&self) -> Want {
        Want::Point
    }
    fn point(&mut self, cx: &mut Cx, p: V2) -> Flow {
        let x = Xform::translate(p - self.origin());
        cx.checkpoint();
        let mut new_ids = vec![];
        for e in &self.items {
            let mut s = e.shape.clone();
            s.transform(&x);
            let layer_ok = cx.tab.doc.layer(&e.layer).is_some();
            let id = cx.doc().add_like(e, s);
            if !layer_ok {
                let cur = cx.tab.doc.current_layer.clone();
                if let Some(m) = cx.doc().get_mut(id) {
                    m.layer = cur;
                }
            }
            new_ids.push(id);
        }
        cx.tab.selection = new_ids;
        Flow::Done
    }
    fn preview(&self, _: &Document, c: V2) -> Vec<Shape> {
        let x = Xform::translate(c - self.origin());
        self.items
            .iter()
            .map(|e| {
                let mut s = e.shape.clone();
                s.transform(&x);
                s
            })
            .collect()
    }
}

/// Hot-grip stretch: move one grip of one entity.
pub struct GripCmd {
    pub id: u64,
    pub grip: usize,
    pub from: V2,
}

impl Command for GripCmd {
    fn name(&self) -> &'static str {
        "GRIP"
    }
    fn prompt(&self, _: &Cx) -> String {
        "** STRETCH ** Specify stretch point:".into()
    }
    fn wants(&self) -> Want {
        Want::Point
    }
    fn base(&self) -> Option<V2> {
        Some(self.from)
    }
    fn point(&mut self, cx: &mut Cx, p: V2) -> Flow {
        cx.checkpoint();
        if let Some(e) = cx.doc().get_mut(self.id) {
            e.shape.set_grip(self.grip, p);
        }
        cx.tab.selection = vec![self.id];
        Flow::Done
    }
    fn preview(&self, doc: &Document, c: V2) -> Vec<Shape> {
        doc.get(self.id)
            .map(|e| {
                let mut s = e.shape.clone();
                s.set_grip(self.grip, c);
                vec![s]
            })
            .unwrap_or_default()
    }
}

pub fn fmt(v: f64) -> String {
    let s = format!("{:.4}", v);
    let s = s.trim_end_matches('0').trim_end_matches('.');
    if s == "-0" {
        "0".into()
    } else {
        s.to_string()
    }
}

#[allow(dead_code)]
fn _unused(_: HAlign, _: VAlign) {}

#[cfg(test)]
mod tests {
    use super::*;

    struct Harness {
        tab: Tab,
        log: Vec<String>,
        mem: Memory,
        view: Option<ViewReq>,
    }

    impl Harness {
        fn new() -> Self {
            Self { tab: Tab::new(Document::new(), "t"), log: vec![], mem: Memory::default(), view: None }
        }
        fn run(&mut self, name: &str, inputs: &[&str]) {
            let mut c = create(name).expect("command");
            let mut cx = Cx { tab: &mut self.tab, log: &mut self.log, mem: &mut self.mem, view_req: &mut self.view };
            let mut flow = c.start(&mut cx);
            let mut last: Option<V2> = None;
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
                    let p = parse_point(rest, None).unwrap();
                    let id = cx.tab.doc.pick(p, 0.5).expect("entity under pick");
                    c.entity(&mut cx, id, p)
                } else if c.wants() == Want::Point {
                    match parse_point(inp, c.base().or(last)) {
                        Some(p) => {
                            last = Some(p);
                            c.point(&mut cx, p)
                        }
                        None => c.value(&mut cx, inp),
                    }
                } else {
                    c.value(&mut cx, inp)
                };
            }
        }
        fn shapes(&self) -> Vec<Shape> {
            self.tab.doc.entities.iter().map(|e| e.shape.clone()).collect()
        }
    }

    #[test]
    fn line_close_makes_triangle() {
        let mut h = Harness::new();
        h.run("L", &["0,0", "@100,0", "@0,50", "C"]);
        assert_eq!(h.shapes().len(), 3);
        assert_eq!(h.shapes()[2], Shape::Line { a: V2::new(100.0, 50.0), b: V2::new(0.0, 0.0) });
    }

    #[test]
    fn rectangle_relative_and_undo() {
        let mut h = Harness::new();
        h.run("REC", &["10,10", "@100,50"]);
        let Shape::Polyline { pts, closed } = &h.shapes()[0] else { panic!() };
        assert!(*closed && pts[2] == V2::new(110.0, 60.0));
        assert!(h.tab.undo());
        assert!(h.tab.doc.entities.is_empty());
        assert!(h.tab.redo());
        assert_eq!(h.tab.doc.entities.len(), 1);
    }

    #[test]
    fn trim_and_fillet_commands() {
        let mut h = Harness::new();
        h.run("L", &["0,0", "100,0", ""]);
        h.run("L", &["50,-10", "50,10", ""]);
        h.run("TR", &["pick:75,0", ""]);
        let lines: Vec<Shape> = h.shapes();
        assert!(lines.contains(&Shape::Line { a: V2::new(0.0, 0.0), b: V2::new(50.0, 0.0) }), "{lines:?}");
        let mut h = Harness::new();
        h.run("L", &["0,0", "100,0", ""]);
        h.run("L", &["0,0", "0,100", ""]);
        h.run("F", &["R", "10", "pick:50,0", "pick:0,50"]);
        assert!(h.shapes().iter().any(|s| matches!(s, Shape::Arc { r, .. } if (*r - 10.0).abs() < 1e-9)));
    }

    #[test]
    fn move_with_preselection_and_dimension() {
        let mut h = Harness::new();
        h.run("C", &["0,0", "10"]);
        h.tab.selection = vec![h.tab.doc.entities[0].id];
        h.run("M", &["0,0", "@20,5"]);
        assert_eq!(h.shapes()[0], Shape::Circle { c: V2::new(20.0, 5.0), r: 10.0 });
        h.run("DLI", &["0,0", "40,0", "20,-15"]);
        let Shape::Dim(d) = &h.shapes()[1] else { panic!() };
        assert_eq!(d.kind, DimKind::Horizontal);
        assert!((d.measurement() - 40.0).abs() < 1e-12);
    }

    #[test]
    fn coordinate_parsing() {
        let last = Some(V2::new(10.0, 10.0));
        assert_eq!(parse_point("5,6", last), Some(V2::new(5.0, 6.0)));
        assert_eq!(parse_point("@5,6", last), Some(V2::new(15.0, 16.0)));
        let p = parse_point("@10<90", last).unwrap();
        assert!((p - V2::new(10.0, 20.0)).len() < 1e-9);
        assert_eq!(parse_point("abc", last), None);
    }
}
