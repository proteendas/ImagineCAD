// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Proteen Das

//! World ↔ screen transform for the drawing canvas.

use eframe::egui::{pos2, Pos2, Rect};
use icad_core::geom::{BBox, V2};

#[derive(Clone, Debug)]
pub struct View {
    /// World point at the centre of the canvas.
    pub center: V2,
    /// Screen pixels per drawing unit.
    pub zoom: f64,
    pub rect: Rect,
}

impl Default for View {
    fn default() -> Self {
        Self { center: V2::new(200.0, 140.0), zoom: 2.0, rect: Rect::from_min_size(Pos2::ZERO, eframe::egui::vec2(800.0, 600.0)) }
    }
}

impl View {
    pub fn to_screen(&self, p: V2) -> Pos2 {
        let c = self.rect.center();
        pos2(c.x + ((p.x - self.center.x) * self.zoom) as f32, c.y - ((p.y - self.center.y) * self.zoom) as f32)
    }

    pub fn to_world(&self, s: Pos2) -> V2 {
        let c = self.rect.center();
        V2::new(self.center.x + (s.x - c.x) as f64 / self.zoom, self.center.y - (s.y - c.y) as f64 / self.zoom)
    }

    /// Zoom by `factor`, keeping the world point under `anchor` fixed.
    pub fn zoom_at(&mut self, anchor: Pos2, factor: f64) {
        let before = self.to_world(anchor);
        self.zoom = (self.zoom * factor).clamp(1e-6, 1e7);
        let after = self.to_world(anchor);
        self.center += before - after;
    }

    pub fn pan_pixels(&mut self, dx: f32, dy: f32) {
        self.center.x -= dx as f64 / self.zoom;
        self.center.y += dy as f64 / self.zoom;
    }

    pub fn fit(&mut self, b: &BBox) {
        if b.is_empty() {
            return;
        }
        let w = b.width().max(1e-6);
        let h = b.height().max(1e-6);
        let zx = self.rect.width() as f64 / (w * 1.08);
        let zy = self.rect.height() as f64 / (h * 1.08);
        self.zoom = zx.min(zy).clamp(1e-6, 1e7);
        self.center = b.center();
    }

    pub fn world_box(&self) -> BBox {
        BBox::from_pts(self.to_world(self.rect.left_bottom()), self.to_world(self.rect.right_top()))
    }

    /// World distance of `px` screen pixels.
    pub fn px(&self, px: f64) -> f64 {
        px / self.zoom
    }
}
