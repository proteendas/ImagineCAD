// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Proteen Das

//! One open drawing: document, view, selection and undo history.

use crate::view::View;
use icad_core::Document;
use std::path::PathBuf;

const UNDO_LIMIT: usize = 200;

/// Constrained points (and radii) as last solved, with their signature.
pub type ConsState = (String, Vec<(icad_core::constraints::PtRef, icad_core::geom::V2, f64)>);

pub struct Tab {
    pub doc: Document,
    pub view: View,
    pub selection: Vec<u64>,
    pub undo_stack: Vec<Document>,
    pub redo_stack: Vec<Document>,
    pub path: Option<PathBuf>,
    pub dirty: bool,
    pub name: String,
    /// Show the paper (layout) instead of model space.
    pub layout: bool,
    /// Frames left in which to fit the view (panel sizes settle over the first frames).
    pub needs_fit: u8,
    /// Last crossing window used for selection (STRETCH).
    pub last_window: Option<icad_core::geom::BBox>,
    /// Previous views for ZOOM Previous.
    pub view_history: Vec<(icad_core::geom::V2, f64)>,
    /// Paper-space layout shown (index into `doc.layouts`), its selected
    /// viewport, and the paper view (millimetres).
    pub paper: Option<usize>,
    pub paper_vp: Option<usize>,
    pub paper_view: View,
    pub paper_fit: bool,
    /// Constrained geometry as last solved (signature, points), to solve again after edits.
    pub cons_state: Option<ConsState>,
}

impl Tab {
    pub fn new(doc: Document, name: impl Into<String>) -> Self {
        Self {
            doc,
            view: View::default(),
            selection: vec![],
            undo_stack: vec![],
            redo_stack: vec![],
            path: None,
            dirty: false,
            name: name.into(),
            layout: false,
            needs_fit: 3,
            last_window: None,
            view_history: vec![],
            paper: None,
            paper_vp: None,
            paper_view: View::default(),
            paper_fit: true,
            cons_state: None,
        }
    }

    /// Record the current state before a modification.
    pub fn checkpoint(&mut self) {
        self.undo_stack.push(self.doc.clone());
        if self.undo_stack.len() > UNDO_LIMIT {
            self.undo_stack.remove(0);
        }
        self.redo_stack.clear();
        self.dirty = true;
    }

    pub fn undo(&mut self) -> bool {
        match self.undo_stack.pop() {
            Some(prev) => {
                self.redo_stack.push(std::mem::replace(&mut self.doc, prev));
                self.selection.retain(|id| self.doc.get(*id).is_some());
                self.dirty = true;
                true
            }
            None => false,
        }
    }

    pub fn redo(&mut self) -> bool {
        match self.redo_stack.pop() {
            Some(next) => {
                self.undo_stack.push(std::mem::replace(&mut self.doc, next));
                self.selection.retain(|id| self.doc.get(*id).is_some());
                self.dirty = true;
                true
            }
            None => false,
        }
    }

    pub fn title(&self) -> String {
        format!("{}{}", self.name, if self.dirty { "*" } else { "" })
    }

    /// Entities that may be edited (visible and on unlocked layers).
    pub fn editable(&self, id: u64) -> bool {
        self.doc.get(id).is_some_and(|e| self.doc.is_visible(e) && !self.doc.is_locked(e))
    }
}
