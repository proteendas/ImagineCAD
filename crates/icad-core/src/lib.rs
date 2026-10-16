// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Proteen Das

//! ImagineCAD core: geometry, the entity model, layers, the drawing sheet and
//! editing operations. This crate has no GUI or I/O dependencies.

pub mod arrange;
pub mod assoc;
pub mod constraints;
pub mod curves;
pub mod document;
pub mod dynblock;
pub mod entity;
pub mod fields;
pub mod fonts;
pub mod geom;
pub mod hatch;
pub mod layout;
pub mod markup;
pub mod ops;
pub mod prim;
pub mod region;
pub mod sheet;
pub mod sheetset;
pub mod style;
pub mod symbols;
pub mod table;
pub mod views;
pub mod xref;

pub use document::Document;
pub use entity::{BlockDef, Ctx, Curve, DimKind, Dimension, Entity, Hatch, Insert, Leader, MText, Shape, SnapKind};
pub use geom::{BBox, Xform, V2};
pub use prim::{HAlign, ImageRef, Prim, Text, VAlign};
pub use sheet::{PaperSize, Projection, Sheet, TitleBlock};
pub use style::{Color, DimStyle, Layer, LineType, LtRef};
