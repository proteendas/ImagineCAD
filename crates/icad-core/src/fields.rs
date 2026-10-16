// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Proteen Das

//! Text fields: `{{NAME}}` placeholders in text, multiline text, leaders,
//! attributes and title blocks that always show the current value.
//!
//! * Drawing: `{{COMPANY}}`, `{{TITLE}}`, `{{DWG_NO}}`, `{{REV}}`, `{{SHEET}}`,
//!   `{{SIZE}}`, `{{SCALE}}`, `{{MATERIAL}}`, `{{FINISH}}`, `{{UNITS}}`,
//!   `{{DRAWN}}`, `{{CHECKED}}`, `{{APPROVED}}`, `{{DATE}}`, `{{TOLERANCE}}`,
//!   `{{PROJECTION}}`, `{{FILENAME}}`, `{{TODAY}}`
//! * Objects (by id, see LIST): `{{LENGTH:12}}`, `{{AREA:12}}`,
//!   `{{RADIUS:12}}`, `{{DIAMETER:12}}`, `{{X:12}}`, `{{Y:12}}`
//! * Counts: `{{COUNT:BLOCKNAME}}` (block references of that block)
//!
//! Unknown fields show `####`.

use crate::document::Document;
use crate::entity::Shape;

/// Names offered by the field picker: (field, description).
pub const FIELDS: &[(&str, &str)] = &[
    ("{{TITLE}}", "Drawing title"),
    ("{{COMPANY}}", "Company"),
    ("{{DWG_NO}}", "Drawing number"),
    ("{{REV}}", "Revision"),
    ("{{SHEET}}", "Sheet number"),
    ("{{SIZE}}", "Paper size"),
    ("{{SCALE}}", "Plot scale"),
    ("{{MATERIAL}}", "Material"),
    ("{{FINISH}}", "Finish"),
    ("{{DRAWN}}", "Drawn by"),
    ("{{CHECKED}}", "Checked by"),
    ("{{APPROVED}}", "Approved by"),
    ("{{DATE}}", "Drawing date"),
    ("{{TODAY}}", "Today's date"),
    ("{{FILENAME}}", "File name"),
    ("{{LENGTH:id}}", "Length of an object"),
    ("{{AREA:id}}", "Area of an object"),
    ("{{DIAMETER:id}}", "Diameter of a circle or arc"),
    ("{{COUNT:block}}", "Number of references of a block"),
];

fn value(doc: &Document, name: &str) -> Option<String> {
    let (key, arg) = match name.split_once(':') {
        Some((k, a)) => (k.trim().to_ascii_uppercase(), Some(a.trim())),
        None => (name.trim().to_ascii_uppercase(), None),
    };
    let fmt = |v: f64| doc.dimstyle.format(v);
    if let Some(arg) = arg {
        if key == "COUNT" {
            let n = doc.entities.iter().filter(|e| matches!(&e.shape, Shape::Insert(i) if i.name.eq_ignore_ascii_case(arg))).count();
            return Some(n.to_string());
        }
        let id: u64 = arg.parse().ok()?;
        let s = &doc.get(id)?.shape;
        return match key.as_str() {
            "LENGTH" | "PERIMETER" => s.length().map(fmt),
            "AREA" => s.area().map(fmt),
            "RADIUS" | "DIAMETER" => match s {
                Shape::Circle { r, .. } | Shape::Arc { r, .. } => Some(fmt(if key == "RADIUS" { *r } else { 2.0 * r })),
                _ => None,
            },
            "X" | "Y" => s.snap_points().first().map(|(_, p)| fmt(if key == "X" { p.x } else { p.y })),
            _ => None,
        };
    }
    match key.as_str() {
        "FILENAME" => Some(doc.file_name.clone()),
        "TODAY" => Some(crate::sheet::today()),
        _ => doc.sheet.field(&key),
    }
}

/// Replace every `{{NAME}}` in `s`.
pub fn resolve(doc: &Document, s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut rest = s;
    while let Some(i) = rest.find("{{") {
        out += &rest[..i];
        let after = &rest[i + 2..];
        match after.find("}}") {
            Some(j) => {
                out += &value(doc, &after[..j]).unwrap_or_else(|| "####".into());
                rest = &after[j + 2..];
            }
            None => {
                out += &rest[i..];
                rest = "";
            }
        }
    }
    out + rest
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::geom::V2;

    #[test]
    fn fields_resolve() {
        let mut doc = Document::new();
        doc.sheet.title.title = "BRACKET".into();
        doc.sheet.scale = 2.0;
        doc.file_name = "bracket.icad".into();
        let c = doc.add(Shape::Circle { c: V2::ZERO, r: 5.0 });
        doc.add(Shape::Insert(crate::entity::Insert::new("BOLT", V2::ZERO)));
        doc.add(Shape::Insert(crate::entity::Insert::new("bolt", V2::ZERO)));
        let s = resolve(
            &doc,
            &format!("{{{{TITLE}}}} {{{{SCALE}}}} Ø{{{{DIAMETER:{c}}}}} x{{{{COUNT:BOLT}}}} {{{{FILENAME}}}} {{{{NOPE}}}} {{{{open"),
        );
        assert_eq!(s, "BRACKET 1:2 Ø10 x2 bracket.icad #### {{open");
    }
}
