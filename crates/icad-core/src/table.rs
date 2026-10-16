// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Proteen Das

//! Tables (parts lists, revision tables, notes) and multileader styles.

use crate::geom::*;
use crate::prim::*;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Cell {
    pub text: String,
    #[serde(default = "center")]
    pub align: HAlign,
    /// Extra columns this cell spans to the right (0 = none).
    #[serde(default)]
    pub span: usize,
}

fn center() -> HAlign {
    HAlign::Center
}

impl Cell {
    pub fn new(text: impl Into<String>) -> Self {
        Self { text: text.into(), align: HAlign::Center, span: 0 }
    }
}

/// A table placed by its top-left corner; sizes in drawing units.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Table {
    pub pos: V2,
    pub col_widths: Vec<f64>,
    pub row_heights: Vec<f64>,
    /// `rows × columns` cells (cells covered by a span are ignored).
    pub cells: Vec<Vec<Cell>>,
    pub text_height: f64,
    /// What generated the table ("BOM" for a bill of materials), so it can
    /// be refreshed in place.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub tag: String,
}

impl Table {
    /// A table of `rows × cols` with uniform sizes.
    pub fn new(pos: V2, rows: usize, cols: usize, col_w: f64, row_h: f64, text_h: f64) -> Self {
        Self {
            pos,
            col_widths: vec![col_w; cols.max(1)],
            row_heights: vec![row_h; rows.max(1)],
            cells: vec![vec![Cell::default(); cols.max(1)]; rows.max(1)],
            text_height: text_h,
            tag: String::new(),
        }
    }

    /// From rows of text (e.g. CSV); column widths fit the text.
    pub fn from_rows(pos: V2, rows: &[Vec<String>], text_h: f64) -> Self {
        let cols = rows.iter().map(|r| r.len()).max().unwrap_or(1).max(1);
        let mut t = Table::new(pos, rows.len().max(1), cols, text_h * 10.0, text_h * 2.2, text_h);
        for (r, row) in rows.iter().enumerate() {
            for (c, s) in row.iter().enumerate() {
                t.cells[r][c] = Cell::new(s.clone());
            }
        }
        t.fit_columns();
        t
    }

    /// Widen every column to its longest text.
    pub fn fit_columns(&mut self) {
        let h = self.text_height;
        for c in 0..self.cols() {
            let w =
                self.cells.iter().filter(|r| r.get(c).is_some_and(|x| x.span == 0)).map(|r| text_width(&r[c].text, h)).fold(0.0, f64::max);
            self.col_widths[c] = (w + h * 1.6).max(h * 4.0);
        }
    }

    pub fn rows(&self) -> usize {
        self.row_heights.len()
    }

    pub fn cols(&self) -> usize {
        self.col_widths.len()
    }

    pub fn width(&self) -> f64 {
        self.col_widths.iter().sum()
    }

    pub fn height(&self) -> f64 {
        self.row_heights.iter().sum()
    }

    /// Keep `cells` in step with the row and column counts.
    pub fn normalize(&mut self) {
        let (r, c) = (self.rows(), self.cols());
        self.cells.resize_with(r, Vec::new);
        for row in &mut self.cells {
            row.resize_with(c, Cell::default);
            for (i, cell) in row.iter_mut().enumerate() {
                cell.span = cell.span.min(c - 1 - i);
            }
        }
    }

    pub fn insert_row(&mut self, at: usize) {
        let h = self.row_heights.get(at.min(self.rows().saturating_sub(1))).copied().unwrap_or(self.text_height * 2.2);
        let at = at.min(self.rows());
        self.row_heights.insert(at, h);
        self.cells.insert(at, vec![Cell::default(); self.cols()]);
    }

    pub fn remove_row(&mut self, at: usize) {
        if self.rows() > 1 && at < self.rows() {
            self.row_heights.remove(at);
            self.cells.remove(at);
        }
    }

    pub fn insert_col(&mut self, at: usize) {
        let w = self.col_widths.get(at.min(self.cols().saturating_sub(1))).copied().unwrap_or(self.text_height * 10.0);
        let at = at.min(self.cols());
        self.col_widths.insert(at, w);
        for row in &mut self.cells {
            row.insert(at, Cell::default());
        }
        self.normalize();
    }

    pub fn remove_col(&mut self, at: usize) {
        if self.cols() > 1 && at < self.cols() {
            self.col_widths.remove(at);
            for row in &mut self.cells {
                row.remove(at);
            }
            self.normalize();
        }
    }

    pub fn prims(&self) -> Vec<Prim> {
        let mut out = vec![];
        let (w, h) = (self.width(), self.height());
        let p0 = self.pos;
        out.push(Prim::Poly(vec![p0, p0 + V2::new(w, 0.0), p0 + V2::new(w, -h), p0 + V2::new(0.0, -h)], true));
        let mut y = p0.y;
        for (r, rh) in self.row_heights.iter().enumerate() {
            let y1 = y - rh;
            if r + 1 < self.rows() {
                out.push(Prim::Line(V2::new(p0.x, y1), V2::new(p0.x + w, y1)));
            }
            let mut x = p0.x;
            let mut c = 0;
            while c < self.cols() {
                let cell = &self.cells[r][c];
                let span = cell.span.min(self.cols() - 1 - c);
                let cw: f64 = self.col_widths[c..=c + span].iter().sum();
                if c + span + 1 < self.cols() {
                    out.push(Prim::Line(V2::new(x + cw, y), V2::new(x + cw, y1)));
                }
                if !cell.text.is_empty() {
                    let pad = self.text_height * 0.6;
                    let tx = match cell.align {
                        HAlign::Left => x + pad,
                        HAlign::Center => x + cw / 2.0,
                        HAlign::Right => x + cw - pad,
                    };
                    for (i, line) in cell.text.lines().enumerate() {
                        let n = cell.text.lines().count().max(1) as f64;
                        let ty = (y + y1) / 2.0 + (n - 1.0) / 2.0 * self.text_height * 1.6 - i as f64 * self.text_height * 1.6;
                        out.push(Prim::Text(Text::new(V2::new(tx, ty), self.text_height, line).aligned(cell.align, VAlign::Middle)));
                    }
                }
                x += cw;
                c += span + 1;
            }
            y = y1;
        }
        out
    }
}

/// Parse CSV (or tab-separated) text into rows.
pub fn parse_csv(s: &str) -> Vec<Vec<String>> {
    let sep = if s.lines().next().is_some_and(|l| l.contains('\t') && !l.contains(',')) { '\t' } else { ',' };
    let mut rows = vec![];
    let mut row = vec![];
    let mut field = String::new();
    let mut quoted = false;
    let mut chars = s.chars().peekable();
    while let Some(c) = chars.next() {
        match (c, quoted) {
            ('"', true) if chars.peek() == Some(&'"') => {
                chars.next();
                field.push('"');
            }
            ('"', true) => quoted = false,
            ('"', false) if field.is_empty() => quoted = true,
            (c, false) if c == sep => row.push(std::mem::take(&mut field)),
            ('\r', false) => {}
            ('\n', false) => {
                row.push(std::mem::take(&mut field));
                rows.push(std::mem::take(&mut row));
            }
            (c, _) => field.push(c),
        }
    }
    if !field.is_empty() || !row.is_empty() {
        row.push(field);
        rows.push(row);
    }
    rows.retain(|r| r.iter().any(|f| !f.trim().is_empty()));
    rows
}

// ------------------------------------------------------------------ multileader styles

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum ArrowKind {
    #[default]
    ClosedFilled,
    Open,
    Dot,
    /// Architectural tick / oblique stroke.
    Tick,
    None,
}

impl ArrowKind {
    pub const ALL: [ArrowKind; 5] = [ArrowKind::ClosedFilled, ArrowKind::Open, ArrowKind::Dot, ArrowKind::Tick, ArrowKind::None];
    pub fn name(self) -> &'static str {
        match self {
            ArrowKind::ClosedFilled => "Closed filled",
            ArrowKind::Open => "Open",
            ArrowKind::Dot => "Dot",
            ArrowKind::Tick => "Tick",
            ArrowKind::None => "None",
        }
    }
    /// The arrowhead at `tip`, pointing along `dir`, of `size`.
    pub fn prims(self, tip: V2, dir: V2, size: f64) -> Vec<Prim> {
        let d = dir.norm();
        let n = d.perp();
        let back = tip - d * size;
        match self {
            ArrowKind::ClosedFilled => vec![Prim::Fill(vec![tip, back + n * (size / 6.0), back - n * (size / 6.0)])],
            ArrowKind::Open => vec![Prim::Poly(vec![back + n * (size / 4.0), tip, back - n * (size / 4.0)], false)],
            ArrowKind::Dot => {
                let r = size * 0.25;
                let pts = arc_points(tip, r, 0.0, std::f64::consts::TAU, r * 0.05);
                vec![Prim::Fill(pts)]
            }
            ArrowKind::Tick => {
                let t = (d + n).norm() * (size * 0.5);
                vec![Prim::Line(tip - t, tip + t)]
            }
            ArrowKind::None => vec![],
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum TextFrame {
    #[default]
    None,
    Box,
    /// Balloon (item numbers).
    Circle,
}

/// Look of multileaders, in paper millimetres.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct MLeaderStyle {
    pub name: String,
    pub arrow: ArrowKind,
    pub arrow_size: f64,
    pub text_height: f64,
    /// Horizontal landing before the text.
    pub landing: bool,
    pub landing_len: f64,
    pub frame: TextFrame,
}

impl MLeaderStyle {
    pub fn standard() -> Self {
        Self {
            name: "Standard".into(),
            arrow: ArrowKind::ClosedFilled,
            arrow_size: 3.0,
            text_height: 3.5,
            landing: true,
            landing_len: 3.0,
            frame: TextFrame::None,
        }
    }
    pub fn balloon() -> Self {
        Self {
            name: "Balloon".into(),
            arrow: ArrowKind::Dot,
            arrow_size: 3.0,
            text_height: 3.5,
            landing: false,
            landing_len: 0.0,
            frame: TextFrame::Circle,
        }
    }

    /// Leader primitives: `pts` tip first; `s` = annotation scale.
    pub fn prims(&self, pts: &[V2], text: &str, s: f64, gap: f64) -> Vec<Prim> {
        let mut out = vec![];
        if pts.len() < 2 {
            return out;
        }
        let th = self.text_height * s;
        let last = pts[pts.len() - 1];
        let prev = pts[pts.len() - 2];
        let sx = if last.x >= prev.x { 1.0 } else { -1.0 };
        let land = if self.landing { last + V2::new(sx * self.landing_len * s, 0.0) } else { last };
        let lines: Vec<&str> = text.lines().collect();
        let tw = lines.iter().map(|l| text_width(l, th)).fold(0.0, f64::max);
        let pitch = th * 1.667;
        let n = lines.len().max(1) as f64;
        let block_h = (n - 1.0) * pitch + th;
        // Frame and where the leader meets it.
        let (centre, end) = match self.frame {
            TextFrame::Circle => {
                let r = (tw.max(th) / 2.0 + th * 0.6).max(block_h / 2.0 + th * 0.4);
                let c = land + V2::new(sx * r, 0.0);
                out.push(Prim::Circle(c, r));
                (c, land)
            }
            TextFrame::Box => {
                let m = th * 0.5;
                let c = land + V2::new(sx * (gap * s + tw / 2.0 + m), 0.0);
                let half = V2::new(tw / 2.0 + m, block_h / 2.0 + m);
                out.push(Prim::Poly(
                    vec![c - half, V2::new(c.x + half.x, c.y - half.y), c + half, V2::new(c.x - half.x, c.y + half.y)],
                    true,
                ));
                (c, land)
            }
            TextFrame::None => (land + V2::new(sx * (gap * s + tw / 2.0), 0.0), land),
        };
        let mut line = pts.to_vec();
        if self.landing && end.dist(last) > 1e-12 {
            line.push(end);
        }
        out.push(Prim::Poly(line, false));
        out.extend(self.arrow.prims(pts[0], pts[0] - pts[1], self.arrow_size * s));
        for (i, l) in lines.iter().enumerate() {
            let y = centre.y + block_h / 2.0 - th / 2.0 - i as f64 * pitch;
            out.push(Prim::Text(Text::new(V2::new(centre.x, y), th, *l).aligned(HAlign::Center, VAlign::Middle)));
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn table_layout_spans_and_csv() {
        let rows = parse_csv("ITEM,PART,\"DESCRIPTION, LONG\",QTY\n1,ICP-1,\"Shaft \"\"main\"\"\",2\n\n2,ICP-2,Key,4\n");
        assert_eq!(rows.len(), 3);
        assert_eq!(rows[1][2], "Shaft \"main\"");
        let mut t = Table::from_rows(V2::new(0.0, 0.0), &rows, 2.5);
        assert_eq!((t.rows(), t.cols()), (3, 4));
        assert!(t.col_widths[2] > t.col_widths[0]);
        t.insert_row(0);
        t.cells[0][0] = Cell { text: "PARTS LIST".into(), align: HAlign::Center, span: 3 };
        let p = t.prims();
        // The title row spans all columns: no inner vertical line in it.
        let top = t.pos.y;
        let first = top - t.row_heights[0];
        assert!(!p.iter().any(|x| matches!(x, Prim::Line(a, b) if (a.x - b.x).abs() < 1e-9 && a.y == top && b.y == first)));
        assert!(p.iter().any(|x| matches!(x, Prim::Text(tx) if tx.content == "PARTS LIST")));
        t.remove_col(3);
        assert_eq!(t.cells[0][0].span, 2);
    }

    #[test]
    fn leader_styles() {
        let pts = [V2::ZERO, V2::new(10.0, 10.0)];
        let std = MLeaderStyle::standard().prims(&pts, "NOTE", 1.0, 1.0);
        assert!(std.iter().any(|p| matches!(p, Prim::Fill(_))));
        let b = MLeaderStyle::balloon().prims(&pts, "3", 1.0, 1.0);
        assert!(b.iter().any(|p| matches!(p, Prim::Circle(..))));
    }
}
