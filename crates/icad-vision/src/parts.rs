// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Proteen Das

//! AI part reconstruction: when the vision model can describe the sketched
//! part parametrically (a turned shaft or a rectangular plate), ImagineCAD
//! builds an exact, fully dimensioned drawing from the written values instead
//! of tracing pencil strokes.

use icad_core::document::layers;
use icad_core::entity::{DimKind, Dimension, Leader, Shape};
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

#[derive(Clone, Debug, PartialEq)]
pub enum PartSpec {
    Shaft { segments: Vec<Segment>, keyways: Vec<Keyway>, chamfers: Vec<Chamfer>, total_length: Option<f64> },
    Plate { width: f64, height: f64, holes: Vec<Hole> },
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
        if ty.contains("shaft") || ty.contains("turn") || p.get("segments").is_some() {
            let segments: Vec<Segment> = p
                .get("segments")?
                .as_array()?
                .iter()
                .filter_map(|s| {
                    let length = field(s, &["length", "l"])?;
                    let diameter = field(s, &["diameter", "d", "dia"])?;
                    (length > 0.0 && diameter > 0.0).then_some(Segment { length, diameter })
                })
                .collect();
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
            return Some(PartSpec::Shaft { segments, keyways, chamfers, total_length: field(p, &["total_length", "overall_length"]) });
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

    /// Build exact geometry and dimensions (mm).
    pub fn build(&self) -> Vec<(String, Shape)> {
        match self {
            PartSpec::Shaft { segments, keyways, chamfers, total_length } => build_shaft(segments, keyways, chamfers, *total_length),
            PartSpec::Plate { width, height, holes } => build_plate(*width, *height, holes),
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
        out.push((layers::DIMENSIONS.into(), Shape::Leader(Leader { pts: vec![tip, V2::new(elbow, y)], text })));
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
