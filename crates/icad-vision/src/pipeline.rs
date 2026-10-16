// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Proteen Das

//! End-to-end sketch → CAD conversion.

use crate::beautify::{self, Tol};
use crate::fit::{fit_path, FitParams, Geo};
use crate::labels::{Label, LabelKind};
use crate::llm::{self, AiSettings, Provider};
use crate::raster::{self, Gray};
use crate::solve::{self, AxisCon, SpanKind};
use crate::{ocr, trace};
use icad_core::document::{layers, Document};
use icad_core::entity::{DimKind, Dimension, Shape};
use icad_core::geom::*;
use icad_core::prim::{HAlign, Text, VAlign};
use icad_core::style::{Layer, LineType};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ConvertOptions {
    /// Longest image side used for processing (px).
    pub max_dim: u32,
    /// Lines within this many degrees of horizontal/vertical are straightened.
    pub snap_deg: f64,
    /// Multiplier on the automatic sketch tolerance (higher = more smoothing).
    pub tolerance: f64,
    /// Overall size (mm) used when no dimensions can be read.
    pub fallback_size_mm: f64,
    /// Add centre lines to circles.
    pub center_marks: bool,
    /// Let the AI describe the whole part (shaft / plate) and build it exactly.
    #[serde(default = "yes")]
    pub reconstruct_parts: bool,
    /// Put the sketch under the result (faded, not plotted) to review it.
    #[serde(default = "yes")]
    pub overlay_sketch: bool,
    /// Fade of that overlay, 0 = opaque … 90.
    #[serde(default = "overlay_fade")]
    pub overlay_fade: u8,
    pub ai: AiSettings,
}

fn overlay_fade() -> u8 {
    60
}

impl Default for ConvertOptions {
    fn default() -> Self {
        Self {
            max_dim: 1600,
            snap_deg: 7.0,
            tolerance: 1.0,
            fallback_size_mm: 100.0,
            center_marks: true,
            reconstruct_parts: true,
            overlay_sketch: true,
            overlay_fade: overlay_fade(),
            ai: AiSettings::default(),
        }
    }
}

#[derive(Clone, Debug, Default)]
pub struct Conversion {
    /// (layer, shape) in millimetres.
    pub shapes: Vec<(String, Shape)>,
    pub title: Option<String>,
    pub log: Vec<String>,
    pub warnings: Vec<String>,
    pub mm_per_px: f64,
    pub labels_found: usize,
    pub dims_applied: usize,
    /// The model actually used when the configured one could not read images.
    pub switched_model: Option<String>,
    /// Description of the AI part reconstruction, when one was used.
    pub reconstructed: Option<String>,
    /// The reconstructed part, kept with the drawing so it can be regenerated.
    pub part: Option<crate::parts::PartSpec>,
    /// Where the whole sketch image lies in the result (corner, size in mm),
    /// for a review overlay. Approximate: the sketch is not to scale, the
    /// result is.
    pub underlay: Option<(V2, V2)>,
}

fn yes() -> bool {
    true
}

pub type Progress<'a> = &'a mut dyn FnMut(f32, &str);

/// Convert an image file's bytes.
pub fn convert(bytes: &[u8], opts: &ConvertOptions, progress: Progress) -> Result<Conversion, String> {
    progress(0.02, "Loading image");
    let gray = raster::load(bytes, opts.max_dim)?;
    let mut log = vec![format!("Image processed at {}×{} px", gray.w, gray.h)];
    let mut warnings = vec![];
    let mut labels: Vec<Label> = vec![];
    let mut title = None;
    let mut switched: Option<String> = None;

    if opts.ai.provider != Provider::Disabled {
        let model = opts.ai.describe();
        progress(0.08, &format!("Reading dimensions with AI ({model}) — this can take a minute"));
        // The model reads small handwritten digits better from a sharper copy.
        let sharp = raster::load(bytes, llm::SEND_MAX).unwrap_or_else(|_| gray.clone());
        match llm::read_labels(&opts.ai, &sharp.for_reading()) {
            Ok(mut r) => {
                // Boxes come back in the sharp copy's pixels: bring them to ours.
                let k = gray.w as f64 / sharp.w as f64;
                for l in &mut r.labels {
                    l.bbox = BBox::from_pts(l.bbox.min * k, l.bbox.max * k);
                }
                let used = match &r.switched_model {
                    Some(m) => {
                        let why = r.switch_reason.as_deref().unwrap_or("it could not be used");
                        warnings.push(format!("{} was not used because {why}; {m} was used instead.", opts.ai.describe()));
                        m.clone()
                    }
                    None => model,
                };
                log.push(format!("AI ({used}) read {} dimension label(s)", r.labels.len()));
                if r.cut_off {
                    warnings
                        .push("The AI answer was cut off; values after the cut are missing. Check the drawing, or convert again.".into());
                }
                // A parametric description of the whole part beats tracing pencil strokes.
                // Only a description that agrees with the written values is drawn
                // as is; otherwise the sketch is traced with those values.
                let texts: Vec<String> = r.labels.iter().map(|l| l.text.clone()).collect();
                let part = r.part.clone().filter(|spec| match spec.acceptable(&texts) {
                    Ok(()) => true,
                    Err(issues) => {
                        warnings.push(format!(
                            "The AI's description of the part did not check out against the written dimensions ({}); the sketch was traced instead.",
                            issues.join(" ")
                        ));
                        false
                    }
                });
                if opts.reconstruct_parts {
                    if let Some(spec) = part {
                        progress(0.9, "Building the part from the AI description");
                        let desc = spec.describe();
                        log.push(format!("AI recognised the part: {desc}"));
                        log.push("Drawing generated from the written dimensions (exact); sketch tracing skipped.".into());
                        warnings.extend(spec.check());
                        let shapes = spec.build();
                        let dims = shapes.iter().filter(|(_, s)| matches!(s, Shape::Dim(_))).count();
                        progress(1.0, "Done");
                        return Ok(Conversion {
                            shapes,
                            title: r.title,
                            log,
                            warnings,
                            mm_per_px: 0.0,
                            labels_found: r.labels.len(),
                            dims_applied: dims,
                            switched_model: r.switched_model,
                            reconstructed: Some(desc),
                            part: Some(spec),
                            underlay: None,
                        });
                    }
                }
                switched = r.switched_model.clone();
                labels.extend(r.labels);
                title = r.title;
            }
            Err(e) => warnings.push(format!("AI dimension reading unavailable: {e}")),
        }
    }
    if opts.ai.use_tesseract {
        if let Some(exe) = ocr::find_tesseract() {
            progress(0.45, "Reading printed text with Tesseract OCR");
            let clean = raster::close(&gray.binarize()).to_image();
            match ocr::read_labels(&exe, &clean) {
                Ok(found) => {
                    let mut added = 0;
                    for l in found {
                        if !labels.iter().any(|o| o.bbox.expand(2.0).intersects(&l.bbox)) {
                            labels.push(l);
                            added += 1;
                        }
                    }
                    log.push(format!("Tesseract OCR added {added} label(s)"));
                }
                Err(e) => warnings.push(format!("Tesseract OCR failed: {e}")),
            }
        }
    }
    if labels.is_empty() {
        warnings.push(
            "No dimension values were read; the drawing is scaled to a nominal size. Connect an AI API key (AI → Connect API Key), \
             or use a local Ollama vision model or Tesseract, to read dimensions — or use SCALE afterwards."
                .into(),
        );
    }
    let mut conv = convert_gray(&gray, labels, opts, progress)?;
    conv.title = title;
    conv.switched_model = switched;
    log.append(&mut conv.log);
    conv.log = log;
    warnings.append(&mut conv.warnings);
    conv.warnings = warnings;
    Ok(conv)
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Bucket {
    Geometry,
    /// Free-standing annotation (dimension lines, text strokes): hidden layer.
    Annotation,
    /// Uncertain: shown on the AI-REVIEW layer.
    Review,
}

/// Convert an already-loaded image with known labels (also used by tests).
pub fn convert_gray(gray: &Gray, labels: Vec<Label>, opts: &ConvertOptions, progress: Progress) -> Result<Conversion, String> {
    let mut log = vec![];
    let mut warnings = vec![];
    progress(0.55, "Separating ink from paper");
    let mut mask = raster::close(&gray.binarize());
    if mask.count() < 50 {
        return Err("No drawing found in the image (it looks blank).".into());
    }
    let (comp_labels, comps) = raster::components(&mask);
    let long = gray.w.max(gray.h) as f64;
    let diag = ((gray.w * gray.w + gray.h * gray.h) as f64).sqrt();

    // Erase text components (inside label boxes) and specks.
    let mut erase = vec![false; comps.len() + 1];
    for (i, c) in comps.iter().enumerate() {
        let cb = BBox::from_pts(V2::new(c.x0 as f64, (gray.h - 1 - c.y1) as f64), V2::new(c.x1 as f64 + 1.0, (gray.h - c.y0) as f64));
        let area = (cb.width() * cb.height()).max(1.0);
        let in_label = labels.iter().any(|l| {
            let lb = l.bbox.expand(l.size() * 0.25 + 2.0);
            let ix = (cb.max.x.min(lb.max.x) - cb.min.x.max(lb.min.x)).max(0.0);
            let iy = (cb.max.y.min(lb.max.y) - cb.min.y.max(lb.min.y)).max(0.0);
            ix * iy / area > 0.6
        });
        let speck = c.count < 12 || (c.w().max(c.h()) as f64) < long * 0.004;
        // The sheet border (and a title block drawn into it): a thin frame
        // round almost the whole image.
        let frame = c.w() as f64 > 0.8 * gray.w as f64 && c.h() as f64 > 0.8 * gray.h as f64 && (c.count as f64) < 0.15 * area;
        erase[i + 1] = in_label || speck || frame;
    }
    for (i, l) in comp_labels.iter().enumerate() {
        if erase[*l as usize] {
            mask.px[i] = 0;
        }
    }

    progress(0.62, "Thinning strokes to centre lines");
    let sk = raster::thin(&mask);
    let stroke = (mask.count() as f64 / sk.count().max(1) as f64).clamp(1.0, 30.0);
    progress(0.72, "Tracing and fitting lines, arcs and circles");
    let paths = trace::trace(&sk, &comp_labels);
    let k = opts.tolerance.max(0.2);
    let params = FitParams { eps: (1.5 * stroke).max(0.006 * diag) * k, stroke, diag };
    let mut geos: Vec<(Geo, u32)> = vec![];
    for p in &paths {
        for g in fit_path(&p.pts, p.closed, &params) {
            geos.push((g, p.comp));
        }
    }
    log.push(format!("Stroke width ≈ {stroke:.1} px; traced {} chains into {} primitives", paths.len(), geos.len()));
    if geos.is_empty() {
        return Err("Could not find any lines in the image.".into());
    }

    // Which components form the part? Part outlines contain closed loops; free
    // standing dimension assemblies, leaders and text are open trees.
    let ncomp = comps.len() + 1;
    let mut cbox = vec![BBox::empty(); ncomp];
    let mut is_circle_only = vec![true; ncomp];
    for (g, c) in &geos {
        cbox[*c as usize] = cbox[*c as usize].union(&g.bbox());
        if !matches!(g, Geo::Circle(..)) {
            is_circle_only[*c as usize] = false;
        }
    }
    let holes = enclosed_holes(&mask, &comp_labels, ncomp, (6.0 * stroke).powi(2).max(0.0004 * (gray.w * gray.h) as f64));
    let mut part_comp: Vec<bool> = (0..ncomp).map(|c| !cbox[c].is_empty() && holes[c] > 0).collect();
    if !part_comp.iter().any(|p| *p) {
        // Open sketch: fall back to the biggest component.
        let area = |b: &BBox| if b.is_empty() { 0.0 } else { b.width().max(1.0) * b.height().max(1.0) };
        if let Some(big) = (0..ncomp).max_by(|a, b| area(&cbox[*a]).total_cmp(&area(&cbox[*b]))) {
            part_comp[big] = true;
        }
    }
    let part_boxes: Vec<BBox> = (0..ncomp).filter(|c| part_comp[*c]).map(|c| cbox[c]).collect();
    for c in 0..ncomp {
        if !part_comp[c] && !cbox[c].is_empty() && (is_circle_only[c] || part_boxes.iter().any(|pb| pb.contains_box(&cbox[c]))) {
            part_comp[c] = true;
        }
    }
    let text_like = |c: u32| {
        let b = &cbox[c as usize];
        !part_comp[c as usize] && b.width().max(b.height()) < 0.05 * long
    };

    let tol = (2.5 * stroke).max(0.008 * diag) * k;
    let mut all: Vec<Geo> = geos.iter().map(|g| g.0).collect();
    beautify::snap_angles(&mut all, opts.snap_deg);
    let part_flags: Vec<bool> = geos.iter().map(|g| part_comp[g.1 as usize]).collect();

    progress(0.8, "Matching dimension values to features");
    let cls = solve::associate(&all, &part_flags, &labels, tol);
    let mut buckets = vec![Bucket::Geometry; all.len()];
    for (i, (_, c)) in geos.iter().enumerate() {
        if !part_comp[*c as usize] {
            buckets[i] = if text_like(*c) { Bucket::Annotation } else { Bucket::Review };
        }
    }
    for &i in &cls.claimed {
        buckets[i] = if part_flags[i] { Bucket::Review } else { Bucket::Annotation };
    }
    // A free-standing component that is mostly dimension strokes is annotation
    // as a whole (arrowheads, overshoots, ticks).
    let mut claimed_len = vec![0.0; ncomp];
    let mut total_len = vec![0.0; ncomp];
    for (i, (g, c)) in geos.iter().enumerate() {
        total_len[*c as usize] += g.length();
        if cls.claimed.contains(&i) {
            claimed_len[*c as usize] += g.length();
        }
    }
    for (i, (_, c)) in geos.iter().enumerate() {
        let c = *c as usize;
        if !part_comp[c] && claimed_len[c] > 0.6 * total_len[c] {
            buckets[i] = Bucket::Annotation;
        }
    }
    let pick = |b: Bucket| -> Vec<Geo> { all.iter().zip(&buckets).filter(|(_, x)| **x == b).map(|(g, _)| *g).collect() };
    let mut geometry = pick(Bucket::Geometry);
    let annotation = pick(Bucket::Annotation);
    let review = pick(Bucket::Review);

    progress(0.86, "Cleaning up geometry");
    let t = Tol {
        align: tol,
        tight: (1.5 * stroke).max(0.004 * diag) * k,
        min_len: (4.0 * stroke).max(0.015 * diag) * k,
        snap_deg: opts.snap_deg,
    };
    beautify::run(&mut geometry, &t);
    let snapped = align_views(&mut geometry, tol);
    if snapped > 0 {
        log.push(format!("Lined up {snapped} edge(s) between projected views"));
    }
    log.push(format!(
        "{} geometry primitives after clean-up; {} annotation strokes hidden; {} strokes flagged for review",
        geometry.len(),
        annotation.len(),
        review.len()
    ));

    // ---- Scale from dimensions.
    progress(0.9, "Solving dimensions");
    let mut ratios: Vec<f64> = vec![];
    for sp in &cls.spans {
        let px = match sp.kind {
            SpanKind::X => (sp.b.x - sp.a.x).abs(),
            SpanKind::Y => (sp.b.y - sp.a.y).abs(),
            SpanKind::Aligned => sp.a.dist(sp.b),
        };
        if px > 1.0 {
            ratios.push(labels[sp.label].value / px);
        }
    }
    for rd in &cls.radials {
        let l = &labels[rd.label];
        let v = if l.kind == LabelKind::Diameter { l.value / 2.0 } else { l.value };
        if rd.r > 1.0 {
            ratios.push(v / rd.r);
        }
    }
    let gbox = geometry.iter().fold(BBox::empty(), |b, g| b.union(&g.bbox()));
    let s = if ratios.is_empty() {
        opts.fallback_size_mm / gbox.width().max(gbox.height()).max(1.0)
    } else {
        ratios.sort_by(f64::total_cmp);
        ratios[ratios.len() / 2]
    };

    let mut xs = vec![];
    let mut ys = vec![];
    for g in &geometry {
        match *g {
            Geo::Line(a, b) => {
                xs.extend([a.x, b.x]);
                ys.extend([a.y, b.y]);
            }
            Geo::Circle(c, _) => {
                xs.push(c.x);
                ys.push(c.y);
            }
            Geo::Arc { c, .. } => {
                let (a, b) = g.endpoints().unwrap();
                xs.extend([a.x, b.x, c.x]);
                ys.extend([a.y, b.y, c.y]);
            }
        }
    }
    let cons = |kind: SpanKind| -> Vec<AxisCon> {
        cls.spans
            .iter()
            .filter(|sp| sp.kind == kind)
            .map(|sp| {
                let (a, b) = if kind == SpanKind::X { (sp.a.x, sp.b.x) } else { (sp.a.y, sp.b.y) };
                AxisCon { a, b, value: labels[sp.label].value, label: sp.label }
            })
            .collect()
    };
    let rx = solve::solve_axis(&xs, &cons(SpanKind::X), s, tol);
    let ry = solve::solve_axis(&ys, &cons(SpanKind::Y), s, tol);
    for (l, r) in rx.conflicts.iter().chain(ry.conflicts.iter()) {
        warnings.push(format!("Dimension \"{}\" conflicts with other dimensions (off by {:.2} mm)", labels[*l].text, r));
    }
    for l in rx.dropped.iter().chain(ry.dropped.iter()) {
        warnings.push(format!("Dimension \"{}\" contradicts the sketch and was not enforced", labels[*l].text));
    }
    let map = |p: V2| V2::new(rx.mapper.map(p.x), ry.mapper.map(p.y));
    let underlay_at = map(V2::ZERO);
    let underlay_size = map(V2::new(gray.w as f64, gray.h as f64)) - underlay_at;

    // ---- Build CAD entities.
    let mut shapes: Vec<(String, Shape)> = vec![];
    let mut circles_mm: Vec<(V2, f64, V2, f64)> = vec![]; // (px centre, px r, mm centre, mm r)
    let radial_r = |c: V2, r: f64| -> Option<f64> {
        cls.radials.iter().find(|rd| rd.center.dist(c) < 2.0 * tol && (rd.r - r).abs() < 2.0 * tol).map(|rd| {
            let l = &labels[rd.label];
            if l.kind == LabelKind::Diameter {
                l.value / 2.0
            } else {
                l.value
            }
        })
    };
    let map_geo = |g: &Geo| -> Option<Shape> {
        match *g {
            Geo::Line(a, b) => Some(Shape::Line { a: map(a), b: map(b) }),
            Geo::Circle(c, r) => Some(Shape::Circle { c: map(c), r: radial_r(c, r).unwrap_or(r * s) }),
            Geo::Arc { .. } => {
                let (a, b) = g.endpoints()?;
                let m = g.arc_mid()?;
                let (c, r, st, en) = arc_3p(map(a), map(m), map(b))?;
                Some(Shape::Arc { c, r, start: st, end: en })
            }
        }
    };
    let mut geo_of: Vec<usize> = vec![];
    for (gi, g) in geometry.iter().enumerate() {
        if let Some(sh) = map_geo(g) {
            geo_of.push(gi);
            shapes.push((layers::OUTLINE.into(), sh));
        }
    }
    // Feature point on the geometry for an extension-line origin.
    let feature = |axis_x: bool, v: f64, near: f64| -> Option<V2> {
        let mut best: Option<(f64, V2)> = None;
        for g in &geometry {
            let pts: Vec<V2> = match *g {
                Geo::Line(a, b) => vec![a, b],
                Geo::Circle(c, _) => vec![c],
                Geo::Arc { c, .. } => {
                    let (a, b) = g.endpoints().unwrap();
                    vec![a, b, c]
                }
            };
            for p in pts {
                let (along, across) = if axis_x { (p.x, p.y) } else { (p.y, p.x) };
                if (along - v).abs() <= 2.0 * tol {
                    let d = (across - near).abs();
                    if best.is_none_or(|b| d < b.0) {
                        best = Some((d, p));
                    }
                }
            }
        }
        best.map(|b| b.1)
    };
    progress(0.94, "Enforcing slanted, angular and radius dimensions");
    let moves = refine_outline(
        &mut shapes,
        &Outline { geometry: &geometry, geo_of: &geo_of, cls: &cls, labels: &labels },
        &map,
        &feature,
        tol,
        s,
        &mut warnings,
    );
    for (k, gi) in geo_of.iter().enumerate() {
        if let (Geo::Circle(pc, pr) | Geo::Arc { c: pc, r: pr, .. }, Shape::Circle { c, r } | Shape::Arc { c, r, .. }) =
            (&geometry[*gi], &shapes[k].1)
        {
            circles_mm.push((*pc, *pr, *c, *r));
        }
    }
    let mut mbox = BBox::empty();
    for (_, sh) in &shapes {
        match sh {
            Shape::Line { a, b } => {
                mbox.add(*a);
                mbox.add(*b);
            }
            Shape::Circle { c, r } | Shape::Arc { c, r, .. } => {
                mbox.add(*c - V2::new(*r, *r));
                mbox.add(*c + V2::new(*r, *r));
            }
            _ => {}
        }
    }
    let extent = mbox.width().max(mbox.height()).max(1.0);
    if opts.center_marks {
        for (_, sh) in shapes.clone() {
            if let Shape::Circle { c, r } = sh {
                let e = r + (0.15 * r).max(extent * 0.01);
                shapes.push((layers::CENTER.into(), Shape::Line { a: c - V2::new(e, 0.0), b: c + V2::new(e, 0.0) }));
                shapes.push((layers::CENTER.into(), Shape::Line { a: c - V2::new(0.0, e), b: c + V2::new(0.0, e) }));
            }
        }
    }

    let min_off = extent * 0.07;
    let mut dims_applied = 0;
    for (si, sp) in cls.spans.iter().enumerate() {
        let l = &labels[sp.label];
        let dim = match sp.kind {
            SpanKind::X | SpanKind::Y => {
                let ax = sp.kind == SpanKind::X;
                let (va, vb, across) = if ax { (sp.a.x, sp.b.x, sp.line_pt.y) } else { (sp.a.y, sp.b.y, sp.line_pt.x) };
                // Extension lines start at the measured features (in mm).
                let fa = feature(ax, va, across);
                let fb = feature(ax, vb, across);
                let m_across = if ax { ry.mapper.map(across) } else { rx.mapper.map(across) };
                let end = |f: Option<V2>, v: f64| -> V2 {
                    match (f, ax) {
                        (Some(p), _) => moves.apply(map(p)),
                        (None, true) => V2::new(rx.mapper.map(v), m_across),
                        (None, false) => V2::new(m_across, ry.mapper.map(v)),
                    }
                };
                let (pa, pb) = (end(fa, va), end(fb, vb));
                let get = |p: V2| if ax { p.y } else { p.x };
                let mut pos = m_across;
                let center = if ax { mbox.center().y } else { mbox.center().x };
                if (pos - get(pa)).abs() < min_off && (pos - get(pb)).abs() < min_off {
                    pos = if pos >= center { get(pa).max(get(pb)) + min_off } else { get(pa).min(get(pb)) - min_off };
                }
                let loc = if ax { V2::new((pa.x + pb.x) / 2.0, pos) } else { V2::new(pos, (pa.y + pb.y) / 2.0) };
                Dimension {
                    kind: if ax { DimKind::Horizontal } else { DimKind::Vertical },
                    p1: pa,
                    p2: pb,
                    loc,
                    text: None,
                    center: None,
                    assoc: None,
                }
            }
            SpanKind::Aligned => {
                let (p1, p2) = moves
                    .aligned
                    .iter()
                    .find(|a| a.0 == si)
                    .map(|a| (a.1, a.2))
                    .unwrap_or((moves.apply(map(sp.a)), moves.apply(map(sp.b))));
                // Keep the sketched offset of the dimension line from the edge.
                let n = (p2 - p1).norm().perp();
                let off = (moves.apply(map(sp.line_pt)) - p1).dot(n);
                let off = if off.abs() < min_off * 0.5 { min_off.copysign(if off == 0.0 { 1.0 } else { off }) } else { off };
                let d = Dimension::new(DimKind::Aligned, p1, p2, p1.mid(p2) + n * off);
                if (d.measurement() - l.value).abs() > (0.002 * l.value).max(0.01) {
                    warnings.push(format!(
                        "Slanted dimension \"{}\" measures {:.2} mm in the result (could not be enforced)",
                        l.text,
                        d.measurement()
                    ));
                }
                d
            }
        };
        if (dim.measurement() - l.value).abs() < (0.002 * l.value).max(0.01) {
            dims_applied += 1;
        }
        shapes.push((layers::DIMENSIONS.into(), Shape::Dim(dim)));
    }
    for rd in &cls.radials {
        let l = &labels[rd.label];
        let Some((_, _, c, r)) = circles_mm
            .iter()
            .copied()
            .min_by(|a, b| (a.0.dist(rd.center) + (a.1 - rd.r).abs()).total_cmp(&(b.0.dist(rd.center) + (b.1 - rd.r).abs())))
            .filter(|x| x.0.dist(rd.center) < 3.0 * tol)
        else {
            continue;
        };
        let lc = map(l.center());
        let mut u = (lc - c).norm();
        if u == V2::ZERO {
            u = V2::new(1.0, 1.0).norm();
        }
        let loc = if lc.dist(c) > r * 1.05 { lc } else { c + u * (r * 1.6 + extent * 0.03) };
        let kind = if l.kind == LabelKind::Diameter { DimKind::Diameter } else { DimKind::Radius };
        let want = if kind == DimKind::Diameter { l.value } else { l.value / 1.0 };
        let dim = Dimension { kind, p1: c, p2: c + u * r, loc, text: None, center: None, assoc: None };
        if (dim.measurement() - want).abs() < (0.002 * want).max(0.01) {
            dims_applied += 1;
        }
        shapes.push((layers::DIMENSIONS.into(), Shape::Dim(dim)));
    }
    for (v, a, b, label) in &moves.angles {
        let l = &labels[*label];
        let sweep = norm_angle((*b - *v).angle() - (*a - *v).angle());
        let reach = v.dist(*a).min(v.dist(*b)) * 0.5;
        let loc = *v + V2::polar(reach.max(extent * 0.05), (*a - *v).angle() + sweep / 2.0);
        let dim = Dimension::angular(*v, *a, *b, loc);
        if (dim.measurement() - l.value).abs() < 0.05 {
            dims_applied += 1;
        } else {
            warnings.push(format!("Angle \"{}\" measures {:.2}° in the result (could not be enforced)", l.text, dim.measurement()));
        }
        shapes.push((layers::DIMENSIONS.into(), Shape::Dim(dim)));
    }
    let th = (extent * 0.025).max(2.5);
    let mut unmatched = 0;
    for (i, l) in labels.iter().enumerate() {
        if !cls.matched[i] {
            unmatched += 1;
            shapes.push((
                layers::AI_REVIEW.into(),
                Shape::Text(Text::new(map(l.center()), th, l.text.clone()).aligned(HAlign::Center, VAlign::Middle)),
            ));
        }
    }
    if unmatched > 0 {
        warnings.push(format!(
            "{unmatched} label(s) could not be attached to a feature; they are placed as text on layer {}",
            layers::AI_REVIEW
        ));
    }
    for g in &annotation {
        if let Some(sh) = map_geo(g) {
            shapes.push((layers::AI_SKETCH.into(), sh));
        }
    }
    for g in &review {
        if let Some(sh) = map_geo(g) {
            shapes.push((layers::AI_REVIEW.into(), sh));
        }
    }
    log.push(format!(
        "{} label(s) → {} linear, {} radial and {} angular dimension(s); scale ≈ {:.4} mm/px; {} dimension(s) satisfied exactly",
        labels.len(),
        cls.spans.len(),
        cls.radials.len(),
        cls.angles.len(),
        s,
        dims_applied
    ));
    progress(1.0, "Done");
    Ok(Conversion {
        shapes,
        title: None,
        log,
        warnings,
        mm_per_px: s,
        labels_found: labels.len(),
        dims_applied,
        switched_model: None,
        reconstructed: None,
        part: None,
        underlay: Some((underlay_at, underlay_size)),
    })
}

/// Separate views of a multi-view sketch: geometry clusters whose boxes touch.
fn views(geometry: &[Geo], tol: f64) -> Vec<(Vec<usize>, BBox)> {
    let boxes: Vec<BBox> = geometry.iter().map(|g| g.bbox().expand(tol)).collect();
    let mut parent: Vec<usize> = (0..geometry.len()).collect();
    fn root(p: &mut [usize], mut i: usize) -> usize {
        while p[i] != i {
            p[i] = p[p[i]];
            i = p[i];
        }
        i
    }
    for i in 0..boxes.len() {
        for j in i + 1..boxes.len() {
            let (a, b) = (&boxes[i], &boxes[j]);
            if a.min.x <= b.max.x && b.min.x <= a.max.x && a.min.y <= b.max.y && b.min.y <= a.max.y {
                let (ri, rj) = (root(&mut parent, i), root(&mut parent, j));
                parent[ri] = rj;
            }
        }
    }
    let mut out: Vec<(usize, Vec<usize>, BBox)> = vec![];
    for (i, g) in geometry.iter().enumerate() {
        let r = root(&mut parent, i);
        match out.iter_mut().find(|v| v.0 == r) {
            Some(v) => {
                v.1.push(i);
                v.2 = v.2.union(&g.bbox());
            }
            None => out.push((r, vec![i], g.bbox())),
        }
    }
    out.into_iter().map(|v| (v.1, v.2)).collect()
}

/// Projected views (front above top, side beside front) share their edges:
/// snap each view's vertical edges and centres to the matching ones of the
/// view above/below it (horizontal ones for views side by side), so the
/// hand-drawn misalignment does not survive into the CAD drawing.
/// Returns the number of coordinates moved.
fn align_views(geometry: &mut [Geo], tol: f64) -> usize {
    let vs = views(geometry, tol);
    let mut moved = 0;
    for axis_x in [true, false] {
        // Stations of a view along the axis: line ends square to it and centres.
        let stations = |g: &[Geo], ids: &[usize]| -> Vec<f64> {
            let mut s: Vec<f64> = vec![];
            for &i in ids {
                match g[i] {
                    Geo::Line(a, b) if (axis_x && (a.x - b.x).abs() < 1e-9) || (!axis_x && (a.y - b.y).abs() < 1e-9) => {
                        s.push(if axis_x { a.x } else { a.y })
                    }
                    Geo::Circle(c, _) | Geo::Arc { c, .. } => s.push(if axis_x { c.x } else { c.y }),
                    _ => {}
                }
            }
            s
        };
        for a in 0..vs.len() {
            for b in 0..vs.len() {
                let (ba, bb) = (&vs[a].1, &vs[b].1);
                // `a` is the reference: the bigger view of a stacked pair.
                if a == b || ba.width() * ba.height() < bb.width() * bb.height() {
                    continue;
                }
                let (lo, hi, len_a, len_b, apart) = if axis_x {
                    (ba.min.x.max(bb.min.x), ba.max.x.min(bb.max.x), ba.width(), bb.width(), ba.max.y < bb.min.y || bb.max.y < ba.min.y)
                } else {
                    (ba.min.y.max(bb.min.y), ba.max.y.min(bb.max.y), ba.height(), bb.height(), ba.max.x < bb.min.x || bb.max.x < ba.min.x)
                };
                if !apart || hi - lo < 0.5 * len_a.min(len_b) {
                    continue;
                }
                let snap = (2.0 * tol).max(0.03 * len_a.max(len_b));
                let ref_st = stations(geometry, &vs[a].0);
                let mut own = stations(geometry, &vs[b].0);
                own.sort_by(f64::total_cmp);
                own.dedup_by(|p, q| (*p - *q).abs() < 1e-9);
                for v in own {
                    let Some(&t) = ref_st.iter().min_by(|p, q| (*p - v).abs().total_cmp(&(*q - v).abs())) else { continue };
                    if (t - v).abs() < 1e-9 || (t - v).abs() > snap {
                        continue;
                    }
                    let set = |p: &mut V2| {
                        let c = if axis_x { &mut p.x } else { &mut p.y };
                        if (*c - v).abs() < 1e-9 {
                            *c = t;
                        }
                    };
                    for &i in &vs[b].0 {
                        match &mut geometry[i] {
                            Geo::Line(p, q) => {
                                set(p);
                                set(q);
                            }
                            Geo::Circle(c, _) | Geo::Arc { c, .. } => set(c),
                        }
                    }
                    moved += 1;
                }
            }
        }
    }
    moved
}

pub(crate) struct Outline<'a> {
    geometry: &'a [Geo],
    /// Geometry index of each outline shape (shapes[k] comes from geometry[geo_of[k]]).
    geo_of: &'a [usize],
    cls: &'a solve::Classified,
    labels: &'a [Label],
}

/// How the refinement moved the outline, for placing dimensions.
#[derive(Default)]
pub(crate) struct Moves {
    from: Vec<V2>,
    to: Vec<V2>,
    /// (span index, p1, p2) of enforced slanted dimensions.
    aligned: Vec<(usize, V2, V2)>,
    /// (vertex, end a, end b, label) of angle dimensions.
    angles: Vec<(V2, V2, V2, usize)>,
}

impl Moves {
    /// Move a point with its nearest outline vertex.
    fn apply(&self, p: V2) -> V2 {
        match self.from.iter().enumerate().min_by(|a, b| a.1.dist(p).total_cmp(&b.1.dist(p))) {
            Some((i, f)) => p + (self.to[i] - *f),
            None => p,
        }
    }
}

/// Enforce the dimensions the per-axis solve cannot: slanted lengths,
/// angles, arc radii, and tangency of fillets, over the outline vertices
/// (see [`crate::refine`]). Outline shapes are rebuilt in place.
#[allow(clippy::too_many_arguments)]
fn refine_outline(
    shapes: &mut [(String, Shape)],
    o: &Outline,
    map: &dyn Fn(V2) -> V2,
    feature: &dyn Fn(bool, f64, f64) -> Option<V2>,
    tol: f64,
    s: f64,
    warnings: &mut Vec<String>,
) -> Moves {
    use crate::refine::Con;
    let n_out = o.geo_of.len();
    let eps = (0.5 * tol * s).max(1e-6);
    let mut pts: Vec<V2> = vec![];
    let vid = |p: V2, pts: &mut Vec<V2>| -> usize {
        if let Some(i) = pts.iter().position(|q| q.dist(p) < eps) {
            return i;
        }
        pts.push(p);
        pts.len() - 1
    };
    enum Kind {
        Line(usize, usize),
        Arc(usize, usize, usize),
        Circle(usize),
    }
    let mut kinds = vec![];
    for (_, sh) in shapes.iter().take(n_out) {
        kinds.push(match *sh {
            Shape::Line { a, b } => Kind::Line(vid(a, &mut pts), vid(b, &mut pts)),
            Shape::Arc { c, r, start, end } => {
                let (i, j) = (vid(c + V2::polar(r, start), &mut pts), vid(c + V2::polar(r, end), &mut pts));
                Kind::Arc(vid(c, &mut pts), i, j)
            }
            Shape::Circle { c, .. } => Kind::Circle(vid(c, &mut pts)),
            _ => Kind::Circle(usize::MAX),
        });
    }
    let nearest = |p: V2, pts: &[V2]| -> usize { (0..pts.len()).min_by(|a, b| pts[*a].dist(p).total_cmp(&pts[*b].dist(p))).unwrap_or(0) };
    let mut cons: Vec<Con> = vec![];
    let mut what: Vec<String> = vec![];
    let ext = pts.iter().fold(BBox::empty(), |mut b, p| {
        b.add(*p);
        b
    });
    let scale = ext.width().max(ext.height()).max(1.0);
    let flat = scale * 1e-9;
    for k in &kinds {
        match *k {
            Kind::Line(a, b) if a != b => {
                if (pts[a].y - pts[b].y).abs() < flat {
                    cons.push(Con::Horizontal { a, b });
                    what.push(String::new());
                } else if (pts[a].x - pts[b].x).abs() < flat {
                    cons.push(Con::Vertical { a, b });
                    what.push(String::new());
                }
            }
            Kind::Arc(c, p, q) => {
                cons.push(Con::Equal { c, p, q });
                what.push(String::new());
                // Tangent where a line continues the arc smoothly.
                for end in [p, q] {
                    for k2 in &kinds {
                        let Kind::Line(a, b) = *k2 else { continue };
                        let other = if a == end {
                            b
                        } else if b == end {
                            a
                        } else {
                            continue;
                        };
                        let (r, t) = ((pts[end] - pts[c]).norm(), (pts[other] - pts[end]).norm());
                        if r.dot(t).abs() < 10f64.to_radians().sin() {
                            cons.push(Con::Tangent { c, p: end, q: other });
                            what.push(String::new());
                        }
                    }
                }
            }
            _ => {}
        }
    }
    let mut strong = 0;
    // Radius and diameter labels on arcs (circles get their size directly).
    for rd in &o.cls.radials {
        let l = &o.labels[rd.label];
        let r = if l.kind == LabelKind::Diameter { l.value / 2.0 } else { l.value };
        let hit = o.geo_of.iter().position(
            |gi| matches!(o.geometry[*gi], Geo::Arc { c, r: gr, .. } if c.dist(rd.center) < 2.0 * tol && (gr - rd.r).abs() < 2.0 * tol),
        );
        if let Some(Kind::Arc(c, p, _)) = hit.map(|k| &kinds[k]) {
            cons.push(Con::Radius { c: *c, p: *p, value: r });
            what.push(l.text.clone());
            strong += 1;
        }
    }
    // Hold the horizontal/vertical dimensions already met by the axis solve.
    for sp in &o.cls.spans {
        let ax = match sp.kind {
            SpanKind::X => true,
            SpanKind::Y => false,
            SpanKind::Aligned => continue,
        };
        let (va, vb, across) = if ax { (sp.a.x, sp.b.x, sp.line_pt.y) } else { (sp.a.y, sp.b.y, sp.line_pt.x) };
        let (Some(fa), Some(fb)) = (feature(ax, va, across), feature(ax, vb, across)) else { continue };
        let (a, b) = (nearest(map(fa), &pts), nearest(map(fb), &pts));
        if a != b {
            cons.push(if ax { Con::Dx { a, b, value: pts[b].x - pts[a].x } } else { Con::Dy { a, b, value: pts[b].y - pts[a].y } });
            what.push(o.labels[sp.label].text.clone());
        }
    }
    // Slanted dimensions: the outline vertices at the ends of the dimension line.
    let mut aligned: Vec<(usize, usize, usize)> = vec![];
    let ends: Vec<V2> = o
        .geometry
        .iter()
        .flat_map(|g| match *g {
            Geo::Line(a, b) => vec![a, b],
            Geo::Arc { .. } => g.endpoints().map(|(a, b)| vec![a, b]).unwrap_or_default(),
            Geo::Circle(c, _) => vec![c],
        })
        .collect();
    for (si, sp) in o.cls.spans.iter().enumerate() {
        if sp.kind != SpanKind::Aligned {
            continue;
        }
        let d = (sp.b - sp.a).norm();
        let n = d.perp();
        let pick = |end: V2| -> Option<V2> {
            ends.iter()
                .filter(|v| (**v - end).dot(d).abs() < 2.0 * tol)
                .min_by(|a, b| {
                    ((**a - end).dot(d).abs() + 0.2 * (**a - end).dot(n).abs())
                        .total_cmp(&((**b - end).dot(d).abs() + 0.2 * (**b - end).dot(n).abs()))
                })
                .copied()
        };
        let (Some(fa), Some(fb)) = (pick(sp.a), pick(sp.b)) else { continue };
        let (a, b) = (nearest(map(fa), &pts), nearest(map(fb), &pts));
        if a != b {
            let l = &o.labels[sp.label];
            cons.push(Con::Dist { a, b, value: l.value });
            what.push(l.text.clone());
            aligned.push((si, a, b));
            strong += 1;
        }
    }
    let mut angles: Vec<(usize, usize, usize, usize)> = vec![];
    for ad in &o.cls.angles {
        let (v, a, b) = (nearest(map(ad.v), &pts), nearest(map(ad.a), &pts), nearest(map(ad.b), &pts));
        if v != a && v != b && a != b {
            let l = &o.labels[ad.label];
            cons.push(Con::Angle { v, a, b, value: l.value.to_radians() });
            what.push(l.text.clone());
            angles.push((v, a, b, ad.label));
            strong += 1;
        }
    }
    let tangents = cons.iter().any(|c| matches!(c, Con::Tangent { .. }));
    let mut moves = Moves { from: pts.clone(), to: pts.clone(), ..Default::default() };
    if strong > 0 || tangents {
        if pts.len() > crate::refine::MAX_POINTS {
            warnings.push(format!(
                "The outline has {} vertices; slanted, angular and arc radius dimensions were not enforced (limit {})",
                pts.len(),
                crate::refine::MAX_POINTS
            ));
        } else {
            let r = crate::refine::refine(&pts, &cons, scale);
            let mut named: Vec<&str> = r.unmet.iter().map(|i| what[*i].as_str()).filter(|w| !w.is_empty()).collect();
            named.dedup();
            if !named.is_empty() {
                warnings.push(format!(
                    "Dimension(s) {} conflict with the rest of the drawing and are only approximately met",
                    named.join(", ")
                ));
            }
            moves.to = r.pts;
        }
    }
    let x = &moves.to;
    for (k, (_, sh)) in shapes.iter_mut().take(n_out).enumerate() {
        match kinds[k] {
            Kind::Line(a, b) => *sh = Shape::Line { a: x[a], b: x[b] },
            Kind::Arc(c, p, q) => {
                let r = (x[c].dist(x[p]) + x[c].dist(x[q])) / 2.0;
                *sh = Shape::Arc { c: x[c], r, start: norm_angle((x[p] - x[c]).angle()), end: norm_angle((x[q] - x[c]).angle()) };
            }
            Kind::Circle(c) if c != usize::MAX => {
                if let Shape::Circle { r, .. } = *sh {
                    *sh = Shape::Circle { c: x[c], r };
                }
            }
            _ => {}
        }
    }
    moves.aligned = aligned.iter().map(|(si, a, b)| (*si, x[*a], x[*b])).collect();
    moves.angles = angles.iter().map(|(v, a, b, l)| (x[*v], x[*a], x[*b], *l)).collect();
    moves
}

/// Number of significant enclosed background regions ("holes") per ink
/// component. Part outlines enclose area; dimension assemblies, filled
/// arrowheads and handwriting do not (or only tiny counters in letters).
fn enclosed_holes(mask: &raster::Mask, comp_labels: &[u32], ncomp: usize, min_area: f64) -> Vec<usize> {
    let (w, h) = (mask.w, mask.h);
    let mut seen = vec![false; w * h];
    let mut holes = vec![0usize; ncomp];
    let mut stack = vec![];
    for start in 0..w * h {
        if mask.px[start] != 0 || seen[start] {
            continue;
        }
        // Flood the background region (4-connected); first pixel is its top-left.
        seen[start] = true;
        stack.push(start);
        let (mut area, mut touches_border) = (0usize, false);
        while let Some(i) = stack.pop() {
            area += 1;
            let (x, y) = (i % w, i / w);
            if x == 0 || y == 0 || x == w - 1 || y == h - 1 {
                touches_border = true;
            }
            let mut push = |j: usize| {
                if mask.px[j] == 0 && !seen[j] {
                    seen[j] = true;
                    stack.push(j);
                }
            };
            if x > 0 {
                push(i - 1);
            }
            if x + 1 < w {
                push(i + 1);
            }
            if y > 0 {
                push(i - w);
            }
            if y + 1 < h {
                push(i + w);
            }
        }
        if !touches_border && area as f64 >= min_area && start >= w {
            // The pixel above the region's first pixel is ink of the enclosing component.
            let c = comp_labels[start - w] as usize;
            if c < ncomp {
                holes[c] += 1;
            }
        }
    }
    holes
}

/// Put the sketch image `path` under a conversion inserted with [`apply`],
/// on its own layer (not plotted), faded by `fade` (0–90).
pub fn overlay(conv: &Conversion, doc: &mut Document, path: &str, fade: u8) -> bool {
    let Some((pos, size)) = conv.underlay else { return false };
    if size.x <= 0.0 || size.y <= 0.0 {
        return false;
    }
    let mut l = Layer::new(layers::AI_IMAGE, 8, LineType::Continuous, 0.13);
    l.plot = false;
    doc.ensure_layer(l);
    doc.add_on(
        layers::AI_IMAGE,
        Shape::Image(icad_core::prim::ImageRef { path: path.into(), pos, w: size.x, h: size.y, fade: fade.min(90) }),
    );
    // Drawn first, under the geometry.
    if let Some(e) = doc.entities.pop() {
        doc.entities.insert(0, e);
    }
    true
}

/// Insert a conversion into a document: creates the AI layers, adds the
/// entities, fills the title, and fits the sheet/title block around them.
pub fn apply(conv: &Conversion, doc: &mut Document) {
    let mut review = Layer::new(layers::AI_REVIEW, 6, LineType::Dashed, 0.18);
    review.plot = false;
    doc.ensure_layer(review);
    let mut sketch = Layer::new(layers::AI_SKETCH, 8, LineType::Continuous, 0.13);
    sketch.visible = false;
    sketch.plot = false;
    doc.ensure_layer(sketch);
    for l in [
        Layer::new(layers::OUTLINE, 7, LineType::Continuous, 0.5),
        Layer::new(layers::CENTER, 1, LineType::Center, 0.18),
        Layer::new(layers::DIMENSIONS, 4, LineType::Continuous, 0.18),
    ] {
        doc.ensure_layer(l);
    }
    // Section hatching and hidden edges, when the drawing has them.
    for (name, l) in [
        (layers::HIDDEN, Layer::new(layers::HIDDEN, 2, LineType::Hidden, 0.25)),
        (layers::HATCH, Layer::new(layers::HATCH, 8, LineType::Continuous, 0.13)),
    ] {
        if conv.shapes.iter().any(|(n, _)| n == name) {
            doc.ensure_layer(l);
        }
    }
    let ids: Vec<u64> = conv.shapes.iter().map(|(layer, shape)| doc.add_on(layer, shape.clone())).collect();
    if let Some(spec) = &conv.part {
        doc.part = Some(icad_core::document::PartRecord { spec: spec.to_json(), ids });
    }
    // Dimensions follow the geometry they measure when it is edited.
    doc.associate_all_dims();
    if let Some(t) = &conv.title {
        doc.sheet.title.title = t.to_uppercase();
    }
    if doc.sheet.title.date.is_empty() {
        doc.sheet.title.date = icad_core::sheet::today();
    }
    // Fit the sheet to the geometry (dimension text scales with the sheet).
    let geo_box = doc
        .entities
        .iter()
        .filter(|e| doc.is_visible(e) && !e.shape.is_annotation())
        .fold(BBox::empty(), |b, e| b.union(&doc.entity_bbox(e)));
    if !geo_box.is_empty() {
        doc.sheet.fit_to(&icad_core::sheet::fit_margin(&geo_box));
        doc.sheet.visible = true;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Render a thick, slightly wobbly stroke.
    fn stroke(img: &mut Gray, a: (f64, f64), b: (f64, f64), w: f64, wobble: f64) {
        let len = ((b.0 - a.0).powi(2) + (b.1 - a.1).powi(2)).sqrt();
        let n = (len * 2.0) as usize + 1;
        let (nx, ny) = (-(b.1 - a.1) / len, (b.0 - a.0) / len);
        for i in 0..=n {
            let t = i as f64 / n as f64;
            let off = wobble * (t * 9.0).sin();
            let (x, y) = (a.0 + (b.0 - a.0) * t + nx * off, a.1 + (b.1 - a.1) * t + ny * off);
            let r = w / 2.0;
            for dy in -(r.ceil() as i64)..=r.ceil() as i64 {
                for dx in -(r.ceil() as i64)..=r.ceil() as i64 {
                    if ((dx * dx + dy * dy) as f64) <= r * r {
                        let (px, py) = (x as i64 + dx, y as i64 + dy);
                        if px >= 0 && py >= 0 && (px as usize) < img.w && (py as usize) < img.h {
                            img.px[py as usize * img.w + px as usize] = 30;
                        }
                    }
                }
            }
        }
    }

    fn label(text: &str, x0: f64, y0: f64, x1: f64, y1: f64, h: f64) -> Label {
        let (kind, value) = crate::labels::parse(text).unwrap();
        // Top-left pixel box → y-up.
        Label { text: text.into(), kind, value, bbox: BBox::from_pts(V2::new(x0, h - y1), V2::new(x1, h - y0)), source: "test" }
    }

    #[test]
    fn sketch_with_dimensions_becomes_exact_cad() {
        let (w, h) = (1200usize, 800usize);
        let mut g = Gray { w, h, px: vec![245; w * h] };
        // Part: 800×400 px rectangle, hole r=80 px.
        stroke(&mut g, (100., 150.), (900., 152.), 5.0, 1.5);
        stroke(&mut g, (900., 152.), (898., 550.), 5.0, 1.5);
        stroke(&mut g, (898., 550.), (100., 548.), 5.0, 1.5);
        stroke(&mut g, (100., 548.), (100., 150.), 5.0, 1.5);
        for i in 0..90 {
            let a0 = i as f64 / 90.0 * std::f64::consts::TAU;
            let a1 = (i + 1) as f64 / 90.0 * std::f64::consts::TAU;
            stroke(&mut g, (500. + 80. * a0.cos(), 350. + 80. * a0.sin()), (500. + 80. * a1.cos(), 350. + 80. * a1.sin()), 5.0, 0.0);
        }
        // Horizontal dimension below (separate strokes with a gap from the part).
        stroke(&mut g, (100., 565.), (100., 670.), 3.0, 0.0);
        stroke(&mut g, (900., 565.), (900., 670.), 3.0, 0.0);
        stroke(&mut g, (100., 655.), (900., 655.), 3.0, 0.0);
        // Vertical dimension on the right.
        stroke(&mut g, (915., 150.), (1020., 150.), 3.0, 0.0);
        stroke(&mut g, (915., 550.), (1020., 550.), 3.0, 0.0);
        stroke(&mut g, (1005., 150.), (1005., 550.), 3.0, 0.0);
        let labels = vec![
            label("200", 470., 615., 530., 645., h as f64),
            // Deliberately out of proportion: the sketch is 2:1 but the part is 200×120.
            label("120", 1012., 335., 1060., 365., h as f64),
            label("Ø60", 600., 220., 680., 250., h as f64),
        ];
        let opts = ConvertOptions {
            ai: AiSettings { provider: Provider::Disabled, use_tesseract: false, ..Default::default() },
            ..Default::default()
        };
        let conv = convert_gray(&g, labels, &opts, &mut |_, _| {}).unwrap();
        let outline: Vec<&Shape> = conv.shapes.iter().filter(|(l, _)| l == layers::OUTLINE).map(|(_, s)| s).collect();
        let lines: Vec<(V2, V2)> =
            outline.iter().filter_map(|s| if let Shape::Line { a, b } = s { Some((*a, *b)) } else { None }).collect();
        let circles: Vec<(V2, f64)> =
            outline.iter().filter_map(|s| if let Shape::Circle { c, r } = s { Some((*c, *r)) } else { None }).collect();
        assert_eq!(lines.len(), 4, "outline lines: {lines:?}\nlog: {:?}\nwarn: {:?}", conv.log, conv.warnings);
        assert_eq!(circles.len(), 1, "{circles:?}");
        let bb = BBox::of(lines.iter().flat_map(|l| [l.0, l.1]));
        assert!((bb.width() - 200.0).abs() < 1e-6, "width {}", bb.width());
        assert!((bb.height() - 120.0).abs() < 1e-6, "height {}", bb.height());
        let (c, r) = circles[0];
        assert!((r - 30.0).abs() < 1e-6, "r {r}");
        // Hole stays centred: x proportional (100 of 200), y proportional (60 of 120).
        assert!((c.x - bb.min.x - 100.0).abs() < 2.0 && (c.y - bb.min.y - 60.0).abs() < 2.0, "c {c:?}");
        let dims = conv.shapes.iter().filter(|(_, s)| matches!(s, Shape::Dim(_))).count();
        assert_eq!(dims, 3);
        let measured: Vec<String> = conv
            .shapes
            .iter()
            .filter_map(|(_, s)| if let Shape::Dim(d) = s { Some(format!("{:?} {}", d.kind, d.measurement())) } else { None })
            .collect();
        assert_eq!(conv.dims_applied, 3, "{:?} {measured:?}", conv.log);
        // The sketched dimension lines are not part of the outline.
        assert!(conv.shapes.iter().any(|(l, _)| l == layers::AI_SKETCH));
        // Review overlay: the whole image under the result, drawn first, not plotted.
        let mut doc = Document::new();
        apply(&conv, &mut doc);
        assert!(overlay(&conv, &mut doc, "sketch.png", 60));
        let Shape::Image(img) = &doc.entities[0].shape else { panic!("overlay is not drawn first") };
        // 1200 px wide image, 800 px of it the 200 mm part; the margins use the median scale.
        assert!((280.0..340.0).contains(&img.w) && img.fade == 60, "{img:?}");
        assert!(img.pos.x < bb.min.x && img.pos.x + img.w > bb.max.x && img.pos.y < bb.min.y && img.pos.y + img.h > bb.max.y);
        assert!(!doc.layer(layers::AI_IMAGE).unwrap().plot);
    }

    #[test]
    fn slanted_angle_and_fillet_dimensions_are_enforced() {
        let (w, h) = (1200usize, 800usize);
        let mut g = Gray { w, h, px: vec![245; w * h] };
        // Plate with a chamfered bottom-right corner and a filleted top-left one.
        stroke(&mut g, (180., 150.), (900., 150.), 5.0, 1.0);
        stroke(&mut g, (900., 150.), (900., 400.), 5.0, 1.0);
        stroke(&mut g, (900., 400.), (700., 550.), 5.0, 1.0);
        stroke(&mut g, (700., 550.), (100., 550.), 5.0, 1.0);
        stroke(&mut g, (100., 550.), (100., 230.), 5.0, 1.0);
        for i in 0..30 {
            let a0 = std::f64::consts::PI + i as f64 / 30.0 * std::f64::consts::FRAC_PI_2;
            let a1 = std::f64::consts::PI + (i + 1) as f64 / 30.0 * std::f64::consts::FRAC_PI_2;
            stroke(&mut g, (180. + 80. * a0.cos(), 230. + 80. * a0.sin()), (180. + 80. * a1.cos(), 230. + 80. * a1.sin()), 5.0, 0.0);
        }
        // Overall width below, height on the left.
        stroke(&mut g, (100., 565.), (100., 670.), 3.0, 0.0);
        stroke(&mut g, (900., 415.), (900., 670.), 3.0, 0.0);
        stroke(&mut g, (100., 655.), (900., 655.), 3.0, 0.0);
        stroke(&mut g, (85., 150.), (55., 150.), 3.0, 0.0);
        stroke(&mut g, (85., 550.), (55., 550.), 3.0, 0.0);
        stroke(&mut g, (70., 150.), (70., 550.), 3.0, 0.0);
        let hh = h as f64;
        let labels = vec![
            label("200", 470., 615., 530., 645., hh),
            label("120", 15., 335., 63., 365., hh),
            // Out of proportion with the sketch on purpose.
            label("60", 815., 495., 845., 520., hh),
            label("135°", 835., 345., 885., 370., hh),
            label("R15", 120., 175., 170., 200., hh),
        ];
        let opts = ConvertOptions {
            ai: AiSettings { provider: Provider::Disabled, use_tesseract: false, ..Default::default() },
            ..Default::default()
        };
        let conv = convert_gray(&g, labels, &opts, &mut |_, _| {}).unwrap();
        let ctx = format!("log: {:?}\nwarn: {:?}", conv.log, conv.warnings);
        let outline: Vec<&Shape> = conv.shapes.iter().filter(|(l, _)| l == layers::OUTLINE).map(|(_, s)| s).collect();
        let lines: Vec<(V2, V2)> =
            outline.iter().filter_map(|s| if let Shape::Line { a, b } = s { Some((*a, *b)) } else { None }).collect();
        let arcs: Vec<(V2, f64, f64, f64)> = outline
            .iter()
            .filter_map(|s| if let Shape::Arc { c, r, start, end } = s { Some((*c, *r, *start, *end)) } else { None })
            .collect();
        assert_eq!((lines.len(), arcs.len()), (5, 1), "{outline:?}\n{ctx}");
        let slant = lines.iter().find(|(a, b)| (a.x - b.x).abs() > 1.0 && (a.y - b.y).abs() > 1.0).expect("slanted edge");
        assert!((slant.0.dist(slant.1) - 60.0).abs() < 1e-3, "slant {} {ctx}", slant.0.dist(slant.1));
        let tilt = (slant.1 - slant.0).angle().to_degrees().rem_euclid(180.0);
        // 135° inside the corner with the vertical edge: the edge runs at 225°.
        assert!((tilt - 45.0).abs() < 0.01, "slant at {tilt}° {ctx}");
        for (a, b) in lines.iter().filter(|l| *l != slant) {
            assert!((a.x - b.x).abs() < 1e-6 || (a.y - b.y).abs() < 1e-6, "{a:?} {b:?} no longer square");
        }
        let (c, r, st, en) = arcs[0];
        assert!((r - 15.0).abs() < 1e-3, "r {r} {ctx}");
        // Tangent to both edges it joins.
        let (p, q) = (c + V2::polar(r, st), c + V2::polar(r, en));
        assert!(
            lines.iter().any(|(a, b)| a.dist(p) < 1e-3 || b.dist(p) < 1e-3)
                && lines.iter().any(|(a, b)| a.dist(q) < 1e-3 || b.dist(q) < 1e-3)
        );
        let bb = BBox::of(lines.iter().flat_map(|l| [l.0, l.1]));
        assert!((bb.width() - 200.0).abs() < 1e-3 && (bb.height() - 120.0).abs() < 1e-3, "{bb:?} {ctx}");
        let kinds: Vec<String> = conv
            .shapes
            .iter()
            .filter_map(|(_, s)| if let Shape::Dim(d) = s { Some(format!("{:?} {:.3}", d.kind, d.measurement())) } else { None })
            .collect();
        assert_eq!(conv.dims_applied, 5, "{kinds:?} {ctx}");
    }

    #[test]
    fn stacked_views_stay_aligned() {
        let (w, h) = (1200usize, 900usize);
        let mut g = Gray { w, h, px: vec![245; w * h] };
        let rect = |g: &mut Gray, x0: f64, y0: f64, x1: f64, y1: f64| {
            stroke(g, (x0, y0), (x1, y0), 5.0, 1.0);
            stroke(g, (x1, y0), (x1, y1), 5.0, 1.0);
            stroke(g, (x1, y1), (x0, y1), 5.0, 1.0);
            stroke(g, (x0, y1), (x0, y0), 5.0, 1.0);
        };
        // Front view above, top view below; hand-drawn a little out of line.
        rect(&mut g, 150., 100., 950., 300.);
        rect(&mut g, 166., 420., 934., 720.);
        // Width on the front view only.
        stroke(&mut g, (150., 85.), (150., 30.), 3.0, 0.0);
        stroke(&mut g, (950., 85.), (950., 30.), 3.0, 0.0);
        stroke(&mut g, (150., 45.), (950., 45.), 3.0, 0.0);
        // Heights on the right of each view.
        for (y0, y1) in [(100., 300.), (420., 720.)] {
            stroke(&mut g, (965., y0), (1040., y0), 3.0, 0.0);
            stroke(&mut g, (965., y1), (1040., y1), 3.0, 0.0);
            stroke(&mut g, (1025., y0), (1025., y1), 3.0, 0.0);
        }
        let hh = h as f64;
        let labels = vec![
            label("200", 520., 5., 580., 35., hh),
            label("60", 1032., 185., 1070., 215., hh),
            label("90", 1032., 555., 1070., 585., hh),
        ];
        let opts = ConvertOptions {
            ai: AiSettings { provider: Provider::Disabled, use_tesseract: false, ..Default::default() },
            ..Default::default()
        };
        let conv = convert_gray(&g, labels, &opts, &mut |_, _| {}).unwrap();
        let ctx = format!("log: {:?}\nwarn: {:?}", conv.log, conv.warnings);
        let verticals: Vec<f64> = conv
            .shapes
            .iter()
            .filter(|(l, _)| l == layers::OUTLINE)
            .filter_map(|(_, s)| if let Shape::Line { a, b } = s { ((a.x - b.x).abs() < 1e-9).then_some(a.x) } else { None })
            .collect();
        assert_eq!(verticals.len(), 4, "{verticals:?} {ctx}");
        let mut xs = verticals.clone();
        xs.sort_by(f64::total_cmp);
        // Both views: the same two edges, 200 apart.
        assert!((xs[1] - xs[0]).abs() < 1e-6 && (xs[3] - xs[2]).abs() < 1e-6, "{xs:?} {ctx}");
        assert!((xs[2] - xs[0] - 200.0).abs() < 1e-6, "{xs:?} {ctx}");
        assert_eq!(conv.dims_applied, 3, "{ctx}");
        assert!(conv.log.iter().any(|l| l.starts_with("Lined up")), "{ctx}");
    }
}
