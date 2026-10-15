// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Proteen Das

//! Engineering annotation symbols: geometric tolerance feature control
//! frames and datum feature symbols (ISO 1101), surface texture symbols
//! (ISO 1302 / 21920) and weld symbols (ISO 2553). All are drawn with plain
//! primitives at paper size × annotation scale, so they plot and export
//! identically everywhere.

use crate::geom::*;
use crate::prim::*;
use serde::{Deserialize, Serialize};
use std::f64::consts::{FRAC_PI_2, PI, TAU};

// ------------------------------------------------------------------ helpers

fn filled_arrow(tip: V2, dir: V2, size: f64) -> Prim {
    let d = dir.norm();
    let n = d.perp() * (size / 6.0);
    let back = tip - d * size;
    Prim::Fill(vec![tip, back + n, back - n])
}

fn rect(out: &mut Vec<Prim>, a: V2, b: V2) {
    out.push(Prim::Poly(vec![a, V2::new(b.x, a.y), b, V2::new(a.x, b.y)], true));
}

fn text(out: &mut Vec<Prim>, at: V2, h: f64, s: &str, ha: HAlign, va: VAlign) {
    if !s.is_empty() {
        out.push(Prim::Text(Text::new(at, h, s).aligned(ha, va)));
    }
}

/// Rotate and translate primitives drawn around the origin.
fn place(prims: Vec<Prim>, at: V2, rot: f64) -> Vec<Prim> {
    let x = crate::document::compose(&Xform::translate(at), &Xform::rotate(V2::ZERO, rot));
    prims
        .into_iter()
        .map(|p| match p {
            // Keep text upright-readable: rotate it with the symbol only within ±90°.
            Prim::Text(mut t) => {
                t.pos = x.apply(t.pos);
                let r = norm_angle(rot);
                t.rotation = if r > FRAC_PI_2 + 1e-6 && r <= 3.0 * FRAC_PI_2 + 1e-6 { norm_angle(r + PI) } else { r };
                Prim::Text(t)
            }
            p => p.transformed(&x),
        })
        .collect()
}

// ------------------------------------------------------------------ GD&T

/// Geometric characteristic symbols (ISO 1101).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum GdtSymbol {
    #[default]
    Position,
    Straightness,
    Flatness,
    Circularity,
    Cylindricity,
    ProfileLine,
    ProfileSurface,
    Angularity,
    Perpendicularity,
    Parallelism,
    Concentricity,
    Symmetry,
    CircularRunout,
    TotalRunout,
}

impl GdtSymbol {
    pub const ALL: [GdtSymbol; 14] = [
        GdtSymbol::Position,
        GdtSymbol::Straightness,
        GdtSymbol::Flatness,
        GdtSymbol::Circularity,
        GdtSymbol::Cylindricity,
        GdtSymbol::ProfileLine,
        GdtSymbol::ProfileSurface,
        GdtSymbol::Angularity,
        GdtSymbol::Perpendicularity,
        GdtSymbol::Parallelism,
        GdtSymbol::Concentricity,
        GdtSymbol::Symmetry,
        GdtSymbol::CircularRunout,
        GdtSymbol::TotalRunout,
    ];

    pub fn name(self) -> &'static str {
        match self {
            GdtSymbol::Position => "Position",
            GdtSymbol::Straightness => "Straightness",
            GdtSymbol::Flatness => "Flatness",
            GdtSymbol::Circularity => "Circularity (roundness)",
            GdtSymbol::Cylindricity => "Cylindricity",
            GdtSymbol::ProfileLine => "Profile of a line",
            GdtSymbol::ProfileSurface => "Profile of a surface",
            GdtSymbol::Angularity => "Angularity",
            GdtSymbol::Perpendicularity => "Perpendicularity",
            GdtSymbol::Parallelism => "Parallelism",
            GdtSymbol::Concentricity => "Concentricity / coaxiality",
            GdtSymbol::Symmetry => "Symmetry",
            GdtSymbol::CircularRunout => "Circular run-out",
            GdtSymbol::TotalRunout => "Total run-out",
        }
    }

    /// Characters of the common GDT font (DXF TOLERANCE strings).
    pub fn gdt_char(self) -> char {
        match self {
            GdtSymbol::Position => 'j',
            GdtSymbol::Straightness => 'u',
            GdtSymbol::Flatness => 'c',
            GdtSymbol::Circularity => 'e',
            GdtSymbol::Cylindricity => 'g',
            GdtSymbol::ProfileLine => 'k',
            GdtSymbol::ProfileSurface => 'd',
            GdtSymbol::Angularity => 'a',
            GdtSymbol::Perpendicularity => 'b',
            GdtSymbol::Parallelism => 'f',
            GdtSymbol::Concentricity => 'r',
            GdtSymbol::Symmetry => 'i',
            GdtSymbol::CircularRunout => 'h',
            GdtSymbol::TotalRunout => 't',
        }
    }

    pub fn from_gdt_char(c: char) -> Option<GdtSymbol> {
        GdtSymbol::ALL.into_iter().find(|s| s.gdt_char() == c)
    }

    /// Draw the symbol centred at `c` in a cell of size `k`.
    pub fn draw(self, out: &mut Vec<Prim>, c: V2, k: f64) {
        let s = k * 0.32;
        let p = |x: f64, y: f64| c + V2::new(x * s, y * s);
        let arrow = |out: &mut Vec<Prim>, from: V2, to: V2| {
            out.push(Prim::Line(from, to));
            out.push(filled_arrow(to, to - from, s * 0.7));
        };
        match self {
            GdtSymbol::Straightness => out.push(Prim::Line(p(-1.0, 0.0), p(1.0, 0.0))),
            GdtSymbol::Flatness => out.push(Prim::Poly(vec![p(-1.0, -0.5), p(0.6, -0.5), p(1.0, 0.5), p(-0.6, 0.5)], true)),
            GdtSymbol::Circularity => out.push(Prim::Circle(c, s * 0.8)),
            GdtSymbol::Cylindricity => {
                out.push(Prim::Circle(c, s * 0.6));
                out.push(Prim::Line(p(-1.0, -0.9), p(-0.3, 0.9)));
                out.push(Prim::Line(p(0.3, -0.9), p(1.0, 0.9)));
            }
            GdtSymbol::ProfileLine => out.push(Prim::Arc { c: p(0.0, -0.45), r: s, start: 0.0, end: PI }),
            GdtSymbol::ProfileSurface => {
                out.push(Prim::Arc { c: p(0.0, -0.45), r: s, start: 0.0, end: PI });
                out.push(Prim::Line(p(-1.0, -0.45), p(1.0, -0.45)));
            }
            GdtSymbol::Angularity => {
                out.push(Prim::Line(p(-1.0, -0.8), p(1.0, -0.8)));
                out.push(Prim::Line(p(-1.0, -0.8), p(1.0, 0.6)));
            }
            GdtSymbol::Perpendicularity => {
                out.push(Prim::Line(p(-1.0, -0.8), p(1.0, -0.8)));
                out.push(Prim::Line(p(0.0, -0.8), p(0.0, 1.0)));
            }
            GdtSymbol::Parallelism => {
                out.push(Prim::Line(p(-1.0, -0.8), p(0.0, 0.9)));
                out.push(Prim::Line(p(0.0, -0.8), p(1.0, 0.9)));
            }
            GdtSymbol::Position => {
                out.push(Prim::Circle(c, s * 0.65));
                out.push(Prim::Line(p(-1.0, 0.0), p(1.0, 0.0)));
                out.push(Prim::Line(p(0.0, -1.0), p(0.0, 1.0)));
            }
            GdtSymbol::Concentricity => {
                out.push(Prim::Circle(c, s * 0.45));
                out.push(Prim::Circle(c, s * 0.9));
            }
            GdtSymbol::Symmetry => {
                out.push(Prim::Line(p(-1.0, 0.0), p(1.0, 0.0)));
                out.push(Prim::Line(p(-0.6, 0.55), p(0.6, 0.55)));
                out.push(Prim::Line(p(-0.6, -0.55), p(0.6, -0.55)));
            }
            GdtSymbol::CircularRunout => arrow(out, p(-0.7, -0.8), p(0.7, 0.9)),
            GdtSymbol::TotalRunout => {
                arrow(out, p(-1.0, -0.8), p(0.0, 0.9));
                arrow(out, p(0.0, -0.8), p(1.0, 0.9));
                out.push(Prim::Line(p(-1.0, -0.8), p(0.0, -0.8)));
            }
        }
    }
}

/// Material condition modifiers.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum Material {
    #[default]
    None,
    /// Ⓜ maximum material condition.
    Mmc,
    /// Ⓛ least material condition.
    Lmc,
    /// Ⓢ regardless of feature size.
    Rfs,
}

impl Material {
    pub fn letter(self) -> &'static str {
        match self {
            Material::None => "",
            Material::Mmc => "M",
            Material::Lmc => "L",
            Material::Rfs => "S",
        }
    }
    pub fn from_letter(s: &str) -> Material {
        match s.trim().to_ascii_uppercase().as_str() {
            "M" => Material::Mmc,
            "L" => Material::Lmc,
            "S" => Material::Rfs,
            _ => Material::None,
        }
    }
    fn gdt_char(self) -> Option<char> {
        match self {
            Material::None => None,
            Material::Mmc => Some('m'),
            Material::Lmc => Some('l'),
            Material::Rfs => Some('s'),
        }
    }
    /// Circled letter after text ending at `x`; returns the new end.
    fn draw(self, out: &mut Vec<Prim>, x: f64, y: f64, h: f64) -> f64 {
        if self == Material::None {
            return x;
        }
        let r = h * 0.65;
        let c = V2::new(x + h * 0.25 + r, y);
        out.push(Prim::Circle(c, r));
        out.push(Prim::Text(Text::new(c, h * 0.8, self.letter()).aligned(HAlign::Center, VAlign::Middle)));
        c.x + r
    }
    fn width(self, h: f64) -> f64 {
        if self == Material::None {
            0.0
        } else {
            h * 0.25 + h * 1.3
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct DatumRef {
    pub letter: String,
    #[serde(default)]
    pub material: Material,
}

/// One row of a feature control frame.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct FcfRow {
    pub symbol: GdtSymbol,
    /// Ø before the tolerance value (cylindrical tolerance zone).
    #[serde(default)]
    pub diameter: bool,
    pub tolerance: String,
    #[serde(default)]
    pub material: Material,
    #[serde(default)]
    pub datums: Vec<DatumRef>,
}

/// Feature control frame with an optional leader to the toleranced feature.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Fcf {
    /// Left end of the first row's centre line.
    pub pos: V2,
    pub rows: Vec<FcfRow>,
    /// Leader from the frame to the feature, arrow tip first (empty = none).
    #[serde(default)]
    pub leader: Vec<V2>,
}

impl Fcf {
    /// Cell widths (symbol, tolerance, datums…) and row height for text height `h`.
    fn layout(&self, h: f64) -> (Vec<Vec<f64>>, f64) {
        let rh = h * 2.0;
        let widths = self
            .rows
            .iter()
            .map(|r| {
                let mut w = vec![rh];
                let tol = format!("{}{}", if r.diameter { "Ø" } else { "" }, r.tolerance);
                w.push((text_width(&tol, h) + r.material.width(h) + h * 1.2).max(rh * 1.6));
                for d in &r.datums {
                    w.push((text_width(&d.letter, h) + d.material.width(h) + h * 1.2).max(rh));
                }
                w
            })
            .collect();
        (widths, rh)
    }

    pub fn prims(&self, h: f64, arrow_size: f64) -> Vec<Prim> {
        let mut out = vec![];
        let (widths, rh) = self.layout(h);
        for (i, (row, w)) in self.rows.iter().zip(&widths).enumerate() {
            let yc = self.pos.y - i as f64 * rh;
            let (y0, y1) = (yc - rh / 2.0, yc + rh / 2.0);
            let mut x = self.pos.x;
            for (k, cw) in w.iter().enumerate() {
                rect(&mut out, V2::new(x, y0), V2::new(x + cw, y1));
                let cx = x + cw / 2.0;
                match k {
                    0 => row.symbol.draw(&mut out, V2::new(cx, yc), rh),
                    1 => {
                        let tol = format!("{}{}", if row.diameter { "Ø" } else { "" }, row.tolerance);
                        let tw = text_width(&tol, h) + row.material.width(h);
                        let start = cx - tw / 2.0;
                        text(&mut out, V2::new(start, yc), h, &tol, HAlign::Left, VAlign::Middle);
                        row.material.draw(&mut out, start + text_width(&tol, h), yc, h);
                    }
                    _ => {
                        let d = &row.datums[k - 2];
                        let tw = text_width(&d.letter, h) + d.material.width(h);
                        let start = cx - tw / 2.0;
                        text(&mut out, V2::new(start, yc), h, &d.letter, HAlign::Left, VAlign::Middle);
                        d.material.draw(&mut out, start + text_width(&d.letter, h), yc, h);
                    }
                }
                x += cw;
            }
        }
        if let Some(&tip) = self.leader.first() {
            // Leader from the nearer end of the first row to the feature.
            let w0: f64 = widths.first().map(|w| w.iter().sum()).unwrap_or(0.0);
            let last = *self.leader.last().unwrap();
            let attach = if last.x > self.pos.x + w0 / 2.0 { V2::new(self.pos.x + w0, self.pos.y) } else { self.pos };
            let mut pts = self.leader.clone();
            pts.push(attach);
            out.push(Prim::Poly(pts.clone(), false));
            out.push(filled_arrow(tip, tip - pts[1], arrow_size));
        }
        out
    }

    /// Total width of the first row.
    pub fn width(&self, h: f64) -> f64 {
        self.layout(h).0.first().map(|w| w.iter().sum()).unwrap_or(0.0)
    }

    /// DXF TOLERANCE string (GDT font codes, `%%v` separators, `^J` rows).
    pub fn to_dxf(&self) -> String {
        let g = |c: char| format!("{{\\Fgdt;{c}}}");
        self.rows
            .iter()
            .map(|r| {
                let mut s = g(r.symbol.gdt_char());
                s += "%%v";
                if r.diameter {
                    s += &g('n');
                }
                s += &r.tolerance;
                if let Some(c) = r.material.gdt_char() {
                    s += &g(c);
                }
                for d in &r.datums {
                    s += "%%v";
                    s += &d.letter;
                    if let Some(c) = d.material.gdt_char() {
                        s += &g(c);
                    }
                }
                s
            })
            .collect::<Vec<_>>()
            .join("^J")
    }

    /// Parse a DXF TOLERANCE string.
    pub fn from_dxf(pos: V2, s: &str) -> Fcf {
        let rows = s
            .split("^J")
            .filter(|r| !r.trim().is_empty())
            .map(|r| {
                let cells: Vec<&str> = r.split("%%v").collect();
                let decode = |c: &str| -> (String, Vec<char>) {
                    // Split "{\Fgdt;x}" codes from plain text.
                    let mut plain = String::new();
                    let mut codes = vec![];
                    let mut rest = c;
                    while let Some(i) = rest.find("{\\Fgdt;") {
                        plain += &rest[..i];
                        let after = &rest[i + 7..];
                        if let Some(ch) = after.chars().next() {
                            codes.push(ch.to_ascii_lowercase());
                        }
                        rest = after.split_once('}').map(|x| x.1).unwrap_or("");
                    }
                    plain += rest;
                    (plain.trim().to_string(), codes)
                };
                let mat = |codes: &[char]| {
                    codes.iter().find_map(|c| match c {
                        'm' => Some(Material::Mmc),
                        'l' => Some(Material::Lmc),
                        's' => Some(Material::Rfs),
                        _ => None,
                    })
                };
                let (_, sym) = decode(cells.first().copied().unwrap_or(""));
                let symbol = sym.first().and_then(|c| GdtSymbol::from_gdt_char(*c)).unwrap_or_default();
                let (tol, codes) = decode(cells.get(1).copied().unwrap_or(""));
                let datums = cells
                    .iter()
                    .skip(2)
                    .map(|c| {
                        let (letter, codes) = decode(c);
                        DatumRef { letter, material: mat(&codes).unwrap_or_default() }
                    })
                    .filter(|d| !d.letter.is_empty())
                    .collect();
                FcfRow { symbol, diameter: codes.contains(&'n'), tolerance: tol, material: mat(&codes).unwrap_or_default(), datums }
            })
            .collect();
        Fcf { pos, rows, leader: vec![] }
    }
}

/// Datum feature symbol: a framed letter joined to a filled triangle on the feature.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct DatumFeature {
    pub letter: String,
    /// Centre of the framed letter.
    pub pos: V2,
    /// Middle of the triangle's base, on the feature.
    pub tip: V2,
}

impl DatumFeature {
    pub fn prims(&self, h: f64) -> Vec<Prim> {
        let mut out = vec![];
        let half = h * 1.1;
        rect(&mut out, self.pos - V2::new(half, half), self.pos + V2::new(half, half));
        text(&mut out, self.pos, h, &self.letter, HAlign::Center, VAlign::Middle);
        let d = self.pos - self.tip;
        let dir = if d.len() < 1e-9 { V2::new(0.0, 1.0) } else { d.norm() };
        // Leave the frame from the side facing the triangle.
        let exit = if dir.x.abs() > dir.y.abs() {
            self.pos - V2::new(half * dir.x.signum(), 0.0)
        } else {
            self.pos - V2::new(0.0, half * dir.y.signum())
        };
        let tri = h * 1.2;
        let apex = self.tip + dir * (tri * 0.87);
        out.push(Prim::Line(apex, exit));
        let n = dir.perp() * (tri / 2.0);
        out.push(Prim::Fill(vec![self.tip + n, self.tip - n, apex]));
        out
    }
}

// ------------------------------------------------------------------ surface texture

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum SurfaceProcess {
    /// Any process allowed.
    Any,
    /// Material removal required (machined).
    #[default]
    Removal,
    /// Material removal not permitted.
    NoRemoval,
}

/// Surface texture symbol with its requirements.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct SurfaceFinish {
    /// Tip of the symbol, on the surface.
    pub pos: V2,
    /// Rotation about the tip (radians); 0 = symbol above a horizontal surface.
    #[serde(default)]
    pub rotation: f64,
    #[serde(default)]
    pub process: SurfaceProcess,
    /// Surface texture requirement, e.g. "Ra 1.6".
    #[serde(default)]
    pub requirement: String,
    /// Manufacturing method, e.g. "ground".
    #[serde(default)]
    pub method: String,
    /// Surface lay symbol: = ⊥ X M C R P.
    #[serde(default)]
    pub lay: String,
    /// Applies all around the outline.
    #[serde(default)]
    pub all_around: bool,
}

impl SurfaceFinish {
    pub fn prims(&self, h: f64) -> Vec<Prim> {
        let mut out = vec![];
        let h1 = h * 1.4;
        let h2 = h1 * 2.0;
        let a = 60f64.to_radians();
        let left = V2::new(-h1 / a.tan(), h1);
        let right = V2::new(h2 / a.tan(), h2);
        out.push(Prim::Line(V2::ZERO, left));
        out.push(Prim::Line(V2::ZERO, right));
        match self.process {
            SurfaceProcess::Removal => out.push(Prim::Line(left, V2::new(h1 / a.tan(), h1))),
            SurfaceProcess::NoRemoval => {
                let r = h1 * 0.38;
                out.push(Prim::Circle(V2::new(0.0, h1 - r * 1.05), r));
            }
            SurfaceProcess::Any => {}
        }
        let has_text = !self.requirement.is_empty() || !self.method.is_empty();
        let bar_len = text_width(&self.requirement, h).max(text_width(&self.method, h)) + h;
        if has_text || self.all_around {
            out.push(Prim::Line(right, right + V2::new(bar_len.max(h * 1.5), 0.0)));
        }
        if self.all_around {
            out.push(Prim::Circle(right, h * 0.35));
        }
        let tx = right.x - h * 0.2;
        text(&mut out, V2::new(tx + h * 0.4, right.y - h * 0.4), h, &self.requirement, HAlign::Left, VAlign::Top);
        text(&mut out, V2::new(tx + h * 0.4, right.y + h * 0.4), h, &self.method, HAlign::Left, VAlign::Bottom);
        let lay_at = V2::new(right.x * 0.55 + h * 0.4, h * 0.1);
        if self.lay == "⊥" {
            // Drawn, as the plot fonts have no ⊥.
            out.push(Prim::Line(lay_at, lay_at + V2::new(h * 0.8, 0.0)));
            out.push(Prim::Line(lay_at + V2::new(h * 0.4, 0.0), lay_at + V2::new(h * 0.4, h)));
        } else {
            text(&mut out, lay_at, h, &self.lay, HAlign::Left, VAlign::Bottom);
        }
        place(out, self.pos, self.rotation)
    }
}

// ------------------------------------------------------------------ welds

/// Elementary weld symbols (ISO 2553).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum WeldType {
    #[default]
    None,
    Fillet,
    SquareButt,
    VButt,
    BevelButt,
    UButt,
    JButt,
    Plug,
    Spot,
    Seam,
}

impl WeldType {
    pub const ALL: [WeldType; 10] = [
        WeldType::None,
        WeldType::Fillet,
        WeldType::SquareButt,
        WeldType::VButt,
        WeldType::BevelButt,
        WeldType::UButt,
        WeldType::JButt,
        WeldType::Plug,
        WeldType::Spot,
        WeldType::Seam,
    ];
    pub fn name(self) -> &'static str {
        match self {
            WeldType::None => "(none)",
            WeldType::Fillet => "Fillet",
            WeldType::SquareButt => "Square butt",
            WeldType::VButt => "Single-V butt",
            WeldType::BevelButt => "Single-bevel butt",
            WeldType::UButt => "Single-U butt",
            WeldType::JButt => "Single-J butt",
            WeldType::Plug => "Plug / slot",
            WeldType::Spot => "Spot",
            WeldType::Seam => "Seam",
        }
    }

    /// Draw the symbol on the reference line at `x`, on the side `s`
    /// (−1 = below / arrow side, +1 = above / other side), size `k`.
    fn draw(self, out: &mut Vec<Prim>, x: f64, y: f64, s: f64, k: f64) {
        let p = |dx: f64, dy: f64| V2::new(x + dx * k, y + dy * k * s);
        match self {
            WeldType::None => {}
            WeldType::Fillet => out.push(Prim::Poly(vec![p(-0.5, 0.0), p(-0.5, 1.0), p(0.5, 0.0)], false)),
            WeldType::SquareButt => {
                out.push(Prim::Line(p(-0.25, 0.0), p(-0.25, 1.0)));
                out.push(Prim::Line(p(0.25, 0.0), p(0.25, 1.0)));
            }
            // Groove symbols: open end on the reference line, root away from it.
            WeldType::VButt => out.push(Prim::Poly(vec![p(-0.6, 0.0), p(0.0, 1.0), p(0.6, 0.0)], false)),
            WeldType::BevelButt => {
                out.push(Prim::Line(p(-0.3, 0.0), p(-0.3, 1.0)));
                out.push(Prim::Line(p(0.4, 0.0), p(-0.3, 1.0)));
            }
            WeldType::UButt => {
                let r = 0.45 * k;
                out.push(Prim::Line(p(-0.45, 0.0), p(-0.45, 0.45)));
                out.push(Prim::Line(p(0.45, 0.0), p(0.45, 0.45)));
                let (a0, a1) = if s < 0.0 { (PI, TAU) } else { (0.0, PI) };
                out.push(Prim::Arc { c: p(0.0, 0.45), r, start: a0, end: a1 });
                out.push(Prim::Line(p(0.0, 0.9), p(0.0, 1.2)));
            }
            WeldType::JButt => {
                let r = 0.45 * k;
                out.push(Prim::Line(p(-0.3, 0.0), p(-0.3, 1.2)));
                out.push(Prim::Line(p(0.45, 0.0), p(0.45, 0.45)));
                let (a0, a1) = if s < 0.0 { (3.0 * FRAC_PI_2, TAU) } else { (0.0, FRAC_PI_2) };
                out.push(Prim::Arc { c: p(0.0, 0.45), r, start: a0, end: a1 });
                out.push(Prim::Line(p(0.0, 0.9), p(-0.3, 0.9)));
            }
            WeldType::Plug => rect(out, p(-0.5, 0.0), p(0.5, 0.6)),
            WeldType::Spot => out.push(Prim::Circle(p(0.0, 0.45), 0.45 * k)),
            WeldType::Seam => {
                out.push(Prim::Circle(p(0.0, 0.45), 0.45 * k));
                out.push(Prim::Line(p(-0.65, 0.3), p(0.65, 0.3)));
                out.push(Prim::Line(p(-0.65, 0.6), p(0.65, 0.6)));
            }
        }
    }
}

/// One side of a weld symbol.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct WeldSide {
    pub kind: WeldType,
    /// Size (throat "a5", leg "z6", depth…), written before the symbol.
    #[serde(default)]
    pub size: String,
    /// Length / count×length(pitch), written after the symbol.
    #[serde(default)]
    pub length: String,
}

/// Weld symbol: arrow line to the joint, reference line with the weld
/// symbols, all-around circle, field-weld flag and tail.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct WeldSymbol {
    /// Arrow tip on the joint.
    pub tip: V2,
    /// Start of the reference line (where the arrow line meets it).
    pub elbow: V2,
    #[serde(default)]
    pub arrow_side: WeldSide,
    #[serde(default)]
    pub other_side: WeldSide,
    #[serde(default)]
    pub all_around: bool,
    #[serde(default)]
    pub field: bool,
    /// Process or reference in the tail (e.g. "111", "ISO 5817-B").
    #[serde(default)]
    pub tail: String,
}

impl WeldSymbol {
    pub fn prims(&self, h: f64, arrow_size: f64) -> Vec<Prim> {
        let mut out = vec![];
        let dir = if self.elbow.x >= self.tip.x { 1.0 } else { -1.0 };
        let k = h * 1.6;
        let side_w = |s: &WeldSide| text_width(&s.size, h) + text_width(&s.length, h);
        let len = (k * 3.0 + side_w(&self.arrow_side).max(side_w(&self.other_side)) + h * 2.0).max(k * 5.0);
        let end = self.elbow + V2::new(dir * len, 0.0);
        out.push(Prim::Line(self.tip, self.elbow));
        out.push(filled_arrow(self.tip, self.tip - self.elbow, arrow_size));
        out.push(Prim::Line(self.elbow, end));
        // Symbols sit a little way along the reference line.
        let sx = self.elbow.x + dir * (k * 1.5 + text_width(&self.arrow_side.size, h).max(text_width(&self.other_side.size, h)) + h);
        let y = self.elbow.y;
        for (side, s) in [(&self.arrow_side, -1.0), (&self.other_side, 1.0)] {
            if side.kind == WeldType::None && side.size.is_empty() && side.length.is_empty() {
                continue;
            }
            side.kind.draw(&mut out, sx, y, s, k);
            let ty = y + s * k * 0.5;
            text(&mut out, V2::new(sx - k * 0.75, ty), h, &side.size, HAlign::Right, VAlign::Middle);
            text(&mut out, V2::new(sx + k * 0.75, ty), h, &side.length, HAlign::Left, VAlign::Middle);
        }
        if self.all_around {
            out.push(Prim::Circle(self.elbow, h * 0.6));
        }
        if self.field {
            let top = self.elbow + V2::new(0.0, k * 1.6);
            out.push(Prim::Line(self.elbow, top));
            out.push(Prim::Fill(vec![top, top - V2::new(0.0, k * 0.6), top + V2::new(dir * k * 0.9, -k * 0.3)]));
        }
        if !self.tail.is_empty() {
            let f = h * 1.2;
            out.push(Prim::Line(end, end + V2::new(dir * f, f)));
            out.push(Prim::Line(end, end + V2::new(dir * f, -f)));
            let ha = if dir > 0.0 { HAlign::Left } else { HAlign::Right };
            text(&mut out, end + V2::new(dir * (f + h * 0.4), 0.0), h, &self.tail, ha, VAlign::Middle);
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn feature_control_frame_layout_and_dxf_codes() {
        let f = Fcf {
            pos: V2::ZERO,
            rows: vec![FcfRow {
                symbol: GdtSymbol::Position,
                diameter: true,
                tolerance: "0.05".into(),
                material: Material::Mmc,
                datums: vec![
                    DatumRef { letter: "A".into(), material: Material::None },
                    DatumRef { letter: "B".into(), material: Material::Mmc },
                ],
            }],
            leader: vec![V2::new(-20.0, -10.0)],
        };
        let p = f.prims(3.5, 3.0);
        // 4 cells, symbol circle + cross, two circled M modifiers, leader and arrow.
        assert_eq!(p.iter().filter(|x| matches!(x, Prim::Poly(_, true))).count(), 4);
        assert_eq!(p.iter().filter(|x| matches!(x, Prim::Circle(..))).count(), 3);
        let s = f.to_dxf();
        assert_eq!(s, "{\\Fgdt;j}%%v{\\Fgdt;n}0.05{\\Fgdt;m}%%vA%%vB{\\Fgdt;m}");
        let back = Fcf::from_dxf(V2::ZERO, &s);
        assert_eq!(back.rows, f.rows);
    }

    #[test]
    fn symbols_draw_something() {
        for g in GdtSymbol::ALL {
            let mut out = vec![];
            g.draw(&mut out, V2::ZERO, 7.0);
            assert!(!out.is_empty(), "{g:?}");
        }
        let s = SurfaceFinish {
            pos: V2::new(10.0, 0.0),
            rotation: 0.0,
            process: SurfaceProcess::Removal,
            requirement: "Ra 1.6".into(),
            method: "ground".into(),
            lay: "=".into(),
            all_around: true,
        };
        let p = s.prims(3.5);
        assert!(p.iter().any(|x| matches!(x, Prim::Text(t) if t.content == "Ra 1.6")));
        // The tip stays on the surface point.
        assert!(p.iter().any(|x| matches!(x, Prim::Line(a, _) if a.dist(V2::new(10.0, 0.0)) < 1e-9)));
        let w = WeldSymbol {
            tip: V2::ZERO,
            elbow: V2::new(10.0, 10.0),
            arrow_side: WeldSide { kind: WeldType::Fillet, size: "a5".into(), length: "100".into() },
            other_side: WeldSide::default(),
            all_around: true,
            field: true,
            tail: "111".into(),
        };
        assert!(w.prims(3.5, 3.0).len() > 8);
        let d = DatumFeature { letter: "A".into(), pos: V2::new(0.0, 15.0), tip: V2::ZERO };
        assert!(d.prims(3.5).iter().any(|x| matches!(x, Prim::Fill(_))));
    }
}
