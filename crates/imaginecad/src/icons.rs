// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Proteen Das

//! Vector icons drawn with the egui painter (no bitmap assets).
//!
//! One visual language for the whole set, designed on a 24-unit grid:
//! neutral line work for existing geometry, **blue** for the object a tool
//! creates or changes, **red** only for what a tool removes, **violet** only
//! for AI, small square grips at picked points. Strokes scale with the icon
//! and straight lines are snapped to the pixel grid so icons stay crisp.

use eframe::egui::{pos2, vec2, Align2, Color32, FontId, Painter, Pos2, Rect, Shape, Stroke};
use std::f32::consts::{PI, TAU};

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
    Tolerance,
    Datum,
    Surface,
    Weld,
    Lock,
    Unlock,
    Table,
    Check,
    Region,
    Link,
    Unlink,
    Block,
    Insert,
    Area,
    List,
    QSelect,
    Image,
    Isolate,
    Key,
    RevCloud,
    Balloon,
    Bom,
    Lineweight,
    SheetSet,
}

/// Every icon, for the icon sheet (`IMAGINECAD_ICON_SHEET=1`).
pub const ALL: &[Icon] = &[
    Icon::Line,
    Icon::Polyline,
    Icon::Circle,
    Icon::Arc,
    Icon::Rect,
    Icon::Polygon,
    Icon::Text,
    Icon::Move,
    Icon::Copy,
    Icon::Rotate,
    Icon::Mirror,
    Icon::Scale,
    Icon::Offset,
    Icon::Trim,
    Icon::Extend,
    Icon::Fillet,
    Icon::Chamfer,
    Icon::Erase,
    Icon::Explode,
    Icon::Array,
    Icon::Match,
    Icon::DimLinear,
    Icon::DimAligned,
    Icon::DimRadius,
    Icon::DimDiameter,
    Icon::CenterMark,
    Icon::Layers,
    Icon::Measure,
    Icon::SelectAll,
    Icon::ImageToCad,
    Icon::AiSettings,
    Icon::Chat,
    Icon::Pdf,
    Icon::Dxf,
    Icon::Dwg,
    Icon::Svg,
    Icon::ZoomExtents,
    Icon::ZoomWindow,
    Icon::Pan,
    Icon::Grid,
    Icon::TitleBlock,
    Icon::PageSetup,
    Icon::New,
    Icon::Open,
    Icon::Save,
    Icon::Undo,
    Icon::Redo,
    Icon::Paste,
    Icon::DimStyle,
    Icon::Properties,
    Icon::Help,
    Icon::Ellipse,
    Icon::Spline,
    Icon::Hatch,
    Icon::Point,
    Icon::XLine,
    Icon::Stretch,
    Icon::Break,
    Icon::Join,
    Icon::Lengthen,
    Icon::PolarArray,
    Icon::Leader,
    Icon::MText,
    Icon::DimAngular,
    Icon::DimBaseline,
    Icon::DimContinue,
    Icon::DimOrdinate,
    Icon::DimArc,
    Icon::QDim,
    Icon::Tolerance,
    Icon::Datum,
    Icon::Surface,
    Icon::Weld,
    Icon::Lock,
    Icon::Unlock,
    Icon::Table,
    Icon::Check,
    Icon::Region,
    Icon::Link,
    Icon::Unlink,
    Icon::Block,
    Icon::Insert,
    Icon::Area,
    Icon::List,
    Icon::QSelect,
    Icon::Image,
    Icon::Isolate,
    Icon::Key,
    Icon::RevCloud,
    Icon::Balloon,
    Icon::Bom,
    Icon::Lineweight,
    Icon::SheetSet,
];

/// Drawing on the icon's 24-unit grid.
struct Pen<'a> {
    p: &'a Painter,
    r: Rect,
    ppp: f32,
}

impl Pen<'_> {
    fn k(&self) -> f32 {
        self.r.width() / 24.0
    }
    /// Grid point → screen, snapped to pixel centres for crisp 1-px lines.
    fn at(&self, x: f32, y: f32) -> Pos2 {
        let q = self.r.min + vec2(x * self.k(), y * self.k());
        let snap = |v: f32| ((v * self.ppp).floor() + 0.5) / self.ppp;
        pos2(snap(q.x), snap(q.y))
    }
    /// Unsnapped grid point (for curves).
    fn raw(&self, x: f32, y: f32) -> Pos2 {
        self.r.min + vec2(x * self.k(), y * self.k())
    }
    fn st(&self, w: f32, c: Color32) -> Stroke {
        Stroke::new(w, c)
    }
    fn line(&self, a: (f32, f32), b: (f32, f32), w: f32, c: Color32) {
        self.p.line_segment([self.at(a.0, a.1), self.at(b.0, b.1)], self.st(w, c));
    }
    fn dash(&self, a: (f32, f32), b: (f32, f32), w: f32, c: Color32) {
        let k = self.k();
        self.p.extend(Shape::dashed_line(&[self.at(a.0, a.1), self.at(b.0, b.1)], self.st(w, c), 2.2 * k, 1.6 * k));
    }
    fn poly(&self, pts: &[(f32, f32)], closed: bool, w: f32, c: Color32) {
        let v: Vec<Pos2> = pts.iter().map(|q| self.at(q.0, q.1)).collect();
        if closed {
            self.p.add(Shape::closed_line(v, self.st(w, c)));
        } else {
            self.p.add(Shape::line(v, self.st(w, c)));
        }
    }
    fn fill(&self, pts: &[(f32, f32)], c: Color32) {
        let v: Vec<Pos2> = pts.iter().map(|q| self.raw(q.0, q.1)).collect();
        self.p.add(Shape::convex_polygon(v, c, Stroke::NONE));
    }
    fn rect(&self, a: (f32, f32), b: (f32, f32), w: f32, c: Color32) {
        self.poly(&[a, (b.0, a.1), b, (a.0, b.1)], true, w, c);
    }
    fn rect_fill(&self, a: (f32, f32), b: (f32, f32), rad: f32, c: Color32) {
        self.p.rect_filled(Rect::from_two_pos(self.raw(a.0, a.1), self.raw(b.0, b.1)), rad * self.k(), c);
    }
    fn round_rect(&self, a: (f32, f32), b: (f32, f32), rad: f32, w: f32, c: Color32) {
        self.p.rect_stroke(
            Rect::from_two_pos(self.at(a.0, a.1), self.at(b.0, b.1)),
            rad * self.k(),
            self.st(w, c),
            eframe::egui::StrokeKind::Middle,
        );
    }
    fn circle(&self, cx: f32, cy: f32, r: f32, w: f32, c: Color32) {
        self.p.circle_stroke(self.raw(cx, cy), r * self.k(), self.st(w, c));
    }
    fn disc(&self, cx: f32, cy: f32, r: f32, c: Color32) {
        self.p.circle_filled(self.raw(cx, cy), r * self.k(), c);
    }
    /// Arc, angles in degrees counter-clockwise from +x (y up on screen).
    fn arc_pts(&self, cx: f32, cy: f32, r: f32, a0: f32, a1: f32) -> Vec<Pos2> {
        let n = ((a1 - a0).abs() / 6.0).ceil().max(4.0) as usize;
        (0..=n)
            .map(|i| {
                let a = (a0 + (a1 - a0) * i as f32 / n as f32).to_radians();
                self.raw(cx + r * a.cos(), cy - r * a.sin())
            })
            .collect()
    }
    #[allow(clippy::too_many_arguments)]
    fn arc(&self, cx: f32, cy: f32, r: f32, a0: f32, a1: f32, w: f32, c: Color32) {
        self.p.add(Shape::line(self.arc_pts(cx, cy, r, a0, a1), self.st(w, c)));
    }
    /// Arc with arrowheads along its own tangent at the start and/or end.
    #[allow(clippy::too_many_arguments)]
    fn arc_arrow(&self, cx: f32, cy: f32, r: f32, a0: f32, a1: f32, heads: (bool, bool), w: f32, c: Color32, size: f32) {
        // Leave room for the heads so the line does not poke through them.
        let back = (size * 0.8 / r).to_degrees() * (a1 - a0).signum();
        let (s0, s1) = (if heads.0 { a0 + back } else { a0 }, if heads.1 { a1 - back } else { a1 });
        self.arc(cx, cy, r, s0, s1, w, c);
        let pt = |a: f32| (cx + r * a.to_radians().cos(), cy - r * a.to_radians().sin());
        if heads.1 {
            self.head(pt(a1), pt(a1 - back * 1.2), size, c);
        }
        if heads.0 {
            self.head(pt(a0), pt(a0 + back * 1.2), size, c);
        }
    }
    /// Filled arrowhead at `tip` pointing away from `from`.
    fn head(&self, tip: (f32, f32), from: (f32, f32), size: f32, c: Color32) {
        let (t, f) = (self.raw(tip.0, tip.1), self.raw(from.0, from.1));
        let d = (t - f).normalized();
        let n = vec2(-d.y, d.x);
        let s = size * self.k();
        let base = t - d * s;
        self.p.add(Shape::convex_polygon(vec![t, base + n * s * 0.42, base - n * s * 0.42], c, Stroke::NONE));
    }
    /// Line ending in an arrowhead.
    fn arrow(&self, a: (f32, f32), b: (f32, f32), w: f32, c: Color32) {
        let d = ((b.0 - a.0).powi(2) + (b.1 - a.1).powi(2)).sqrt().max(1e-3);
        let back = (b.0 - (b.0 - a.0) / d * 2.5, b.1 - (b.1 - a.1) / d * 2.5);
        self.line(a, back, w, c);
        self.head(b, a, 4.2, c);
    }
    /// Square grip at a picked point.
    fn grip(&self, x: f32, y: f32) {
        let q = self.raw(x, y);
        let h = (1.25 * self.k()).max(1.5);
        self.p.rect_filled(Rect::from_center_size(q, vec2(2.0 * h, 2.0 * h)), 0.0, ACC);
    }
    fn text(&self, x: f32, y: f32, s: &str, size: f32, c: Color32) {
        self.p.text(self.raw(x, y), Align2::CENTER_CENTER, s, FontId::proportional(size * self.k()), c);
    }
}

const ACC: Color32 = crate::theme::ACCENT;
const RED: Color32 = Color32::from_rgb(240, 86, 76);
const AI: Color32 = crate::theme::AI_ACCENT;
const AMBER: Color32 = Color32::from_rgb(242, 178, 52);
const GREEN: Color32 = Color32::from_rgb(74, 190, 120);

/// Draw `icon` inside `r` with `col` as the ink colour.
pub fn draw(p: &Painter, r: Rect, icon: Icon, col: Color32) {
    let ppp = p.ctx().pixels_per_point();
    let k = r.width() / 24.0;
    // 1.5 units at 24 px; never thinner than one physical pixel.
    let w = (1.5 * k).max(1.0);
    let t = (1.0 * k).max(1.0 / ppp).max(0.8);
    let g = Pen { p, r, ppp };
    let ink = col;
    let dim = col.gamma_multiply(0.45);
    let tint = ACC.gamma_multiply(0.22);
    match icon {
        // ---------------------------------------------------------- draw
        Icon::Line => {
            g.line((4.0, 20.0), (20.0, 4.0), w, ACC);
            g.grip(4.0, 20.0);
            g.grip(20.0, 4.0);
        }
        Icon::Polyline => {
            let pts = [(3.0, 19.0), (8.5, 8.0), (15.0, 15.5), (21.0, 5.0)];
            g.poly(&pts, false, w, ACC);
            for q in pts {
                g.grip(q.0, q.1);
            }
        }
        Icon::Circle => {
            g.circle(12.0, 12.0, 8.5, w, ACC);
            g.line((9.5, 12.0), (14.5, 12.0), t, dim);
            g.line((12.0, 9.5), (12.0, 14.5), t, dim);
        }
        Icon::Arc => {
            g.arc(12.0, 17.0, 9.0, 0.0, 180.0, w, ACC);
            g.grip(3.0, 17.0);
            g.grip(12.0, 8.0);
            g.grip(21.0, 17.0);
        }
        Icon::Rect => {
            g.rect((4.0, 6.5), (20.0, 17.5), w, ACC);
            g.grip(4.0, 17.5);
            g.grip(20.0, 6.5);
        }
        Icon::Polygon => {
            let pts: Vec<(f32, f32)> = (0..6)
                .map(|i| (12.0 + 9.0 * (i as f32 * 60.0f32).to_radians().cos(), 12.0 + 9.0 * (i as f32 * 60.0f32).to_radians().sin()))
                .collect();
            g.poly(&pts, true, w, ACC);
            g.disc(12.0, 12.0, 1.0, dim);
        }
        Icon::Ellipse => {
            let pts: Vec<Pos2> = (0..=48)
                .map(|i| {
                    let a = i as f32 / 48.0 * TAU;
                    g.raw(12.0 + 9.5 * a.cos(), 12.0 + 5.8 * a.sin())
                })
                .collect();
            p.add(Shape::closed_line(pts, Stroke::new(w, ACC)));
            g.dash((2.5, 12.0), (21.5, 12.0), t, dim);
            g.dash((12.0, 6.2), (12.0, 17.8), t, dim);
        }
        Icon::Spline => {
            let ctrl = [(3.0, 18.0), (8.0, 5.0), (16.0, 19.0), (21.0, 6.0)];
            g.poly(&ctrl, false, t, dim);
            let pts: Vec<Pos2> = (0..=32)
                .map(|i| {
                    let u = i as f32 / 32.0;
                    let b = |a: f32, b1: f32, c1: f32, d: f32| {
                        (1.0 - u).powi(3) * a + 3.0 * (1.0 - u).powi(2) * u * b1 + 3.0 * (1.0 - u) * u * u * c1 + u.powi(3) * d
                    };
                    g.raw(b(ctrl[0].0, ctrl[1].0, ctrl[2].0, ctrl[3].0), b(ctrl[0].1, ctrl[1].1, ctrl[2].1, ctrl[3].1))
                })
                .collect();
            p.add(Shape::line(pts, Stroke::new(w, ACC)));
            for q in ctrl {
                g.grip(q.0, q.1);
            }
        }
        Icon::Point => {
            g.circle(12.0, 12.0, 4.5, w, ACC);
            g.line((5.0, 12.0), (19.0, 12.0), t, ink);
            g.line((12.0, 5.0), (12.0, 19.0), t, ink);
        }
        Icon::XLine => {
            g.line((2.0, 19.0), (22.0, 5.0), w, ACC);
            g.head((22.0, 5.0), (2.0, 19.0), 4.0, ACC);
            g.head((2.0, 19.0), (22.0, 5.0), 4.0, ACC);
            g.grip(12.0, 12.0);
        }
        Icon::Hatch => {
            g.rect((4.0, 4.0), (20.0, 20.0), w, ink);
            for i in 1..6 {
                let d = 16.0 * i as f32 / 6.0 * 2.0;
                // 45° lines clipped to the square.
                let (a, b) = if d <= 16.0 { ((4.0, 4.0 + d), (4.0 + d, 4.0)) } else { ((4.0 + d - 16.0, 20.0), (20.0, 4.0 + d - 16.0)) };
                g.line(a, b, t, ACC);
            }
        }
        Icon::Region => {
            g.fill(&[(4.0, 6.0), (20.0, 6.0), (20.0, 18.0), (4.0, 18.0)], tint);
            g.rect((4.0, 6.0), (20.0, 18.0), w, ACC);
            g.disc(12.0, 12.0, 3.4, crate::theme::RIBBON);
            g.circle(12.0, 12.0, 3.4, t, ink);
        }
        Icon::Text => {
            g.text(11.0, 12.5, "A", 19.0, ink);
            g.line((19.0, 5.0), (19.0, 20.0), w, ACC);
        }
        Icon::MText => {
            g.line((4.0, 6.0), (20.0, 6.0), w, ACC);
            for (i, len) in [16.0, 16.0, 10.0].iter().enumerate() {
                let y = 11.0 + i as f32 * 4.5;
                g.line((4.0, y), (4.0 + len, y), t, ink);
            }
        }
        Icon::RevCloud => {
            // Scalloped outline round a rectangle.
            let (x0, y0, x1, y1) = (4.0, 6.0, 20.0, 18.0);
            let n = 4;
            for i in 0..n {
                let x = x0 + (x1 - x0) * (i as f32 + 0.5) / n as f32;
                g.arc(x, y0, 2.0, 0.0, 180.0, w, ACC);
                g.arc(x, y1, 2.0, 180.0, 360.0, w, ACC);
            }
            for i in 0..3 {
                let y = y0 + (y1 - y0) * (i as f32 + 0.5) / 3.0;
                g.arc(x0, y, 2.0, 90.0, 270.0, w, ACC);
                g.arc(x1, y, 2.0, -90.0, 90.0, w, ACC);
            }
            g.line((8.0, 11.0), (16.0, 11.0), t, dim);
            g.line((8.0, 14.0), (13.0, 14.0), t, dim);
        }
        // ---------------------------------------------------------- modify
        Icon::Move => {
            g.rect((3.0, 13.0), (10.0, 20.0), t, dim);
            g.rect((14.0, 4.0), (21.0, 11.0), w, ACC);
            g.arrow((8.0, 15.0), (15.5, 8.5), w, ink);
        }
        Icon::Copy => {
            g.rect((3.0, 10.0), (13.0, 20.0), w, ink);
            g.rect_fill((11.0, 4.0), (21.0, 14.0), 0.0, crate::theme::RIBBON);
            g.rect((11.0, 4.0), (21.0, 14.0), w, ACC);
        }
        Icon::Rotate => {
            // The square turned about its corner, with the turning arrow.
            g.rect((4.0, 11.0), (13.0, 20.0), t, dim);
            let a = 30.0f32.to_radians();
            let rot =
                |x: f32, y: f32| (4.0 + (x - 4.0) * a.cos() + (y - 20.0) * a.sin(), 20.0 - (x - 4.0) * a.sin() + (y - 20.0) * a.cos());
            g.poly(&[rot(4.0, 11.0), rot(13.0, 11.0), rot(13.0, 20.0), rot(4.0, 20.0)], true, w, ACC);
            g.arc_arrow(4.0, 20.0, 16.0, 8.0, 52.0, (false, true), t, ink, 4.0);
            g.grip(4.0, 20.0);
        }
        Icon::Mirror => {
            g.dash((12.0, 2.5), (12.0, 21.5), t, dim);
            g.poly(&[(10.0, 19.0), (3.0, 19.0), (10.0, 6.0)], true, w, ink);
            g.poly(&[(14.0, 19.0), (21.0, 19.0), (14.0, 6.0)], true, w, ACC);
        }
        Icon::Scale => {
            g.rect((4.0, 4.0), (20.0, 20.0), w, ACC);
            g.rect_fill((4.0, 13.0), (11.0, 20.0), 0.0, ink.gamma_multiply(0.25));
            g.rect((4.0, 13.0), (11.0, 20.0), t, ink);
            g.arrow((11.0, 13.0), (17.5, 6.5), w, ink);
        }
        Icon::Offset => {
            g.arc(12.0, 22.0, 9.0, 20.0, 160.0, w, ink);
            g.arc(12.0, 22.0, 15.0, 25.0, 155.0, w, ACC);
        }
        Icon::Trim => {
            g.line((13.0, 3.0), (13.0, 21.0), w, ink);
            g.line((3.0, 12.0), (13.0, 12.0), w, ink);
            g.dash((13.0, 12.0), (21.0, 12.0), w, RED);
            g.line((16.0, 15.0), (20.0, 19.0), t, RED);
            g.line((20.0, 15.0), (16.0, 19.0), t, RED);
        }
        Icon::Extend => {
            g.line((20.0, 3.0), (20.0, 21.0), w, ink);
            g.line((3.0, 12.0), (10.0, 12.0), w, ink);
            g.arrow((10.0, 12.0), (19.5, 12.0), w, ACC);
        }
        Icon::Fillet => {
            g.line((5.0, 3.0), (5.0, 11.0), w, ink);
            g.line((13.0, 19.0), (21.0, 19.0), w, ink);
            g.dash((5.0, 11.0), (5.0, 19.0), t, dim);
            g.dash((5.0, 19.0), (13.0, 19.0), t, dim);
            g.arc(13.0, 11.0, 8.0, 180.0, 270.0, w, ACC);
        }
        Icon::Chamfer => {
            g.line((5.0, 3.0), (5.0, 12.0), w, ink);
            g.line((12.0, 19.0), (21.0, 19.0), w, ink);
            g.dash((5.0, 12.0), (5.0, 19.0), t, dim);
            g.dash((5.0, 19.0), (12.0, 19.0), t, dim);
            g.line((5.0, 12.0), (12.0, 19.0), w, ACC);
        }
        Icon::Erase => {
            // Eraser block, tilted, its working end red.
            let q = [(9.0, 21.0), (3.0, 15.0), (13.0, 5.0), (19.0, 11.0)];
            g.fill(&[(9.0, 21.0), (3.0, 15.0), (8.0, 10.0), (14.0, 16.0)], RED);
            g.poly(&q, true, w, ink);
            g.line((8.0, 10.0), (14.0, 16.0), t, ink);
            g.line((9.0, 21.0), (21.0, 21.0), t, dim);
        }
        Icon::Explode => {
            g.poly(&[(4.0, 10.0), (4.0, 4.0), (10.0, 4.0)], false, w, ACC);
            g.poly(&[(14.0, 4.0), (20.0, 4.0), (20.0, 10.0)], false, w, ACC);
            g.poly(&[(20.0, 14.0), (20.0, 20.0), (14.0, 20.0)], false, w, ACC);
            g.poly(&[(10.0, 20.0), (4.0, 20.0), (4.0, 14.0)], false, w, ACC);
            for (dx, dy) in [(-1.0, -1.0), (1.0, -1.0), (1.0, 1.0), (-1.0, 1.0)] {
                g.line((12.0 + dx * 1.5, 12.0 + dy * 1.5), (12.0 + dx * 4.0, 12.0 + dy * 4.0), t, ink);
            }
        }
        Icon::Array => {
            for i in 0..3 {
                for j in 0..3 {
                    let (x, y) = (3.0 + i as f32 * 7.0, 3.0 + j as f32 * 7.0);
                    let c = if i == 0 && j == 2 { ink } else { ACC };
                    g.rect((x, y), (x + 4.5, y + 4.5), t, c);
                }
            }
        }
        Icon::PolarArray => {
            g.circle(12.0, 12.0, 7.5, t, dim);
            for i in 0..6 {
                let a = (90.0 + i as f32 * 60.0f32).to_radians();
                let (x, y) = (12.0 + 7.5 * a.cos(), 12.0 - 7.5 * a.sin());
                g.disc(x, y, 2.0, crate::theme::RIBBON);
                g.circle(x, y, 2.0, w, if i == 0 { ink } else { ACC });
            }
            g.disc(12.0, 12.0, 1.2, ink);
        }
        Icon::Stretch => {
            g.poly(&[(3.0, 19.0), (3.0, 9.0), (11.0, 9.0)], false, w, ink);
            g.poly(&[(11.0, 9.0), (17.0, 9.0), (17.0, 19.0), (3.0, 19.0)], false, w, ACC);
            g.dash((9.0, 6.0), (21.0, 6.0), t, ACC);
            g.dash((21.0, 6.0), (21.0, 21.0), t, ACC);
            g.dash((9.0, 21.0), (21.0, 21.0), t, ACC);
            g.dash((9.0, 6.0), (9.0, 21.0), t, ACC);
            g.arrow((13.0, 14.0), (19.5, 14.0), t, ink);
        }
        Icon::Break => {
            g.line((3.0, 15.0), (9.5, 11.0), w, ink);
            g.line((14.5, 13.0), (21.0, 9.0), w, ink);
            g.line((9.5, 11.0), (14.5, 13.0), t, RED);
            g.grip(9.5, 11.0);
            g.grip(14.5, 13.0);
        }
        Icon::Join => {
            g.line((3.0, 16.0), (10.0, 12.0), w, ink);
            g.line((14.0, 12.0), (21.0, 8.0), w, ink);
            g.line((10.0, 12.0), (14.0, 12.0), w, ACC);
            g.disc(12.0, 12.0, 1.8, ACC);
        }
        Icon::Lengthen => {
            g.line((3.0, 12.0), (12.0, 12.0), w, ink);
            g.arrow((12.0, 12.0), (21.0, 12.0), w, ACC);
            g.line((12.0, 9.0), (12.0, 15.0), t, dim);
        }
        Icon::Match => {
            // Brush: handle, ferrule and blue bristles.
            g.line((20.0, 3.0), (13.0, 10.0), w * 1.5, ink);
            g.poly(&[(12.0, 9.0), (15.0, 12.0), (12.5, 14.5), (9.5, 11.5)], true, t, ink);
            g.fill(&[(9.5, 11.5), (12.5, 14.5), (7.0, 21.0), (3.0, 17.0)], ACC);
        }
        // ---------------------------------------------------------- annotate
        Icon::DimLinear => {
            g.line((4.0, 20.0), (20.0, 20.0), w, ink);
            g.line((4.0, 18.5), (4.0, 7.0), t, ACC);
            g.line((20.0, 18.5), (20.0, 7.0), t, ACC);
            g.line((6.0, 9.0), (18.0, 9.0), t, ACC);
            g.head((4.3, 9.0), (10.0, 9.0), 4.0, ACC);
            g.head((19.7, 9.0), (14.0, 9.0), 4.0, ACC);
            g.line((9.5, 5.0), (14.5, 5.0), w, ink);
        }
        Icon::DimAligned => {
            // Slanted edge and its parallel dimension, offset along the normal.
            g.line((3.0, 14.0), (14.0, 3.0), w, ink);
            g.line((4.5, 15.5), (9.5, 20.5), t, ACC);
            g.line((15.5, 4.5), (20.5, 9.5), t, ACC);
            g.line((10.5, 17.5), (17.5, 10.5), t, ACC);
            g.head((8.5, 19.5), (12.0, 16.0), 4.0, ACC);
            g.head((19.5, 8.5), (16.0, 12.0), 4.0, ACC);
        }
        Icon::DimAngular => {
            g.line((4.0, 20.0), (21.0, 20.0), w, ink);
            g.line((4.0, 20.0), (15.0, 5.5), w, ink);
            g.arc_arrow(4.0, 20.0, 12.0, 2.0, 50.0, (true, true), t, ACC, 3.6);
        }
        Icon::DimRadius => {
            g.circle(12.0, 12.0, 8.5, w, ink);
            g.disc(12.0, 12.0, 1.1, ink);
            g.arrow((12.0, 12.0), (18.0, 6.0), t, ACC);
        }
        Icon::DimDiameter => {
            g.circle(12.0, 12.0, 8.5, w, ink);
            g.line((8.0, 16.0), (16.0, 8.0), t, ACC);
            g.head((18.0, 6.0), (12.0, 12.0), 4.0, ACC);
            g.head((6.0, 18.0), (12.0, 12.0), 4.0, ACC);
        }
        Icon::DimOrdinate => {
            g.poly(&[(4.0, 3.0), (4.0, 20.0), (21.0, 20.0)], false, w, ink);
            for x in [9.0, 14.0, 19.0] {
                g.line((x, 18.0), (x, 8.0), t, ACC);
                g.line((x - 1.5, 6.0), (x + 1.5, 6.0), w, ACC);
            }
            g.grip(4.0, 20.0);
        }
        Icon::DimArc => {
            g.arc(12.0, 21.0, 9.0, 25.0, 155.0, w, ink);
            g.arc(12.0, 21.0, 14.0, 25.0, 155.0, t, ACC);
            g.arc(12.0, 6.0, 2.5, 20.0, 160.0, t, ACC);
        }
        Icon::DimBaseline => {
            g.line((3.0, 20.0), (21.0, 20.0), w, ink);
            for x in [3.0, 12.0, 21.0] {
                g.line((x, 18.5), (x, 4.0), t, dim);
            }
            g.line((4.5, 13.0), (10.5, 13.0), t, ACC);
            g.head((3.3, 13.0), (8.0, 13.0), 3.4, ACC);
            g.head((11.7, 13.0), (7.0, 13.0), 3.4, ACC);
            g.line((4.5, 7.0), (19.5, 7.0), t, ACC);
            g.head((3.3, 7.0), (8.0, 7.0), 3.4, ACC);
            g.head((20.7, 7.0), (16.0, 7.0), 3.4, ACC);
        }
        Icon::DimContinue => {
            g.line((3.0, 20.0), (21.0, 20.0), w, ink);
            for x in [3.0, 12.0, 21.0] {
                g.line((x, 18.5), (x, 7.0), t, dim);
            }
            for (a, b) in [(3.0, 12.0), (12.0, 21.0)] {
                g.line((a + 1.5, 10.0), (b - 1.5, 10.0), t, ACC);
                g.head((a + 0.3, 10.0), (a + 5.0, 10.0), 3.4, ACC);
                g.head((b - 0.3, 10.0), (b - 5.0, 10.0), 3.4, ACC);
            }
        }
        Icon::QDim => {
            g.line((3.0, 20.0), (21.0, 20.0), w, ink);
            for x in [3.0, 10.0, 21.0] {
                g.line((x, 18.5), (x, 11.0), t, dim);
            }
            g.line((3.0, 13.0), (21.0, 13.0), t, ACC);
            g.fill(&[(19.0, 1.5), (14.5, 7.0), (17.0, 7.0)], AMBER);
            g.fill(&[(17.0, 7.0), (14.5, 7.0), (15.5, 10.5), (19.5, 5.0)], AMBER);
        }
        Icon::DimStyle => {
            g.line((3.0, 9.0), (21.0, 9.0), t, ACC);
            g.head((3.0, 9.0), (8.0, 9.0), 4.0, ACC);
            g.head((21.0, 9.0), (16.0, 9.0), 4.0, ACC);
            g.line((3.0, 5.0), (3.0, 13.0), t, dim);
            g.line((21.0, 5.0), (21.0, 13.0), t, dim);
            // Style: a small gear.
            gear(&g, 12.0, 17.5, 4.5, ink, w);
        }
        Icon::CenterMark => {
            g.circle(12.0, 12.0, 7.0, w, ink);
            g.line((2.5, 12.0), (8.0, 12.0), t, ACC);
            g.line((16.0, 12.0), (21.5, 12.0), t, ACC);
            g.line((12.0, 2.5), (12.0, 8.0), t, ACC);
            g.line((12.0, 16.0), (12.0, 21.5), t, ACC);
            g.line((10.5, 12.0), (13.5, 12.0), t, ACC);
            g.line((12.0, 10.5), (12.0, 13.5), t, ACC);
        }
        Icon::Leader => {
            g.line((4.0, 20.0), (11.0, 9.0), t, ACC);
            g.line((11.0, 9.0), (14.0, 9.0), t, ACC);
            g.head((3.5, 20.8), (7.0, 15.0), 4.6, ACC);
            g.line((15.5, 7.0), (21.0, 7.0), w, ink);
            g.line((15.5, 11.0), (19.0, 11.0), w, ink);
        }
        Icon::Balloon => {
            g.circle(15.0, 9.0, 6.0, w, ACC);
            g.text(15.0, 9.2, "1", 8.5, ink);
            g.line((10.8, 13.2), (5.0, 19.0), t, ACC);
            g.disc(4.5, 19.5, 1.5, ACC);
        }
        Icon::Tolerance => {
            g.rect((2.5, 7.0), (21.5, 17.0), t, ink);
            g.line((9.5, 7.0), (9.5, 17.0), t, ink);
            g.line((16.0, 7.0), (16.0, 17.0), t, ink);
            g.circle(6.0, 12.0, 2.2, t, ACC);
            g.line((3.8, 12.0), (8.2, 12.0), t, ACC);
            g.line((6.0, 9.8), (6.0, 14.2), t, ACC);
            g.line((11.0, 12.0), (14.5, 12.0), w, ink);
            g.text(18.8, 12.2, "A", 6.5, ink);
        }
        Icon::Datum => {
            g.rect((7.0, 2.5), (17.0, 12.0), t, ink);
            g.text(12.0, 7.4, "A", 7.5, ink);
            g.line((12.0, 12.0), (12.0, 16.0), t, ink);
            g.fill(&[(8.5, 20.5), (15.5, 20.5), (12.0, 15.5)], ACC);
            g.line((3.0, 20.5), (21.0, 20.5), w, ink);
        }
        Icon::Surface => {
            g.line((3.0, 20.0), (21.0, 20.0), t, dim);
            g.poly(&[(4.0, 12.0), (8.5, 20.0), (17.0, 4.0), (22.0, 4.0)], false, w, ink);
            g.line((4.0, 12.0), (12.8, 12.0), w, ACC);
        }
        Icon::Weld => {
            g.arrow((10.0, 12.0), (3.5, 19.5), t, ink);
            g.line((10.0, 12.0), (21.0, 12.0), w, ink);
            g.fill(&[(13.0, 12.0), (13.0, 18.0), (19.0, 12.0)], ACC);
            g.line((21.0, 12.0), (23.0, 9.0), t, ink);
            g.line((21.0, 12.0), (23.0, 15.0), t, ink);
        }
        Icon::Table => {
            g.rect_fill((3.0, 4.0), (21.0, 8.5), 0.0, ACC);
            g.rect((3.0, 4.0), (21.0, 20.0), t, ink);
            for y in [8.5, 12.5, 16.5] {
                g.line((3.0, y), (21.0, y), t, ink);
            }
            for x in [9.0, 15.0] {
                g.line((x, 8.5), (x, 20.0), t, ink);
            }
        }
        Icon::Bom => {
            g.rect_fill((3.0, 4.0), (21.0, 8.0), 0.0, ACC);
            g.rect((3.0, 4.0), (21.0, 20.0), t, ink);
            for y in [8.0, 12.0, 16.0] {
                g.line((3.0, y), (21.0, y), t, ink);
            }
            g.line((7.0, 8.0), (7.0, 20.0), t, ink);
            for (i, y) in [10.0f32, 14.0, 18.0].iter().enumerate() {
                g.text(5.0, *y, &(i + 1).to_string(), 3.6, ACC);
                g.line((9.0, *y), (18.0 - i as f32 * 3.0, *y), t, dim);
            }
        }
        Icon::Check => {
            g.text(9.5, 8.5, "abc", 8.0, ink);
            g.poly(&[(6.0, 15.0), (10.0, 19.5), (19.0, 10.5)], false, w * 1.3, GREEN);
        }
        Icon::Properties => {
            g.round_rect((3.0, 3.0), (21.0, 21.0), 1.5, t, ink);
            for i in 0..3 {
                let y = 8.0 + i as f32 * 4.5;
                g.line((6.0, y), (10.5, y), t, dim);
                g.rect_fill((12.5, y - 1.2), (18.0, y + 1.2), 0.4, if i == 0 { ACC } else { ink.gamma_multiply(0.7) });
            }
        }
        Icon::TitleBlock => {
            g.rect((2.5, 4.0), (21.5, 20.0), t, ink);
            g.rect_fill((12.5, 14.0), (21.5, 20.0), 0.0, ACC);
            g.line((12.5, 17.0), (21.5, 17.0), t, crate::theme::RIBBON);
            g.line((17.0, 17.0), (17.0, 20.0), t, crate::theme::RIBBON);
        }
        Icon::PageSetup => {
            g.rect((5.0, 3.0), (19.0, 21.0), t, ink);
            g.rect((7.5, 5.5), (16.5, 18.5), t, dim);
            g.arrow((12.0, 12.0), (12.0, 6.0), t, ACC);
            g.arrow((12.0, 12.0), (12.0, 18.0), t, ACC);
        }
        // ---------------------------------------------------------- layers, blocks
        Icon::Layers | Icon::Isolate => {
            let fills = if icon == Icon::Layers { [AMBER, GREEN, ACC] } else { [dim, ACC, dim] };
            for (i, f) in fills.iter().enumerate() {
                let y = 16.0 - i as f32 * 4.5;
                g.fill(&[(3.0, y), (12.0, y - 4.5), (21.0, y), (12.0, y + 4.5)], *f);
                g.poly(&[(3.0, y), (12.0, y - 4.5), (21.0, y), (12.0, y + 4.5)], true, (0.6 * k).max(0.6), crate::theme::RIBBON);
            }
        }
        Icon::Lock | Icon::Unlock => {
            let open = icon == Icon::Unlock;
            g.rect_fill((5.0, 11.0), (19.0, 21.0), 1.5, if open { ink.gamma_multiply(0.75) } else { AMBER });
            let cx = if open { 15.0 } else { 12.0 };
            let lift = if open { 2.5 } else { 0.0 };
            g.line((cx - 4.0, 11.0 - lift), (cx - 4.0, 8.0 - lift), w, ink);
            g.line((cx + 4.0, 11.0 - (if open { 0.0 } else { lift })), (cx + 4.0, 8.0 - lift), w, ink);
            g.arc(cx, 8.0 - lift, 4.0, 0.0, 180.0, w, ink);
            g.disc(12.0, 15.5, 1.4, crate::theme::RIBBON);
            g.line((12.0, 16.0), (12.0, 18.5), w, crate::theme::RIBBON);
        }
        Icon::Block => {
            g.round_rect((3.0, 3.0), (21.0, 21.0), 2.0, w, ink);
            g.circle(9.0, 10.0, 3.0, t, ACC);
            g.poly(&[(12.5, 17.5), (18.0, 17.5), (15.25, 12.0)], true, t, ACC);
            g.grip(3.0, 21.0);
        }
        Icon::Insert => {
            g.round_rect((8.0, 8.0), (21.0, 21.0), 2.0, w, ink);
            g.circle(14.5, 14.5, 2.5, t, ink);
            g.arrow((2.0, 2.0), (12.5, 12.5), w, ACC);
        }
        Icon::Link | Icon::Unlink => {
            g.round_rect((2.5, 9.0), (13.0, 15.0), 3.0, w, ink);
            g.round_rect((11.0, 9.0), (21.5, 15.0), 3.0, w, ACC);
            if icon == Icon::Unlink {
                g.rect_fill((10.0, 5.0), (14.0, 19.0), 0.0, crate::theme::RIBBON);
                g.line((9.0, 4.0), (15.0, 20.0), w, RED);
            }
        }
        // ---------------------------------------------------------- inquiry, select
        Icon::Measure => {
            g.round_rect((2.5, 8.0), (21.5, 16.0), 1.0, w, ink);
            for i in 1..9 {
                let x = 2.5 + 19.0 * i as f32 / 9.0;
                let h = if i % 3 == 0 { 4.0 } else { 2.2 };
                g.line((x, 8.0), (x, 8.0 + h), t, if i % 3 == 0 { ACC } else { ink });
            }
        }
        Icon::Area => {
            let pts = [(3.0, 19.0), (6.0, 6.0), (17.0, 4.0), (21.0, 15.0), (12.0, 20.0)];
            g.fill(&[(3.0, 19.0), (6.0, 6.0), (17.0, 4.0), (21.0, 15.0)], tint);
            g.fill(&[(3.0, 19.0), (21.0, 15.0), (12.0, 20.0)], tint);
            g.poly(&pts, true, w, ACC);
        }
        Icon::List => {
            for i in 0..4 {
                let y = 5.0 + i as f32 * 4.7;
                g.rect_fill((3.0, y - 1.2), (5.4, y + 1.2), 0.3, ACC);
                g.line((8.0, y), (21.0 - if i == 3 { 5.0 } else { 0.0 }, y), w, ink);
            }
        }
        Icon::QSelect => {
            g.dash((2.5, 2.5), (21.5, 2.5), t, ACC);
            g.dash((21.5, 2.5), (21.5, 21.5), t, ACC);
            g.dash((21.5, 21.5), (2.5, 21.5), t, ACC);
            g.dash((2.5, 21.5), (2.5, 2.5), t, ACC);
            // Filter funnel.
            g.poly(&[(6.0, 7.0), (18.0, 7.0), (13.5, 12.5), (13.5, 18.0), (10.5, 16.5), (10.5, 12.5)], true, w, ink);
        }
        Icon::SelectAll => {
            g.dash((2.5, 2.5), (21.5, 2.5), t, ACC);
            g.dash((21.5, 2.5), (21.5, 21.5), t, ACC);
            g.dash((21.5, 21.5), (2.5, 21.5), t, ACC);
            g.dash((2.5, 21.5), (2.5, 2.5), t, ACC);
            g.circle(9.0, 9.5, 3.5, w, ink);
            g.rect((11.5, 12.0), (18.5, 18.5), w, ink);
        }
        // ---------------------------------------------------------- view
        Icon::ZoomExtents | Icon::ZoomWindow => {
            g.circle(10.0, 10.0, 6.5, w, ink);
            g.line((14.8, 14.8), (21.0, 21.0), w * 1.8, ink);
            if icon == Icon::ZoomWindow {
                g.rect((7.0, 7.5), (13.0, 12.5), t, ACC);
            } else {
                for (a, b, c) in [
                    ((6.0, 9.0), (6.0, 6.0), (9.0, 6.0)),
                    ((11.0, 6.0), (14.0, 6.0), (14.0, 9.0)),
                    ((14.0, 11.0), (14.0, 14.0), (11.0, 14.0)),
                    ((9.0, 14.0), (6.0, 14.0), (6.0, 11.0)),
                ] {
                    g.poly(&[a, b, c], false, t, ACC);
                }
            }
        }
        Icon::Pan => {
            for (dx, dy) in [(1.0, 0.0), (-1.0, 0.0), (0.0, 1.0), (0.0, -1.0)] {
                g.arrow((12.0 + dx * 2.5, 12.0 + dy * 2.5), (12.0 + dx * 10.0, 12.0 + dy * 10.0), w, ink);
            }
            g.disc(12.0, 12.0, 1.6, ACC);
        }
        Icon::Lineweight => {
            for (i, wt) in [0.8f32, 1.6, 3.0].iter().enumerate() {
                let y = 6.0 + i as f32 * 6.0;
                g.line((3.0, y), (21.0, y), (wt * k).max(0.8), if i == 2 { ACC } else { ink });
            }
        }
        Icon::SheetSet => {
            // Stacked sheets, the front one with its title block.
            g.rect((7.0, 2.5), (21.0, 16.5), t, dim);
            g.rect((5.0, 4.5), (19.0, 18.5), t, ink.gamma_multiply(0.7));
            g.rect_fill((3.0, 6.5), (17.0, 21.0), 0.0, crate::theme::RIBBON);
            g.rect((3.0, 6.5), (17.0, 21.0), w, ink);
            g.rect_fill((10.0, 17.0), (17.0, 21.0), 0.0, ACC);
        }
        Icon::Grid => {
            for i in 0..5 {
                for j in 0..5 {
                    g.disc(
                        4.0 + i as f32 * 4.0,
                        4.0 + j as f32 * 4.0,
                        if (i + j) % 4 == 0 { 1.1 } else { 0.8 },
                        if i == 2 && j == 2 { ACC } else { ink },
                    );
                }
            }
        }
        // ---------------------------------------------------------- files
        Icon::New => {
            page(&g, ink, w);
            g.line((12.0, 11.0), (12.0, 19.0), w, ACC);
            g.line((8.0, 15.0), (16.0, 15.0), w, ACC);
        }
        Icon::Open => {
            g.poly(&[(2.5, 19.5), (2.5, 5.0), (8.5, 5.0), (10.5, 7.5), (19.0, 7.5), (19.0, 10.0)], false, w, ink);
            g.poly(&[(2.5, 19.5), (6.0, 10.5), (22.0, 10.5), (18.5, 19.5)], true, w, ACC);
        }
        Icon::Save => {
            g.poly(&[(3.0, 3.0), (17.5, 3.0), (21.0, 6.5), (21.0, 21.0), (3.0, 21.0)], true, w, ink);
            g.rect_fill((7.0, 3.0), (15.0, 8.5), 0.0, ACC);
            g.rect((6.0, 13.0), (18.0, 21.0), t, ink);
        }
        Icon::Undo | Icon::Redo => {
            // A curved arrow turning back to the left (undo) or on to the right (redo).
            if icon == Icon::Undo {
                g.arc_arrow(13.0, 15.0, 7.0, -30.0, 160.0, (false, true), w, ink, 5.0);
            } else {
                g.arc_arrow(11.0, 15.0, 7.0, 210.0, 20.0, (false, true), w, ink, 5.0);
            }
        }
        Icon::Paste => {
            g.round_rect((4.0, 5.0), (18.0, 21.0), 1.5, w, ink);
            g.rect_fill((8.0, 3.0), (14.0, 7.0), 1.0, ink);
            g.rect_fill((11.0, 10.0), (21.0, 22.0), 0.5, crate::theme::RIBBON);
            g.rect((11.0, 10.0), (21.0, 22.0), w, ACC);
        }
        Icon::Pdf | Icon::Dxf | Icon::Dwg | Icon::Svg => {
            page(&g, ink, t);
            let (label, bg) = match icon {
                Icon::Pdf => ("PDF", Color32::from_rgb(214, 64, 56)),
                Icon::Dxf => ("DXF", Color32::from_rgb(46, 132, 214)),
                Icon::Dwg => ("DWG", Color32::from_rgb(34, 150, 112)),
                _ => ("SVG", Color32::from_rgb(222, 140, 34)),
            };
            g.rect_fill((2.5, 12.0), (21.5, 19.0), 1.0, bg);
            g.text(12.0, 15.6, label, 6.2, Color32::WHITE);
        }
        Icon::Image => {
            g.round_rect((2.5, 4.5), (21.5, 19.5), 1.5, w, ink);
            g.fill(&[(4.0, 18.0), (10.0, 10.0), (15.0, 18.0)], ACC);
            g.fill(&[(11.5, 18.0), (15.5, 13.0), (20.0, 18.0)], ACC.gamma_multiply(0.6));
            g.disc(16.5, 8.5, 1.8, AMBER);
        }
        // ---------------------------------------------------------- AI and help
        Icon::ImageToCad => {
            // Photo → exact geometry, with a spark.
            g.round_rect((2.0, 3.0), (11.0, 11.0), 1.0, t, ink);
            g.fill(&[(3.0, 10.0), (6.0, 6.0), (9.0, 10.0)], ink.gamma_multiply(0.6));
            g.arrow((7.0, 13.0), (11.0, 17.0), t, ink);
            g.rect((12.0, 12.0), (21.5, 21.0), w, ACC);
            g.circle(16.75, 16.5, 2.0, t, ACC);
            spark(&g, 18.0, 5.5, 4.0);
        }
        Icon::AiSettings => {
            gear(&g, 11.0, 13.0, 7.5, ink, w);
            spark(&g, 19.0, 5.0, 3.5);
        }
        Icon::Chat => {
            g.round_rect((2.5, 4.0), (21.5, 16.5), 4.0, w, ink);
            g.poly(&[(7.0, 16.5), (5.0, 21.0), (11.5, 16.5)], false, w, ink);
            for x in [8.0, 12.0, 16.0] {
                g.disc(x, 10.25, 1.4, AI);
            }
        }
        Icon::Key => {
            g.circle(7.5, 12.0, 4.5, w, ink);
            g.disc(7.5, 12.0, 1.3, ink);
            g.line((12.0, 12.0), (21.5, 12.0), w, ink);
            g.line((18.5, 12.0), (18.5, 16.0), w, ink);
            g.line((15.5, 12.0), (15.5, 15.0), w, ink);
            spark(&g, 19.0, 5.0, 3.0);
        }
        Icon::Help => {
            g.circle(12.0, 12.0, 9.0, w, ink);
            g.text(12.0, 12.5, "?", 13.0, ACC);
        }
    }
    let _ = PI;
}

/// Document page with a folded corner.
fn page(g: &Pen, c: Color32, w: f32) {
    g.poly(&[(5.0, 2.5), (14.5, 2.5), (19.0, 7.0), (19.0, 21.5), (5.0, 21.5)], true, w, c);
    g.poly(&[(14.5, 2.5), (14.5, 7.0), (19.0, 7.0)], false, w * 0.7, c);
}

/// A gear of radius `r` (teeth included).
fn gear(g: &Pen, cx: f32, cy: f32, r: f32, c: Color32, w: f32) {
    let n = 8;
    let mut pts = vec![];
    for i in 0..n {
        let a = i as f32 * TAU / n as f32;
        let h = TAU / n as f32 / 4.0;
        for (ang, rad) in [(a - h * 1.2, r * 0.78), (a - h * 0.6, r), (a + h * 0.6, r), (a + h * 1.2, r * 0.78)] {
            pts.push((cx + rad * ang.cos(), cy + rad * ang.sin()));
        }
    }
    g.poly(&pts, true, w, c);
    g.circle(cx, cy, r * 0.32, w, c);
}

/// Four-point AI spark.
fn spark(g: &Pen, cx: f32, cy: f32, r: f32) {
    let q = r * 0.28;
    g.fill(&[(cx, cy - r), (cx + q, cy - q), (cx + r, cy), (cx + q, cy + q)], AI);
    g.fill(&[(cx, cy - r), (cx + q, cy + q), (cx, cy + r), (cx - q, cy - q)], AI);
    g.fill(&[(cx - r, cy), (cx - q, cy - q), (cx, cy + r), (cx - q, cy + q)], AI);
    g.fill(&[(cx, cy - r), (cx - q, cy - q), (cx - r, cy), (cx - q, cy + q)], AI);
    g.fill(&[(cx, cy + r), (cx + q, cy + q), (cx + r, cy), (cx, cy)], AI);
}
