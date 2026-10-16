// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Proteen Das

//! Decoding DWG objects: the common entity and object parts, table records
//! (layers, linetypes, block records) and the drawing entities ImagineCAD
//! uses. Every object has a data stream, a handle stream after it and, since
//! R2007, a string stream at its end.

use super::bits::{Bits, Color, Handle, Ver};
use super::container::Container;

/// An object opened for reading.
pub struct Obj<'a> {
    pub ty: u16,
    pub handle: u64,
    pub d: Bits<'a>,
    /// The string stream (R2007+); before R2007 strings are in `d`.
    pub s: Option<Bits<'a>>,
    pub h: Bits<'a>,
    pub ver: Ver,
    /// Extended data: (application handle, its strings).
    pub eed: Vec<(u64, Vec<String>)>,
}

impl<'a> Obj<'a> {
    pub fn t(&mut self) -> String {
        match &mut self.s {
            Some(s) => s.t(),
            None => self.d.t(),
        }
    }

    /// The next handle reference, made absolute.
    pub fn href(&mut self) -> u64 {
        let h = self.h.h();
        h.absolute(self.handle)
    }

    fn cmc(&mut self) -> Color {
        let Obj { d, s, .. } = self;
        d.cmc(s.as_mut())
    }
}

/// Open the object at `offset`: its size, type and the three streams.
pub fn open(c: &Container, offset: usize) -> Option<Obj<'_>> {
    let data = c.objects.get(offset..)?;
    let mut b = Bits::new(data, c.ver);
    let size = b.ms() as usize;
    let hsize = if c.ver >= Ver::R2010 { b.umc() as usize } else { 0 };
    let start = b.pos;
    let end = start + size * 8;
    if size == 0 || end > data.len() * 8 {
        return None;
    }
    let mut d = b.sub(start, end);
    let ty = if c.ver >= Ver::R2010 { d.bot() } else { d.bs() };
    let bitsize = if c.ver >= Ver::R2010 {
        size * 8 - hsize.min(size * 8)
    } else {
        // R2000–R2007: the size of the data up to the handles, in bits.
        d.rl() as usize
    };
    if bitsize > size * 8 {
        return None;
    }
    let h = b.sub(start + bitsize, end);
    let s = if c.ver >= Ver::R2007 { Some(string_stream(&b, start, bitsize)) } else { None };
    // The data stream ends where the handles begin.
    d.end = start + bitsize;
    Some(Obj { ty, handle: 0, d, s, h, ver: c.ver, eed: vec![] })
}

/// The string stream of an R2007+ object: its size is stored just before
/// the last data bit, which says whether there are strings.
fn string_stream<'a>(b: &Bits<'a>, start: usize, bitsize: usize) -> Bits<'a> {
    let empty = b.sub(start, start);
    if bitsize < 17 {
        return empty;
    }
    let flag_at = start + bitsize - 1;
    let mut r = b.sub(flag_at, flag_at + 1);
    if !r.b() {
        return empty;
    }
    let mut at = flag_at - 16;
    let mut r = b.sub(at, at + 16);
    let mut size = r.rs() as usize;
    if size & 0x8000 != 0 {
        at -= 16;
        let mut r = b.sub(at, at + 16);
        let hi = r.rs() as usize;
        size = (size & 0x7fff) | hi << 15;
    }
    if size > at - start {
        return empty;
    }
    b.sub(at - size, at)
}

/// Lineweight index → millimetres (None = ByLayer/ByBlock/default).
pub fn lineweight(i: u8) -> Option<f64> {
    const LW: [u16; 24] = [0, 5, 9, 13, 15, 18, 20, 25, 30, 35, 40, 50, 53, 60, 70, 80, 90, 100, 106, 120, 140, 158, 200, 211];
    LW.get(i as usize).map(|v| *v as f64 / 100.0)
}

/// What every entity carries.
#[derive(Clone, Debug, Default)]
pub struct Common {
    /// 0 = owned by `owner`, 1 = paper space, 2 = model space.
    pub entmode: u8,
    pub owner: u64,
    pub layer: u64,
    pub color: Color,
    /// 0 ByLayer, 1 ByBlock, 2 Continuous, 3 `ltype`.
    pub ltype_flags: u8,
    pub ltype: u64,
    pub lineweight: u8,
    pub invisible: bool,
}

/// Extended data: per application, the strings it holds (other values are
/// passed over).
fn read_eed(d: &mut Bits) -> Vec<(u64, Vec<String>)> {
    let mut out = vec![];
    for _ in 0..4096 {
        let size = d.bs() as usize;
        if size == 0 || d.overrun {
            break;
        }
        let app = d.h().value;
        if size * 8 > d.left().max(0) as usize {
            d.overrun = true;
            break;
        }
        let end = d.pos + size * 8;
        let mut r = d.sub(d.pos, end);
        let mut strings = vec![];
        while r.left() >= 8 && !r.overrun {
            if r.rc() != 0 {
                break;
            }
            if d.ver >= Ver::R2007 {
                let n = r.rs() as usize;
                if n * 16 > r.left().max(0) as usize {
                    break;
                }
                let units: Vec<u16> = (0..n).map(|_| r.rs()).collect();
                strings.push(String::from_utf16_lossy(&units));
            } else {
                let n = r.rc() as usize;
                r.rc();
                r.rc();
                if n * 8 > r.left().max(0) as usize {
                    break;
                }
                let bytes: Vec<u8> = (0..n).map(|_| r.rc()).collect();
                strings.push(super::bits::cp1252(&bytes));
            }
        }
        out.push((app, strings));
        d.pos = end;
    }
    out
}

/// The common entity part (data and handles).
pub fn entity_common(o: &mut Obj) -> Common {
    let ver = o.ver;
    o.handle = o.d.h().value;
    o.eed = read_eed(&mut o.d);
    if o.d.b() {
        // Preview graphics.
        let n = if ver >= Ver::R2010 { o.d.bll() as usize } else { o.d.rl() as usize };
        o.d.pos = o.d.pos.saturating_add(n.saturating_mul(8));
    }
    let entmode = o.d.bb();
    let reactors = o.d.bl();
    let xdic_missing = ver >= Ver::R2004 && o.d.b();
    let nolinks = if ver == Ver::R2000 { o.d.b() } else { true };
    if ver >= Ver::R2013 {
        o.d.b();
    }
    let (color, book) = if ver >= Ver::R2004 { o.d.enc() } else { (Color { index: o.d.bs() as i16, rgb: None }, false) };
    o.d.bd();
    let ltype_flags = o.d.bb();
    let (mut material, mut shadow) = (0, 0);
    let plotstyle = if ver < Ver::R2007 {
        o.d.bb()
    } else {
        material = o.d.bb();
        let p = o.d.bb();
        shadow = o.d.rc();
        p
    };
    let mut vs = [false; 3];
    if ver >= Ver::R2010 {
        for v in &mut vs {
            *v = o.d.b();
        }
    }
    let invisible = o.d.bs() & 1 != 0;
    let lineweight = o.d.rc();

    let mut c = Common { entmode, color, ltype_flags, lineweight, invisible, ..Default::default() };
    if book {
        o.href();
    }
    if entmode == 0 {
        c.owner = o.href();
    }
    for _ in 0..reactors.min(10000) {
        o.href();
    }
    if !xdic_missing {
        o.href();
    }
    if !nolinks {
        o.href();
        o.href();
    }
    c.layer = o.href();
    if ltype_flags == 3 {
        c.ltype = o.href();
    }
    if ver >= Ver::R2007 {
        if material == 3 {
            o.href();
        }
        if shadow == 3 {
            o.href();
        }
    }
    if plotstyle == 3 {
        o.href();
    }
    for v in vs {
        if v {
            o.href();
        }
    }
    c
}

/// The common part of a non-entity object; returns the owner.
pub fn object_common(o: &mut Obj) -> u64 {
    let ver = o.ver;
    o.handle = o.d.h().value;
    o.eed = read_eed(&mut o.d);
    let reactors = o.d.bl();
    let xdic_missing = ver >= Ver::R2004 && o.d.b();
    if ver >= Ver::R2013 {
        o.d.b();
    }
    let owner = o.href();
    for _ in 0..reactors.min(10000) {
        o.href();
    }
    if !xdic_missing {
        o.href();
    }
    owner
}

/// The common part of a table record: its name (and the xref handle).
pub fn table_common(o: &mut Obj) -> String {
    object_common(o);
    let name = o.t();
    if o.ver <= Ver::R2004 {
        o.d.b();
        o.d.bs();
        o.d.b();
    } else {
        o.d.bs();
    }
    o.href();
    name
}

pub type P2 = (f64, f64);

/// A drawing entity as read.
#[derive(Clone, Debug, PartialEq)]
pub enum Ent {
    Line(P2, P2),
    Circle(P2, f64),
    /// Centre, radius, start and end angle (radians).
    Arc(P2, f64, f64, f64),
    Point(P2),
    /// Centre, major axis end (relative), ratio, start, end.
    Ellipse(P2, P2, f64, f64, f64),
    /// Vertices with bulges, closed.
    Poly(Vec<(P2, f64)>, bool),
    /// Four corners (the third and fourth may be equal).
    Solid([P2; 4]),
    Text {
        pos: P2,
        align: Option<P2>,
        height: f64,
        rotation: f64,
        width: f64,
        oblique: f64,
        h: u16,
        v: u16,
        text: String,
    },
    MText {
        pos: P2,
        xdir: P2,
        width: f64,
        height: f64,
        attach: u16,
        spacing: f64,
        text: String,
    },
    Insert {
        block: u64,
        pos: P2,
        scale: (f64, f64),
        rotation: f64,
        grid: Option<(u16, u16, f64, f64)>,
    },
    /// An attribute of a block reference (or a definition in a block).
    Attrib {
        tag: String,
        value: String,
        def: bool,
        prompt: String,
        text: Box<Ent>,
    },
    Spline {
        degree: u32,
        knots: Vec<f64>,
        ctrl: Vec<P2>,
        fit: Vec<P2>,
        closed: bool,
    },
    /// Polyline header (vertices follow as separate objects).
    PolyHeader {
        closed: bool,
    },
    /// Construction line (`ray` = one-sided): point and direction.
    XLine(P2, P2, bool),
    /// Raster image: corner, u and v pixel vectors, size in pixels, definition, fade.
    Image {
        pos: P2,
        u: P2,
        v: P2,
        size: P2,
        def: u64,
        fade: u8,
    },
    /// Leader line from its arrow tip (its text is a separate entity).
    Leader(Vec<P2>),
    /// Feature control frame: position and its text codes.
    Tolerance(P2, String),
    /// Boundary loops (already tessellated), pattern, solid, scale, angle.
    Hatch {
        loops: Vec<Vec<P2>>,
        pattern: String,
        solid: bool,
        scale: f64,
        angle: f64,
    },
    Vertex(P2, f64),
    /// Dimension: kind (0 rotated/linear, 1 aligned, 2 angular 2-line,
    /// 3 diameter, 4 radius, 5 angular 3-point, 6 ordinate), its points
    /// keyed by DXF group (10, 11, 13, 14, 15, 16), rotation, text.
    Dim {
        kind: u8,
        pts: Vec<(u8, P2)>,
        rotation: f64,
        text: String,
        xordinate: bool,
        block: u64,
    },
}

fn p2(t: (f64, f64, f64)) -> P2 {
    (t.0, t.1)
}

fn text_data(o: &mut Obj) -> Ent {
    let d = &mut o.d;
    let flags = d.rc();
    if flags & 1 == 0 {
        d.rd();
    }
    let pos = d.p2rd();
    let align = if flags & 2 == 0 { Some(d.p2dd(pos)) } else { None };
    d.be();
    d.bt();
    let oblique = if flags & 4 == 0 { d.rd() } else { 0.0 };
    let rotation = if flags & 8 == 0 { d.rd() } else { 0.0 };
    let height = d.rd();
    let width = if flags & 0x10 == 0 { d.rd() } else { 1.0 };
    let text = o.t();
    let d = &mut o.d;
    if flags & 0x20 == 0 {
        d.bs();
    }
    let h = if flags & 0x40 == 0 { d.bs() } else { 0 };
    let v = if flags & 0x80 == 0 { d.bs() } else { 0 };
    Ent::Text { pos, align, height, rotation, width, oblique, h, v, text }
}

/// Entity types the reader converts.
pub fn is_entity_read(ty: u16) -> bool {
    matches!(ty, 1..=3 | 7 | 8 | 10 | 11 | 15 | 16 | 17..=27 | 31 | 35 | 36 | 40 | 41 | 44..=46 | 77 | 78)
}

/// Decode an entity of a known type; None for other types.
pub fn entity(o: &mut Obj) -> Option<(Common, Ent)> {
    let ty = o.ty;
    if !is_entity_read(ty) {
        return None;
    }
    let c = entity_common(o);
    let ver = o.ver;
    let e = match ty {
        19 => {
            let d = &mut o.d;
            let z0 = d.b();
            let sx = d.rd();
            let ex = d.dd(sx);
            let sy = d.rd();
            let ey = d.dd(sy);
            if !z0 {
                let sz = d.rd();
                d.dd(sz);
            }
            Ent::Line((sx, sy), (ex, ey))
        }
        18 => {
            let cen = p2(o.d.p3bd());
            let r = o.d.bd();
            Ent::Circle(cen, r)
        }
        17 => {
            let d = &mut o.d;
            let cen = p2(d.p3bd());
            let r = d.bd();
            d.bt();
            d.be();
            let s = d.bd();
            let e = d.bd();
            Ent::Arc(cen, r, s, e)
        }
        27 => {
            let d = &mut o.d;
            let x = d.bd();
            let y = d.bd();
            Ent::Point((x, y))
        }
        35 => {
            let d = &mut o.d;
            let cen = p2(d.p3bd());
            let major = p2(d.p3bd());
            d.p3bd();
            let ratio = d.bd();
            let s = d.bd();
            let e = d.bd();
            Ent::Ellipse(cen, major, ratio, s, e)
        }
        31 => {
            let d = &mut o.d;
            d.bt();
            d.bd();
            Ent::Solid([d.p2rd(), d.p2rd(), d.p2rd(), d.p2rd()])
        }
        77 => {
            let d = &mut o.d;
            let flag = d.bs();
            if flag & 4 != 0 {
                d.bd();
            }
            if flag & 8 != 0 {
                d.bd();
            }
            if flag & 2 != 0 {
                d.bd();
            }
            if flag & 1 != 0 {
                d.p3bd();
            }
            let n = d.bl() as usize;
            let nb = if flag & 16 != 0 { d.bl() as usize } else { 0 };
            let nid = if ver >= Ver::R2010 && flag & 1024 != 0 { d.bl() as usize } else { 0 };
            let nw = if flag & 32 != 0 { d.bl() as usize } else { 0 };
            if n > 100_000 || nb > 100_000 || nw > 100_000 || nid > 100_000 {
                return None;
            }
            let mut pts: Vec<P2> = Vec::with_capacity(n);
            for i in 0..n {
                let p = if i == 0 { d.p2rd() } else { d.p2dd(pts[i - 1]) };
                pts.push(p);
            }
            let bulges: Vec<f64> = (0..nb).map(|_| d.bd()).collect();
            for _ in 0..nid {
                d.bl();
            }
            for _ in 0..nw {
                d.bd();
                d.bd();
            }
            let closed = flag & 512 != 0;
            Ent::Poly(pts.into_iter().enumerate().map(|(i, p)| (p, bulges.get(i).copied().unwrap_or(0.0))).collect(), closed)
        }
        1 => {
            let t = text_data(o);
            o.href(); // style
            t
        }
        2 | 3 => {
            let t = text_data(o);
            if ver >= Ver::R2010 {
                o.d.rc();
            }
            if ver >= Ver::R2018 && o.d.rc() > 1 {
                // Multi-line attributes (embedded multiline text) are not read.
                return None;
            }
            let tag = o.t();
            o.d.bs();
            let flags = o.d.rc();
            if ver >= Ver::R2007 {
                o.d.b();
            }
            if ver >= Ver::R2010 {
                o.d.rc();
            }
            let def = ty == 3;
            let prompt = if def { o.t() } else { String::new() };
            let value = if let Ent::Text { text, .. } = &t { text.clone() } else { String::new() };
            let _ = flags;
            Ent::Attrib { tag, value, def, prompt, text: Box::new(t) }
        }
        44 => {
            let d = &mut o.d;
            let pos = p2(d.p3bd());
            d.p3bd();
            let xdir = p2(d.p3bd());
            let width = d.bd();
            if ver >= Ver::R2007 {
                d.bd();
            }
            let height = d.bd();
            let attach = d.bs();
            d.bs();
            d.bd();
            d.bd();
            let text = o.t();
            let d = &mut o.d;
            let mut spacing = 1.0;
            if ver >= Ver::R2000 {
                d.bs();
                spacing = d.bd();
                d.b();
            }
            Ent::MText { pos, xdir, width, height, attach, spacing, text }
        }
        7 | 8 => {
            let d = &mut o.d;
            let pos = p2(d.p3bd());
            let (sx, sy) = match d.bb() {
                3 => (1.0, 1.0),
                1 => {
                    let y = d.dd(1.0);
                    d.dd(1.0);
                    (1.0, y)
                }
                2 => {
                    let x = d.rd();
                    (x, x)
                }
                _ => {
                    let x = d.rd();
                    let y = d.dd(x);
                    d.dd(x);
                    (x, y)
                }
            };
            let rotation = d.bd();
            let ext = d.p3bd();
            let has_attribs = d.b();
            let owned = if ver >= Ver::R2004 && has_attribs { d.bl() } else { 0 };
            let grid = if ty == 8 { Some((d.bs(), d.bs(), d.bd(), d.bd())) } else { None };
            let block = o.href();
            let _ = owned;
            // A mirrored reference (extrusion pointing down) flips x.
            let sx = if ext.2 < 0.0 { -sx } else { sx };
            Ent::Insert { block, pos, scale: (sx, sy), rotation, grid }
        }
        15 => {
            let d = &mut o.d;
            let flag = d.bs();
            Ent::PolyHeader { closed: flag & 1 != 0 }
        }
        16 => {
            let d = &mut o.d;
            d.rc();
            let flag = d.rc();
            Ent::PolyHeader { closed: flag & 1 != 0 }
        }
        11 => {
            let d = &mut o.d;
            d.rc();
            Ent::Vertex(p2(d.p3bd()), 0.0)
        }
        40 | 41 => {
            let d = &mut o.d;
            let p = p2(d.p3bd());
            let dir = p2(d.p3bd());
            Ent::XLine(p, dir, ty == 40)
        }
        46 => {
            let d = &mut o.d;
            let p = p2(d.p3bd());
            d.p3bd();
            d.p3bd();
            let text = o.t();
            Ent::Tolerance(p, text)
        }
        78 => hatch(o)?,
        45 => {
            let d = &mut o.d;
            d.b();
            d.bs();
            d.bs();
            let n = d.bl() as usize;
            if n > 10_000 {
                return None;
            }
            Ent::Leader((0..n).map(|_| p2(d.p3bd())).collect())
        }
        10 => {
            let d = &mut o.d;
            d.rc();
            let p = p2(d.p3bd());
            let sw = d.bd();
            if sw >= 0.0 {
                d.bd();
            }
            let bulge = d.bd();
            Ent::Vertex(p, bulge)
        }
        36 => {
            let d = &mut o.d;
            let mut scenario = d.bl();
            if ver >= Ver::R2013 {
                let flags = d.bl();
                let knotparam = d.bl();
                scenario = if knotparam == 15 {
                    1
                } else if flags & 1 != 0 {
                    2
                } else {
                    scenario
                };
            }
            let degree = d.bl();
            if scenario & 1 != 0 {
                d.b();
                let closed = d.b();
                d.b();
                d.bd();
                d.bd();
                let nk = d.bl() as usize;
                let nc = d.bl() as usize;
                let weighted = d.b();
                if nk > 100_000 || nc > 100_000 {
                    return None;
                }
                let knots: Vec<f64> = (0..nk).map(|_| d.bd()).collect();
                let mut ctrl = Vec::with_capacity(nc);
                for _ in 0..nc {
                    ctrl.push(p2(d.p3bd()));
                    if weighted {
                        d.bd();
                    }
                }
                Ent::Spline { degree, knots, ctrl, fit: vec![], closed }
            } else {
                d.bd();
                d.p3bd();
                d.p3bd();
                let nf = d.bl() as usize;
                if nf > 100_000 {
                    return None;
                }
                let fit: Vec<P2> = (0..nf).map(|_| p2(d.p3bd())).collect();
                Ent::Spline { degree, knots: vec![], ctrl: vec![], fit, closed: false }
            }
        }
        6 => return None,
        20..=26 => dimension(o, ty)?,
        _ => return None,
    };
    if o.d.overrun || o.h.overrun {
        return None;
    }
    Some((c, e))
}

fn arc_pts(c: P2, r: f64, s: f64, e: f64, ccw: bool) -> Vec<P2> {
    // Clockwise arcs store their angles mirrored.
    let (s, e) = if ccw { (s, e) } else { (-s, -e) };
    let mut sweep = e - s;
    while sweep <= 0.0 {
        sweep += std::f64::consts::TAU;
    }
    let n = ((sweep / std::f64::consts::TAU) * 64.0).ceil().max(2.0) as usize;
    (0..=n)
        .map(|i| {
            let a = s + sweep * i as f64 / n as f64;
            let (x, y) = (c.0 + r * a.cos(), c.1 + r * a.sin());
            if ccw {
                (x, y)
            } else {
                (x, 2.0 * c.1 - y)
            }
        })
        .collect()
}

fn hatch(o: &mut Obj) -> Option<Ent> {
    let ver = o.ver;
    if ver >= Ver::R2004 {
        let d = &mut o.d;
        let gradient = d.bl();
        d.bl();
        d.bd();
        d.bd();
        d.bl();
        d.bd();
        let nc = d.bl() as usize;
        if gradient != 0 && nc > 1000 {
            return None;
        }
        for _ in 0..nc {
            o.d.bd();
            o.cmc();
        }
        o.t();
    }
    let d = &mut o.d;
    d.bd();
    d.p3bd();
    let pattern = o.t();
    let d = &mut o.d;
    let solid = d.b();
    d.b();
    let np = d.bl() as usize;
    if np > 10_000 {
        return None;
    }
    let mut loops = vec![];
    let mut derived = false;
    for _ in 0..np {
        let flag = d.bl();
        derived |= flag & 4 != 0;
        let mut pts: Vec<P2> = vec![];
        if flag & 2 == 0 {
            let ns = d.bl() as usize;
            if ns > 10_000 {
                return None;
            }
            for _ in 0..ns {
                match d.rc() {
                    1 => {
                        let a = d.p2rd();
                        let b = d.p2rd();
                        if pts.last().is_none_or(|l| (l.0 - a.0).hypot(l.1 - a.1) > 1e-9) {
                            pts.push(a);
                        }
                        pts.push(b);
                    }
                    2 => {
                        let c = d.p2rd();
                        let r = d.bd();
                        let s = d.bd();
                        let e = d.bd();
                        let ccw = d.b();
                        pts.extend(arc_pts(c, r, s, e, ccw));
                    }
                    3 => {
                        let c = d.p2rd();
                        let m = d.p2rd();
                        let ratio = d.bd();
                        let s = d.bd();
                        let e = d.bd();
                        let ccw = d.b();
                        let mut ps = icad_core::curves::ellipse_points(
                            icad_core::geom::V2::new(c.0, c.1),
                            icad_core::geom::V2::new(m.0, m.1),
                            ratio,
                            s,
                            e,
                            (m.0.hypot(m.1) * 0.002).max(1e-6),
                        );
                        if !ccw {
                            ps.reverse();
                        }
                        pts.extend(ps.into_iter().map(|p| (p.x, p.y)));
                    }
                    4 => {
                        let degree = d.bl() as usize;
                        let rational = d.b();
                        d.b();
                        let nk = d.bl() as usize;
                        let nc = d.bl() as usize;
                        if nk > 10_000 || nc > 10_000 {
                            return None;
                        }
                        let knots: Vec<f64> = (0..nk).map(|_| d.bd()).collect();
                        let mut ctrl = vec![];
                        for _ in 0..nc {
                            let p = d.p2rd();
                            if rational {
                                d.bd();
                            }
                            ctrl.push(icad_core::geom::V2::new(p.0, p.1));
                        }
                        if ver >= Ver::R2010 {
                            let nf = d.bl() as usize;
                            if nf > 0 {
                                for _ in 0..nf.min(10_000) {
                                    d.p2rd();
                                }
                                d.p2rd();
                                d.p2rd();
                            }
                        }
                        let degree = degree.max(1);
                        if ctrl.len() >= 2 && knots.len() > ctrl.len() + degree {
                            let (t0, t1) = (knots[degree], knots[knots.len() - 1 - degree]);
                            for k in 0..=(nc * 8) {
                                let t = t0 + (t1 - t0) * k as f64 / (nc * 8) as f64;
                                let p = icad_core::curves::eval_bspline(&ctrl, &knots, degree, t);
                                pts.push((p.x, p.y));
                            }
                        }
                    }
                    _ => return None,
                }
            }
        } else {
            let bulges = d.b();
            let closed = d.b();
            let nv = d.bl() as usize;
            if nv > 100_000 {
                return None;
            }
            let mut verts = vec![];
            for _ in 0..nv {
                let p = d.p2rd();
                let b = if bulges { d.bd() } else { 0.0 };
                verts.push((icad_core::geom::V2::new(p.0, p.1), b));
            }
            pts = crate::dxf::bulge_points(&verts, closed).into_iter().map(|p| (p.x, p.y)).collect();
        }
        let nh = d.bl();
        for _ in 0..nh.min(100_000) {
            o.h.h();
        }
        if pts.len() >= 3 {
            loops.push(pts);
        }
    }
    d.bs();
    d.bs();
    let (mut angle, mut scale) = (0.0, 1.0);
    if !solid {
        angle = d.bd();
        scale = d.bd();
        d.b();
        let nl = d.bs() as usize;
        for _ in 0..nl {
            d.bd();
            d.bd();
            d.bd();
            d.bd();
            d.bd();
            let nd = d.bs() as usize;
            for _ in 0..nd {
                d.bd();
            }
        }
    }
    if derived {
        d.bd();
    }
    if loops.is_empty() {
        return None;
    }
    Some(Ent::Hatch { loops, pattern, solid, scale: scale.max(1e-6), angle })
}

/// Stand-in type number for the class-defined arc-length dimension.
pub const ARC_DIMENSION: u16 = 0xffff;

/// An arc-length dimension (a class-defined type).
pub fn arc_dimension(o: &mut Obj) -> Option<(Common, Ent)> {
    let c = entity_common(o);
    let e = dimension(o, ARC_DIMENSION)?;
    (!o.d.overrun && !o.h.overrun).then_some((c, e))
}

fn dimension(o: &mut Obj, ty: u16) -> Option<Ent> {
    let ver = o.ver;
    let d = &mut o.d;
    if ver >= Ver::R2010 {
        d.rc();
    }
    d.p3bd();
    let mid = d.p2rd();
    d.bd();
    d.rc();
    let text = o.t();
    let d = &mut o.d;
    d.bd();
    d.bd();
    d.p3bd();
    d.bd();
    d.bs();
    d.bs();
    d.bd();
    d.bd();
    if ver >= Ver::R2007 {
        d.b();
        d.b();
        d.b();
    }
    d.p2rd();
    let mut pts = vec![(11u8, mid)];
    let mut rotation = 0.0;
    let mut xordinate = false;
    let kind = match ty {
        20 => {
            pts.push((10, p2(d.p3bd())));
            pts.push((13, p2(d.p3bd())));
            pts.push((14, p2(d.p3bd())));
            xordinate = d.rc() & 1 != 0;
            6
        }
        21 | 22 => {
            pts.push((13, p2(d.p3bd())));
            pts.push((14, p2(d.p3bd())));
            pts.push((10, p2(d.p3bd())));
            d.bd();
            if ty == 21 {
                rotation = d.bd();
                0
            } else {
                1
            }
        }
        23 => {
            pts.push((10, p2(d.p3bd())));
            pts.push((13, p2(d.p3bd())));
            pts.push((14, p2(d.p3bd())));
            pts.push((15, p2(d.p3bd())));
            5
        }
        24 => {
            // The arc position comes first (DXF 10), the second line's end
            // last (DXF 16).
            pts.push((10, d.p2rd()));
            pts.push((13, p2(d.p3bd())));
            pts.push((14, p2(d.p3bd())));
            pts.push((15, p2(d.p3bd())));
            pts.push((16, p2(d.p3bd())));
            2
        }
        25 => {
            pts.push((10, p2(d.p3bd())));
            pts.push((15, p2(d.p3bd())));
            d.bd();
            4
        }
        26 => {
            pts.push((15, p2(d.p3bd())));
            pts.push((10, p2(d.p3bd())));
            d.bd();
            3
        }
        ARC_DIMENSION => {
            pts.push((10, p2(d.p3bd())));
            pts.push((13, p2(d.p3bd())));
            pts.push((14, p2(d.p3bd())));
            pts.push((15, p2(d.p3bd())));
            7
        }
        _ => return None,
    };
    o.href(); // dimension style
    let block = o.href();
    Some(Ent::Dim { kind, pts, rotation, text, xordinate, block })
}

#[derive(Clone, Debug, Default)]
pub struct LayerRec {
    pub name: String,
    pub color: i16,
    pub off: bool,
    pub frozen: bool,
    pub locked: bool,
    pub plot: bool,
    pub lineweight: u8,
    pub ltype: u64,
}

pub fn layer(o: &mut Obj) -> LayerRec {
    let name = table_common(o);
    let flags = o.d.bs();
    let color = o.cmc();
    if o.ver >= Ver::R2000 {
        o.href(); // plot style
    }
    if o.ver >= Ver::R2007 {
        o.href(); // material
    }
    let ltype = o.href();
    LayerRec {
        name,
        color: color.index,
        off: flags & 2 != 0 || color.index < 0,
        frozen: flags & 1 != 0,
        locked: flags & 8 != 0,
        plot: flags & 16 != 0,
        lineweight: ((flags & 0x3e0) >> 5) as u8,
        ltype,
    }
}

#[derive(Clone, Debug, Default)]
pub struct BlockRec {
    pub name: String,
    pub base: P2,
    pub xref: bool,
    /// The external drawing of an xref.
    pub path: String,
}

pub fn block_header(o: &mut Obj) -> BlockRec {
    let name = table_common(o);
    let d = &mut o.d;
    d.b();
    d.b();
    let xref = d.b();
    let overlaid = d.b();
    d.b();
    if o.ver >= Ver::R2004 && !xref && !overlaid {
        d.bl();
    }
    let base = p2(d.p3bd());
    let path = o.t();
    BlockRec { name, base, xref: xref || overlaid, path }
}

/// A raster image entity (a class-defined type).
pub fn image(o: &mut Obj) -> Option<(Common, Ent)> {
    let c = entity_common(o);
    let d = &mut o.d;
    if d.bl() > 10 {
        return None;
    }
    let pos = p2(d.p3bd());
    let u = p2(d.p3bd());
    let v = p2(d.p3bd());
    let size = d.p2rd();
    d.bs();
    d.b();
    d.rc();
    d.rc();
    let fade = d.rc();
    let def = o.href();
    (!o.d.overrun).then_some((c, Ent::Image { pos, u, v, size, def, fade: fade.min(100) }))
}

/// The file of an image definition.
pub fn imagedef(o: &mut Obj) -> Option<String> {
    object_common(o);
    if o.d.bl() > 10 {
        return None;
    }
    o.d.p2rd();
    let path = o.t();
    (!o.d.overrun && !path.is_empty()).then_some(path)
}

/// The group-code strings of an XRECORD (other values end the list).
pub fn xrecord_strings(o: &mut Obj) -> Option<Vec<(u16, String)>> {
    object_common(o);
    let size = o.d.bl() as usize;
    if size * 8 > o.d.left().max(0) as usize {
        return None;
    }
    let end = o.d.pos + size * 8;
    let mut r = o.d.sub(o.d.pos, end);
    let mut out = vec![];
    while r.left() >= 16 && !r.overrun {
        let code = r.rs();
        // Strings: 1–9, 100–102, 300–309, 410–419, 430–439, 470–479, 999, 1000–1009.
        let string = matches!(code, 1..=9 | 100..=102 | 300..=309 | 410..=419 | 430..=439 | 470..=479 | 999 | 1000..=1009);
        if !string {
            break;
        }
        let n = r.rs() as usize;
        if o.ver >= Ver::R2007 {
            if n * 16 > r.left().max(0) as usize {
                break;
            }
            let units: Vec<u16> = (0..n).map(|_| r.rs()).collect();
            out.push((code, String::from_utf16_lossy(&units)));
        } else {
            r.rc();
            if n * 8 > r.left().max(0) as usize {
                break;
            }
            let bytes: Vec<u8> = (0..n).map(|_| r.rc()).collect();
            out.push((code, super::bits::cp1252(&bytes)));
        }
    }
    (!r.overrun).then_some(out)
}

/// Handle and type of each object, for the caller to pick from.
pub fn handle_of(o: &mut Obj) -> Handle {
    let mut probe = o.d.clone();
    probe.h()
}
