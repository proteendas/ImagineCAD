// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Proteen Das

//! ImagineCAD sketch-to-CAD conversion.
pub mod assistant;
pub mod beautify;
pub mod fit;
pub mod labels;
pub mod llm;
pub mod marks;
pub mod ocr;
pub mod parts;
pub mod pipeline;
pub mod providers;
pub mod raster;
pub mod refine;
pub mod solve;
pub mod trace;

pub use pipeline::{apply, convert, overlay, Conversion, ConvertOptions};
