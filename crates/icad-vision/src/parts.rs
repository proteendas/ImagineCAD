// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Proteen Das

//! AI part reconstruction: when the vision model can describe the sketched
//! part parametrically (a turned shaft, a round part such as a ring, bush,
//! flange or pulley, or a rectangular plate), ImagineCAD
//! builds an exact, fully dimensioned drawing from the written values instead
//! of tracing pencil strokes.

use icad_core::document::layers;
use icad_core::entity::{DimKind, Dimension, Hatch, Leader, Shape};
use icad_core::geom::*;
use icad_core::prim::text_width;
use icad_core::sheet::{fit_margin, Sheet};
use icad_core::style::DimStyle;
use serde_json::Value;

#[derive(Clone, Debug, PartialEq)]
pub struct Segment {
    pub length: f64,
    pub diameter: f64,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Keyway {
    /// Distance from the left end of the shaft to the start of the keyway.
    pub start: f64,
    pub length: f64,
    pub width: f64,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Chamfer {
    /// Boundary index: 0 = left end, i = between segment i-1 and i, n = right end.
    pub at: usize,
    pub size: f64,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Hole {
    pub x: f64,
    pub y: f64,
    pub diameter: f64,
}

/// Grooves round the rim of a pulley (V-belt grooves when `angle` > 0).
#[derive(Clone, Debug, PartialEq)]
pub struct Grooves {
    pub count: usize,
    /// Included angle (degrees); 0 for square grooves.
    pub angle: f64,
    pub depth: f64,
    /// Width at the rim (0 = from the rim width).
    pub width: f64,
}

/// A round part seen end-on and in section: ring, washer, bush, flange,
/// wheel or pulley. Lengths along the axis, diameters across it.
#[derive(Clone, Debug, PartialEq)]
pub struct Wheel {
    /// "ring", "pulley", "flange", "bush", "wheel" (for the title and wording).
    pub kind: String,
    pub od: f64,
    /// Axial width of the rim (thickness of a ring).
    pub width: f64,
    /// Bore diameter (0 = solid).
    pub bore: f64,
    /// Hub diameter and axial length, when the hub differs from the rim.
    pub hub: Option<(f64, f64)>,
    /// Axial thickness of the web joining hub and rim (None = solid disc).
    pub web: Option<f64>,
    /// Radial thickness of the rim when there is a web.
    pub rim: Option<f64>,
    pub grooves: Option<Grooves>,
    /// Keyway in the bore: width and depth (from the bore surface).
    pub keyway: Option<(f64, f64)>,
    /// Bolt holes: count, diameter, pitch circle diameter.
    pub holes: Option<(usize, f64, f64)>,
}

/// A dimension of a [`Profile`], on its exact geometry (mm).
#[derive(Clone, Debug, PartialEq)]
pub enum PDim {
    /// Horizontal, vertical or aligned distance between two points.
    Linear {
        kind: DimKind,
        a: V2,
        b: V2,
        value: f64,
    },
    Diameter {
        c: V2,
        value: f64,
    },
    Radius {
        c: V2,
        value: f64,
    },
    /// Angle (degrees) at `v` between the rays to `a` and `b`.
    Angle {
        v: V2,
        a: V2,
        b: V2,
        value: f64,
    },
}

/// Any other part: exact geometry (mm, y up, views placed in projection)
/// built by the model from the written values, with the dimensions it read.
/// Every dimension is measured on the geometry before the part is accepted.
#[derive(Clone, Debug, PartialEq)]
pub struct Profile {
    pub name: String,
    /// (layer, shape): lines, arcs, circles and polylines.
    pub shapes: Vec<(String, Shape)>,
    pub dims: Vec<PDim>,
}

#[derive(Clone, Debug, PartialEq)]
pub enum PartSpec {
    Shaft { segments: Vec<Segment>, keyways: Vec<Keyway>, chamfers: Vec<Chamfer>, total_length: Option<f64> },
    Plate { width: f64, height: f64, holes: Vec<Hole> },
    Wheel(Wheel),
    Profile(Profile),
}

fn pt(v: &Value) -> Option<V2> {
    let a = v.as_array()?;
    Some(V2::new(num(a.first()?)?, num(a.get(1)?)?))
}

fn nums(v: &Value) -> Option<Vec<f64>> {
    v.as_array()?.iter().map(num).collect()
}

impl Profile {
    fn from_json(p: &Value) -> Option<Profile> {
        let mut shapes = vec![];
        for e in p.get("entities")?.as_array()? {
            let layer = match e.get("style").and_then(|s| s.as_str()).unwrap_or("") {
                s if s.eq_ignore_ascii_case("hidden") => layers::HIDDEN,
                s if s.eq_ignore_ascii_case("center") || s.eq_ignore_ascii_case("centre") => layers::CENTER,
                _ => layers::OUTLINE,
            };
            let shape = if let Some(v) = e.get("line").and_then(nums).filter(|v| v.len() == 4) {
                Shape::Line { a: V2::new(v[0], v[1]), b: V2::new(v[2], v[3]) }
            } else if let Some(v) = e.get("circle").and_then(nums).filter(|v| v.len() == 3 && v[2] > 0.0) {
                Shape::Circle { c: V2::new(v[0], v[1]), r: v[2] / 2.0 }
            } else if let Some(v) = e.get("arc").and_then(nums).filter(|v| v.len() == 5 && v[2] > 0.0) {
                Shape::Arc { c: V2::new(v[0], v[1]), r: v[2], start: norm_angle(v[3].to_radians()), end: norm_angle(v[4].to_radians()) }
            } else if let Some(pts) =
                e.get("polyline").and_then(|v| v.as_array()).and_then(|a| a.iter().map(pt).collect::<Option<Vec<_>>>())
            {
                if pts.len() < 2 {
                    continue;
                }
                Shape::Polyline { pts, closed: e.get("closed").and_then(|c| c.as_bool()).unwrap_or(false) }
            } else {
                continue;
            };
            shapes.push((layer.to_string(), shape));
        }
        if shapes.is_empty() {
            return None;
        }
        let mut dims = vec![];
        for d in p.get("dimensions").and_then(|d| d.as_array()).into_iter().flatten() {
            let kind = d.get("kind").or_else(|| d.get("type")).and_then(|k| k.as_str()).unwrap_or("").to_ascii_lowercase();
            let Some(value) = d.get("value").and_then(num).filter(|v| *v > 0.0) else { continue };
            let get = |k: &str| d.get(k).and_then(pt);
            let dim = if kind.starts_with("dia") {
                get("center").map(|c| PDim::Diameter { c, value })
            } else if kind.starts_with("rad") {
                get("center").map(|c| PDim::Radius { c, value })
            } else if kind.starts_with("ang") {
                match (get("vertex"), get("from"), get("to")) {
                    (Some(v), Some(a), Some(b)) => Some(PDim::Angle { v, a, b, value }),
                    _ => None,
                }
            } else {
                let k = if kind.starts_with('h') {
                    DimKind::Horizontal
                } else if kind.starts_with('v') {
                    DimKind::Vertical
                } else {
                    DimKind::Aligned
                };
                match (get("from"), get("to")) {
                    (Some(a), Some(b)) => Some(PDim::Linear { kind: k, a, b, value }),
                    _ => None,
                }
            };
            dims.extend(dim);
        }
        let name = p.get("name").and_then(|n| n.as_str()).unwrap_or("part").to_string();
        Some(Profile { name, shapes, dims })
    }

    fn to_json(&self) -> Value {
        let ents: Vec<Value> = self
            .shapes
            .iter()
            .filter_map(|(l, sh)| {
                let mut e = match sh {
                    Shape::Line { a, b } => serde_json::json!({"line": [a.x, a.y, b.x, b.y]}),
                    Shape::Circle { c, r } => serde_json::json!({"circle": [c.x, c.y, 2.0 * r]}),
                    Shape::Arc { c, r, start, end } => serde_json::json!({"arc": [c.x, c.y, r, start.to_degrees(), end.to_degrees()]}),
                    Shape::Polyline { pts, closed } => {
                        serde_json::json!({"polyline": pts.iter().map(|p| [p.x, p.y]).collect::<Vec<_>>(), "closed": closed})
                    }
                    _ => return None,
                };
                if l == layers::HIDDEN {
                    e["style"] = "hidden".into();
                } else if l == layers::CENTER {
                    e["style"] = "center".into();
                }
                Some(e)
            })
            .collect();
        let dims: Vec<Value> = self
            .dims
            .iter()
            .map(|d| match d {
                PDim::Linear { kind, a, b, value } => serde_json::json!({"kind": match kind { DimKind::Horizontal => "horizontal", DimKind::Vertical => "vertical", _ => "aligned" }, "from": [a.x, a.y], "to": [b.x, b.y], "value": value}),
                PDim::Diameter { c, value } => serde_json::json!({"kind": "diameter", "center": [c.x, c.y], "value": value}),
                PDim::Radius { c, value } => serde_json::json!({"kind": "radius", "center": [c.x, c.y], "value": value}),
                PDim::Angle { v, a, b, value } => serde_json::json!({"kind": "angle", "vertex": [v.x, v.y], "from": [a.x, a.y], "to": [b.x, b.y], "value": value}),
            })
            .collect();
        serde_json::json!({"type": "profile", "name": self.name, "entities": ents, "dimensions": dims})
    }

    fn extent(&self) -> BBox {
        self.shapes.iter().fold(BBox::empty(), |b, (_, s)| b.union(&shape_box(s)))
    }

    /// Points a dimension may start from: ends, vertices and centres.
    fn anchors(&self) -> Vec<V2> {
        let mut v = vec![];
        for (_, s) in &self.shapes {
            match s {
                Shape::Line { a, b } => v.extend([*a, *b]),
                Shape::Polyline { pts, .. } => v.extend(pts.iter().copied()),
                Shape::Circle { c, r } => {
                    v.extend([*c, *c + V2::new(*r, 0.0), *c - V2::new(*r, 0.0), *c + V2::new(0.0, *r), *c - V2::new(0.0, *r)])
                }
                Shape::Arc { c, r, start, end } => v.extend([*c, *c + V2::polar(*r, *start), *c + V2::polar(*r, *end)]),
                _ => {}
            }
        }
        v
    }

    fn on_geometry(&self, p: V2, tol: f64) -> bool {
        self.anchors().iter().any(|q| q.dist(p) <= tol)
            || self.shapes.iter().any(|(_, s)| match s {
                Shape::Line { a, b } => dist_point_segment(p, *a, *b) <= tol,
                Shape::Polyline { pts, closed } => {
                    let n = pts.len();
                    (0..n - 1).any(|i| dist_point_segment(p, pts[i], pts[i + 1]) <= tol)
                        || (*closed && n > 2 && dist_point_segment(p, pts[n - 1], pts[0]) <= tol)
                }
                Shape::Circle { c, r } | Shape::Arc { c, r, .. } => (c.dist(p) - r).abs() <= tol,
                _ => false,
            })
    }

    /// Dimensions that do not measure what they say on the geometry, or that
    /// do not touch it.
    pub fn self_check(&self) -> Vec<String> {
        let mut out = vec![];
        let ext = self.extent();
        let size = ext.width().max(ext.height());
        if !(size.is_finite() && size > 0.0) {
            return vec!["The geometry has no size.".into()];
        }
        let tol = (size * 0.002).max(1e-6);
        let same = |a: f64, b: f64| (a - b).abs() <= (0.002 * a.abs().max(b.abs())).max(tol);
        let at = |p: &V2| format!("({}, {})", fmt(p.x), fmt(p.y));
        let curves: Vec<(V2, f64)> = self
            .shapes
            .iter()
            .filter_map(|(_, s)| if let Shape::Circle { c, r } | Shape::Arc { c, r, .. } = s { Some((*c, *r)) } else { None })
            .collect();
        for d in &self.dims {
            match d {
                PDim::Linear { kind, a, b, value } => {
                    let m = match kind {
                        DimKind::Horizontal => (b.x - a.x).abs(),
                        DimKind::Vertical => (b.y - a.y).abs(),
                        _ => a.dist(*b),
                    };
                    if !same(m, *value) {
                        out.push(format!("The dimension {} from {} to {} measures {} on the geometry.", fmt(*value), at(a), at(b), fmt(m)));
                    }
                    for p in [a, b] {
                        if !self.on_geometry(*p, tol * 2.0) {
                            out.push(format!("The dimension {} starts at {}, which is not on the geometry.", fmt(*value), at(p)));
                        }
                    }
                }
                PDim::Diameter { c, value } | PDim::Radius { c, value } => {
                    let want = if matches!(d, PDim::Diameter { .. }) { value / 2.0 } else { *value };
                    if !curves.iter().any(|(cc, r)| cc.dist(*c) <= tol * 2.0 && same(*r, want)) {
                        let what = if matches!(d, PDim::Diameter { .. }) { "Ø" } else { "R" };
                        out.push(format!("There is no circle or arc {what}{} centred at {}.", fmt(*value), at(c)));
                    }
                }
                PDim::Angle { v, a, b, value } => {
                    let (u, w) = (*a - *v, *b - *v);
                    let m = u.cross(w).atan2(u.dot(w)).abs().to_degrees();
                    if (m - value).abs() > 0.2 && (180.0 - m - value).abs() > 0.2 {
                        out.push(format!("The angle {}° at {} measures {}° on the geometry.", fmt(*value), at(v), fmt(m)));
                    }
                }
            }
        }
        out
    }

    fn values(&self) -> Vec<f64> {
        self.dims
            .iter()
            .map(|d| match d {
                PDim::Linear { value, .. } | PDim::Diameter { value, .. } | PDim::Radius { value, .. } | PDim::Angle { value, .. } => {
                    *value
                }
            })
            .collect()
    }

    /// Exact geometry, centre marks and dimensions laid out clear of the part
    /// and of each other.
    fn build(&self) -> Vec<(String, Shape)> {
        let mut out = self.shapes.clone();
        let ext = self.extent();
        let ds = DimStyle::default();
        let mut probe = Sheet::default();
        probe.fit_to(&fit_margin(&ext));
        let th = ds.text_height * probe.scale;
        let centre = ext.center();
        let dim = |d: Dimension| (layers::DIMENSIONS.to_string(), Shape::Dim(d));
        // Centre marks on circles and on arcs of half a turn or more.
        for (_, s) in &self.shapes {
            let (c, r) = match s {
                Shape::Circle { c, r } => (*c, *r),
                Shape::Arc { c, r, start, end } if arc_sweep(*start, *end) >= std::f64::consts::PI - 1e-6 => (*c, *r),
                _ => continue,
            };
            let e = r + (0.15 * r).max(th * 0.6);
            out.push((layers::CENTER.into(), Shape::Line { a: c - V2::new(e, 0.0), b: c + V2::new(e, 0.0) }));
            out.push((layers::CENTER.into(), Shape::Line { a: c - V2::new(0.0, e), b: c + V2::new(0.0, e) }));
        }
        // Linear dimensions in rows outside the part, shortest nearest.
        let mut sides: [Vec<(f64, f64, usize)>; 4] = Default::default(); // below, above, left, right
        let lin: Vec<(usize, DimKind, V2, V2, f64)> = self
            .dims
            .iter()
            .enumerate()
            .filter_map(|(i, d)| if let PDim::Linear { kind, a, b, value } = d { Some((i, *kind, *a, *b, *value)) } else { None })
            .collect();
        let mut order: Vec<&(usize, DimKind, V2, V2, f64)> = lin.iter().collect();
        order.sort_by(|p, q| p.2.dist(p.3).total_cmp(&q.2.dist(q.3)));
        let pad = 0.6 * th;
        for &&(i, kind, a, b, value) in &order {
            let tw = text_width(&fmt(value), th);
            match kind {
                DimKind::Horizontal | DimKind::Vertical => {
                    let h = kind == DimKind::Horizontal;
                    let mid = a.mid(b);
                    let side = if h { usize::from(mid.y >= centre.y) } else { 2 + usize::from(mid.x >= centre.x) };
                    let (lo, hi) = if h { (a.x.min(b.x), a.x.max(b.x)) } else { (a.y.min(b.y), a.y.max(b.y)) };
                    let c = (lo + hi) / 2.0;
                    let iv = (lo.min(c - tw / 2.0) - pad, hi.max(c + tw / 2.0) + pad);
                    let row = (0..).find(|k| sides[side].iter().all(|&(l, hgh, rk)| rk != *k || iv.1 < l || iv.0 > hgh)).unwrap_or(0);
                    sides[side].push((iv.0, iv.1, row));
                    let off = 2.0 * th + row as f64 * 2.2 * th;
                    let loc = match side {
                        0 => V2::new(c, ext.min.y - off),
                        1 => V2::new(c, ext.max.y + off),
                        2 => V2::new(ext.min.x - off, c),
                        _ => V2::new(ext.max.x + off, c),
                    };
                    out.push(dim(Dimension::new(kind, a, b, loc)));
                }
                _ => {
                    let n = (b - a).norm().perp();
                    let n = if (a.mid(b) - centre).dot(n) < 0.0 { -n } else { n };
                    out.push(dim(Dimension::new(DimKind::Aligned, a, b, a.mid(b) + n * (2.2 * th))));
                }
            }
            let _ = i;
        }
        for d in &self.dims {
            match d {
                PDim::Diameter { c, value } | PDim::Radius { c, value } => {
                    let dia = matches!(d, PDim::Diameter { .. });
                    let r = if dia { value / 2.0 } else { *value };
                    let mut u = (*c - centre).norm();
                    if u == V2::ZERO {
                        u = V2::new(1.0, 1.0).norm();
                    }
                    // Arcs: point at the middle of the arc.
                    if let Some((_, Shape::Arc { start, end, .. })) = self
                        .shapes
                        .iter()
                        .find(|(_, s)| matches!(s, Shape::Arc { c: cc, r: rr, .. } if cc.dist(*c) < 1e-6 && (rr - r).abs() < 1e-6))
                    {
                        u = V2::polar(1.0, start + arc_sweep(*start, *end) / 2.0);
                    }
                    let kind = if dia { DimKind::Diameter } else { DimKind::Radius };
                    // The text goes clear of the part.
                    let mut loc = *c + u * (r + 2.5 * th);
                    let clear = ext.expand(1.5 * th);
                    for _ in 0..200 {
                        if !clear.contains(loc) {
                            break;
                        }
                        loc += u * th;
                    }
                    out.push(dim(Dimension::new(kind, *c, *c + u * r, loc)));
                }
                PDim::Angle { v, a, b, .. } => {
                    let (ua, ub) = ((*a - *v).norm(), (*b - *v).norm());
                    let mut bis = (ua + ub).norm();
                    if bis == V2::ZERO {
                        bis = ua.perp();
                    }
                    let reach = v.dist(*a).min(v.dist(*b)) * 0.6;
                    out.push(dim(Dimension::angular(*v, *a, *b, *v + bis * reach.max(3.0 * th))));
                }
                PDim::Linear { .. } => {}
            }
        }
        out
    }
}

fn shape_box(s: &Shape) -> BBox {
    match s {
        Shape::Line { a, b } => BBox::from_pts(*a, *b),
        Shape::Polyline { pts, .. } => BBox::of(pts.iter().copied()),
        Shape::Circle { c, r } => BBox::from_pts(*c - V2::new(*r, *r), *c + V2::new(*r, *r)),
        Shape::Arc { c, r, start, end } => {
            let mut b = BBox::from_pts(*c + V2::polar(*r, *start), *c + V2::polar(*r, *end));
            for k in 0..4 {
                let a = k as f64 * std::f64::consts::FRAC_PI_2;
                if angle_on_arc(a, *start, *end) {
                    b.add(*c + V2::polar(*r, a));
                }
            }
            b
        }
        _ => BBox::empty(),
    }
}

fn num(v: &Value) -> Option<f64> {
    match v {
        Value::Number(n) => n.as_f64(),
        Value::String(s) => crate::labels::parse(s).map(|(_, v)| v).or_else(|| s.trim().parse().ok()),
        _ => None,
    }
    .filter(|x| x.is_finite())
}

fn field(v: &Value, keys: &[&str]) -> Option<f64> {
    keys.iter().find_map(|k| v.get(*k).and_then(num))
}

impl PartSpec {
    /// Parse the `part` object of a model reply. Returns None when absent or unusable.
    pub fn from_json(v: &Value) -> Option<PartSpec> {
        let p = v.get("part")?;
        let ty = p.get("type").and_then(|t| t.as_str()).unwrap_or("").to_ascii_lowercase();
        if ty.contains("profile") || ty.contains("general") || p.get("entities").is_some() {
            return Profile::from_json(p).map(PartSpec::Profile);
        }
        if ty.contains("shaft") || ty.contains("turn") || p.get("segments").is_some() {
            let total = field(p, &["total_length", "overall_length"]);
            let mut raw: Vec<(Option<f64>, Option<f64>)> = p
                .get("segments")?
                .as_array()?
                .iter()
                .map(|s| {
                    let pos = |x: Option<f64>| x.filter(|v| *v > 0.0);
                    (pos(field(s, &["length", "l"])), pos(field(s, &["diameter", "d", "dia"])))
                })
                .filter(|s| s.0.is_some() || s.1.is_some())
                .collect();
            // One missing length follows from the written total.
            let missing: Vec<usize> = (0..raw.len()).filter(|i| raw[*i].0.is_none()).collect();
            if let (Some(t), [i]) = (total, missing.as_slice()) {
                let rest: f64 = raw.iter().filter_map(|s| s.0).sum();
                if t > rest {
                    raw[*i].0 = Some(t - rest);
                }
            }
            // A shaft with a step of unknown size is not drawn from the description
            // (the sketch is traced instead): a wrong part is worse than none.
            let segments: Vec<Segment> = raw.iter().map(|s| Some(Segment { length: s.0?, diameter: s.1? })).collect::<Option<Vec<_>>>()?;
            if segments.is_empty() {
                return None;
            }
            let keyways = p
                .get("keyways")
                .and_then(|k| k.as_array())
                .map(|a| {
                    a.iter()
                        .filter_map(|k| {
                            let length = field(k, &["length"])?;
                            let start = field(k, &["start", "offset", "x"]).unwrap_or(0.0);
                            let width = field(k, &["width"]).unwrap_or(0.0);
                            (length > 0.0).then_some(Keyway { start, length, width })
                        })
                        .collect()
                })
                .unwrap_or_default();
            let chamfers = p
                .get("chamfers")
                .and_then(|k| k.as_array())
                .map(|a| {
                    a.iter()
                        .filter_map(|c| {
                            let size = field(c, &["size", "c"])?;
                            let at = field(c, &["at", "boundary", "between"])?;
                            (size > 0.0 && at >= 0.0).then_some(Chamfer { at: at as usize, size })
                        })
                        .collect()
                })
                .unwrap_or_default();
            return Some(PartSpec::Shaft { segments, keyways, chamfers, total_length: total });
        }
        let round = ["wheel", "ring", "washer", "pulley", "sheave", "flange", "bush", "disc", "disk", "gear", "collar", "spacer"];
        if round.iter().any(|r| ty.contains(r)) || p.get("outer_diameter").is_some() {
            let od = field(p, &["outer_diameter", "od", "diameter"])?;
            let width = field(p, &["width", "thickness", "length", "face_width"])?;
            let pos = |x: Option<f64>| x.filter(|v| *v > 0.0);
            let bore = pos(field(p, &["bore", "inner_diameter", "id"])).unwrap_or(0.0);
            let hub = match (pos(field(p, &["hub_diameter"])), pos(field(p, &["hub_length", "hub_width"]))) {
                (Some(d), Some(l)) => Some((d, l)),
                (Some(d), None) => Some((d, width)),
                (None, Some(l)) => Some(((bore * 1.8).max(od * 0.3).min(od * 0.9), l)),
                _ => None,
            };
            let grooves = p.get("grooves").filter(|g| g.is_object()).and_then(|g| {
                let count = field(g, &["count", "n", "number"]).map(|c| c.round() as usize).unwrap_or(1).max(1);
                let depth = pos(field(g, &["depth", "h"]))?;
                Some(Grooves {
                    count,
                    angle: field(g, &["angle"]).unwrap_or(0.0).clamp(0.0, 120.0),
                    depth,
                    width: field(g, &["width", "top_width"]).unwrap_or(0.0),
                })
            });
            let keyway = p
                .get("keyway")
                .filter(|k| k.is_object())
                .and_then(|k| Some((pos(field(k, &["width", "b"]))?, pos(field(k, &["depth", "t", "t2"]))?)));
            let holes = p.get("holes").filter(|h| h.is_object()).and_then(|h| {
                let n = field(h, &["count", "n", "number"])?.round() as usize;
                Some((n.max(1), pos(field(h, &["diameter", "d"]))?, pos(field(h, &["pcd", "pitch_circle", "pitch_diameter"]))?))
            });
            let kind = round.iter().find(|r| ty.contains(*r)).map(|r| r.to_string()).unwrap_or_else(|| "wheel".into());
            let w = Wheel {
                kind,
                od,
                width,
                bore,
                hub,
                web: pos(field(p, &["web_thickness", "web"])),
                rim: pos(field(p, &["rim_thickness", "rim"])),
                grooves,
                keyway,
                holes,
            };
            return (od > 0.0 && width > 0.0 && bore < od).then_some(PartSpec::Wheel(w));
        }
        if ty.contains("plate") || ty.contains("rect") {
            let width = field(p, &["width", "length", "w"])?;
            let height = field(p, &["height", "h"])?;
            let holes = p
                .get("holes")
                .and_then(|h| h.as_array())
                .map(|a| {
                    a.iter()
                        .filter_map(|h| Some(Hole { x: field(h, &["x"])?, y: field(h, &["y"])?, diameter: field(h, &["diameter", "d"])? }))
                        .filter(|h| h.diameter > 0.0)
                        .collect()
                })
                .unwrap_or_default();
            return (width > 0.0 && height > 0.0).then_some(PartSpec::Plate { width, height, holes });
        }
        None
    }

    /// Gaps in a reply's part description (steps without a length or a
    /// diameter), for a correction round.
    pub fn gaps(v: &Value) -> Vec<String> {
        let mut out = vec![];
        let p = &v["part"];
        for (key, needs) in [("grooves", &["depth"][..]), ("keyway", &["width", "depth"][..]), ("holes", &["count", "diameter", "pcd"][..])]
        {
            if p[key].is_object() {
                for n in needs {
                    if field(&p[key], &[n]).is_none_or(|x| x <= 0.0) {
                        out.push(format!("The {key} have no {n}; read it from the sketch."));
                    }
                }
            }
        }
        for (i, s) in v["part"]["segments"].as_array().into_iter().flatten().enumerate() {
            for (name, keys) in [("length", &["length", "l"][..]), ("diameter", &["diameter", "d", "dia"][..])] {
                if field(s, keys).is_none_or(|x| x <= 0.0) {
                    out.push(format!("Segment {} has no {name}; read it from the sketch.", i + 1));
                }
            }
        }
        out
    }

    /// The `part` object in the same form `from_json` reads.
    pub fn to_json(&self) -> Value {
        match self {
            PartSpec::Shaft { segments, keyways, chamfers, total_length } => serde_json::json!({
                "type": "shaft",
                "total_length": total_length,
                "segments": segments.iter().map(|s| serde_json::json!({"length": s.length, "diameter": s.diameter})).collect::<Vec<_>>(),
                "keyways": keyways.iter().map(|k| serde_json::json!({"start": k.start, "length": k.length, "width": k.width})).collect::<Vec<_>>(),
                "chamfers": chamfers.iter().map(|c| serde_json::json!({"at": c.at, "size": c.size})).collect::<Vec<_>>(),
            }),
            PartSpec::Profile(pr) => pr.to_json(),
            PartSpec::Wheel(w) => serde_json::json!({
                "type": w.kind,
                "outer_diameter": w.od,
                "width": w.width,
                "bore": w.bore,
                "hub_diameter": w.hub.map(|h| h.0),
                "hub_length": w.hub.map(|h| h.1),
                "web_thickness": w.web,
                "rim_thickness": w.rim,
                "grooves": w.grooves.as_ref().map(|g| serde_json::json!({"count": g.count, "angle": g.angle, "depth": g.depth, "width": g.width})),
                "keyway": w.keyway.map(|k| serde_json::json!({"width": k.0, "depth": k.1})),
                "holes": w.holes.map(|h| serde_json::json!({"count": h.0, "diameter": h.1, "pcd": h.2})),
            }),
            PartSpec::Plate { width, height, holes } => serde_json::json!({
                "type": "plate",
                "width": width,
                "height": height,
                "holes": holes.iter().map(|h| serde_json::json!({"x": h.x, "y": h.y, "diameter": h.diameter})).collect::<Vec<_>>(),
            }),
        }
    }

    pub fn describe(&self) -> String {
        match self {
            PartSpec::Shaft { segments, keyways, chamfers, .. } => {
                let len: f64 = segments.iter().map(|s| s.length).sum();
                format!(
                    "shaft: {} segment(s), length {}, Ø {}–{}, {} keyway(s), {} chamfer(s)",
                    segments.len(),
                    fmt(len),
                    fmt(segments.iter().map(|s| s.diameter).fold(f64::INFINITY, f64::min)),
                    fmt(segments.iter().map(|s| s.diameter).fold(0.0, f64::max)),
                    keyways.len(),
                    chamfers.len()
                )
            }
            PartSpec::Plate { width, height, holes } => format!("plate: {} × {}, {} hole(s)", fmt(*width), fmt(*height), holes.len()),
            PartSpec::Profile(p) => format!("{}: {} object(s), {} dimension(s)", p.name, p.shapes.len(), p.dims.len()),
            PartSpec::Wheel(w) => {
                let mut d = format!("{}: Ø{} × {}", w.kind, fmt(w.od), fmt(w.width));
                if w.bore > 0.0 {
                    d += &format!(", bore Ø{}", fmt(w.bore));
                }
                if let Some(g) = &w.grooves {
                    d += &format!(", {} groove(s)", g.count);
                }
                if let Some((n, hd, pcd)) = w.holes {
                    d += &format!(", {n}× Ø{} on PCD {}", fmt(hd), fmt(pcd));
                }
                d
            }
        }
    }

    /// Consistency warnings (e.g. segment lengths not adding up to the written total).
    pub fn check(&self) -> Vec<String> {
        let mut w = vec![];
        if let PartSpec::Shaft { segments, total_length: Some(t), .. } = self {
            let sum: f64 = segments.iter().map(|s| s.length).sum();
            if (sum - t).abs() > 0.01 * t.max(1.0) {
                w.push(format!("Segment lengths add up to {} but the sketch states a total length of {}.", fmt(sum), fmt(*t)));
            }
        }
        w
    }

    /// Where the description disagrees with the values written on the sketch
    /// (`labels`: the dimension texts the model read), as plain sentences for
    /// a correction round. Empty when everything agrees.
    pub fn issues_with(&self, labels: &[String]) -> Vec<String> {
        use crate::labels::LabelKind;
        let mut out = self.check();
        let read: Vec<(LabelKind, f64)> = labels.iter().filter_map(|t| crate::labels::parse(t)).collect();
        let same = |a: f64, b: f64| (a - b).abs() <= 0.005 * a.abs().max(b.abs()).max(1.0);
        let dias: Vec<f64> = read.iter().filter(|r| r.0 == LabelKind::Diameter).map(|r| r.1).collect();
        let lins: Vec<f64> = read.iter().filter(|r| r.0 == LabelKind::Linear).map(|r| r.1).collect();
        match self {
            PartSpec::Shaft { segments, keyways, chamfers, total_length } => {
                for d in &dias {
                    if !segments.iter().any(|s| same(s.diameter, *d)) {
                        out.push(format!("The sketch shows Ø{} but no segment has that diameter.", fmt(*d)));
                    }
                }
                if !dias.is_empty() {
                    for (i, s) in segments.iter().enumerate() {
                        if !dias.iter().any(|d| same(*d, s.diameter)) {
                            out.push(format!("Segment {} has diameter {}, which is not written on the sketch.", i + 1, fmt(s.diameter)));
                        }
                    }
                }
                // A length is used when it is a segment, a run of neighbouring
                // segments, the total, or part of a keyway or chamfer.
                let mut known: Vec<f64> = total_length.iter().copied().collect();
                for i in 0..segments.len() {
                    let mut run = 0.0;
                    for s in &segments[i..] {
                        run += s.length;
                        known.push(run);
                    }
                }
                for k in keyways {
                    known.extend([k.start, k.length, k.width]);
                }
                known.extend(chamfers.iter().map(|c| c.size));
                for v in lins.iter().filter(|v| !known.iter().any(|k| same(*k, **v))) {
                    out.push(format!(
                        "The written length {} is not the length of any segment, of neighbouring segments together, of a keyway or of a chamfer.",
                        fmt(*v)
                    ));
                }
            }
            PartSpec::Profile(pr) => {
                out.extend(pr.self_check());
                let vals = pr.values();
                for r in read.iter().filter(|r| !vals.iter().any(|v| same(*v, r.1))) {
                    out.push(format!("The written value {} has no dimension in the description.", fmt(r.1)));
                }
            }
            PartSpec::Wheel(w) => {
                let mut dia_vals = vec![w.od, w.bore];
                dia_vals.extend(w.hub.map(|h| h.0));
                if let Some((_, hd, pcd)) = w.holes {
                    dia_vals.extend([hd, pcd]);
                }
                if let Some(g) = &w.grooves {
                    dia_vals.push(w.od - 2.0 * g.depth);
                }
                if let Some(r) = w.rim {
                    dia_vals.push(w.od - 2.0 * r);
                }
                for d in &dias {
                    if !dia_vals.iter().any(|v| same(*v, *d)) {
                        out.push(format!("The sketch shows Ø{} but the description has no diameter of that size.", fmt(*d)));
                    }
                }
                if !dias.is_empty() && !dias.iter().any(|d| same(*d, w.od)) {
                    out.push(format!("The outer diameter {} is not written on the sketch.", fmt(w.od)));
                }
                if w.bore > 0.0 && !dias.is_empty() && !dias.iter().any(|d| same(*d, w.bore)) {
                    out.push(format!("The bore Ø{} is not written on the sketch.", fmt(w.bore)));
                }
                let mut known = vec![w.width, w.od, w.bore];
                known.extend(w.hub.map(|h| h.1));
                known.extend(w.web);
                known.extend(w.rim);
                if let Some(g) = &w.grooves {
                    known.extend([g.depth, g.width, g.angle, g.count as f64]);
                    if g.count > 1 {
                        known.push(w.width / g.count as f64);
                    }
                }
                if let Some(k) = w.keyway {
                    known.extend([k.0, k.1, w.bore + k.1]);
                }
                if let Some((n, hd, pcd)) = w.holes {
                    known.extend([n as f64, hd, pcd]);
                }
                if let Some((hd, hl)) = w.hub {
                    known.extend([(hl - w.width) / 2.0, hl - w.width, (hd - w.bore) / 2.0]);
                }
                for v in lins.iter().filter(|v| !known.iter().any(|k| same(*k, **v))) {
                    out.push(format!("The written value {} is not used by the description (groove, web, rim or hub size?).", fmt(*v)));
                }
                let angles: Vec<f64> = read.iter().filter(|r| r.0 == LabelKind::Angle).map(|r| r.1).collect();
                for a in angles {
                    if !w.grooves.as_ref().is_some_and(|g| same(g.angle, a)) {
                        out.push(format!("The angle {}° is not used (groove angle?).", fmt(a)));
                    }
                }
            }
            PartSpec::Plate { width, height, holes } => {
                for d in &dias {
                    if !holes.iter().any(|h| same(h.diameter, *d)) {
                        out.push(format!("The sketch shows Ø{} but no hole has that diameter.", fmt(*d)));
                    }
                }
                if !lins.is_empty() && !lins.iter().any(|v| same(*v, *width)) {
                    out.push(format!("The width {} is not written on the sketch.", fmt(*width)));
                }
                if !lins.is_empty() && !lins.iter().any(|v| same(*v, *height)) {
                    out.push(format!("The height {} is not written on the sketch.", fmt(*height)));
                }
            }
        }
        out
    }

    /// Whether the description is good enough to be drawn as is: it may
    /// differ from the written values in one point (a misread digit) but a
    /// general profile must measure exactly what its own dimensions say.
    pub fn acceptable(&self, labels: &[String]) -> Result<(), Vec<String>> {
        let issues = self.issues_with(labels);
        let own = if let PartSpec::Profile(p) = self { p.self_check().len() } else { 0 };
        if own == 0 && issues.len() <= 1 {
            Ok(())
        } else {
            Err(issues)
        }
    }

    /// Build exact geometry and dimensions (mm).
    pub fn build(&self) -> Vec<(String, Shape)> {
        match self {
            PartSpec::Shaft { segments, keyways, chamfers, total_length } => build_shaft(segments, keyways, chamfers, *total_length),
            PartSpec::Plate { width, height, holes } => build_plate(*width, *height, holes),
            PartSpec::Wheel(w) => build_wheel(w),
            PartSpec::Profile(p) => p.build(),
        }
    }
}

fn fmt(v: f64) -> String {
    let s = format!("{v:.2}");
    s.trim_end_matches('0').trim_end_matches('.').to_string()
}

fn obround(x0: f64, x1: f64, w: f64) -> Vec<Shape> {
    let r = (w / 2.0).max(1e-6).min((x1 - x0) / 2.0);
    let (a, b) = (x0 + r, x1 - r);
    let mut v = vec![
        Shape::Arc { c: V2::new(a, 0.0), r, start: std::f64::consts::FRAC_PI_2, end: 3.0 * std::f64::consts::FRAC_PI_2 },
        Shape::Arc { c: V2::new(b, 0.0), r, start: 3.0 * std::f64::consts::FRAC_PI_2, end: std::f64::consts::FRAC_PI_2 },
    ];
    if b > a {
        v.push(Shape::Line { a: V2::new(a, r), b: V2::new(b, r) });
        v.push(Shape::Line { a: V2::new(a, -r), b: V2::new(b, -r) });
    }
    v
}

fn build_shaft(segments: &[Segment], keyways: &[Keyway], chamfers: &[Chamfer], total: Option<f64>) -> Vec<(String, Shape)> {
    let mut out: Vec<(String, Shape)> = vec![];
    let n = segments.len();
    let mut xs = vec![0.0];
    for s in segments {
        xs.push(xs.last().unwrap() + s.length);
    }
    let length = xs[n];
    let r: Vec<f64> = segments.iter().map(|s| s.diameter / 2.0).collect();
    let rmax = r.iter().cloned().fold(0.0, f64::max);
    let chamfer_at = |b: usize| chamfers.iter().find(|c| c.at == b).map(|c| c.size);

    // Upper profile, left → right, with chamfered corners.
    let mut top: Vec<V2> = vec![];
    for b in 0..=n {
        let x = xs[b];
        let left = if b > 0 { Some(r[b - 1]) } else { None };
        let right = if b < n { Some(r[b]) } else { None };
        let c = chamfer_at(b).unwrap_or(0.0);
        match (left, right) {
            (None, Some(rr)) => {
                let c = c.min(rr * 0.5).min(segments[0].length * 0.5);
                top.push(V2::new(x, 0.0));
                top.push(V2::new(x, rr - c));
                if c > 0.0 {
                    top.push(V2::new(x + c, rr));
                }
            }
            (Some(rl), None) => {
                let c = c.min(rl * 0.5).min(segments[n - 1].length * 0.5);
                if c > 0.0 {
                    top.push(V2::new(x - c, rl));
                }
                top.push(V2::new(x, rl - c));
                top.push(V2::new(x, 0.0));
            }
            (Some(rl), Some(rr)) => {
                if (rl - rr).abs() < 1e-9 {
                    continue;
                }
                if rr > rl {
                    let c = c.min(rr - rl).min(segments[b].length * 0.5);
                    top.push(V2::new(x, rl));
                    top.push(V2::new(x, rr - c));
                    if c > 0.0 {
                        top.push(V2::new(x + c, rr));
                    }
                } else {
                    let c = c.min(rl - rr).min(segments[b - 1].length * 0.5);
                    if c > 0.0 {
                        top.push(V2::new(x - c, rl));
                    }
                    top.push(V2::new(x, rl - c));
                    top.push(V2::new(x, rr));
                }
            }
            (None, None) => {}
        }
    }
    // Close with the mirrored lower half.
    let mut outline = top.clone();
    outline.extend(top.iter().rev().skip(1).take(top.len().saturating_sub(2)).map(|p| V2::new(p.x, -p.y)));
    outline.retain(|p| p.y.abs() > 1e-9 || p.x == 0.0 || (p.x - length).abs() < 1e-9);
    // Remove the centre points at the ends (they were only anchors).
    outline.retain(|p| p.y.abs() > 1e-9);
    out.push((layers::OUTLINE.into(), Shape::Polyline { pts: outline, closed: true }));
    // Shoulder lines across the shaft at internal steps.
    for b in 1..n {
        let span = r[b - 1].min(r[b]);
        if (r[b - 1] - r[b]).abs() > 1e-9 {
            out.push((layers::OUTLINE.into(), Shape::Line { a: V2::new(xs[b], -span), b: V2::new(xs[b], span) }));
        }
    }
    // Annotation sizes in model units, for the sheet scale the drawing will get.
    let ds = DimStyle::default();
    let mut probe = Sheet::default();
    probe.fit_to(&fit_margin(&BBox::from_pts(V2::new(0.0, -rmax), V2::new(length, rmax))));
    let th = ds.text_height * probe.scale;
    let ar = ds.arrow_size * probe.scale;
    let gap = ds.text_gap * probe.scale;
    let pad = 0.5 * th;
    let tw = |s: &str| text_width(s, th);
    let dim = |d: Dimension| (layers::DIMENSIONS.to_string(), Shape::Dim(d));

    // Centre line, a little past both ends.
    let ext = 0.8 * th;
    out.push((layers::CENTER.into(), Shape::Line { a: V2::new(-ext, 0.0), b: V2::new(length + ext, 0.0) }));

    // Keyways (seen from above, drawn on the centre line), length dimensioned above the shaft.
    let mut slots: Vec<(f64, f64)> = vec![];
    for k in keyways {
        let x0 = k.start.clamp(0.0, length);
        let x1 = (k.start + k.length).clamp(0.0, length);
        if x1 - x0 > 1e-6 {
            let seg_r =
                segments.iter().zip(xs.iter()).filter(|(_, x)| **x <= x0 + 1e-9).map(|(s, _)| s.diameter / 2.0).next_back().unwrap_or(rmax);
            let w = if k.width > 0.0 { k.width } else { (seg_r * 0.55).max(1.0) };
            for s in obround(x0, x1, w) {
                out.push((layers::OUTLINE.into(), s));
            }
            out.push(dim(Dimension::new(
                DimKind::Horizontal,
                V2::new(x0, w / 2.0),
                V2::new(x1, w / 2.0),
                V2::new((x0 + x1) / 2.0, rmax + 1.2 * th),
            )));
            slots.push((x0, x1));
        }
    }
    let clear = |a: f64, b: f64| slots.iter().all(|&(s0, s1)| b + 0.2 * th < s0 || a - 0.2 * th > s1);

    // Diameters: inside the segment when the text fits beside any keyway, on the end
    // face for the end segments, otherwise as a leader callout above the shaft.
    let mut callouts: Vec<(V2, String)> = vec![];
    for i in 0..n {
        let label = format!("Ø{}", fmt(segments[i].diameter));
        let (a, b) = (xs[i], xs[i + 1]);
        let fits = 2.0 * r[i] >= tw(&label) + 1.6 * ar && b - a >= 1.6 * th;
        // The text sits left of a vertical dimension line: it needs [x - 1.4 th, x + 0.2 th].
        let spot = if fits {
            let mid = (a + b) / 2.0 + 0.6 * th;
            let step = th / 4.0;
            (0..((b - a) / step) as i64 + 1)
                .map(|k| mid + if k % 2 == 0 { 1.0 } else { -1.0 } * (k as f64 / 2.0).ceil() * step)
                .find(|&x| x - 1.4 * th >= a + 0.1 * th && x + 0.2 * th <= b - 0.1 * th && clear(x - 1.4 * th, x + 0.2 * th))
        } else {
            None
        };
        if let Some(x) = spot {
            let mut d = Dimension::new(DimKind::Vertical, V2::new(x, -r[i]), V2::new(x, r[i]), V2::new(x, 0.0));
            d.text = Some("Ø<>".into());
            out.push(dim(d));
        } else if i == 0 || i == n - 1 {
            let (x, off) = if i == 0 { (0.0, -1.2 * th) } else { (length, 2.4 * th) };
            let mut d = Dimension::new(DimKind::Vertical, V2::new(x, -r[i]), V2::new(x, r[i]), V2::new(x + off, 0.0));
            d.text = Some("Ø<>".into());
            out.push(dim(d));
        } else {
            // Leader from the top edge, away from keyways when possible.
            let free = [
                (a, slots.iter().map(|s| s.0).filter(|&s0| s0 > a).fold(b, f64::min)),
                (slots.iter().map(|s| s.1).filter(|&s1| s1 < b).fold(a, f64::max), b),
            ];
            let (fa, fb) = free.into_iter().max_by(|p, q| (p.1 - p.0).total_cmp(&(q.1 - q.0))).unwrap_or((a, b));
            callouts.push((V2::new((fa + fb) / 2.0, r[i]), label));
        }
    }
    for c in chamfers {
        if c.at > n {
            continue;
        }
        let x = xs[c.at];
        let rr = if c.at < n { r[c.at] } else { r[n - 1] };
        let rl = if c.at > 0 { r[c.at - 1] } else { 0.0 };
        callouts.push((V2::new(x, rl.max(rr)), format!("{}x45°", fmt(c.size))));
    }
    // Callouts rise to the right in rows; working right to left keeps every leader
    // line clear of the texts already placed in the rows below it.
    callouts.sort_by(|p, q| q.0.x.total_cmp(&p.0.x));
    let mut rows: Vec<Vec<(f64, f64)>> = vec![];
    for (tip, text) in callouts {
        let elbow = tip.x + 0.5 * th;
        let span = (tip.x - 0.1 * th, elbow + ar + gap + tw(&text));
        // Lowest free row; when tips are too close for any, start a new row on top.
        let row = (0..=rows.len())
            .find(|&i| {
                (0..=i).all(|j| {
                    let iv = if j == i { span } else { (tip.x - 0.1 * th, elbow + 0.1 * th) };
                    rows.get(j).is_none_or(|r| r.iter().all(|&(lo, hi)| iv.1 + pad < lo || iv.0 - pad > hi))
                })
            })
            .unwrap_or(rows.len());
        while rows.len() <= row {
            rows.push(vec![]);
        }
        rows[row].push(span);
        for r in rows.iter_mut().take(row) {
            r.push((tip.x - 0.1 * th, elbow + 0.1 * th));
        }
        let y = rmax + 3.4 * th + row as f64 * 1.8 * th;
        out.push((layers::DIMENSIONS.into(), Shape::Leader(Leader { pts: vec![tip, V2::new(elbow, y)], text, style: None })));
    }

    // Length chain below the shaft (short neighbours staggered), then the overall length.
    let mut rows: Vec<Vec<(f64, f64)>> = vec![];
    let mut used = 0;
    for i in 0..n {
        let mid = (xs[i] + xs[i + 1]) / 2.0;
        let w = tw(&fmt(segments[i].length));
        let iv = (mid - w / 2.0, mid + w / 2.0);
        let row = (0..=rows.len())
            .find(|&j| rows.get(j).is_none_or(|r: &Vec<(f64, f64)>| r.iter().all(|&(lo, hi)| iv.1 + pad < lo || iv.0 - pad > hi)))
            .unwrap_or(rows.len());
        while rows.len() <= row {
            rows.push(vec![]);
        }
        rows[row].push(iv);
        used = used.max(row + 1);
        let y = -(rmax + 1.8 * th + row as f64 * 2.0 * th);
        out.push(dim(Dimension::new(DimKind::Horizontal, V2::new(xs[i], -r[i]), V2::new(xs[i + 1], -r[i]), V2::new(mid, y))));
    }
    let y = -(rmax + 1.8 * th + used as f64 * 2.0 * th);
    let mut overall = Dimension::new(DimKind::Horizontal, V2::new(0.0, -r[0]), V2::new(length, -r[n - 1]), V2::new(length / 2.0, y));
    if let Some(t) = total {
        if (t - length).abs() > 0.01 * t.max(1.0) {
            overall.text = Some(format!("<> (sketch: {})", fmt(t)));
        }
    }
    out.push(dim(overall));
    out
}

/// End view (left, centred on the origin) and full section (right, axis
/// horizontal) of a round part, hatched and dimensioned.
fn build_wheel(w: &Wheel) -> Vec<(String, Shape)> {
    let mut out: Vec<(String, Shape)> = vec![];
    let r = w.od / 2.0;
    let b = (w.bore / 2.0).min(r * 0.98);
    let (hub_d, hub_l) = w.hub.unwrap_or((0.0, 0.0));
    let rh = (hub_d / 2.0).clamp(b, r);
    let half_w = w.width / 2.0;
    let half_l = (hub_l / 2.0).max(if w.hub.is_some() { 0.0 } else { half_w });
    let axial = half_w.max(half_l);
    // Annotation sizes for the sheet the drawing will get.
    let ds = DimStyle::default();
    let mut probe = Sheet::default();
    let gap0 = (0.6 * r).max(4.0 * axial).max(r * 0.35);
    probe.fit_to(&fit_margin(&BBox::from_pts(V2::new(-r, -r), V2::new(r + gap0 + 2.0 * axial, r))));
    let th = ds.text_height * probe.scale;
    let gap = gap0.max(8.0 * th);
    let cx = r + gap + axial;
    let dim = |d: Dimension| (layers::DIMENSIONS.to_string(), Shape::Dim(d));
    let line = |layer: &str, a: V2, b: V2| (layer.to_string(), Shape::Line { a, b });
    let ext = (0.08 * r).max(1.2 * th);

    // ---- End view.
    let o = V2::ZERO;
    out.push((layers::OUTLINE.into(), Shape::Circle { c: o, r }));
    if let Some(g) = &w.grooves {
        // Groove bottoms seen end-on.
        out.push((layers::HIDDEN.into(), Shape::Circle { c: o, r: r - g.depth }));
    }
    if w.web.is_some() {
        let rim_in = r - w.rim.unwrap_or((r - rh) * 0.25).min(r - rh);
        out.push((layers::OUTLINE.into(), Shape::Circle { c: o, r: rim_in }));
        if rh > b {
            out.push((layers::OUTLINE.into(), Shape::Circle { c: o, r: rh }));
        }
    } else if rh > b && hub_l > w.width {
        out.push((layers::OUTLINE.into(), Shape::Circle { c: o, r: rh }));
    }
    if b > 0.0 {
        match w.keyway {
            Some((kw, kd)) if kw < 2.0 * b => {
                // Bore with the keyway notch at the top.
                let hw = kw / 2.0;
                let a0 = (hw / b).asin();
                let top = (b * b - hw * hw).sqrt();
                out.push((
                    layers::OUTLINE.into(),
                    Shape::Arc { c: o, r: b, start: std::f64::consts::FRAC_PI_2 + a0, end: std::f64::consts::FRAC_PI_2 - a0 },
                ));
                let (y1, y2) = (top, b + kd);
                out.push(line(layers::OUTLINE, V2::new(-hw, y1), V2::new(-hw, y2)));
                out.push(line(layers::OUTLINE, V2::new(-hw, y2), V2::new(hw, y2)));
                out.push(line(layers::OUTLINE, V2::new(hw, y2), V2::new(hw, y1)));
                out.push((
                    layers::DIMENSIONS.into(),
                    Shape::Leader(Leader {
                        pts: vec![V2::new(hw, y2), V2::new(r * 0.55, r + 1.5 * th)],
                        text: format!("Keyway {} × {}", fmt(kw), fmt(kd)),
                        style: None,
                    }),
                ));
            }
            _ => out.push((layers::OUTLINE.into(), Shape::Circle { c: o, r: b })),
        }
    }
    if let Some((n, hd, pcd)) = w.holes {
        out.push((layers::CENTER.into(), Shape::Circle { c: o, r: pcd / 2.0 }));
        for i in 0..n {
            let a = std::f64::consts::FRAC_PI_2 + std::f64::consts::TAU * i as f64 / n as f64 + std::f64::consts::PI / n as f64;
            let c = V2::polar(pcd / 2.0, a);
            out.push((layers::OUTLINE.into(), Shape::Circle { c, r: hd / 2.0 }));
        }
        let c0 = V2::polar(pcd / 2.0, std::f64::consts::FRAC_PI_2 + std::f64::consts::PI / n as f64);
        let tip = c0 + (c0 - o).norm() * (hd / 2.0);
        out.push((
            layers::DIMENSIONS.into(),
            Shape::Leader(Leader {
                pts: vec![tip, V2::new(-r * 0.75, r + 1.5 * th)],
                text: format!("{n}× Ø{} on PCD {}", fmt(hd), fmt(pcd)),
                style: None,
            }),
        ));
    }
    out.push(line(layers::CENTER, V2::new(-r - ext, 0.0), V2::new(r + ext, 0.0)));
    out.push(line(layers::CENTER, V2::new(0.0, -r - ext), V2::new(0.0, r + ext)));
    // Diameters are dimensioned once, on the section.

    // ---- Section (cut through the axis), upper half then mirrored.
    let mut top: Vec<V2> = vec![];
    let rim_in = r - w.rim.unwrap_or((r - rh.max(b)) * 0.25).min(r - rh.max(b));
    let web = w.web.map(|t| t.min(w.width).min(if hub_l > 0.0 { hub_l } else { w.width }) / 2.0);
    let (hl, hr) = if w.hub.is_some() { (half_l, rh) } else { (half_w, b) };
    match web {
        Some(t) => {
            top.extend([
                V2::new(-hl, b),
                V2::new(-hl, hr),
                V2::new(-t, hr),
                V2::new(-t, rim_in),
                V2::new(-half_w, rim_in),
                V2::new(-half_w, r),
            ]);
        }
        None if w.hub.is_some() && hl > half_w => {
            top.extend([V2::new(-hl, b), V2::new(-hl, hr), V2::new(-half_w, hr), V2::new(-half_w, r)]);
        }
        None => top.extend([V2::new(-half_w, b), V2::new(-half_w, r)]),
    }
    // The left flank (hub, web, rim side) is mirrored for the right one.
    let flank: Vec<V2> = top.clone();
    // Rim with its grooves, left to right.
    if let Some(g) = &w.grooves {
        let n = g.count.max(1);
        let pitch = w.width / n as f64;
        let gw = if g.width > 0.0 { g.width.min(pitch) } else { pitch * 0.8 };
        let bw = (gw - 2.0 * g.depth * (g.angle.to_radians() / 2.0).tan()).max(0.0);
        for i in 0..n {
            let xc = -half_w + pitch * (i as f64 + 0.5);
            top.extend([V2::new(xc - gw / 2.0, r), V2::new(xc - bw / 2.0, r - g.depth)]);
            if bw > 1e-9 {
                top.push(V2::new(xc + bw / 2.0, r - g.depth));
            }
            top.push(V2::new(xc + gw / 2.0, r));
        }
    }
    top.extend(flank.iter().rev().map(|p| V2::new(-p.x, p.y)));
    top.dedup_by(|a, b| a.dist(*b) < 1e-9);
    let place = |p: &V2, sy: f64| V2::new(cx + p.x, p.y * sy);
    let upper: Vec<V2> = top.iter().map(|p| place(p, 1.0)).collect();
    let lower: Vec<V2> = top.iter().map(|p| place(p, -1.0)).collect();
    for poly in [&upper, &lower] {
        out.push((layers::OUTLINE.into(), Shape::Polyline { pts: poly.clone(), closed: true }));
        out.push((
            layers::HATCH.into(),
            Shape::Hatch(Hatch { loops: vec![poly.clone()], pattern: "ANSI31".into(), scale: 1.0, angle: 0.0 }),
        ));
    }
    if b > 0.0 {
        // Bore surface seen beyond the cut.
        out.push(line(layers::OUTLINE, V2::new(cx - hl, b), V2::new(cx + hl, b)));
        out.push(line(layers::OUTLINE, V2::new(cx - hl, -b), V2::new(cx + hl, -b)));
    }
    out.push(line(layers::CENTER, V2::new(cx - axial - ext, 0.0), V2::new(cx + axial + ext, 0.0)));
    // Section dimensions: width below, hub length above, diameters on the right.
    let low = -r - 2.0 * th;
    out.push(dim(Dimension::new(DimKind::Horizontal, V2::new(cx - half_w, -r), V2::new(cx + half_w, -r), V2::new(cx, low))));
    if w.hub.is_some() && (hl - half_w).abs() > 1e-6 {
        out.push(dim(Dimension::new(DimKind::Horizontal, V2::new(cx - hl, -hr), V2::new(cx + hl, -hr), V2::new(cx, low - 2.2 * th))));
    }
    if let Some(t) = web {
        out.push(dim(Dimension::new(
            DimKind::Horizontal,
            V2::new(cx - t, rim_in),
            V2::new(cx + t, rim_in),
            V2::new(cx, (rim_in + hr) / 2.0),
        )));
    }
    let mut x = cx + axial + 2.5 * th;
    let vd = |y: f64, x: &mut f64| -> Dimension {
        let mut d = Dimension::new(DimKind::Vertical, V2::new(cx + half_w, -y), V2::new(cx + half_w, y), V2::new(*x, 0.0));
        d.text = Some("Ø<>".into());
        *x += 2.2 * th;
        d
    };
    if w.hub.is_some() && hr < r && hr > b {
        let mut d = vd(hr, &mut x);
        d.p1 = V2::new(cx + hl, -hr);
        d.p2 = V2::new(cx + hl, hr);
        out.push(dim(d));
    }
    out.push(dim(vd(r, &mut x)));
    if let Some(g) = &w.grooves {
        let n = g.count.max(1);
        let pitch = w.width / n as f64;
        let gw = if g.width > 0.0 { g.width.min(pitch) } else { pitch * 0.8 };
        let xc = cx - half_w + pitch * 0.5;
        let mut d = Dimension::new(
            DimKind::Vertical,
            V2::new(xc, r - g.depth),
            V2::new(xc + gw / 2.0, r),
            V2::new(cx - axial - 2.5 * th, r - g.depth / 2.0),
        );
        d.p1 = V2::new(xc, r - g.depth);
        out.push(dim(d));
        let text = if g.angle > 0.0 { format!("{n}× groove {}°", fmt(g.angle)) } else { format!("{n}× groove") };
        out.push((
            layers::DIMENSIONS.into(),
            Shape::Leader(Leader {
                pts: vec![V2::new(xc + gw / 4.0, r - g.depth / 2.0), V2::new(cx - axial, r + 2.0 * th)],
                text,
                style: None,
            }),
        ));
        if n > 1 {
            out.push(dim(Dimension::new(
                DimKind::Horizontal,
                V2::new(xc, r),
                V2::new(xc + pitch, r),
                V2::new(xc + pitch / 2.0, r + 4.0 * th),
            )));
        }
    }
    if b > 0.0 {
        let mut d = Dimension::new(DimKind::Vertical, V2::new(cx - hl, -b), V2::new(cx - hl, b), V2::new(cx - axial - 2.5 * th, 0.0));
        d.text = Some("Ø<>".into());
        out.push(dim(d));
    }
    out
}

fn build_plate(w: f64, h: f64, holes: &[Hole]) -> Vec<(String, Shape)> {
    let mut out: Vec<(String, Shape)> = vec![];
    let pts = vec![V2::new(0.0, 0.0), V2::new(w, 0.0), V2::new(w, h), V2::new(0.0, h)];
    out.push((layers::OUTLINE.into(), Shape::Polyline { pts, closed: true }));
    let m = w.max(h);
    out.push((
        layers::DIMENSIONS.into(),
        Shape::Dim(Dimension::new(DimKind::Horizontal, V2::new(0.0, 0.0), V2::new(w, 0.0), V2::new(w / 2.0, -0.15 * m))),
    ));
    out.push((
        layers::DIMENSIONS.into(),
        Shape::Dim(Dimension::new(DimKind::Vertical, V2::new(w, 0.0), V2::new(w, h), V2::new(w + 0.15 * m, h / 2.0))),
    ));
    for (i, hole) in holes.iter().enumerate() {
        let c = V2::new(hole.x, hole.y);
        let r = hole.diameter / 2.0;
        out.push((layers::OUTLINE.into(), Shape::Circle { c, r }));
        let e = r * 1.2 + 0.01 * m;
        out.push((layers::CENTER.into(), Shape::Line { a: c - V2::new(e, 0.0), b: c + V2::new(e, 0.0) }));
        out.push((layers::CENTER.into(), Shape::Line { a: c - V2::new(0.0, e), b: c + V2::new(0.0, e) }));
        let off = 0.08 * m * (i as f64 + 1.0);
        out.push((
            layers::DIMENSIONS.into(),
            Shape::Dim(Dimension::new(DimKind::Horizontal, V2::new(0.0, h), c, V2::new(c.x / 2.0, h + off))),
        ));
        out.push((
            layers::DIMENSIONS.into(),
            Shape::Dim(Dimension::new(DimKind::Vertical, V2::new(0.0, 0.0), c, V2::new(-off, c.y / 2.0))),
        ));
        let u = V2::new(1.0, 1.0).norm();
        out.push((layers::DIMENSIONS.into(), Shape::Dim(Dimension::new(DimKind::Diameter, c, c + u * r, c + u * (r * 1.8 + 0.04 * m)))));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shaft_from_reply_builds_exact_drawing() {
        let reply = serde_json::json!({"part": {"type": "shaft", "total_length": 2836,
            "segments": [{"length": 155, "diameter": 160}, {"length": 330, "diameter": 170}, {"length": 1390, "diameter": 180},
                         {"length": 330, "diameter": 170}, {"length": 155, "diameter": 160}, {"length": 122, "diameter": 140},
                         {"length": 50, "diameter": 120}, {"length": 254, "diameter": 110}, {"length": 50, "diameter": 120}],
            "keyways": [{"start": 20, "length": 100, "width": 45}],
            "chamfers": [{"at": 2, "size": 2.5}]}});
        let spec = PartSpec::from_json(&reply).unwrap();
        assert!(spec.check().is_empty());
        let shapes = spec.build();
        let outline = shapes.iter().find_map(|(_, s)| if let Shape::Polyline { pts, .. } = s { Some(pts.clone()) } else { None }).unwrap();
        let b = BBox::of(outline.iter().copied());
        assert!((b.width() - 2836.0).abs() < 1e-9 && (b.height() - 180.0).abs() < 1e-9, "{b:?}");
        let ds = icad_core::style::DimStyle::default();
        // Diameters too tight for an inline dimension become leader callouts.
        let dims: Vec<String> = shapes
            .iter()
            .filter_map(|(_, s)| match s {
                Shape::Dim(d) => Some(d.label(&ds)),
                Shape::Leader(l) => Some(l.text.clone()),
                _ => None,
            })
            .collect();
        for want in ["155", "330", "1390", "122", "254", "2836", "Ø180", "Ø110", "Ø120", "100", "2.5x45°"] {
            assert!(dims.iter().any(|d| d == want), "missing {want} in {dims:?}");
        }
        // Chamfer on the 170→180 step removes the sharp corner.
        assert!(!outline.contains(&V2::new(485.0, 90.0)));
        assert!(outline.contains(&V2::new(487.5, 90.0)));
    }

    /// Optional dump for a visual check: ICAD_DUMP=dir writes NAME.icad.
    fn dump(name: &str, conv: &crate::Conversion) {
        if let Ok(dir) = std::env::var("ICAD_DUMP") {
            let mut doc = icad_core::Document::new();
            crate::apply(conv, &mut doc);
            std::fs::write(format!("{dir}/{name}.icad"), serde_json::to_string(&doc).unwrap()).unwrap();
        }
    }

    fn conv_of(spec: &PartSpec) -> crate::Conversion {
        crate::Conversion { shapes: spec.build(), part: Some(spec.clone()), ..Default::default() }
    }

    #[test]
    fn pulley_and_ring() {
        let reply = serde_json::json!({"part": {"type": "pulley", "outer_diameter": 200, "width": 60, "bore": 30,
            "hub_diameter": 70, "hub_length": 80, "web_thickness": 16, "rim_thickness": 20,
            "grooves": {"count": 3, "angle": 38, "depth": 12, "width": 16}, "keyway": {"width": 8, "depth": 3.3}}});
        let spec = PartSpec::from_json(&reply).unwrap();
        let PartSpec::Wheel(w) = &spec else { panic!("{spec:?}") };
        assert_eq!((w.od, w.bore, w.hub, w.web), (200.0, 30.0, Some((70.0, 80.0)), Some(16.0)));
        let labels: Vec<String> = ["Ø200", "Ø30", "Ø70", "60", "80", "16", "12", "38°", "8", "3.3"].iter().map(|s| s.to_string()).collect();
        assert!(spec.issues_with(&labels).is_empty(), "{:?}", spec.issues_with(&labels));
        let shapes = spec.build();
        let hatches = shapes.iter().filter(|(_, s)| matches!(s, Shape::Hatch(_))).count();
        assert_eq!(hatches, 2);
        // Section outline: three V-grooves on the rim (two points at the bottom of each).
        let Some(Shape::Polyline { pts, .. }) = shapes.iter().map(|x| &x.1).find(|s| matches!(s, Shape::Polyline { closed: true, .. }))
        else {
            panic!()
        };
        let bottoms = pts.iter().filter(|p| (p.y - 88.0).abs() < 1e-9).count();
        assert_eq!(bottoms, 6, "{pts:?}");
        let ds = DimStyle::default();
        let texts: Vec<String> = shapes
            .iter()
            .filter_map(|(_, s)| match s {
                Shape::Dim(d) => Some(d.label(&ds)),
                Shape::Leader(l) => Some(l.text.clone()),
                _ => None,
            })
            .collect();
        for want in ["Ø200", "Ø30", "60", "80", "Ø70", "16", "12", "Keyway 8 × 3.3", "3× groove 38°"] {
            assert!(texts.iter().any(|t| t == want), "missing {want} in {texts:?}");
        }
        dump("pulley", &conv_of(&spec));
        let ring = PartSpec::from_json(&serde_json::json!({"part": {"type": "ring", "outer_diameter": "Ø80", "width": 10, "bore": "Ø50"}}))
            .unwrap();
        let shapes = ring.build();
        let circles = shapes.iter().filter(|(l, s)| l == layers::OUTLINE && matches!(s, Shape::Circle { .. })).count();
        assert_eq!(circles, 2);
        dump("ring", &conv_of(&ring));
        let flange = PartSpec::from_json(&serde_json::json!({"part": {"type": "flange", "outer_diameter": 150, "width": 18, "bore": 60,
            "hub_diameter": 90, "hub_length": 40, "holes": {"count": 4, "diameter": 14, "pcd": 120}}}))
        .unwrap();
        let holes = flange
            .build()
            .iter()
            .filter(|(l, s)| l == layers::OUTLINE && matches!(s, Shape::Circle { r, .. } if (*r - 7.0).abs() < 1e-9))
            .count();
        assert_eq!(holes, 4);
        dump("flange", &conv_of(&flange));
    }

    #[test]
    fn general_profile_is_measured_before_use() {
        // L-bracket 120 × 80, arm 30 thick, R10 inside fillet, Ø20 hole.
        let reply = serde_json::json!({"part": {"type": "profile", "name": "bracket", "entities": [
            {"polyline": [[0,0],[120,0],[120,30],[40,30],[30,40],[30,80],[0,80]], "closed": true},
            {"circle": [80, 15, 20]}],
          "dimensions": [
            {"kind": "horizontal", "from": [0,0], "to": [120,0], "value": 120},
            {"kind": "vertical", "from": [0,0], "to": [0,80], "value": 80},
            {"kind": "vertical", "from": [120,0], "to": [120,30], "value": 30},
            {"kind": "horizontal", "from": [0,80], "to": [30,80], "value": 30},
            {"kind": "horizontal", "from": [0,0], "to": [80,15], "value": 80},
            {"kind": "diameter", "center": [80,15], "value": "Ø20"},
            {"kind": "angle", "vertex": [40,30], "from": [120,30], "to": [30,40], "value": 135}]}});
        let spec = PartSpec::from_json(&reply).unwrap();
        let PartSpec::Profile(p) = &spec else { panic!() };
        assert!(p.self_check().is_empty(), "{:?}", p.self_check());
        let labels: Vec<String> = ["120", "80", "30", "30", "80", "Ø20", "135°"].iter().map(|s| s.to_string()).collect();
        assert!(spec.acceptable(&labels).is_ok(), "{:?}", spec.issues_with(&labels));
        let shapes = spec.build();
        assert_eq!(shapes.iter().filter(|(_, s)| matches!(s, Shape::Dim(_))).count(), 7);
        // Dimension lines of the bottom row do not collide: different offsets for 120 and 80.
        let below: Vec<f64> = shapes
            .iter()
            .filter_map(
                |(_, s)| if let Shape::Dim(d) = s { (d.kind == DimKind::Horizontal && d.loc.y < 0.0).then_some(d.loc.y) } else { None },
            )
            .collect();
        assert!(below.len() == 2 && (below[0] - below[1]).abs() > 1.0, "{below:?}");
        dump("bracket", &conv_of(&spec));
        // A dimension that does not measure true on its geometry is caught.
        let mut bad = reply.clone();
        bad["part"]["dimensions"][0]["value"] = 125.into();
        let spec = PartSpec::from_json(&bad).unwrap();
        let issues = spec.acceptable(&labels).unwrap_err();
        assert!(issues.iter().any(|i| i.contains("measures 120")), "{issues:?}");
    }

    #[test]
    fn disagreements_with_the_written_values() {
        let shaft = PartSpec::Shaft {
            segments: vec![Segment { length: 50.0, diameter: 40.0 }, Segment { length: 100.0, diameter: 30.0 }],
            keyways: vec![],
            chamfers: vec![],
            total_length: Some(150.0),
        };
        let ok: Vec<String> = ["50", "100", "150", "Ø40", "Ø30"].iter().map(|s| s.to_string()).collect();
        assert!(shaft.issues_with(&ok).is_empty());
        let bad: Vec<String> = ["50", "120", "Ø40", "Ø35"].iter().map(|s| s.to_string()).collect();
        let issues = shaft.issues_with(&bad).join(" | ");
        assert!(issues.contains("Ø35") && issues.contains("diameter 30") && issues.contains("120"), "{issues}");
    }

    #[test]
    fn plate_and_inconsistent_total() {
        let reply =
            serde_json::json!({"part": {"type": "plate", "width": "160", "height": 100, "holes": [{"x": 60, "y": 50, "diameter": "Ø40"}]}});
        let spec = PartSpec::from_json(&reply).unwrap();
        assert_eq!(spec, PartSpec::Plate { width: 160.0, height: 100.0, holes: vec![Hole { x: 60.0, y: 50.0, diameter: 40.0 }] });
        let bad = PartSpec::Shaft {
            segments: vec![Segment { length: 10.0, diameter: 5.0 }],
            keyways: vec![],
            chamfers: vec![],
            total_length: Some(12.0),
        };
        assert_eq!(bad.check().len(), 1);
    }

    #[test]
    fn crowded_callouts_terminate_and_stack() {
        // Many short steps with chamfers everywhere: every diameter needs a leader and
        // the tips are closer than one text height.
        let segments: Vec<Segment> = (0..12).map(|i| Segment { length: 4.0, diameter: 40.0 - 2.0 * i as f64 }).collect();
        let chamfers = (0..=12).map(|at| Chamfer { at, size: 0.5 }).collect();
        let spec = PartSpec::Shaft { segments, keyways: vec![], chamfers, total_length: None };
        let shapes = spec.build();
        let leaders = shapes.iter().filter(|(_, s)| matches!(s, Shape::Leader(_))).count();
        assert!(leaders >= 10, "{leaders}");
    }
}
