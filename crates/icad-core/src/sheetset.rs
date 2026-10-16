// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Proteen Das

//! Sheet sets: a named, ordered set of drawing sheets (a drawing's main
//! sheet or one of its layouts) with sheet numbers and titles, and project
//! values shared by every title block. Stored as `.icadset` (JSON); drawing
//! paths are kept relative to the set file.

use crate::document::Document;
use crate::geom::V2;
use crate::table::{Cell, Table};
use crate::HAlign;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct SheetRef {
    /// Drawing file, relative to the set file when possible.
    pub file: String,
    /// Layout name; `None` for the drawing's main sheet.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub layout: Option<String>,
    pub number: String,
    pub title: String,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct SheetSet {
    pub name: String,
    /// Shared title block values.
    #[serde(default)]
    pub company: String,
    #[serde(default)]
    pub project: String,
    #[serde(default)]
    pub drawn_by: String,
    #[serde(default)]
    pub checked_by: String,
    #[serde(default)]
    pub approved_by: String,
    /// Drawing numbers: prefix + sheet number (e.g. "PRJ-01-").
    #[serde(default)]
    pub number_prefix: String,
    #[serde(default)]
    pub sheets: Vec<SheetRef>,
}

impl Default for SheetSet {
    fn default() -> Self {
        Self {
            name: "New Sheet Set".into(),
            company: String::new(),
            project: String::new(),
            drawn_by: String::new(),
            checked_by: String::new(),
            approved_by: String::new(),
            number_prefix: String::new(),
            sheets: vec![],
        }
    }
}

impl SheetSet {
    pub fn load(path: &Path) -> Result<SheetSet, String> {
        let s = std::fs::read_to_string(path).map_err(|e| format!("Cannot read {}: {e}", path.display()))?;
        serde_json::from_str(&s).map_err(|e| format!("Not a sheet set: {e}"))
    }

    pub fn save(&self, path: &Path) -> Result<(), String> {
        let s = serde_json::to_string_pretty(self).map_err(|e| e.to_string())?;
        std::fs::write(path, s).map_err(|e| format!("Cannot write {}: {e}", path.display()))
    }

    /// `file` as stored for a set saved at `set_path` (relative when inside its folder).
    pub fn store_path(set_path: Option<&Path>, file: &Path) -> String {
        if let Some(dir) = set_path.and_then(|p| p.parent()) {
            if let Ok(rel) = file.strip_prefix(dir) {
                return rel.to_string_lossy().replace('\\', "/");
            }
        }
        file.to_string_lossy().into_owned()
    }

    /// Absolute path of a sheet's drawing.
    pub fn resolve(&self, set_path: Option<&Path>, sheet: &SheetRef) -> PathBuf {
        let p = PathBuf::from(&sheet.file);
        if p.is_absolute() {
            return p;
        }
        set_path.and_then(|s| s.parent()).map(|d| d.join(&p)).unwrap_or(p)
    }

    /// Number the sheets 01, 02, … in their order.
    pub fn renumber(&mut self) {
        let width = self.sheets.len().to_string().len().max(2);
        for (i, s) in self.sheets.iter_mut().enumerate() {
            s.number = format!("{:0width$}", i + 1);
        }
    }

    /// Write the set's values into sheet `index`'s title block in `doc`
    /// (the drawing's main sheet, or its layout of that name).
    pub fn apply(&self, index: usize, doc: &mut Document) {
        let Some(s) = self.sheets.get(index) else { return };
        let n = self.sheets.len();
        let sheet = match &s.layout {
            Some(name) => match doc.layouts.iter_mut().find(|l| l.name.eq_ignore_ascii_case(name)) {
                Some(l) => &mut l.sheet,
                None => return,
            },
            None => &mut doc.sheet,
        };
        let tb = &mut sheet.title;
        tb.sheet_no = format!("{} OF {n}", index + 1);
        if !s.title.trim().is_empty() {
            tb.title = s.title.to_uppercase();
        }
        if !s.number.trim().is_empty() {
            tb.drawing_no = format!("{}{}", self.number_prefix, s.number);
        }
        for (v, field) in [
            (&self.company, &mut tb.company),
            (&self.drawn_by, &mut tb.drawn_by),
            (&self.checked_by, &mut tb.checked_by),
            (&self.approved_by, &mut tb.approved_by),
        ] {
            if !v.trim().is_empty() {
                *field = v.clone();
            }
        }
    }

    /// Sheet index as a table: number and title, under the set (project) name.
    pub fn sheet_list(&self, pos: V2, text_h: f64) -> Table {
        let mut rows = vec![vec!["SHEET".to_string(), "TITLE".to_string()]];
        for s in &self.sheets {
            rows.push(vec![format!("{}{}", self.number_prefix, s.number), s.title.clone()]);
        }
        let mut t = Table::from_rows(pos, &rows, text_h);
        t.insert_row(0);
        let head = if self.project.trim().is_empty() { self.name.clone() } else { self.project.clone() };
        t.cells[0][0] = Cell { text: format!("SHEET LIST — {}", head.to_uppercase()), align: HAlign::Center, span: 1 };
        t.tag = "SHEETLIST".into();
        t.fit_columns();
        t
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::layout::Layout;

    #[test]
    fn numbers_titles_and_shared_values_reach_the_title_blocks() {
        let mut set = SheetSet {
            name: "Gearbox".into(),
            company: "ACME".into(),
            project: "Gearbox GB-40".into(),
            number_prefix: "GB-".into(),
            ..Default::default()
        };
        set.sheets.push(SheetRef { file: "housing.icad".into(), layout: None, number: String::new(), title: "Housing".into() });
        set.sheets.push(SheetRef {
            file: "shaft.icad".into(),
            layout: Some("Detail".into()),
            number: String::new(),
            title: "Input shaft".into(),
        });
        set.renumber();
        assert_eq!(set.sheets[1].number, "02");
        let mut doc = Document::new();
        let ext = doc.extents();
        doc.layouts.push(Layout::new("Detail", doc.sheet.clone(), &ext));
        set.apply(1, &mut doc);
        let tb = &doc.layouts[0].sheet.title;
        assert_eq!(
            (tb.sheet_no.as_str(), tb.title.as_str(), tb.drawing_no.as_str(), tb.company.as_str()),
            ("2 OF 2", "INPUT SHAFT", "GB-02", "ACME")
        );
        // The main sheet is untouched by a layout's sheet.
        assert_ne!(doc.sheet.title.sheet_no, "2 OF 2");
        let t = set.sheet_list(V2::ZERO, 2.5);
        assert_eq!(t.cells[3][1].text, "Input shaft");
        assert!(t.cells[0][0].text.contains("GEARBOX GB-40"));
        // Paths are kept relative to the set file.
        let set_path = Path::new("/projects/gb/gearbox.icadset");
        assert_eq!(SheetSet::store_path(Some(set_path), Path::new("/projects/gb/parts/shaft.icad")), "parts/shaft.icad");
        assert_eq!(set.resolve(Some(set_path), &set.sheets[0]), PathBuf::from("/projects/gb/housing.icad"));
    }
}
