// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Proteen Das

//! Drawing sheet: paper size, plot scale, border and the ISO 7200-style title
//! block in the bottom-right corner.
//!
//! The sheet lives in model space: its lower-left corner sits at `origin` and it
//! spans `paper × scale` drawing units, so a 1:N plot maps it 1:1 onto paper.

use crate::geom::*;
use crate::prim::*;
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum PaperSize {
    A4,
    A3,
    A2,
    A1,
    A0,
    Letter,
    AnsiB,
    AnsiC,
    AnsiD,
}

impl PaperSize {
    pub const ALL: [PaperSize; 9] = [
        PaperSize::A4,
        PaperSize::A3,
        PaperSize::A2,
        PaperSize::A1,
        PaperSize::A0,
        PaperSize::Letter,
        PaperSize::AnsiB,
        PaperSize::AnsiC,
        PaperSize::AnsiD,
    ];

    /// Portrait width × height in millimetres.
    pub fn mm(self) -> (f64, f64) {
        match self {
            PaperSize::A4 => (210.0, 297.0),
            PaperSize::A3 => (297.0, 420.0),
            PaperSize::A2 => (420.0, 594.0),
            PaperSize::A1 => (594.0, 841.0),
            PaperSize::A0 => (841.0, 1189.0),
            PaperSize::Letter => (215.9, 279.4),
            PaperSize::AnsiB => (279.4, 431.8),
            PaperSize::AnsiC => (431.8, 558.8),
            PaperSize::AnsiD => (558.8, 863.6),
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            PaperSize::A4 => "A4",
            PaperSize::A3 => "A3",
            PaperSize::A2 => "A2",
            PaperSize::A1 => "A1",
            PaperSize::A0 => "A0",
            PaperSize::Letter => "Letter",
            PaperSize::AnsiB => "ANSI B",
            PaperSize::AnsiC => "ANSI C",
            PaperSize::AnsiD => "ANSI D",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Projection {
    FirstAngle,
    ThirdAngle,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct TitleBlock {
    pub company: String,
    pub title: String,
    pub drawing_no: String,
    pub revision: String,
    pub drawn_by: String,
    pub checked_by: String,
    pub approved_by: String,
    pub date: String,
    pub material: String,
    pub finish: String,
    pub sheet_no: String,
    pub units: String,
    pub tolerance: String,
    pub projection: Projection,
}

impl Default for TitleBlock {
    fn default() -> Self {
        Self {
            company: "YOUR COMPANY NAME".into(),
            title: "UNTITLED PART".into(),
            drawing_no: "DWG-0001".into(),
            revision: "A".into(),
            drawn_by: String::new(),
            checked_by: String::new(),
            approved_by: String::new(),
            date: String::new(),
            material: String::new(),
            finish: String::new(),
            sheet_no: "1 OF 1".into(),
            units: "mm".into(),
            tolerance: "LINEAR ±0.1  ANGULAR ±0.5°".into(),
            projection: Projection::FirstAngle,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Sheet {
    pub paper: PaperSize,
    pub landscape: bool,
    /// Drawing units per paper millimetre (1:N → N, 2:1 → 0.5).
    pub scale: f64,
    /// Model-space position of the sheet's lower-left corner.
    pub origin: V2,
    pub visible: bool,
    pub title: TitleBlock,
    /// Block drawn as the title block instead of the built-in one; its
    /// attributes are filled from the title fields by tag.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub title_block: Option<String>,
}

impl Default for Sheet {
    fn default() -> Self {
        Self {
            paper: PaperSize::A3,
            landscape: true,
            scale: 1.0,
            origin: V2::ZERO,
            visible: true,
            title: TitleBlock::default(),
            title_block: None,
        }
    }
}

/// Title block size in paper millimetres.
pub const TB_W: f64 = 180.0;
pub const TB_H: f64 = 60.0;
const MARGIN: f64 = 10.0;
const BIND: f64 = 20.0;

pub const STANDARD_SCALES: [f64; 15] = [0.1, 0.2, 0.5, 1.0, 2.0, 2.5, 5.0, 10.0, 20.0, 25.0, 50.0, 100.0, 200.0, 500.0, 1000.0];

pub fn scale_label(scale: f64) -> String {
    if scale >= 1.0 {
        format!("1:{}", trim_num(scale))
    } else {
        format!("{}:1", trim_num(1.0 / scale))
    }
}

fn trim_num(v: f64) -> String {
    let s = format!("{:.3}", v);
    s.trim_end_matches('0').trim_end_matches('.').to_string()
}

/// Parse "1:2", "2:1", "1/2" or a bare number into drawing-units-per-mm.
pub fn parse_scale(s: &str) -> Option<f64> {
    let s = s.trim();
    let parts: Vec<&str> = s.split([':', '/']).collect();
    let v = match parts.as_slice() {
        [a, b] => b.trim().parse::<f64>().ok()? / a.trim().parse::<f64>().ok()?,
        [a] => a.parse::<f64>().ok()?,
        _ => return None,
    };
    (v.is_finite() && v > 0.0).then_some(v)
}

/// `content` plus the margin left for dimensions when fitting a sheet to it.
pub fn fit_margin(content: &BBox) -> BBox {
    let (big, small) = (content.width().max(content.height()), content.width().min(content.height()));
    // Slender parts get a margin from both sides instead of one tied to their length.
    let m = if big > 4.0 * small { 0.06 * big + 0.25 * small } else { 0.12 * big };
    content.expand(m)
}

impl Sheet {
    /// Paper width × height in mm, honouring orientation.
    pub fn paper_mm(&self) -> (f64, f64) {
        let (w, h) = self.paper.mm();
        if self.landscape {
            (h, w)
        } else {
            (w, h)
        }
    }

    pub fn bounds(&self) -> BBox {
        let (w, h) = self.paper_mm();
        BBox::from_pts(self.origin, self.origin + V2::new(w, h) * self.scale)
    }

    /// Map paper millimetres to model coordinates.
    pub fn p2m(&self, x: f64, y: f64) -> V2 {
        self.origin + V2::new(x, y) * self.scale
    }

    /// Model coordinates → paper millimetres.
    pub fn m2p(&self, p: V2) -> V2 {
        (p - self.origin) / self.scale
    }

    /// Inner drawing frame in paper mm.
    pub fn frame_mm(&self) -> BBox {
        let (w, h) = self.paper_mm();
        BBox::from_pts(V2::new(BIND, MARGIN), V2::new(w - MARGIN, h - MARGIN))
    }

    /// Paper-mm area available for the drawing (frame minus the title block band).
    pub fn drawable_mm(&self) -> BBox {
        let f = self.frame_mm();
        BBox::from_pts(V2::new(f.min.x + 5.0, f.min.y + TB_H + 5.0), f.max - V2::new(5.0, 5.0))
    }

    pub fn scale_label(&self) -> String {
        scale_label(self.scale)
    }

    /// Choose paper, scale and origin so that `content` (model bbox) fits nicely.
    pub fn fit_to(&mut self, content: &BBox) {
        if content.is_empty() {
            return;
        }
        let sizes = [PaperSize::A4, PaperSize::A3, PaperSize::A2, PaperSize::A1, PaperSize::A0];
        let mut best: Option<(f64, PaperSize, f64)> = None;
        for (pi, paper) in sizes.iter().enumerate() {
            let probe = Sheet { paper: *paper, landscape: true, ..self.clone() };
            let avail = probe.drawable_mm();
            // ISO 5455 preferred scales only.
            for &s in [0.1, 0.2, 0.5, 1.0, 2.0, 5.0, 10.0, 20.0, 50.0, 100.0, 200.0, 500.0, 1000.0].iter() {
                let fill = (content.width() / s / avail.width()).max(content.height() / s / avail.height());
                if fill > 0.92 {
                    continue;
                }
                let mut cost = pi as f64 * 1.0 + s.log10().abs() * 3.0 + if fill < 0.3 { 3.0 } else { 0.0 };
                // Long slender parts (shafts) need room across for their dimensions.
                let (big, small) = (content.width().max(content.height()), content.width().min(content.height()));
                if big > 4.0 * small && small / s < 50.0 {
                    cost += 2.0;
                }
                if best.is_none_or(|b| cost < b.0) {
                    best = Some((cost, *paper, s));
                }
            }
        }
        let (_, paper, s) = best.unwrap_or((0.0, PaperSize::A0, 1000.0));
        self.paper = paper;
        self.landscape = true;
        self.scale = s;
        self.center_on(content);
    }

    /// Move the sheet so that `content` is centred in the drawable area.
    pub fn center_on(&mut self, content: &BBox) {
        let avail = self.drawable_mm();
        self.origin = content.center() - avail.center() * self.scale;
    }

    /// Border, zones, title block and projection symbol as model-space primitives,
    /// each with a lineweight in mm.
    pub fn prims(&self) -> Vec<(Prim, f64)> {
        self.prims_with(true)
    }

    /// Border and zones, plus the built-in title block when `builtin_title`.
    pub fn prims_with(&self, builtin_title: bool) -> Vec<(Prim, f64)> {
        let mut out: Vec<(Prim, f64)> = Vec::new();
        let (w, h) = self.paper_mm();
        let s = self.scale;
        let p = |x: f64, y: f64| self.p2m(x, y);
        let rect = |x0: f64, y0: f64, x1: f64, y1: f64| Prim::Poly(vec![p(x0, y0), p(x1, y0), p(x1, y1), p(x0, y1)], true);
        let line = |x0: f64, y0: f64, x1: f64, y1: f64| Prim::Line(p(x0, y0), p(x1, y1));
        let text = |x: f64, y: f64, ht: f64, t: &str, ha: HAlign, va: VAlign| Prim::Text(Text::new(p(x, y), ht * s, t).aligned(ha, va));

        // Trim edge and frame.
        out.push((rect(0.0, 0.0, w, h), 0.13));
        let f = self.frame_mm();
        out.push((rect(f.min.x, f.min.y, f.max.x, f.max.y), 0.7));

        // Grid reference zones (ISO 5457).
        let nx = ((f.width() / 50.0).round() as usize).max(2);
        let ny = ((f.height() / 50.0).round() as usize).max(2);
        for i in 0..nx {
            let x0 = f.min.x + f.width() * i as f64 / nx as f64;
            let xc = x0 + f.width() / nx as f64 / 2.0;
            if i > 0 {
                out.push((line(x0, f.min.y - 5.0, x0, f.min.y), 0.25));
                out.push((line(x0, f.max.y, x0, f.max.y + 5.0), 0.25));
            }
            let label = format!("{}", i + 1);
            out.push((text(xc, f.min.y - 2.5, 2.5, &label, HAlign::Center, VAlign::Middle), 0.25));
            out.push((text(xc, f.max.y + 2.5, 2.5, &label, HAlign::Center, VAlign::Middle), 0.25));
        }
        for j in 0..ny {
            let y0 = f.min.y + f.height() * j as f64 / ny as f64;
            let yc = y0 + f.height() / ny as f64 / 2.0;
            if j > 0 {
                out.push((line(f.min.x - 5.0, y0, f.min.x, y0), 0.25));
                out.push((line(f.max.x, y0, f.max.x + 5.0, y0), 0.25));
            }
            let label = ((b'A' + (ny - 1 - j) as u8 % 26) as char).to_string();
            out.push((text(f.min.x - 5.0, yc, 2.5, &label, HAlign::Center, VAlign::Middle), 0.25));
            out.push((text(f.max.x + 5.0, yc, 2.5, &label, HAlign::Center, VAlign::Middle), 0.25));
        }

        if builtin_title {
            self.draw_title(&mut out, &|tag| self.field(tag).unwrap_or_default());
        }
        out
    }

    /// Title block field by tag (as used by title block attributes and text
    /// fields): COMPANY, TITLE, DWG_NO, REV, SHEET, SIZE, MATERIAL, FINISH,
    /// SCALE, UNITS, DRAWN, CHECKED, APPROVED, DATE, TOLERANCE, PROJECTION.
    pub fn field(&self, tag: &str) -> Option<String> {
        let tb = &self.title;
        let key = tag.trim().to_ascii_uppercase().replace([' ', '-', '.'], "_");
        Some(match key.as_str() {
            "COMPANY" => tb.company.clone(),
            "TITLE" => tb.title.clone(),
            "DWG_NO" | "DWGNO" | "DRAWING_NO" | "DRAWING_NUMBER" | "NUMBER" => tb.drawing_no.clone(),
            "REV" | "REVISION" => tb.revision.clone(),
            "SHEET" | "SHEET_NO" => tb.sheet_no.clone(),
            "SIZE" | "PAPER" => self.paper.name().to_string(),
            "MATERIAL" => tb.material.clone(),
            "FINISH" => tb.finish.clone(),
            "SCALE" => self.scale_label(),
            "UNITS" => tb.units.clone(),
            "DRAWN" | "DRAWN_BY" | "DRAWNBY" => tb.drawn_by.clone(),
            "CHECKED" | "CHECKED_BY" => tb.checked_by.clone(),
            "APPROVED" | "APPROVED_BY" => tb.approved_by.clone(),
            "DATE" => tb.date.clone(),
            "TOLERANCE" | "TOLERANCES" | "GENERAL_TOLERANCES" => tb.tolerance.clone(),
            // The tolerance note as laid out on two lines in the title block.
            "TOLERANCE_1" | "TOLERANCE_2" => {
                let lines = split_two(&tb.tolerance);
                lines.get(if key.ends_with('1') { 0 } else { 1 }).cloned().unwrap_or_default()
            }
            "PROJECTION" => match tb.projection {
                Projection::FirstAngle => "First angle".into(),
                Projection::ThirdAngle => "Third angle".into(),
            },
            _ => return None,
        })
    }

    /// Model position of the frame's bottom-right corner (title block anchor).
    pub fn title_anchor(&self) -> V2 {
        let f = self.frame_mm();
        self.p2m(f.max.x, f.min.y)
    }

    /// The built-in title block with each value from `value(tag)`.
    fn draw_title(&self, out: &mut Vec<(Prim, f64)>, value: &dyn Fn(&str) -> String) {
        let s = self.scale;
        let p = |x: f64, y: f64| self.p2m(x, y);
        let rect = |x0: f64, y0: f64, x1: f64, y1: f64| Prim::Poly(vec![p(x0, y0), p(x1, y0), p(x1, y1), p(x0, y1)], true);
        let line = |x0: f64, y0: f64, x1: f64, y1: f64| Prim::Line(p(x0, y0), p(x1, y1));
        let text = |x: f64, y: f64, ht: f64, t: &str, ha: HAlign, va: VAlign| Prim::Text(Text::new(p(x, y), ht * s, t).aligned(ha, va));
        let f = self.frame_mm();
        // Title block anchored at the frame's bottom-right corner.
        let x0 = f.max.x - TB_W;
        let y0 = f.min.y;
        out.push((rect(x0, y0, f.max.x, y0 + TB_H), 0.7));
        for r in 1..5 {
            let y = y0 + 12.0 * r as f64;
            out.push((line(x0, y, f.max.x, y), 0.35));
        }
        // Each cell: a divider on its left edge, a small caption and the value.
        let cell = |out: &mut Vec<(Prim, f64)>, cx0: f64, row: usize, label: &str, value: &str, vh: f64| {
            let cy0 = y0 + 12.0 * row as f64;
            if cx0 > x0 + 1e-9 {
                out.push((line(cx0, cy0, cx0, cy0 + 12.0), 0.35));
            }
            out.push((text(cx0 + 1.2, cy0 + 12.0 - 1.2, 1.8, label, HAlign::Left, VAlign::Top), 0.18));
            if !value.is_empty() {
                out.push((text(cx0 + 1.5, cy0 + 2.2, vh, value, HAlign::Left, VAlign::Baseline), 0.25));
            }
        };
        let c = |dx: f64| x0 + dx;
        // Row 0: drawing number, revision, sheet, size.
        cell(out, c(0.0), 0, "DWG NO.", &value("DWG_NO"), 4.0);
        cell(out, c(95.0), 0, "REV", &value("REV"), 4.0);
        cell(out, c(115.0), 0, "SHEET", &value("SHEET"), 3.0);
        cell(out, c(145.0), 0, "SIZE", &value("SIZE"), 3.5);
        // Row 1: material, finish, scale, units.
        cell(out, c(0.0), 1, "MATERIAL", &value("MATERIAL"), 3.0);
        cell(out, c(70.0), 1, "FINISH", &value("FINISH"), 3.0);
        cell(out, c(115.0), 1, "SCALE", &value("SCALE"), 3.5);
        cell(out, c(145.0), 1, "UNITS", &value("UNITS"), 3.5);
        // Row 2: people, date and projection.
        cell(out, c(0.0), 2, "DRAWN", &value("DRAWN"), 2.5);
        cell(out, c(40.0), 2, "CHECKED", &value("CHECKED"), 2.5);
        cell(out, c(80.0), 2, "APPROVED", &value("APPROVED"), 2.5);
        cell(out, c(115.0), 2, "DATE", &value("DATE"), 2.5);
        cell(out, c(145.0), 2, "PROJECTION", "", 2.5);
        self.projection_symbol(out, c(150.0), y0 + 24.0 + 1.2, self.title.projection);
        // Row 3: title.
        cell(out, c(0.0), 3, "TITLE", "", 2.5);
        out.push((text(c(90.0), y0 + 36.0 + 4.5, 5.0, &value("TITLE"), HAlign::Center, VAlign::Middle), 0.35));
        // Row 4: company and general tolerance.
        out.push((line(c(125.0), y0 + 48.0, c(125.0), y0 + 60.0), 0.35));
        out.push((text(c(62.5), y0 + 54.0, 6.0, &value("COMPANY"), HAlign::Center, VAlign::Middle), 0.5));
        out.push((text(c(126.2), y0 + 60.0 - 1.2, 1.8, "GENERAL TOLERANCES", HAlign::Left, VAlign::Top), 0.18));
        for (i, tag) in ["TOLERANCE_1", "TOLERANCE_2"].into_iter().enumerate() {
            let v = value(tag);
            if !v.is_empty() {
                out.push((text(c(126.5), y0 + 54.4 - i as f64 * 3.3, 2.2, &v, HAlign::Left, VAlign::Middle), 0.18));
            }
        }
    }

    /// The built-in title block as block contents in paper millimetres, base
    /// point at its bottom-right corner, with an attribute definition for each
    /// value (tags as in `field`). The starting point for a custom title block.
    pub fn title_block_template(&self) -> Vec<(crate::entity::Shape, f64)> {
        use crate::entity::{AttDef, Shape};
        let mut unit = self.clone();
        unit.scale = 1.0;
        let f = unit.frame_mm();
        unit.origin = V2::new(-f.max.x, -f.min.y);
        let mut prims = vec![];
        unit.draw_title(&mut prims, &|tag| format!("%{tag}%"));
        prims
            .into_iter()
            .filter_map(|(p, lw)| {
                let s = match p {
                    Prim::Line(a, b) => Shape::Line { a, b },
                    Prim::Poly(pts, closed) => Shape::Polyline { pts, closed },
                    Prim::Circle(c, r) => Shape::Circle { c, r },
                    Prim::Arc { c, r, start, end } => Shape::Arc { c, r, start, end },
                    Prim::Text(t) => match t.content.strip_prefix('%').and_then(|x| x.strip_suffix('%')) {
                        Some(tag) => {
                            let prompt = tag.replace('_', " ").to_lowercase();
                            Shape::AttDef(AttDef {
                                tag: tag.to_string(),
                                prompt: prompt[..1].to_uppercase() + &prompt[1..],
                                default: self.field(tag).unwrap_or_default(),
                                text: Text { content: String::new(), ..t },
                                invisible: false,
                            })
                        }
                        None => Shape::Text(t),
                    },
                    Prim::Fill(_) | Prim::Image(_) => return None,
                };
                Some((s, lw))
            })
            .collect()
    }

    fn projection_symbol(&self, out: &mut Vec<(Prim, f64)>, x: f64, y: f64, proj: Projection) {
        // Truncated cone: trapezoid (side view) + two concentric circles (end view).
        let s = self.scale;
        let p = |px: f64, py: f64| self.p2m(px, py);
        let cy = y + 3.6;
        let (tx, cx) = match proj {
            Projection::FirstAngle => (x, x + 20.0),
            Projection::ThirdAngle => (x + 10.0, x + 3.5),
        };
        // Trapezoid: narrow end on the left, wide end on the right.
        out.push((Prim::Poly(vec![p(tx, cy - 1.6), p(tx + 9.0, cy - 3.2), p(tx + 9.0, cy + 3.2), p(tx, cy + 1.6)], true), 0.25));
        out.push((Prim::Circle(p(cx, cy), 3.2 * s), 0.25));
        out.push((Prim::Circle(p(cx, cy), 1.6 * s), 0.25));
    }
}

fn split_two(s: &str) -> Vec<String> {
    if s.len() <= 26 {
        return vec![s.to_string()];
    }
    let words: Vec<&str> = s.split_whitespace().collect();
    let mut best = vec![s.to_string()];
    let mut best_d = usize::MAX;
    for i in 1..words.len() {
        let a = words[..i].join(" ");
        let b = words[i..].join(" ");
        let d = a.len().max(b.len());
        if d < best_d {
            best_d = d;
            best = vec![a, b];
        }
    }
    best
}

/// Today's date as YYYY-MM-DD (UTC), without pulling in a date crate.
pub fn today() -> String {
    let secs = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0) as i64;
    let z = secs / 86400 + 719468;
    let era = z.div_euclid(146097);
    let doe = z - era * 146097;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = yoe + era * 400 + if m <= 2 { 1 } else { 0 };
    format!("{y:04}-{m:02}-{d:02}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scales_round_trip() {
        assert_eq!(parse_scale("1:2"), Some(2.0));
        assert_eq!(parse_scale("2:1"), Some(0.5));
        assert_eq!(scale_label(0.5), "2:1");
        assert_eq!(scale_label(5.0), "1:5");
    }

    #[test]
    fn fit_picks_reasonable_sheet() {
        let mut s = Sheet::default();
        s.fit_to(&BBox::from_pts(V2::new(0.0, 0.0), V2::new(150.0, 80.0)));
        assert_eq!(s.scale, 1.0);
        let d = s.drawable_mm();
        let c = s.m2p(V2::new(75.0, 40.0));
        assert!(d.contains(c));
        s.fit_to(&BBox::from_pts(V2::new(0.0, 0.0), V2::new(3000.0, 1500.0)));
        assert!(s.scale >= 10.0);
    }

    #[test]
    fn date_format() {
        assert_eq!(today().len(), 10);
    }
}
