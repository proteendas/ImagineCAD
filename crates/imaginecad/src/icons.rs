// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Proteen Das

//! Vector ribbon icons drawn with the egui painter (no bitmap assets).

use eframe::egui::{pos2, vec2, Align2, Color32, FontId, Painter, Pos2, Rect, Shape, Stroke};
use std::f32::consts::{FRAC_PI_2, PI, TAU};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Icon {
    Line,
    Polyline,
    Circle,
    Arc,
    Rect,
    Polygon,
    Text,
    Move,
    Copy,
    Rotate,
    Mirror,
    Scale,
    Offset,
    Trim,
    Extend,
    Fillet,
    Chamfer,
    Erase,
    Explode,
    Array,
    Match,
    DimLinear,
    DimAligned,
    DimRadius,
    DimDiameter,
    CenterMark,
    Layers,
    Measure,
    SelectAll,
    ImageToCad,
    AiSettings,
    Chat,
    Pdf,
    Dxf,
    Dwg,
    Svg,
    ZoomExtents,
    ZoomWindow,
    Pan,
    Grid,
    TitleBlock,
    PageSetup,
    New,
    Open,
    Save,
    Undo,
    Redo,
    Paste,
    DimStyle,
    Properties,
    Help,
    Ellipse,
    Spline,
    Hatch,
    Point,
    XLine,
    Stretch,
    Break,
    Join,
    Lengthen,
    PolarArray,
    Leader,
    MText,
    DimAngular,
    DimBaseline,
    DimContinue,
    DimOrdinate,
    DimArc,
    QDim,
    Block,
    Insert,
    Area,
    List,
    QSelect,
    Image,
    Isolate,
    Key,
}

fn ln(p: &Painter, a: Pos2, b: Pos2, s: Stroke) {
    p.line_segment([a, b], s);
}

fn arc_pts(c: Pos2, r: f32, a0: f32, a1: f32) -> Vec<Pos2> {
    let n = 24;
    (0..=n)
        .map(|i| {
            let a = a0 + (a1 - a0) * i as f32 / n as f32;
            pos2(c.x + r * a.cos(), c.y - r * a.sin())
        })
        .collect()
}

fn page(p: &Painter, r: Rect, s: Stroke) -> Rect {
    let w = r.width() * 0.62;
    let pr = Rect::from_center_size(r.center(), vec2(w, r.height() * 0.82));
    let fold = w * 0.3;
    p.add(Shape::closed_line(
        vec![pr.left_top(), pos2(pr.right() - fold, pr.top()), pos2(pr.right(), pr.top() + fold), pr.right_bottom(), pr.left_bottom()],
        s,
    ));
    pr
}

fn arrow_head(p: &Painter, tip: Pos2, dir: eframe::egui::Vec2, size: f32, c: Color32) {
    let d = dir.normalized();
    let n = vec2(-d.y, d.x);
    let base = tip - d * size;
    p.add(Shape::convex_polygon(vec![tip, base + n * size * 0.45, base - n * size * 0.45], c, Stroke::NONE));
}

/// Draw `icon` inside `r` with the given colour.
pub fn draw(p: &Painter, r: Rect, icon: Icon, col: Color32) {
    let s = Stroke::new((r.width() / 14.0).clamp(1.2, 2.2), col);
    let thin = Stroke::new(1.0f32, col);
    let c = r.center();
    let u = r.width() / 2.0 * 0.78; // half-extent
    let tl = c + vec2(-u, -u);
    let br = c + vec2(u, u);
    let tr = c + vec2(u, -u);
    let bl = c + vec2(-u, u);
    let grip = |pt: Pos2| p.rect_filled(Rect::from_center_size(pt, vec2(u * 0.28, u * 0.28)), 0.0, crate::theme::GRIP);
    let accent = crate::theme::AI_ACCENT;
    match icon {
        Icon::Line => {
            ln(p, bl, tr, s);
            grip(bl);
            grip(tr);
        }
        Icon::Polyline => {
            let pts = vec![bl, c + vec2(-u * 0.3, -u * 0.5), c + vec2(u * 0.3, u * 0.4), tr];
            p.add(Shape::line(pts.clone(), s));
            for q in pts {
                grip(q);
            }
        }
        Icon::Circle => {
            p.circle_stroke(c, u * 0.9, s);
            p.circle_filled(c, u * 0.12, col);
        }
        Icon::Arc => {
            p.add(Shape::line(arc_pts(c + vec2(0.0, u * 0.5), u, 0.0, PI), s));
            grip(c + vec2(-u, u * 0.5));
            grip(c + vec2(u, u * 0.5));
            grip(c + vec2(0.0, -u * 0.5));
        }
        Icon::Rect => {
            p.rect_stroke(Rect::from_min_max(tl + vec2(0.0, u * 0.3), br - vec2(0.0, u * 0.3)), 0.0, s, eframe::egui::StrokeKind::Middle);
        }
        Icon::Polygon => {
            let pts: Vec<Pos2> = (0..6)
                .map(|i| {
                    let a = i as f32 * TAU / 6.0;
                    pos2(c.x + u * a.cos(), c.y + u * a.sin())
                })
                .collect();
            p.add(Shape::closed_line(pts, s));
        }
        Icon::Text => {
            p.text(c, Align2::CENTER_CENTER, "A", FontId::proportional(u * 2.0), col);
        }
        Icon::Move => {
            for d in [vec2(1.0, 0.0), vec2(-1.0, 0.0), vec2(0.0, 1.0), vec2(0.0, -1.0)] {
                ln(p, c, c + d * u * 0.8, s);
                arrow_head(p, c + d * u, d, u * 0.4, col);
            }
        }
        Icon::Copy => {
            p.rect_stroke(Rect::from_min_size(tl, vec2(u * 1.2, u * 1.2)), 0.0, s, eframe::egui::StrokeKind::Middle);
            p.rect_stroke(
                Rect::from_min_size(c - vec2(u * 0.2, u * 0.2), vec2(u * 1.2, u * 1.2)),
                0.0,
                s,
                eframe::egui::StrokeKind::Middle,
            );
        }
        Icon::Rotate => {
            p.add(Shape::line(arc_pts(c, u * 0.85, 0.3, 1.75 * PI), s));
            let a = 0.3f32;
            arrow_head(p, pos2(c.x + u * 0.85 * a.cos(), c.y - u * 0.85 * a.sin()), vec2(-a.sin(), -a.cos()) * -1.0, u * 0.45, col);
        }
        Icon::Mirror => {
            let dash = Stroke::new(1.0f32, crate::theme::TEXT_DIM);
            for i in 0..5 {
                let y0 = tl.y + i as f32 * u * 0.42;
                ln(p, pos2(c.x, y0), pos2(c.x, y0 + u * 0.25), dash);
            }
            p.add(Shape::closed_line(vec![pos2(c.x - u * 0.2, bl.y), pos2(tl.x, bl.y), pos2(c.x - u * 0.2, tl.y + u * 0.4)], s));
            p.add(Shape::closed_line(vec![pos2(c.x + u * 0.2, bl.y), pos2(tr.x, bl.y), pos2(c.x + u * 0.2, tl.y + u * 0.4)], thin));
        }
        Icon::Scale => {
            p.rect_stroke(Rect::from_min_max(c + vec2(-u, 0.0), c + vec2(0.0, u)), 0.0, thin, eframe::egui::StrokeKind::Middle);
            p.rect_stroke(Rect::from_min_max(tl, br), 0.0, s, eframe::egui::StrokeKind::Middle);
            arrow_head(p, tr, vec2(1.0, -1.0), u * 0.45, col);
        }
        Icon::Offset => {
            p.add(Shape::line(arc_pts(c + vec2(0.0, u * 0.9), u * 0.9, 0.2, PI - 0.2), s));
            p.add(Shape::line(arc_pts(c + vec2(0.0, u * 0.9), u * 1.5, 0.35, PI - 0.35), thin));
        }
        Icon::Trim => {
            ln(p, pos2(c.x, tl.y), pos2(c.x, bl.y), s);
            ln(p, pos2(tl.x, c.y), pos2(c.x, c.y), s);
            let dash = Stroke::new(1.0f32, crate::theme::ERR);
            ln(p, pos2(c.x + u * 0.15, c.y), pos2(tr.x, c.y), dash);
            ln(p, c + vec2(u * 0.4, -u * 0.25), c + vec2(u * 0.8, u * 0.25), dash);
            ln(p, c + vec2(u * 0.4, u * 0.25), c + vec2(u * 0.8, -u * 0.25), dash);
        }
        Icon::Extend => {
            ln(p, pos2(tr.x, tl.y), pos2(tr.x, bl.y), s);
            ln(p, pos2(tl.x, c.y), pos2(c.x, c.y), s);
            ln(p, pos2(c.x, c.y), pos2(tr.x - u * 0.3, c.y), Stroke::new(1.0f32, crate::theme::ACCENT));
            arrow_head(p, pos2(tr.x - u * 0.05, c.y), vec2(1.0, 0.0), u * 0.4, crate::theme::ACCENT);
        }
        Icon::Fillet => {
            ln(p, tl, pos2(tl.x, c.y + u * 0.1), s);
            ln(p, pos2(c.x - u * 0.1, br.y), br, s);
            p.add(Shape::line(arc_pts(pos2(c.x - u * 0.1, c.y + u * 0.1), u * 0.9, PI, 1.5 * PI), s));
        }
        Icon::Chamfer => {
            ln(p, tl, pos2(tl.x, c.y + u * 0.2), s);
            ln(p, pos2(tl.x, c.y + u * 0.2), pos2(c.x - u * 0.2, br.y), s);
            ln(p, pos2(c.x - u * 0.2, br.y), br, s);
        }
        Icon::Erase => {
            let e = Stroke::new(s.width, crate::theme::ERR);
            p.add(Shape::closed_line(
                vec![c + vec2(-u, u * 0.2), c + vec2(-u * 0.2, -u * 0.6), c + vec2(u, u * 0.6 - u * 0.0), c + vec2(u * 0.2, u)],
                e,
            ));
            ln(p, c + vec2(-u * 0.6, -u * 0.2), c + vec2(u * 0.6, u * 1.0 - u * 0.2), thin);
        }
        Icon::Explode => {
            for i in 0..8 {
                let a = i as f32 * TAU / 8.0;
                let d = vec2(a.cos(), a.sin());
                ln(p, c + d * u * 0.35, c + d * u, s);
            }
        }
        Icon::Array => {
            for i in 0..3 {
                for j in 0..3 {
                    let q = tl + vec2(i as f32 * u * 0.75, j as f32 * u * 0.75);
                    p.rect_stroke(Rect::from_min_size(q, vec2(u * 0.45, u * 0.45)), 0.0, thin, eframe::egui::StrokeKind::Middle);
                }
            }
        }
        Icon::Match => {
            p.rect_filled(Rect::from_min_size(tl, vec2(u * 2.0, u * 0.7)), 2.0, col);
            ln(p, c + vec2(0.0, -u * 0.3), c + vec2(0.0, u), Stroke::new(s.width * 1.8, col));
        }
        Icon::DimLinear | Icon::DimAligned => {
            let (a, b) =
                if icon == Icon::DimLinear { (bl + vec2(0.0, -u * 0.2), br + vec2(0.0, -u * 0.2)) } else { (bl, tr + vec2(0.0, u * 0.6)) };
            let up = if icon == Icon::DimLinear { vec2(0.0, -1.0) } else { (b - a).normalized().rot90() };
            let up = if up.y > 0.0 { -up } else { up };
            let off = up * u * 1.1;
            ln(p, a, a + off * 1.2, thin);
            ln(p, b, b + off * 1.2, thin);
            ln(p, a + off, b + off, thin);
            let d = (b - a).normalized();
            arrow_head(p, a + off, -d, u * 0.4, col);
            arrow_head(p, b + off, d, u * 0.4, col);
            ln(p, a, b, s);
        }
        Icon::DimRadius | Icon::DimDiameter => {
            p.circle_stroke(c, u * 0.9, s);
            let d = vec2(std::f32::consts::FRAC_1_SQRT_2, -std::f32::consts::FRAC_1_SQRT_2);
            let from = if icon == Icon::DimRadius { c } else { c - d * u * 0.9 };
            ln(p, from, c + d * u * 0.9, thin);
            arrow_head(p, c + d * u * 0.9, d, u * 0.4, col);
            if icon == Icon::DimDiameter {
                arrow_head(p, c - d * u * 0.9, -d, u * 0.4, col);
            }
        }
        Icon::CenterMark => {
            p.circle_stroke(c, u * 0.7, s);
            let red = Stroke::new(1.0f32, Color32::from_rgb(255, 90, 90));
            ln(p, c - vec2(u, 0.0), c + vec2(u, 0.0), red);
            ln(p, c - vec2(0.0, u), c + vec2(0.0, u), red);
        }
        Icon::Layers => {
            for (i, fill) in [crate::theme::ACCENT, Color32::from_rgb(90, 200, 120), Color32::from_rgb(240, 190, 60)].iter().enumerate() {
                let y = c.y + u * 0.5 - i as f32 * u * 0.5;
                let pts = vec![pos2(c.x - u, y), pos2(c.x, y - u * 0.45), pos2(c.x + u, y), pos2(c.x, y + u * 0.45)];
                p.add(Shape::convex_polygon(pts, *fill, Stroke::new(1.0f32, Color32::from_gray(30))));
            }
        }
        Icon::Measure => {
            let rr = Rect::from_center_size(c, vec2(u * 2.0, u * 0.8));
            p.rect_stroke(rr, 1.0, s, eframe::egui::StrokeKind::Middle);
            for i in 1..6 {
                let x = rr.left() + rr.width() * i as f32 / 6.0;
                let h = if i % 2 == 0 { 0.5 } else { 0.3 };
                ln(p, pos2(x, rr.top()), pos2(x, rr.top() + rr.height() * h), thin);
            }
        }
        Icon::SelectAll => {
            let d = Stroke::new(1.0f32, col);
            p.rect_stroke(Rect::from_min_max(tl, br), 0.0, d, eframe::egui::StrokeKind::Middle);
            p.add(Shape::convex_polygon(
                vec![c, c + vec2(0.0, u * 0.9), c + vec2(u * 0.25, u * 0.65), c + vec2(u * 0.6, u * 0.6)],
                col,
                Stroke::NONE,
            ));
        }
        Icon::ImageToCad => {
            let pic = Rect::from_min_max(tl + vec2(0.0, u * 0.2), c + vec2(u * 0.3, u * 0.9));
            p.rect_stroke(pic, 1.0, thin, eframe::egui::StrokeKind::Middle);
            p.add(Shape::line(
                vec![
                    pic.left_bottom() + vec2(2.0, -2.0),
                    pic.center() + vec2(-2.0, 1.0),
                    pic.center() + vec2(2.0, -2.0),
                    pic.right_bottom() + vec2(-2.0, -2.0),
                ],
                thin,
            ));
            // sparkle
            let sc = tr + vec2(-u * 0.25, u * 0.35);
            for (d, k) in [(vec2(1.0, 0.0), 0.55), (vec2(0.0, 1.0), 0.55)] {
                ln(p, sc - d * u * k, sc + d * u * k, Stroke::new(s.width, accent));
            }
            p.circle_filled(sc, u * 0.12, accent);
            ln(p, br + vec2(-u * 0.9, 0.0), br, Stroke::new(s.width, crate::theme::ACCENT));
            ln(p, br, br + vec2(0.0, -u * 0.6), Stroke::new(s.width, crate::theme::ACCENT));
        }
        Icon::AiSettings => {
            p.circle_stroke(c, u * 0.45, s);
            for i in 0..8 {
                let a = i as f32 * TAU / 8.0;
                let d = vec2(a.cos(), a.sin());
                ln(p, c + d * u * 0.6, c + d * u * 0.95, Stroke::new(s.width * 1.4, col));
            }
            p.circle_filled(c, u * 0.18, accent);
        }
        Icon::Chat => {
            // Speech bubble with three dots.
            let b = Rect::from_min_max(tl + vec2(0.0, u * 0.15), br - vec2(0.0, u * 0.55));
            p.rect_stroke(b, u * 0.35, s, eframe::egui::StrokeKind::Middle);
            p.add(Shape::line(
                vec![
                    b.left_bottom() + vec2(u * 0.45, 0.0),
                    b.left_bottom() + vec2(u * 0.25, u * 0.5),
                    b.left_bottom() + vec2(u * 0.95, 0.0),
                ],
                s,
            ));
            for k in [-1.0, 0.0, 1.0] {
                p.circle_filled(b.center() + vec2(k * u * 0.45, 0.0), u * 0.12, accent);
            }
        }
        Icon::Pdf | Icon::Dxf | Icon::Dwg | Icon::Svg => {
            let pr = page(p, r, thin);
            let (label, bg) = match icon {
                Icon::Pdf => ("PDF", Color32::from_rgb(200, 50, 50)),
                Icon::Dxf => ("DXF", Color32::from_rgb(40, 130, 200)),
                Icon::Dwg => ("DWG", Color32::from_rgb(40, 150, 110)),
                _ => ("SVG", Color32::from_rgb(220, 140, 30)),
            };
            let band = Rect::from_min_size(pos2(pr.left() - u * 0.25, pr.center().y), vec2(pr.width() + u * 0.5, u * 0.7));
            p.rect_filled(band, 1.0, bg);
            p.text(band.center(), Align2::CENTER_CENTER, label, FontId::proportional(u * 0.62), Color32::WHITE);
        }
        Icon::ZoomExtents | Icon::ZoomWindow => {
            let lc = c + vec2(-u * 0.2, -u * 0.2);
            p.circle_stroke(lc, u * 0.6, s);
            ln(p, lc + vec2(u * 0.42, u * 0.42), br, Stroke::new(s.width * 1.6, col));
            if icon == Icon::ZoomWindow {
                p.rect_stroke(Rect::from_center_size(lc, vec2(u * 0.6, u * 0.5)), 0.0, thin, eframe::egui::StrokeKind::Middle);
            } else {
                for d in [vec2(-1.0, 0.0), vec2(1.0, 0.0)] {
                    ln(p, lc, lc + d * u * 0.35, thin);
                }
            }
        }
        Icon::Pan => {
            for d in [vec2(1.0, 0.0), vec2(-1.0, 0.0), vec2(0.0, 1.0), vec2(0.0, -1.0)] {
                ln(p, c + d * u * 0.2, c + d * u * 0.75, s);
                arrow_head(p, c + d * u, d, u * 0.35, col);
            }
            p.circle_filled(c, u * 0.2, col);
        }
        Icon::Grid => {
            for i in 0..4 {
                let t = i as f32 / 3.0;
                ln(p, pos2(tl.x + 2.0 * u * t, tl.y), pos2(tl.x + 2.0 * u * t, bl.y), thin);
                ln(p, pos2(tl.x, tl.y + 2.0 * u * t), pos2(tr.x, tl.y + 2.0 * u * t), thin);
            }
        }
        Icon::TitleBlock | Icon::PageSetup => {
            let pr = Rect::from_min_max(tl + vec2(-u * 0.1, u * 0.2), br + vec2(u * 0.1, -u * 0.1));
            p.rect_stroke(pr, 0.0, thin, eframe::egui::StrokeKind::Middle);
            let tb = Rect::from_min_max(pos2(pr.center().x, pr.bottom() - pr.height() * 0.35), pr.right_bottom());
            p.rect_filled(tb, 0.0, if icon == Icon::TitleBlock { crate::theme::ACCENT } else { col.gamma_multiply(0.5) });
            ln(p, pos2(tb.left(), tb.center().y), pos2(tb.right(), tb.center().y), Stroke::new(1.0f32, Color32::from_gray(20)));
        }
        Icon::New => {
            page(p, r, s);
        }
        Icon::Open => {
            p.add(Shape::closed_line(
                vec![
                    bl,
                    pos2(bl.x, tl.y + u * 0.3),
                    pos2(c.x - u * 0.3, tl.y + u * 0.3),
                    pos2(c.x, tl.y + u * 0.6),
                    pos2(tr.x, tl.y + u * 0.6),
                    br,
                ],
                s,
            ));
        }
        Icon::Save => {
            p.rect_stroke(Rect::from_min_max(tl, br), 2.0, s, eframe::egui::StrokeKind::Middle);
            p.rect_filled(Rect::from_min_max(tl + vec2(u * 0.4, 0.0), pos2(tr.x - u * 0.4, tl.y + u * 0.6)), 0.0, col);
            p.rect_stroke(
                Rect::from_min_max(pos2(bl.x + u * 0.3, c.y + u * 0.2), br - vec2(u * 0.3, 0.0)),
                0.0,
                thin,
                eframe::egui::StrokeKind::Middle,
            );
        }
        Icon::Undo | Icon::Redo => {
            let flip = if icon == Icon::Undo { 1.0 } else { -1.0 };
            let pts: Vec<Pos2> =
                arc_pts(c + vec2(0.0, u * 0.3), u * 0.7, 0.0, PI).into_iter().map(|q| pos2(c.x + (q.x - c.x) * flip, q.y)).collect();
            let end = pts[pts.len() - 1];
            p.add(Shape::line(pts, s));
            arrow_head(p, end + vec2(0.0, u * 0.35), vec2(0.0, 1.0), u * 0.45, col);
        }
        Icon::Paste => {
            p.rect_stroke(
                Rect::from_min_max(tl + vec2(u * 0.2, u * 0.1), br - vec2(u * 0.2, 0.0)),
                2.0,
                s,
                eframe::egui::StrokeKind::Middle,
            );
            p.rect_filled(Rect::from_center_size(pos2(c.x, tl.y + u * 0.1), vec2(u * 0.8, u * 0.4)), 1.0, col);
        }
        Icon::DimStyle => {
            ln(p, bl + vec2(0.0, -u * 0.6), br + vec2(0.0, -u * 0.6), thin);
            arrow_head(p, bl + vec2(0.0, -u * 0.6), vec2(-1.0, 0.0), u * 0.4, col);
            arrow_head(p, br + vec2(0.0, -u * 0.6), vec2(1.0, 0.0), u * 0.4, col);
            p.text(c + vec2(0.0, -u * 0.35), Align2::CENTER_CENTER, "1.5", FontId::proportional(u * 0.8), col);
        }
        Icon::Properties => {
            p.rect_stroke(Rect::from_min_max(tl, br), 1.0, thin, eframe::egui::StrokeKind::Middle);
            for i in 0..3 {
                let y = tl.y + u * 0.5 + i as f32 * u * 0.55;
                ln(p, pos2(tl.x + u * 0.3, y), pos2(c.x, y), thin);
                p.rect_filled(Rect::from_min_max(pos2(c.x + u * 0.15, y - u * 0.15), pos2(tr.x - u * 0.3, y + u * 0.15)), 0.0, col);
            }
        }
        Icon::Help => {
            p.circle_stroke(c, u * 0.9, s);
            p.text(c, Align2::CENTER_CENTER, "?", FontId::proportional(u * 1.3), col);
        }
        Icon::Ellipse => {
            let pts: Vec<Pos2> = (0..=40)
                .map(|i| {
                    let a = i as f32 / 40.0 * TAU;
                    pos2(c.x + u * a.cos(), c.y + u * 0.55 * a.sin())
                })
                .collect();
            p.add(Shape::line(pts, s));
            p.circle_filled(c, u * 0.1, col);
        }
        Icon::Spline => {
            let pts: Vec<Pos2> = (0..=30)
                .map(|i| {
                    let t = i as f32 / 30.0;
                    pos2(tl.x + 2.0 * u * t, c.y + u * 0.7 * (t * TAU).sin())
                })
                .collect();
            p.add(Shape::line(pts, s));
            grip(pos2(tl.x, c.y));
            grip(pos2(tr.x, c.y));
        }
        Icon::Hatch => {
            let rr = Rect::from_min_max(tl, br);
            p.rect_stroke(rr, 0.0, s, eframe::egui::StrokeKind::Middle);
            for i in 1..6 {
                let d = 2.0 * u * i as f32 / 6.0;
                let a = pos2(tl.x, tl.y + d);
                let b = pos2(tl.x + d, tl.y);
                p.line_segment([a, b], thin);
                p.line_segment([pos2(tl.x + d, bl.y), pos2(tr.x, tl.y + d)], thin);
            }
        }
        Icon::Point => {
            ln(p, c - vec2(u * 0.7, 0.0), c + vec2(u * 0.7, 0.0), s);
            ln(p, c - vec2(0.0, u * 0.7), c + vec2(0.0, u * 0.7), s);
            p.circle_stroke(c, u * 0.35, thin);
        }
        Icon::XLine => {
            ln(p, bl - vec2(u * 0.2, -u * 0.2), tr + vec2(u * 0.2, -u * 0.2), Stroke::new(1.0f32, crate::theme::TEXT_DIM));
            grip(c);
            arrow_head(p, tr, vec2(1.0, -1.0), u * 0.35, col);
            arrow_head(p, bl, vec2(-1.0, 1.0), u * 0.35, col);
        }
        Icon::Stretch => {
            p.add(Shape::closed_line(vec![bl, pos2(bl.x, tl.y + u * 0.6), pos2(c.x, tl.y + u * 0.6), pos2(c.x, bl.y)], thin));
            let d = Stroke::new(1.0f32, crate::theme::ACCENT);
            p.rect_stroke(
                Rect::from_min_max(pos2(c.x - u * 0.3, tl.y + u * 0.3), pos2(tr.x, bl.y - u * 0.3)),
                0.0,
                d,
                eframe::egui::StrokeKind::Middle,
            );
            arrow_head(p, pos2(tr.x, c.y), vec2(1.0, 0.0), u * 0.4, col);
        }
        Icon::Break => {
            ln(p, pos2(tl.x, c.y), pos2(c.x - u * 0.3, c.y), s);
            ln(p, pos2(c.x + u * 0.3, c.y), pos2(tr.x, c.y), s);
            grip(pos2(c.x - u * 0.3, c.y));
            grip(pos2(c.x + u * 0.3, c.y));
        }
        Icon::Join => {
            ln(p, pos2(tl.x, c.y + u * 0.3), pos2(c.x, c.y + u * 0.3), s);
            ln(p, pos2(c.x, c.y + u * 0.3), pos2(tr.x, c.y - u * 0.5), s);
            p.circle_filled(pos2(c.x, c.y + u * 0.3), u * 0.2, crate::theme::ACCENT);
        }
        Icon::Lengthen => {
            ln(p, pos2(tl.x, c.y), pos2(c.x, c.y), s);
            ln(p, pos2(c.x, c.y), pos2(tr.x - u * 0.3, c.y), Stroke::new(1.0f32, crate::theme::ACCENT));
            arrow_head(p, pos2(tr.x, c.y), vec2(1.0, 0.0), u * 0.4, crate::theme::ACCENT);
        }
        Icon::PolarArray => {
            for i in 0..6 {
                let a = i as f32 * TAU / 6.0;
                p.circle_stroke(c + vec2(a.cos(), a.sin()) * u * 0.72, u * 0.2, thin);
            }
            p.circle_filled(c, u * 0.12, col);
        }
        Icon::Leader => {
            ln(p, bl, c, thin);
            ln(p, c, c + vec2(u * 0.4, 0.0), thin);
            arrow_head(p, bl, vec2(-1.0, 1.0), u * 0.45, col);
            p.text(tr + vec2(-u * 0.2, u * 0.6), Align2::RIGHT_CENTER, "A", FontId::proportional(u * 1.1), col);
        }
        Icon::MText => {
            for i in 0..4 {
                let y = tl.y + u * 0.25 + i as f32 * u * 0.5;
                let w = if i == 3 { 1.1 } else { 2.0 };
                ln(p, pos2(tl.x, y), pos2(tl.x + u * w, y), s);
            }
        }
        Icon::DimAngular => {
            ln(p, bl, br, s);
            ln(p, bl, bl + vec2(u * 1.4, -u * 1.4), s);
            p.add(Shape::line(arc_pts(bl, u * 1.3, 0.0, PI / 4.0), thin));
            p.text(c + vec2(u * 0.5, u * 0.2), Align2::CENTER_CENTER, "°", FontId::proportional(u * 1.0), col);
        }
        Icon::DimBaseline | Icon::DimContinue => {
            let y1 = tl.y + u * 0.4;
            let y2 = tl.y + u * 1.0;
            ln(p, pos2(tl.x, y1), pos2(c.x, y1), thin);
            if icon == Icon::DimBaseline {
                ln(p, pos2(tl.x, y2), pos2(tr.x, y2), thin);
            } else {
                ln(p, pos2(c.x, y1), pos2(tr.x, y1), thin);
            }
            for x in [tl.x, c.x, tr.x] {
                ln(p, pos2(x, tl.y), pos2(x, bl.y), Stroke::new(1.0f32, crate::theme::TEXT_DIM));
            }
            ln(p, bl, br, s);
        }
        Icon::DimOrdinate => {
            // Datum corner with three ordinate leaders.
            ln(p, bl, br, s);
            ln(p, bl, tl, s);
            for k in [0.35, 0.7, 1.0] {
                let x = bl.x + (br.x - bl.x) * k;
                ln(p, pos2(x, bl.y - u * 0.2), pos2(x, tl.y + u * 0.5), thin);
                p.circle_filled(pos2(x, tl.y + u * 0.35), u * 0.12, accent);
            }
        }
        Icon::DimArc => {
            p.add(Shape::line(arc_pts(pos2(c.x, bl.y), u * 1.0, PI * 0.15, PI * 0.85), s));
            p.add(Shape::line(arc_pts(pos2(c.x, bl.y), u * 1.5, PI * 0.15, PI * 0.85), thin));
            p.add(Shape::line(arc_pts(pos2(c.x, tl.y + u * 0.35), u * 0.35, PI * 0.2, PI * 0.8), Stroke::new(s.width, accent)));
        }
        Icon::QDim => {
            ln(p, bl + vec2(0.0, -u * 0.6), br + vec2(0.0, -u * 0.6), s);
            for k in [0.0, 0.4, 1.0] {
                let x = bl.x + (br.x - bl.x) * k;
                ln(p, pos2(x, tl.y + u * 0.3), pos2(x, bl.y), thin);
            }
            // lightning bolt = quick
            p.add(Shape::line(
                vec![
                    c + vec2(u * 0.1, -u * 0.9),
                    c + vec2(-u * 0.25, -u * 0.25),
                    c + vec2(u * 0.2, -u * 0.25),
                    c + vec2(-u * 0.15, u * 0.4),
                ],
                Stroke::new(s.width, accent),
            ));
        }
        Icon::Block | Icon::Insert => {
            p.rect_stroke(
                Rect::from_min_max(tl + vec2(u * 0.2, u * 0.2), br - vec2(u * 0.2, u * 0.2)),
                2.0,
                s,
                eframe::egui::StrokeKind::Middle,
            );
            p.circle_stroke(c, u * 0.35, thin);
            if icon == Icon::Insert {
                arrow_head(p, c, vec2(1.0, 1.0), u * 0.5, crate::theme::ACCENT);
                ln(p, c - vec2(u * 0.9, u * 0.9), c - vec2(u * 0.3, u * 0.3), Stroke::new(s.width, crate::theme::ACCENT));
            }
        }
        Icon::Area => {
            p.add(Shape::convex_polygon(vec![bl, pos2(c.x, tl.y), tr, br], col.gamma_multiply(0.35), s));
        }
        Icon::List => {
            for i in 0..4 {
                let y = tl.y + u * 0.3 + i as f32 * u * 0.45;
                p.circle_filled(pos2(tl.x + u * 0.15, y), u * 0.08, col);
                ln(p, pos2(tl.x + u * 0.4, y), pos2(tr.x, y), thin);
            }
        }
        Icon::QSelect => {
            p.rect_stroke(Rect::from_min_max(tl, br), 0.0, Stroke::new(1.0f32, col), eframe::egui::StrokeKind::Middle);
            p.text(c, Align2::CENTER_CENTER, "Q", FontId::proportional(u * 1.3), crate::theme::ACCENT);
        }
        Icon::Image => {
            let pic = Rect::from_min_max(tl + vec2(0.0, u * 0.2), br - vec2(0.0, u * 0.2));
            p.rect_stroke(pic, 1.0, s, eframe::egui::StrokeKind::Middle);
            p.add(Shape::convex_polygon(
                vec![pic.left_bottom(), pic.center() + vec2(-u * 0.2, 0.0), pic.center_bottom() + vec2(u * 0.2, 0.0)],
                col.gamma_multiply(0.6),
                Stroke::NONE,
            ));
            p.circle_filled(pic.right_top() + vec2(-u * 0.45, u * 0.4), u * 0.18, crate::theme::WARN);
        }
        Icon::Isolate => {
            for (i, f) in [col.gamma_multiply(0.3), crate::theme::ACCENT, col.gamma_multiply(0.3)].iter().enumerate() {
                let y = c.y + u * 0.5 - i as f32 * u * 0.5;
                p.add(Shape::convex_polygon(
                    vec![pos2(c.x - u, y), pos2(c.x, y - u * 0.45), pos2(c.x + u, y), pos2(c.x, y + u * 0.45)],
                    *f,
                    Stroke::new(1.0f32, Color32::from_gray(30)),
                ));
            }
        }
        Icon::Key => {
            p.circle_stroke(c + vec2(-u * 0.45, 0.0), u * 0.4, s);
            ln(p, c + vec2(-u * 0.05, 0.0), c + vec2(u, 0.0), s);
            ln(p, c + vec2(u * 0.7, 0.0), c + vec2(u * 0.7, u * 0.35), s);
            ln(p, c + vec2(u * 0.45, 0.0), c + vec2(u * 0.45, u * 0.3), s);
        }
    }
    let _ = FRAC_PI_2;
}
