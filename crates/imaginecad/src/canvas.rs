// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Proteen Das

//! Canvas rendering: grid, paper, entities, previews, grips, object snaps.

use crate::settings::OsnapModes;
use crate::theme;
use crate::view::View;
use eframe::egui::{self, epaint::TextShape, pos2, vec2, Align2, Color32, FontId, Painter, Pos2, Rect, Shape, Stroke};
use icad_core::entity::{Curve, SnapKind};
use icad_core::geom::*;
use icad_core::prim::Prim;
use icad_core::Document;

pub struct Style {
    pub color: Color32,
    pub width: f32,
    /// Dash pattern in world units (empty = continuous).
    pub dash: Vec<f64>,
}

fn dashed(points: &[Pos2], pattern_px: &[f32], stroke: Stroke, out: &mut Vec<Shape>) {
    let total: f32 = pattern_px.iter().map(|v| v.abs()).sum();
    if pattern_px.is_empty() || total < 3.0 {
        out.push(Shape::line(points.to_vec(), stroke));
        return;
    }
    let mut idx = 0usize;
    let mut left = pattern_px[0].abs().max(0.5);
    let mut on = pattern_px[0] >= 0.0;
    let mut cur: Vec<Pos2> = vec![];
    for w in points.windows(2) {
        let (mut a, b) = (w[0], w[1]);
        let mut seg = (b - a).length();
        if seg < 1e-6 {
            continue;
        }
        let dir = (b - a) / seg;
        while seg > 0.0 {
            let step = left.min(seg);
            let next = a + dir * step;
            if on {
                if cur.is_empty() {
                    cur.push(a);
                }
                cur.push(next);
            }
            seg -= step;
            left -= step;
            a = next;
            if left <= 1e-4 {
                if on && cur.len() >= 2 {
                    out.push(Shape::line(std::mem::take(&mut cur), stroke));
                }
                cur.clear();
                idx = (idx + 1) % pattern_px.len();
                let v = pattern_px[idx];
                on = v >= 0.0;
                left = if v == 0.0 { stroke.width.max(1.0) } else { v.abs() };
            }
        }
    }
    if on && cur.len() >= 2 {
        out.push(Shape::line(cur, stroke));
    }
}

/// Liang–Barsky clip of a segment to a box (keeps "infinite" lines drawable).
pub fn clip_segment(a: V2, b: V2, bx: &BBox) -> Option<(V2, V2)> {
    let d = b - a;
    let (mut t0, mut t1) = (0.0f64, 1.0f64);
    for (p, q) in [(-d.x, a.x - bx.min.x), (d.x, bx.max.x - a.x), (-d.y, a.y - bx.min.y), (d.y, bx.max.y - a.y)] {
        if p.abs() < 1e-15 {
            if q < 0.0 {
                return None;
            }
        } else {
            let r = q / p;
            if p < 0.0 {
                t0 = t0.max(r);
            } else {
                t1 = t1.min(r);
            }
        }
    }
    (t0 <= t1).then(|| (a + d * t0, a + d * t1))
}

/// Loaded image textures for raster underlays, keyed by file path.
#[derive(Default)]
pub struct ImageCache {
    map: std::collections::HashMap<String, Option<egui::TextureHandle>>,
}

impl ImageCache {
    pub fn get(&mut self, ctx: &egui::Context, path: &str) -> Option<egui::TextureHandle> {
        self.map
            .entry(path.to_string())
            .or_insert_with(|| {
                let img = image::open(path).ok()?.to_rgba8();
                let size = [img.width() as usize, img.height() as usize];
                let ci = egui::ColorImage::from_rgba_unmultiplied(size, img.as_raw());
                Some(ctx.load_texture(path, ci, egui::TextureOptions::LINEAR))
            })
            .clone()
    }
}

pub fn prim_shapes(view: &View, p: &Prim, st: &Style, out: &mut Vec<Shape>) {
    let stroke = Stroke::new(st.width, st.color);
    let tol = view.px(0.35);
    let pattern: Vec<f32> = st.dash.iter().map(|d| (d * view.zoom) as f32).collect();
    let poly = |pts: Vec<Pos2>, out: &mut Vec<Shape>| {
        if pattern.is_empty() {
            out.push(Shape::line(pts, stroke));
        } else {
            dashed(&pts, &pattern, stroke, out);
        }
    };
    match p {
        Prim::Line(a, b) => {
            let Some((a, b)) = clip_segment(*a, *b, &view.world_box().expand(view.px(50.0))) else { return };
            let pts = vec![view.to_screen(a), view.to_screen(b)];
            if pattern.is_empty() {
                out.push(Shape::line_segment([pts[0], pts[1]], stroke));
            } else {
                poly(pts, out);
            }
        }
        Prim::Poly(pts, closed) => {
            let mut s: Vec<Pos2> = pts.iter().map(|q| view.to_screen(*q)).collect();
            if *closed && s.len() > 2 {
                s.push(s[0]);
            }
            poly(s, out);
        }
        Prim::Circle(c, r) => {
            let rp = (*r * view.zoom) as f32;
            if pattern.is_empty() {
                out.push(Shape::circle_stroke(view.to_screen(*c), rp, stroke));
            } else {
                let pts = arc_points(*c, *r, 0.0, std::f64::consts::TAU, tol);
                poly(pts.iter().map(|q| view.to_screen(*q)).collect(), out);
            }
        }
        Prim::Arc { c, r, start, end } => {
            let pts = arc_points(*c, *r, *start, *end, tol);
            poly(pts.iter().map(|q| view.to_screen(*q)).collect(), out);
        }
        Prim::Fill(pts) => {
            let s: Vec<Pos2> = pts.iter().map(|q| view.to_screen(*q)).collect();
            out.push(Shape::convex_polygon(s, st.color, Stroke::NONE));
        }
        Prim::Text(_) | Prim::Image(_) => {}
    }
}

/// Text needs the painter (font layout), so it is drawn separately.
pub fn draw_text(painter: &Painter, view: &View, t: &icad_core::prim::Text, color: Color32) {
    // Text styles with a TrueType font, width factor or slant: font outlines.
    if let Some(fills) = icad_core::fonts::outline(t) {
        for p in fills {
            if let Prim::Fill(pts) = p {
                painter.add(Shape::convex_polygon(pts.iter().map(|q| view.to_screen(*q)).collect(), color, Stroke::NONE));
            }
        }
        return;
    }
    let size = (t.height / 0.72 * view.zoom) as f32;
    if t.content.is_empty() {
        return;
    }
    if size < 2.5 {
        // Too small to read: draw a placeholder bar like greeked text.
        let c = t.corners();
        let a = view.to_screen(c[0].mid(c[3]));
        let b = view.to_screen(c[1].mid(c[2]));
        painter.line_segment([a, b], Stroke::new(1.0f32, color.gamma_multiply(0.6)));
        return;
    }
    let galley = painter.layout_no_wrap(t.content.clone(), FontId::proportional(size.min(2000.0)), color);
    let baseline = galley.rows.first().and_then(|r| r.glyphs.first()).map(|g| g.pos.y).unwrap_or(size * 0.8);
    // Our width model (Helvetica metrics) vs egui's font: align by the egui width.
    let w = galley.size().x as f64 / view.zoom;
    let mut tt = t.clone();
    let ox = match t.halign {
        icad_core::HAlign::Left => 0.0,
        icad_core::HAlign::Center => -w / 2.0,
        icad_core::HAlign::Right => -w,
    };
    let oy = match t.valign {
        icad_core::VAlign::Baseline | icad_core::VAlign::Bottom => 0.0,
        icad_core::VAlign::Middle => -t.height / 2.0,
        icad_core::VAlign::Top => -t.height,
    };
    tt.pos = t.pos + V2::new(ox, oy).rotate(t.rotation);
    let base_left = view.to_screen(tt.pos);
    let rot = -(t.rotation as f32);
    // Screen "up" for text rotated by θ (CCW in world) is (-sin θ, -cos θ).
    let screen_up = vec2(-(t.rotation as f32).sin(), -(t.rotation as f32).cos());
    let top_left = base_left + screen_up * baseline;
    painter.add(TextShape::new(top_left, galley, color).with_angle(rot));
}

pub struct RenderOpts {
    pub on_paper: bool,
    pub lineweights: bool,
}

fn item_style(doc: &Document, color: u8, lt: icad_core::LineType, lw: f64, o: &RenderOpts) -> Style {
    Style {
        color: theme::aci(color, o.on_paper),
        width: if o.lineweights { (lw * 3.2).clamp(1.0, 5.0) as f32 } else { 1.0 },
        dash: lt.pattern().iter().map(|v| v * doc.ltscale * doc.ascale()).collect(),
    }
}

#[allow(clippy::too_many_arguments)]
pub fn draw_document(
    painter: &Painter,
    view: &View,
    doc: &Document,
    selection: &[u64],
    hover: Option<u64>,
    o: &RenderOpts,
    images: &mut ImageCache,
) {
    let wb = view.world_box();
    let mut shapes = Vec::with_capacity(doc.entities.len() * 2);
    let mut texts = vec![];
    for e in &doc.entities {
        if !doc.is_visible(e) {
            continue;
        }
        let items = doc.render_items(e);
        let bb = items.iter().fold(BBox::empty(), |b, it| b.union(&it.prim.bbox()));
        if !bb.intersects(&wb) {
            continue;
        }
        let selected = selection.contains(&e.id);
        for it in &items {
            let mut st = item_style(doc, it.color, it.linetype, it.lineweight, o);
            if selected {
                st.color = theme::SELECT_HL;
                st.dash = vec![3.0 / view.zoom, -3.0 / view.zoom];
            } else if hover == Some(e.id) {
                st.width += 1.5;
            }
            match &it.prim {
                Prim::Text(t) => texts.push((t.clone(), st.color)),
                Prim::Image(img) => {
                    let r = Rect::from_two_pos(view.to_screen(img.pos), view.to_screen(img.pos + V2::new(img.w, img.h)));
                    match images.get(painter.ctx(), &img.path) {
                        Some(tex) => {
                            let alpha = ((100 - img.fade.min(100)) as f32 / 100.0 * 255.0) as u8;
                            painter.image(
                                tex.id(),
                                r,
                                Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0)),
                                Color32::from_white_alpha(alpha),
                            );
                        }
                        None => {
                            painter.text(r.center(), Align2::CENTER_CENTER, "image not found", FontId::proportional(12.0), theme::ERR);
                        }
                    }
                    let frame = Prim::Poly(img.corners().to_vec(), true);
                    prim_shapes(view, &frame, &st, &mut shapes);
                }
                p => prim_shapes(view, p, &st, &mut shapes),
            }
        }
    }
    painter.extend(shapes);
    for (t, c) in texts {
        draw_text(painter, view, &t, c);
    }
}

pub fn draw_preview(painter: &Painter, view: &View, doc: &Document, shapes: &[icad_core::Shape], on_paper: bool) {
    let col = if on_paper { Color32::from_rgb(0, 90, 200) } else { theme::PREVIEW };
    let st = Style { color: col, width: 1.0, dash: vec![] };
    let mut out = vec![];
    let ctx = doc.ctx();
    for s in shapes {
        for p in s.prims(&ctx) {
            match &p {
                Prim::Text(t) => draw_text(painter, view, t, col),
                _ => prim_shapes(view, &p, &st, &mut out),
            }
        }
    }
    painter.extend(out);
}

/// Polar tracking: snap the direction from `base` to the nearest multiple of
/// `step_deg` when within `tol_px` (screen) of that ray. Returns the point and angle.
pub fn polar_track(view: &View, base: V2, p: V2, step_deg: f64, tol_px: f64) -> Option<(V2, f64)> {
    let d = p - base;
    let len = d.len();
    if len * view.zoom < 10.0 {
        return None;
    }
    let ang = deg(norm_angle(d.angle()));
    let snapped = (ang / step_deg).round() * step_deg;
    let q = base + V2::polar(len, rad(snapped));
    (q.dist(p) * view.zoom <= tol_px).then_some((q, snapped % 360.0))
}

pub fn draw_tracking_ray(painter: &Painter, view: &View, base: V2, ang: f64, rect: Rect) {
    let far = base + V2::polar(view.px((rect.width() + rect.height()) as f64 * 2.0), rad(ang));
    let st = Style { color: Color32::from_rgb(120, 255, 140), width: 1.0, dash: vec![view.px(5.0), -view.px(4.0)] };
    let mut out = vec![];
    prim_shapes(view, &Prim::Line(base, far), &st, &mut out);
    painter.extend(out);
}

pub fn draw_grips(painter: &Painter, view: &View, doc: &Document, selection: &[u64], hot: Option<(u64, usize)>) {
    if selection.len() > 200 {
        return;
    }
    for id in selection {
        if let Some(e) = doc.get(*id) {
            for (i, g) in e.shape.grips().iter().enumerate() {
                let p = view.to_screen(*g);
                let col = if hot == Some((*id, i)) { theme::GRIP_HOT } else { theme::GRIP };
                let r = Rect::from_center_size(p, vec2(9.0, 9.0));
                painter.rect_filled(r, 0.0, col);
                painter.rect_stroke(r, 0.0, Stroke::new(1.0f32, Color32::from_gray(20)), egui::StrokeKind::Outside);
            }
        }
    }
}

/// Grip under the cursor (entity id, grip index, world position).
pub fn grip_at(doc: &Document, view: &View, selection: &[u64], s: Pos2) -> Option<(u64, usize, V2)> {
    for id in selection {
        if let Some(e) = doc.get(*id) {
            for (i, g) in e.shape.grips().iter().enumerate() {
                if (view.to_screen(*g) - s).length() <= 7.0 {
                    return Some((*id, i, *g));
                }
            }
        }
    }
    None
}

pub fn draw_grid(painter: &Painter, view: &View, layout: bool) {
    if layout {
        return;
    }
    let wb = view.world_box();
    // Adaptive spacing: 1-2-5 series with ≥ 14 px between minor lines.
    let mut sp = 10f64.powf((14.0 / view.zoom).log10().floor());
    while sp * view.zoom < 14.0 {
        sp *= if (sp.log10().fract().abs() - 0.0).abs() < 1e-9 { 2.0 } else { 2.5 };
    }
    let major = sp * 5.0;
    let nx = (wb.width() / sp) as usize;
    if nx > 600 {
        return;
    }
    let mut shapes = vec![];
    let mut x = (wb.min.x / sp).floor() * sp;
    while x <= wb.max.x {
        let is_major = ((x / major).round() * major - x).abs() < sp * 1e-3;
        let col = if x.abs() < sp * 1e-3 {
            Color32::from_rgb(70, 50, 50)
        } else if is_major {
            theme::GRID_MAJOR
        } else {
            theme::GRID_MINOR
        };
        shapes.push(Shape::line_segment(
            [view.to_screen(V2::new(x, wb.min.y)), view.to_screen(V2::new(x, wb.max.y))],
            Stroke::new(1.0f32, col),
        ));
        x += sp;
    }
    let mut y = (wb.min.y / sp).floor() * sp;
    while y <= wb.max.y {
        let is_major = ((y / major).round() * major - y).abs() < sp * 1e-3;
        let col = if y.abs() < sp * 1e-3 {
            Color32::from_rgb(50, 70, 50)
        } else if is_major {
            theme::GRID_MAJOR
        } else {
            theme::GRID_MINOR
        };
        shapes.push(Shape::line_segment(
            [view.to_screen(V2::new(wb.min.x, y)), view.to_screen(V2::new(wb.max.x, y))],
            Stroke::new(1.0f32, col),
        ));
        y += sp;
    }
    painter.extend(shapes);
}

/// Sheet border + title block. In layout mode the paper is drawn white.
pub fn draw_sheet(painter: &Painter, view: &View, doc: &Document, layout: bool, lineweights: bool) {
    let sh = &doc.sheet;
    if layout {
        let b = sh.bounds();
        let r = Rect::from_two_pos(view.to_screen(b.min), view.to_screen(b.max));
        painter.rect_filled(r.translate(vec2(5.0, 5.0)), 0.0, Color32::from_black_alpha(90));
        painter.rect_filled(r, 0.0, theme::PAPER);
    }
    if !sh.visible {
        return;
    }
    let col = if layout { Color32::from_gray(25) } else { Color32::from_rgb(150, 160, 175) };
    let mut out = vec![];
    for (p, w) in doc.sheet_prims() {
        let st = Style { color: col, width: if lineweights { (w * 3.2).clamp(1.0, 4.0) as f32 } else { 1.0 }, dash: vec![] };
        match &p {
            Prim::Text(t) => draw_text(painter, view, t, col),
            _ => prim_shapes(view, &p, &st, &mut out),
        }
    }
    painter.extend(out);
}

pub fn draw_crosshair(painter: &Painter, rect: Rect, p: Pos2, layout: bool, show_box: bool) {
    let col = if layout { Color32::from_gray(40) } else { Color32::from_gray(230) };
    let s = Stroke::new(1.0f32, col);
    let len = 60.0;
    painter.line_segment([pos2((p.x - len).max(rect.left()), p.y), pos2((p.x + len).min(rect.right()), p.y)], s);
    painter.line_segment([pos2(p.x, (p.y - len).max(rect.top())), pos2(p.x, (p.y + len).min(rect.bottom()))], s);
    if show_box {
        painter.rect_stroke(Rect::from_center_size(p, vec2(9.0, 9.0)), 0.0, s, egui::StrokeKind::Middle);
    }
}

pub fn draw_ucs(painter: &Painter, rect: Rect, layout: bool) {
    let o = rect.left_bottom() + vec2(30.0, -30.0);
    let col = if layout { Color32::from_gray(60) } else { Color32::from_gray(200) };
    let s = Stroke::new(1.5f32, col);
    painter.line_segment([o, o + vec2(40.0, 0.0)], Stroke::new(1.5f32, Color32::from_rgb(230, 80, 80)));
    painter.line_segment([o, o + vec2(0.0, -40.0)], Stroke::new(1.5f32, Color32::from_rgb(80, 200, 100)));
    painter.text(o + vec2(46.0, 0.0), Align2::LEFT_CENTER, "X", FontId::proportional(11.0), col);
    painter.text(o + vec2(0.0, -48.0), Align2::CENTER_CENTER, "Y", FontId::proportional(11.0), col);
    painter.rect_stroke(Rect::from_center_size(o, vec2(6.0, 6.0)), 0.0, s, egui::StrokeKind::Middle);
}

fn mode_on(m: &OsnapModes, k: SnapKind) -> bool {
    match k {
        SnapKind::End => m.end,
        SnapKind::Mid => m.mid,
        SnapKind::Center => m.center,
        SnapKind::Quadrant => m.quadrant,
        SnapKind::Intersection => m.intersection,
        SnapKind::Node | SnapKind::Insertion => m.node,
        SnapKind::Nearest => m.nearest,
        SnapKind::Perpendicular => m.perpendicular,
    }
}

/// Find the best object snap near `p` within `aperture_px`.
pub fn find_snap(doc: &Document, view: &View, p: V2, base: Option<V2>, modes: &OsnapModes, aperture_px: f64) -> Option<(SnapKind, V2)> {
    let ap = view.px(aperture_px);
    let probe = BBox::from_pts(p, p).expand(ap);
    let near: Vec<&icad_core::Entity> =
        doc.entities.iter().filter(|e| doc.is_visible(e) && doc.entity_bbox(e).expand(ap).intersects(&probe)).take(400).collect();
    let mut cands: Vec<(SnapKind, V2)> = vec![];
    for e in &near {
        cands.extend(doc.snap_points_of(e).into_iter().filter(|(k, _)| mode_on(modes, *k)));
    }
    if modes.intersection {
        let curves: Vec<Curve> = near.iter().flat_map(|e| doc.curves_of(e)).collect();
        for i in 0..curves.len() {
            for j in i + 1..curves.len() {
                for q in curves[i].intersect(&curves[j]) {
                    if q.dist(p) <= ap {
                        cands.push((SnapKind::Intersection, q));
                    }
                }
            }
        }
    }
    if let (true, Some(b)) = (modes.perpendicular, base) {
        for e in &near {
            for c in doc.curves_of(e) {
                let q = match c {
                    Curve::Seg(a, bb) => a.lerp(bb, project_t(b, a, bb)),
                    Curve::Circle(cc, r) | Curve::Arc(cc, r, ..) => cc + (b - cc).norm() * r,
                };
                if q.dist(p) <= ap && q.dist(b) > 1e-9 {
                    cands.push((SnapKind::Perpendicular, q));
                }
            }
        }
    }
    let best = cands.into_iter().filter(|(_, q)| q.dist(p) <= ap).min_by(|a, b| {
        let pa = if a.0 == SnapKind::Perpendicular { ap * 0.3 } else { 0.0 };
        let pb = if b.0 == SnapKind::Perpendicular { ap * 0.3 } else { 0.0 };
        (a.1.dist(p) + pa).total_cmp(&(b.1.dist(p) + pb))
    });
    if best.is_some() {
        return best;
    }
    if modes.nearest {
        return near.iter().filter_map(|e| e.shape.nearest(p)).filter(|q| q.dist(p) <= ap).map(|q| (SnapKind::Nearest, q)).next();
    }
    None
}

pub fn draw_snap_marker(painter: &Painter, s: Pos2, kind: SnapKind) {
    let st = Stroke::new(2.0f32, theme::SNAP);
    let r = 6.0;
    match kind {
        SnapKind::End => {
            painter.rect_stroke(Rect::from_center_size(s, vec2(2.0 * r, 2.0 * r)), 0.0, st, egui::StrokeKind::Middle);
        }
        SnapKind::Mid => {
            painter.add(Shape::closed_line(vec![s + vec2(0.0, -r), s + vec2(r, r * 0.8), s + vec2(-r, r * 0.8)], st));
        }
        SnapKind::Center => {
            painter.circle_stroke(s, r, st);
        }
        SnapKind::Quadrant => {
            painter.add(Shape::closed_line(vec![s + vec2(0.0, -r), s + vec2(r, 0.0), s + vec2(0.0, r), s + vec2(-r, 0.0)], st));
        }
        SnapKind::Intersection => {
            painter.line_segment([s + vec2(-r, -r), s + vec2(r, r)], st);
            painter.line_segment([s + vec2(-r, r), s + vec2(r, -r)], st);
        }
        SnapKind::Insertion => {
            painter.add(Shape::closed_line(
                vec![s + vec2(-r, -r), s + vec2(0.0, -r), s + vec2(0.0, 0.0), s + vec2(r, 0.0), s + vec2(r, r), s + vec2(-r, r)],
                st,
            ));
        }
        SnapKind::Node => {
            painter.circle_stroke(s, r, st);
            painter.line_segment([s + vec2(-r, -r), s + vec2(r, r)], st);
            painter.line_segment([s + vec2(-r, r), s + vec2(r, -r)], st);
        }
        SnapKind::Nearest => {
            painter.add(Shape::closed_line(vec![s + vec2(-r, -r), s + vec2(r, r), s + vec2(-r, r), s + vec2(r, -r)], st));
        }
        SnapKind::Perpendicular => {
            painter.line_segment([s + vec2(-r, r), s + vec2(r, r)], st);
            painter.line_segment([s + vec2(-r, r), s + vec2(-r, -r)], st);
            painter.line_segment([s + vec2(-r, 0.0), s + vec2(0.0, 0.0)], st);
            painter.line_segment([s + vec2(0.0, 0.0), s + vec2(0.0, r)], st);
        }
    }
    painter.text(s + vec2(12.0, 14.0), Align2::LEFT_TOP, kind.label(), FontId::proportional(11.0), theme::SNAP);
}

/// Draw primitives fitted into a box of `size` (dialog previews).
pub fn preview_prims(ui: &mut egui::Ui, prims: &[Prim], size: egui::Vec2) {
    let (rect, _) = ui.allocate_exact_size(size, egui::Sense::hover());
    let painter = ui.painter_at(rect);
    painter.rect_filled(rect, 4.0, theme::CANVAS);
    let b = prims.iter().fold(BBox::empty(), |b, p| b.union(&p.bbox()));
    let mut view = View { rect: rect.shrink(10.0), ..Default::default() };
    view.fit(&b);
    let st = Style { color: theme::TEXT, width: 1.2, dash: vec![] };
    let mut shapes = vec![];
    for p in prims {
        prim_shapes(&view, p, &st, &mut shapes);
        if let Prim::Text(t) = p {
            draw_text(&painter, &view, t, theme::TEXT);
        }
    }
    painter.extend(shapes);
}
