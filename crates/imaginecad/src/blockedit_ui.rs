// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Proteen Das

//! The block editor (BEDIT … BCLOSE): a block definition opened in its own
//! tab, where its geometry is edited and dynamic parameters are added —
//! BSTRETCH, BFLIP, BVSTATE (visibility states) and BLOOKUP (lookup table).

use crate::app::App;
use crate::canvas::{prim_shapes, Style};
use crate::commands::{fmt, num, Command, Cx, Flow, Want};
use crate::tab::Tab;
use crate::theme;
use crate::view::View;
use eframe::egui::{self, Align2, Color32, FontId, Painter, RichText};
use icad_core::dynblock::DynParam;
use icad_core::geom::*;
use icad_core::prim::Prim;
use icad_core::Document;

/// What the block editor tab is editing.
#[derive(Clone, Debug)]
pub struct BlockEdit {
    pub name: String,
    pub params: Vec<DynParam>,
    /// The drawing (tab) the block belongs to.
    pub parent: u64,
}

const PARAM: Color32 = Color32::from_rgb(255, 170, 60);

fn next_name(params: &[DynParam], stem: &str) -> String {
    let mut k = 1;
    while params.iter().any(|p| p.name().eq_ignore_ascii_case(&format!("{stem}{k}"))) {
        k += 1;
    }
    format!("{stem}{k}")
}

fn editing<'a>(cx: &'a mut Cx) -> Option<&'a mut BlockEdit> {
    cx.tab.block_edit.as_mut()
}

// ------------------------------------------------------------------ BSTRETCH

#[derive(Default)]
pub struct BStretchCmd {
    pts: Vec<V2>,
}

impl Command for BStretchCmd {
    fn name(&self) -> &'static str {
        "BSTRETCH"
    }
    fn prompt(&self, _: &Cx) -> String {
        match self.pts.len() {
            0 => "Specify base point of the stretch parameter:".into(),
            1 => "Specify end point (direction and default distance):".into(),
            2 => "Specify first corner of the stretch frame:".into(),
            3 => "Specify opposite corner of the stretch frame:".into(),
            _ => "Select objects to stretch:".into(),
        }
    }
    fn wants(&self) -> Want {
        if self.pts.len() < 4 {
            Want::Point
        } else {
            Want::Select
        }
    }
    fn base(&self) -> Option<V2> {
        self.pts.last().copied()
    }
    fn start(&mut self, cx: &mut Cx) -> Flow {
        if cx.tab.block_edit.is_none() {
            cx.msg("Dynamic parameters are added in the block editor: BEDIT a block first.");
            return Flow::Done;
        }
        Flow::Continue
    }
    fn point(&mut self, cx: &mut Cx, p: V2) -> Flow {
        if self.pts.len() == 1 && p.dist(self.pts[0]) < 1e-9 {
            cx.msg("The end point must differ from the base point.");
            return Flow::Continue;
        }
        self.pts.push(p);
        Flow::Continue
    }
    fn selected(&mut self, cx: &mut Cx, ids: Vec<u64>) -> Flow {
        if ids.is_empty() {
            cx.msg("Select the objects the parameter stretches.");
            return Flow::Continue;
        }
        let (base, end) = (self.pts[0], self.pts[1]);
        let frame = BBox::from_pts(self.pts[2], self.pts[3]);
        let Some(be) = editing(cx) else { return Flow::Done };
        let name = next_name(&be.params, "Distance");
        be.params.push(DynParam::Stretch {
            name: name.clone(),
            base,
            dir: (end - base).norm(),
            default: base.dist(end),
            min: None,
            max: None,
            frame,
            ids,
        });
        cx.msg(format!("Stretch parameter {name} added (default {}).", fmt(base.dist(end))));
        Flow::Done
    }
}

// ------------------------------------------------------------------ BFLIP

#[derive(Default)]
pub struct BFlipCmd {
    pts: Vec<V2>,
}

impl Command for BFlipCmd {
    fn name(&self) -> &'static str {
        "BFLIP"
    }
    fn prompt(&self, _: &Cx) -> String {
        match self.pts.len() {
            0 => "Specify first point of the flip axis:".into(),
            1 => "Specify second point of the flip axis:".into(),
            _ => "Select objects to flip:".into(),
        }
    }
    fn wants(&self) -> Want {
        if self.pts.len() < 2 {
            Want::Point
        } else {
            Want::Select
        }
    }
    fn base(&self) -> Option<V2> {
        self.pts.last().copied()
    }
    fn start(&mut self, cx: &mut Cx) -> Flow {
        if cx.tab.block_edit.is_none() {
            cx.msg("Dynamic parameters are added in the block editor: BEDIT a block first.");
            return Flow::Done;
        }
        Flow::Continue
    }
    fn point(&mut self, _: &mut Cx, p: V2) -> Flow {
        self.pts.push(p);
        Flow::Continue
    }
    fn selected(&mut self, cx: &mut Cx, ids: Vec<u64>) -> Flow {
        let (a, b) = (self.pts[0], self.pts[1]);
        let Some(be) = editing(cx) else { return Flow::Done };
        let name = next_name(&be.params, "Flip");
        be.params.push(DynParam::Flip { name: name.clone(), a, b, ids });
        cx.msg(format!("Flip parameter {name} added."));
        Flow::Done
    }
}

// ------------------------------------------------------------------ BVSTATE

#[derive(Default)]
pub struct BVStateCmd {
    state: Option<String>,
}

impl Command for BVStateCmd {
    fn name(&self) -> &'static str {
        "BVSTATE"
    }
    fn prompt(&self, _: &Cx) -> String {
        match &self.state {
            None => "Visibility state name:".into(),
            Some(s) => format!("Select the objects shown in state {s}:"),
        }
    }
    fn wants(&self) -> Want {
        if self.state.is_none() {
            Want::Text
        } else {
            Want::Select
        }
    }
    fn start(&mut self, cx: &mut Cx) -> Flow {
        if cx.tab.block_edit.is_none() {
            cx.msg("Dynamic parameters are added in the block editor: BEDIT a block first.");
            return Flow::Done;
        }
        Flow::Continue
    }
    fn value(&mut self, cx: &mut Cx, s: &str) -> Flow {
        if s.trim().is_empty() {
            cx.msg("Name the state.");
        } else {
            self.state = Some(s.trim().to_string());
        }
        Flow::Continue
    }
    fn selected(&mut self, cx: &mut Cx, ids: Vec<u64>) -> Flow {
        let state = self.state.clone().unwrap_or_default();
        let Some(be) = editing(cx) else { return Flow::Done };
        match be.params.iter_mut().find_map(|p| if let DynParam::Visibility { states, .. } = p { Some(states) } else { None }) {
            Some(states) => match states.iter_mut().find(|s| s.0.eq_ignore_ascii_case(&state)) {
                Some(s) => s.1 = ids,
                None => states.push((state.clone(), ids)),
            },
            None => be.params.push(DynParam::Visibility { name: "Visibility".into(), states: vec![(state.clone(), ids)] }),
        }
        cx.msg(format!("Visibility state {state} saved."));
        Flow::Done
    }
}

pub fn create(name: &str) -> Option<Box<dyn Command>> {
    match name {
        "BSTRETCH" | "BPSTRETCH" => Some(Box::new(BStretchCmd::default())),
        "BFLIP" | "BPFLIP" => Some(Box::new(BFlipCmd::default())),
        "BVSTATE" | "BVISIBILITY" => Some(Box::new(BVStateCmd::default())),
        _ => None,
    }
}

// ------------------------------------------------------------------ display

/// Parameter markers in the block editor (not part of the block).
pub fn draw_params(painter: &Painter, view: &View, t: &Tab) {
    let Some(be) = &t.block_edit else { return };
    let st = Style { color: PARAM, width: 1.2, dash: vec![view.px(4.0), -view.px(3.0)] };
    let solid = Style { color: PARAM, width: 1.5, dash: vec![] };
    let mut out = vec![];
    let label = |at: V2, s: &str| painter.text(view.to_screen(at), Align2::LEFT_BOTTOM, s, FontId::proportional(12.0), PARAM);
    for p in &be.params {
        match p {
            DynParam::Stretch { name, base, dir, default, frame, .. } => {
                let end = *base + *dir * *default;
                prim_shapes(view, &Prim::Line(*base, end), &solid, &mut out);
                let c = [frame.min, V2::new(frame.max.x, frame.min.y), frame.max, V2::new(frame.min.x, frame.max.y)];
                for i in 0..4 {
                    prim_shapes(view, &Prim::Line(c[i], c[(i + 1) % 4]), &st, &mut out);
                }
                label(end, &format!(" {name} = {}", fmt(*default)));
            }
            DynParam::Flip { name, a, b, .. } => {
                prim_shapes(view, &Prim::Line(*a, *b), &st, &mut out);
                label(*b, &format!(" {name}"));
            }
            DynParam::Visibility { name, states } => {
                let names: Vec<&str> = states.iter().map(|s| s.0.as_str()).collect();
                let at = t.doc.extents();
                if !at.is_empty() {
                    label(V2::new(at.min.x, at.max.y), &format!("{name}: {}", names.join(" | ")));
                }
            }
            DynParam::Lookup { name, rows, .. } => {
                let at = t.doc.extents();
                if !at.is_empty() {
                    label(V2::new(at.min.x, at.min.y), &format!("{name}: {} row(s)", rows.len()));
                }
            }
        }
    }
    painter.extend(out);
}

// ------------------------------------------------------------------ app

impl App {
    /// BEDIT: open block `name` of the active drawing in a block editor tab.
    pub fn block_edit(&mut self, name: &str) {
        let parent = &self.tabs[self.active];
        if parent.block_edit.is_some() {
            self.log.push("Close this block editor first (BCLOSE).".into());
            return;
        }
        let Some(b) = parent.doc.block(name).cloned() else {
            let names: Vec<String> =
                parent.doc.blocks.iter().filter(|b| !b.name.starts_with('*') && b.xref.is_none()).map(|b| b.name.clone()).collect();
            self.log.push(format!("No block {name:?}. Blocks: {}", if names.is_empty() { "(none)".into() } else { names.join(", ") }));
            return;
        };
        if b.xref.is_some() {
            self.log.push("An external reference is edited in its own drawing (XREF → Open).".into());
            return;
        }
        let mut doc: Document = parent.doc.clone();
        doc.entities = b.entities.clone();
        doc.constraints.clear();
        doc.layouts.clear();
        doc.sheet.visible = false;
        doc.next_id = doc.entities.iter().map(|e| e.id + 1).max().unwrap_or(1).max(doc.next_id);
        let mut t = Tab::new(doc, format!("Block: {}", b.name));
        t.block_edit = Some(BlockEdit { name: b.name.clone(), params: b.params.clone(), parent: parent.uid });
        self.push_tab(t);
        self.log.push(format!(
            "Editing block {}. Add parameters with BSTRETCH, BFLIP, BVSTATE, BLOOKUP; BCLOSE saves and returns ({} parameter(s)).",
            b.name,
            b.params.len()
        ));
    }

    /// BCLOSE / BSAVE: write the edited block back into its drawing.
    pub fn block_save(&mut self, close: bool) {
        let Some(be) = self.tabs[self.active].block_edit.clone() else {
            self.log.push("Not in the block editor.".into());
            return;
        };
        let Some(pi) = self.tabs.iter().position(|t| t.uid == be.parent) else {
            self.error("The drawing this block belongs to was closed; the block was not saved.".into());
            return;
        };
        let entities = self.tabs[self.active].doc.entities.clone();
        let parent = &mut self.tabs[pi];
        parent.checkpoint();
        if let Some(b) = parent.doc.blocks.iter_mut().find(|b| b.name.eq_ignore_ascii_case(&be.name)) {
            b.entities = entities;
            // Parameters referring to objects that no longer exist are dropped.
            let ids: Vec<u64> = b.entities.iter().map(|e| e.id).collect();
            b.params = be.params.clone();
            for p in &mut b.params {
                match p {
                    DynParam::Stretch { ids: v, .. } | DynParam::Flip { ids: v, .. } => v.retain(|i| ids.contains(i)),
                    DynParam::Visibility { states, .. } => states.iter_mut().for_each(|s| s.1.retain(|i| ids.contains(i))),
                    DynParam::Lookup { .. } => {}
                }
            }
        }
        parent.dirty = true;
        self.log.push(format!("Block {} saved ({} parameter(s)).", be.name, be.params.len()));
        if close {
            let editor = self.active;
            self.tabs[editor].dirty = false;
            self.close_tab(editor);
            if let Some(k) = self.tabs.iter().position(|t| t.uid == be.parent) {
                self.active = k;
            }
        } else {
            self.tabs[self.active].dirty = false;
        }
    }

    /// BLOOKUP: the lookup table of the block being edited.
    pub fn lookup_dialog(&mut self, ctx: &egui::Context) {
        if !self.dlg.lookup {
            return;
        }
        let t = &mut self.tabs[self.active];
        let Some(be) = t.block_edit.as_mut() else {
            self.dlg.lookup = false;
            self.log.push("A lookup table belongs to a block: BEDIT a block first.".into());
            return;
        };
        let columns: Vec<String> =
            be.params.iter().filter(|p| !matches!(p, DynParam::Lookup { .. })).map(|p| p.name().to_string()).collect();
        if columns.is_empty() {
            self.dlg.lookup = false;
            self.log.push("Add stretch, flip or visibility parameters first; a lookup row sets them together.".into());
            return;
        }
        // The table, kept in step with the columns.
        let idx = be.params.iter().position(|p| matches!(p, DynParam::Lookup { .. })).unwrap_or_else(|| {
            be.params.push(DynParam::Lookup { name: "Lookup".into(), columns: columns.clone(), rows: vec![] });
            be.params.len() - 1
        });
        let defaults: Vec<String> =
            columns.iter().map(|c| be.params.iter().find(|p| p.name() == c).map(|p| p.default_value()).unwrap_or_default()).collect();
        let choices: Vec<Vec<String>> =
            columns.iter().map(|c| be.params.iter().find(|p| p.name() == c).map(|p| p.choices()).unwrap_or_default()).collect();
        let mut open = true;
        if let DynParam::Lookup { columns: cols, rows, .. } = &mut be.params[idx] {
            if *cols != columns {
                for r in rows.iter_mut() {
                    let old: Vec<(String, String)> = cols.iter().cloned().zip(r.1.iter().cloned()).collect();
                    r.1 = columns
                        .iter()
                        .zip(&defaults)
                        .map(|(c, d)| old.iter().find(|(n, _)| n == c).map(|(_, v)| v.clone()).unwrap_or_else(|| d.clone()))
                        .collect();
                }
                *cols = columns.clone();
            }
            egui::Window::new("Lookup Table").open(&mut open).default_width(480.0).show(ctx, |ui| {
                ui.label(
                    RichText::new("Each row is one choice for the block reference; it sets all the parameters at once.")
                        .color(theme::TEXT_DIM),
                );
                let mut remove = None;
                egui::Grid::new("lookup").striped(true).spacing([6.0, 4.0]).show(ui, |ui| {
                    ui.label(RichText::new("Row").strong());
                    for c in &columns {
                        ui.label(RichText::new(c).strong());
                    }
                    ui.label("");
                    ui.end_row();
                    for (i, (label, vals)) in rows.iter_mut().enumerate() {
                        ui.add(egui::TextEdit::singleline(label).desired_width(110.0).min_size(egui::vec2(110.0, 0.0)));
                        for (k, v) in vals.iter_mut().enumerate() {
                            if choices[k].is_empty() {
                                ui.add(egui::TextEdit::singleline(v).desired_width(70.0).min_size(egui::vec2(70.0, 0.0)));
                            } else {
                                egui::ComboBox::from_id_salt(("lk", i, k)).selected_text(v.clone()).show_ui(ui, |ui| {
                                    for c in &choices[k] {
                                        ui.selectable_value(v, c.clone(), c);
                                    }
                                });
                            }
                        }
                        if ui.small_button("×").clicked() {
                            remove = Some(i);
                        }
                        ui.end_row();
                    }
                });
                if let Some(i) = remove {
                    rows.remove(i);
                }
                if ui.button("+ Row").clicked() {
                    rows.push((format!("Size {}", rows.len() + 1), defaults.clone()));
                }
            });
            // Numbers must be numbers.
            for r in rows.iter_mut() {
                for (k, v) in r.1.iter_mut().enumerate() {
                    if choices[k].is_empty() && num(v).is_none() && !v.trim().is_empty() {
                        *v = defaults[k].clone();
                    }
                }
            }
        }
        self.dlg.lookup = open;
    }
}
