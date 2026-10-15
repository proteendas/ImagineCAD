// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Proteen Das

//! User settings persisted as JSON in the platform config directory.

use icad_vision::ConvertOptions;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct OsnapModes {
    pub end: bool,
    pub mid: bool,
    pub center: bool,
    pub quadrant: bool,
    pub intersection: bool,
    pub node: bool,
    pub perpendicular: bool,
    pub nearest: bool,
}

impl Default for OsnapModes {
    fn default() -> Self {
        Self { end: true, mid: true, center: true, quadrant: true, intersection: true, node: true, perpendicular: true, nearest: false }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    pub convert: ConvertOptions,
    pub company: String,
    pub drawn_by: String,
    pub recent: Vec<PathBuf>,
    pub grid: bool,
    pub snap: bool,
    pub snap_spacing: f64,
    pub ortho: bool,
    pub osnap: bool,
    pub osnap_modes: OsnapModes,
    pub lineweights: bool,
    pub dyn_input: bool,
    pub show_properties: bool,
    pub monochrome_plot: bool,
    pub explode_dims_dxf: bool,
    /// Write DXF R12 instead of DXF 2000.
    pub dxf_r12: bool,
    pub polar: bool,
    pub polar_angle: f64,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            convert: ConvertOptions::default(),
            company: String::new(),
            drawn_by: String::new(),
            recent: vec![],
            grid: true,
            snap: false,
            snap_spacing: 5.0,
            ortho: false,
            osnap: true,
            osnap_modes: OsnapModes::default(),
            lineweights: true,
            dyn_input: true,
            show_properties: true,
            monochrome_plot: true,
            explode_dims_dxf: false,
            dxf_r12: false,
            polar: true,
            polar_angle: 15.0,
        }
    }
}

impl Settings {
    fn path() -> Option<PathBuf> {
        Some(dirs::config_dir()?.join("ImagineCAD").join("settings.json"))
    }

    pub fn load() -> Self {
        Self::path().and_then(|p| std::fs::read_to_string(p).ok()).and_then(|s| serde_json::from_str(&s).ok()).unwrap_or_default()
    }

    pub fn save(&self) {
        if let Some(p) = Self::path() {
            if let Some(dir) = p.parent() {
                let _ = std::fs::create_dir_all(dir);
            }
            let _ = std::fs::write(p, serde_json::to_string_pretty(self).unwrap_or_default());
        }
    }

    pub fn push_recent(&mut self, p: PathBuf) {
        self.recent.retain(|r| r != &p);
        self.recent.insert(0, p);
        self.recent.truncate(10);
    }
}
