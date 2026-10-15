// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Proteen Das

//! Colors (CAD Color Index, ACI), linetypes, layers and dimension style.

use serde::{Deserialize, Serialize};

/// Entity color: inherit from layer or an explicit CAD Color Index (ACI) (1..=255).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum Color {
    #[default]
    ByLayer,
    Aci(u8),
}

pub const ACI_NAMES: [(u8, &str); 9] =
    [(1, "Red"), (2, "Yellow"), (3, "Green"), (4, "Cyan"), (5, "Blue"), (6, "Magenta"), (7, "White"), (8, "Gray"), (9, "Light Gray")];

pub fn aci_name(i: u8) -> String {
    ACI_NAMES.iter().find(|(n, _)| *n == i).map(|(_, s)| s.to_string()).unwrap_or_else(|| format!("Color {i}"))
}

/// RGB of a CAD Color Index (ACI). Index 7 is "white" on dark backgrounds.
pub fn aci_rgb(i: u8) -> [u8; 3] {
    match i {
        0 | 7 => [255, 255, 255],
        1 => [255, 0, 0],
        2 => [255, 255, 0],
        3 => [0, 255, 0],
        4 => [0, 255, 255],
        5 => [0, 0, 255],
        6 => [255, 0, 255],
        8 => [128, 128, 128],
        9 => [192, 192, 192],
        250..=255 => {
            let g = [51, 91, 132, 173, 214, 255][(i - 250) as usize];
            [g, g, g]
        }
        _ => {
            let hue = ((i / 10) as f64 - 1.0) * 15.0;
            let sub = i % 10;
            let v = [1.0, 1.0, 0.8, 0.8, 0.6, 0.6, 0.5, 0.5, 0.3, 0.3][sub as usize];
            let s = if sub.is_multiple_of(2) { 1.0 } else { 0.5 };
            hsv(hue, s, v)
        }
    }
}

fn hsv(h: f64, s: f64, v: f64) -> [u8; 3] {
    let c = v * s;
    let hp = (h.rem_euclid(360.0)) / 60.0;
    let x = c * (1.0 - (hp % 2.0 - 1.0).abs());
    let (r, g, b) = match hp as i32 {
        0 => (c, x, 0.0),
        1 => (x, c, 0.0),
        2 => (0.0, c, x),
        3 => (0.0, x, c),
        4 => (x, 0.0, c),
        _ => (c, 0.0, x),
    };
    let m = v - c;
    [((r + m) * 255.0) as u8, ((g + m) * 255.0) as u8, ((b + m) * 255.0) as u8]
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum LineType {
    #[default]
    Continuous,
    Dashed,
    Hidden,
    Center,
    Phantom,
    Dot,
}

impl LineType {
    pub const ALL: [LineType; 6] =
        [LineType::Continuous, LineType::Dashed, LineType::Hidden, LineType::Center, LineType::Phantom, LineType::Dot];

    /// Standard DXF linetype name.
    pub fn name(self) -> &'static str {
        match self {
            LineType::Continuous => "CONTINUOUS",
            LineType::Dashed => "DASHED",
            LineType::Hidden => "HIDDEN",
            LineType::Center => "CENTER",
            LineType::Phantom => "PHANTOM",
            LineType::Dot => "DOT",
        }
    }

    pub fn description(self) -> &'static str {
        match self {
            LineType::Continuous => "Solid line",
            LineType::Dashed => "Dashed __ __ __ __",
            LineType::Hidden => "Hidden _ _ _ _ _",
            LineType::Center => "Center ____ _ ____ _",
            LineType::Phantom => "Phantom ____ _ _ ____",
            LineType::Dot => "Dot . . . . . .",
        }
    }

    pub fn from_name(s: &str) -> Option<LineType> {
        let s = s.to_ascii_uppercase();
        LineType::ALL.into_iter().find(|l| s == l.name() || s.starts_with(l.name()))
    }

    /// Dash pattern in paper millimetres: positive = dash, negative = gap, 0 = dot.
    pub fn pattern(self) -> &'static [f64] {
        match self {
            LineType::Continuous => &[],
            LineType::Dashed => &[6.0, -3.0],
            LineType::Hidden => &[3.0, -1.5],
            LineType::Center => &[15.0, -3.0, 3.0, -3.0],
            LineType::Phantom => &[15.0, -3.0, 3.0, -3.0, 3.0, -3.0],
            LineType::Dot => &[0.0, -1.5],
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum LtRef {
    #[default]
    ByLayer,
    Lt(LineType),
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Layer {
    pub name: String,
    /// ACI color.
    pub color: u8,
    pub linetype: LineType,
    /// Lineweight in millimetres.
    pub lineweight: f64,
    pub visible: bool,
    pub locked: bool,
    #[serde(default = "yes")]
    pub plot: bool,
}

fn yes() -> bool {
    true
}

impl Layer {
    pub fn new(name: &str, color: u8, linetype: LineType, lineweight: f64) -> Self {
        Self { name: name.to_string(), color, linetype, lineweight, visible: true, locked: false, plot: true }
    }
}

pub const LINEWEIGHTS: [f64; 12] = [0.0, 0.13, 0.18, 0.25, 0.3, 0.35, 0.5, 0.6, 0.7, 0.8, 1.0, 1.4];

/// Dimension style; all sizes are in paper millimetres and get multiplied by
/// the sheet scale (the DXF DIMSCALE variable).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct DimStyle {
    pub text_height: f64,
    pub arrow_size: f64,
    /// Gap between the feature and the start of the extension line (DIMEXO).
    pub ext_offset: f64,
    /// Extension beyond the dimension line (DIMEXE).
    pub ext_beyond: f64,
    /// Gap between dimension line and text (DIMGAP).
    pub text_gap: f64,
    pub decimals: usize,
    /// Strip trailing zeros (DIMZIN 8).
    pub suppress_trailing_zeros: bool,
}

impl Default for DimStyle {
    fn default() -> Self {
        Self {
            text_height: 3.5,
            arrow_size: 3.0,
            ext_offset: 1.0,
            ext_beyond: 1.5,
            text_gap: 1.0,
            decimals: 2,
            suppress_trailing_zeros: true,
        }
    }
}

impl DimStyle {
    pub fn format(&self, v: f64) -> String {
        let mut s = format!("{:.*}", self.decimals, v);
        if self.suppress_trailing_zeros && s.contains('.') {
            while s.ends_with('0') {
                s.pop();
            }
            if s.ends_with('.') {
                s.pop();
            }
        }
        if s == "-0" {
            s = "0".into();
        }
        s
    }
}
