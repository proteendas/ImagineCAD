// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Proteen Das

//! ImagineCAD file formats.
//!
//! | Format | Read | Write | Notes |
//! |--------|------|-------|-------|
//! | `.icad` | ✓ | ✓ | Native JSON, lossless |
//! | `.dxf`  | ✓ | ✓ | R12 DXF, real DIMENSION entities |
//! | `.dwg`  | ✓ | ✓ | via the bundled LibreDWG tools (or ODA File Converter) |
//! | `.pdf`  |   | ✓ | Vector, true scale, lineweights |
//! | `.svg`  |   | ✓ | Vector, paper millimetres |

pub mod dwg;
pub mod dxf;
pub mod pdf;
pub mod plot;
pub mod svg;

use icad_core::Document;
use std::path::Path;

pub use dxf::{DxfOptions, DxfVersion};
pub use plot::PlotOptions;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Format {
    Icad,
    Dxf,
    Dwg,
    Pdf,
    Svg,
}

impl Format {
    pub fn from_path(p: &Path) -> Option<Format> {
        match p.extension()?.to_str()?.to_ascii_lowercase().as_str() {
            "icad" | "json" => Some(Format::Icad),
            "dxf" => Some(Format::Dxf),
            "dwg" => Some(Format::Dwg),
            "pdf" => Some(Format::Pdf),
            "svg" => Some(Format::Svg),
            _ => None,
        }
    }
}

#[derive(Clone, Debug, Default)]
pub struct ExportOptions {
    pub dxf: DxfOptions,
    pub plot: PlotOptions,
}

/// Open a drawing (.icad, .dxf, or .dwg when a converter is installed).
pub fn open(path: &Path) -> Result<Document, String> {
    match Format::from_path(path) {
        Some(Format::Icad) => Document::from_json(&std::fs::read_to_string(path).map_err(|e| e.to_string())?),
        Some(Format::Dxf) => {
            let bytes = std::fs::read(path).map_err(|e| e.to_string())?;
            dxf::read_dxf(&String::from_utf8_lossy(&bytes))
        }
        Some(Format::Dwg) => {
            let conv = dwg::find_converter()
                .ok_or("DWG support files are missing: reinstall ImagineCAD, or install the free ODA File Converter or LibreDWG (see docs/FILE_FORMATS.md)")?;
            dxf::read_dxf(&dwg::dwg_to_dxf(path, &conv)?)
        }
        _ => Err("Unsupported file type".into()),
    }
}

/// Save / export a drawing; the format is chosen from the extension.
pub fn save(doc: &Document, path: &Path, opts: &ExportOptions) -> Result<(), String> {
    let fmt = Format::from_path(path).ok_or("Unknown file extension")?;
    let w = |bytes: &[u8]| std::fs::write(path, bytes).map_err(|e| e.to_string());
    match fmt {
        Format::Icad => w(doc.to_json().as_bytes()),
        Format::Dxf => w(dxf::write_dxf(doc, &opts.dxf).as_bytes()),
        Format::Pdf => w(&pdf::write_pdf(doc, &opts.plot)),
        Format::Svg => w(svg::write_svg(doc, &opts.plot).as_bytes()),
        Format::Dwg => {
            let conv = dwg::find_converter().ok_or(
                "DWG support files are missing: reinstall ImagineCAD, or install the free ODA File Converter or LibreDWG (see docs/FILE_FORMATS.md). DXF export always works.",
            )?;
            // DWG is produced from DXF 2000 (LibreDWG cannot import R12).
            let dopts = DxfOptions { version: DxfVersion::R2000, ..opts.dxf.clone() };
            dwg::dxf_to_dwg(&dxf::write_dxf(doc, &dopts), path, &conv)
        }
    }
}
