// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Proteen Das

//! Dimension labels read from the sketch (by OCR or a vision-language model).

use icad_core::geom::*;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LabelKind {
    Linear,
    Diameter,
    Radius,
    Angle,
}

#[derive(Clone, Debug)]
pub struct Label {
    pub text: String,
    pub kind: LabelKind,
    pub value: f64,
    /// Bounding box in y-up pixel coordinates of the processed image.
    pub bbox: BBox,
    pub source: &'static str,
}

impl Label {
    pub fn center(&self) -> V2 {
        self.bbox.center()
    }
    pub fn size(&self) -> f64 {
        self.bbox.width().max(self.bbox.height())
    }
}

/// Parse dimension text such as `50`, `12.5`, `Ø20`, `2x Ø8`, `R5`, `M8`,
/// `45°`, `50 mm`, `25±0.1`. Returns (kind, value).
pub fn parse(text: &str) -> Option<(LabelKind, f64)> {
    let mut s = text.trim().to_string();
    if s.is_empty() {
        return None;
    }
    // Normalise common OCR confusions and symbols.
    for (from, to) in [
        ("⌀", "Ø"),
        ("∅", "Ø"),
        ("ø", "Ø"),
        ("%%c", "Ø"),
        ("%%C", "Ø"),
        ("DIA.", "Ø"),
        ("DIA", "Ø"),
        ("dia", "Ø"),
        ("%%d", "°"),
        (",", "."),
        ("mm", ""),
        ("MM", ""),
    ] {
        s = s.replace(from, to);
    }
    // Drop multiplicity prefixes like "2x", "4 X", "3 HOLES".
    let lower = s.to_lowercase();
    let s = if let Some(pos) = lower.find(['x', '×']) {
        let head = lower[..pos].trim();
        if !head.is_empty() && head.chars().all(|c| c.is_ascii_digit()) {
            s[pos + lower[pos..].chars().next().unwrap().len_utf8()..].trim().to_string()
        } else {
            s
        }
    } else {
        s
    };
    // Strip tolerances.
    let s = s.split(['±', '+']).next().unwrap_or("").trim().to_string();
    let (kind, rest) = if let Some(r) = s.strip_prefix('Ø') {
        (LabelKind::Diameter, r)
    } else if let Some(r) = s.strip_prefix(['R', 'r']) {
        (LabelKind::Radius, r)
    } else if let Some(r) = s.strip_prefix(['M']) {
        (LabelKind::Diameter, r)
    } else if s.ends_with('°') {
        (LabelKind::Angle, s.trim_end_matches('°'))
    } else if let Some(r) = s.strip_prefix(['O', 'o', '0', 'Q']).filter(|r| r.starts_with(' ') && !r.trim().is_empty()) {
        // "O 20" style hand-written diameter.
        (LabelKind::Diameter, r)
    } else {
        (LabelKind::Linear, s.as_str())
    };
    let num: String = rest
        .trim()
        .chars()
        .map(|c| match c {
            'O' | 'o' => '0',
            'l' | 'I' | '|' => '1',
            'S' | 's' => '5',
            'B' => '8',
            c => c,
        })
        .take_while(|c| c.is_ascii_digit() || *c == '.')
        .collect();
    let v: f64 = num.trim_end_matches('.').parse().ok()?;
    (v > 0.0 && v < 1e6).then_some((kind, v))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_common_forms() {
        assert_eq!(parse("50"), Some((LabelKind::Linear, 50.0)));
        assert_eq!(parse("12,5 mm"), Some((LabelKind::Linear, 12.5)));
        assert_eq!(parse("Ø20"), Some((LabelKind::Diameter, 20.0)));
        assert_eq!(parse("2x ø8"), Some((LabelKind::Diameter, 8.0)));
        assert_eq!(parse("R5"), Some((LabelKind::Radius, 5.0)));
        assert_eq!(parse("45°"), Some((LabelKind::Angle, 45.0)));
        assert_eq!(parse("25±0.1"), Some((LabelKind::Linear, 25.0)));
        assert_eq!(parse("M8"), Some((LabelKind::Diameter, 8.0)));
        assert_eq!(parse("5O"), Some((LabelKind::Linear, 50.0)));
        assert_eq!(parse("hello"), None);
    }
}
