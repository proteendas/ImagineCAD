// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Proteen Das

//! Geometric tolerance, datum, surface texture and weld symbols: editors (used
//! by the creation dialog, double-click editing and the properties palette)
//! and the placement command.

use crate::app::App;
use crate::commands::{dim_layer, Command, Cx, Flow, Want};
use crate::theme;
use eframe::egui::{self, RichText};
use icad_core::entity::Shape;
use icad_core::geom::*;
use icad_core::symbols::*;
use icad_core::Document;

/// Which symbol a dialog or command works on.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SymbolKind {
    Tolerance,
    Datum,
    Surface,
    Weld,
}

impl SymbolKind {
    pub fn title(self) -> &'static str {
        match self {
            SymbolKind::Tolerance => "Geometric Tolerance",
            SymbolKind::Datum => "Datum Feature Symbol",
            SymbolKind::Surface => "Surface Texture",
            SymbolKind::Weld => "Weld Symbol",
        }
    }

    /// A new symbol with sensible defaults.
    pub fn prototype(self, next_datum: &str) -> Shape {
        match self {
            SymbolKind::Tolerance => Shape::Tolerance(Fcf {
                pos: V2::ZERO,
                rows: vec![FcfRow {
                    symbol: GdtSymbol::Position,
                    diameter: true,
                    tolerance: "0.1".into(),
                    material: Material::None,
                    datums: vec![DatumRef { letter: "A".into(), material: Material::None }],
                }],
                leader: vec![],
            }),
            SymbolKind::Datum => Shape::Datum(DatumFeature { letter: next_datum.into(), pos: V2::ZERO, tip: V2::ZERO }),
            SymbolKind::Surface => Shape::Surface(SurfaceFinish {
                pos: V2::ZERO,
                rotation: 0.0,
                process: SurfaceProcess::Removal,
                requirement: "Ra 3.2".into(),
                method: String::new(),
                lay: String::new(),
                all_around: false,
            }),
            SymbolKind::Weld => Shape::Weld(WeldSymbol {
                tip: V2::ZERO,
                elbow: V2::ZERO,
                arrow_side: WeldSide { kind: WeldType::Fillet, size: "a5".into(), length: String::new() },
                other_side: WeldSide::default(),
                all_around: false,
                field: false,
                tail: String::new(),
            }),
        }
    }

    pub fn of(s: &Shape) -> Option<SymbolKind> {
        match s {
            Shape::Tolerance(_) => Some(SymbolKind::Tolerance),
            Shape::Datum(_) => Some(SymbolKind::Datum),
            Shape::Surface(_) => Some(SymbolKind::Surface),
            Shape::Weld(_) => Some(SymbolKind::Weld),
            _ => None,
        }
    }
}

fn material_combo(ui: &mut egui::Ui, id: impl std::hash::Hash, m: &mut Material) -> bool {
    let label = |m: Material| match m {
        Material::None => "–",
        Material::Mmc => "Ⓜ MMC",
        Material::Lmc => "Ⓛ LMC",
        Material::Rfs => "Ⓢ RFS",
    };
    let mut changed = false;
    egui::ComboBox::from_id_salt(id).width(64.0).selected_text(label(*m)).show_ui(ui, |ui| {
        for v in [Material::None, Material::Mmc, Material::Lmc, Material::Rfs] {
            changed |= ui.selectable_value(m, v, label(v)).changed();
        }
    });
    changed
}

/// Edit a symbol's contents (not its position). Returns true when changed.
pub fn symbol_editor(ui: &mut egui::Ui, s: &mut Shape) -> bool {
    let mut ch = false;
    match s {
        Shape::Tolerance(f) => {
            let mut remove = None;
            let rows = f.rows.len();
            for (i, r) in f.rows.iter_mut().enumerate() {
                ui.push_id(("fcf_row", i), |ui| {
                    ui.horizontal(|ui| {
                        egui::ComboBox::from_id_salt("sym").width(150.0).selected_text(r.symbol.name()).show_ui(ui, |ui| {
                            for g in GdtSymbol::ALL {
                                ch |= ui.selectable_value(&mut r.symbol, g, g.name()).changed();
                            }
                        });
                        ch |= ui.checkbox(&mut r.diameter, "Ø").on_hover_text(crate::i18n::tr("Cylindrical tolerance zone")).changed();
                        ch |= ui.add(egui::TextEdit::singleline(&mut r.tolerance).desired_width(54.0).hint_text("0.1")).changed();
                        ch |= material_combo(ui, "mat", &mut r.material);
                        if rows > 1 && ui.small_button("✖").on_hover_text(crate::i18n::tr("Remove this row")).clicked() {
                            remove = Some(i);
                        }
                    });
                    ui.horizontal(|ui| {
                        ui.label(RichText::new(crate::i18n::tr("Datums")).size(11.5).color(theme::TEXT_DIM));
                        r.datums.resize_with(3, Default::default);
                        for (k, d) in r.datums.iter_mut().enumerate() {
                            ch |= ui
                                .add(egui::TextEdit::singleline(&mut d.letter).desired_width(28.0).hint_text(["A", "B", "C"][k]))
                                .changed();
                            ch |= material_combo(ui, ("dm", k), &mut d.material);
                        }
                        r.datums.retain(|d| !d.letter.trim().is_empty());
                    });
                });
                ui.add_space(4.0);
            }
            if let Some(i) = remove {
                f.rows.remove(i);
                ch = true;
            }
            if f.rows.len() < 2 && ui.small_button(crate::i18n::tr("+ Second row (composite)")).clicked() {
                let mut r = f.rows[0].clone();
                r.tolerance.clear();
                f.rows.push(r);
                ch = true;
            }
        }
        Shape::Datum(d) => {
            ui.horizontal(|ui| {
                ui.label(crate::i18n::tr("Datum letter"));
                ch |= ui.add(egui::TextEdit::singleline(&mut d.letter).desired_width(40.0)).changed();
            });
        }
        Shape::Surface(sf) => {
            ui.horizontal(|ui| {
                ui.label(crate::i18n::tr("Process"));
                for (v, l) in
                    [(SurfaceProcess::Removal, "Machined"), (SurfaceProcess::Any, "Any"), (SurfaceProcess::NoRemoval, "No removal")]
                {
                    ch |= ui.radio_value(&mut sf.process, v, l).changed();
                }
            });
            egui::Grid::new("sf").num_columns(2).show(ui, |ui| {
                ui.label(crate::i18n::tr("Requirement"));
                ch |= ui.add(egui::TextEdit::singleline(&mut sf.requirement).hint_text("Ra 1.6")).changed();
                ui.end_row();
                ui.label(crate::i18n::tr("Method"));
                ch |= ui.add(egui::TextEdit::singleline(&mut sf.method).hint_text(crate::i18n::tr("ground, turned…"))).changed();
                ui.end_row();
                ui.label(crate::i18n::tr("Lay"));
                egui::ComboBox::from_id_salt("lay").selected_text(if sf.lay.is_empty() { "–" } else { sf.lay.as_str() }).show_ui(
                    ui,
                    |ui| {
                        for (v, tip) in [
                            ("", "none"),
                            ("=", "parallel to the projection plane"),
                            ("⊥", "perpendicular"),
                            ("X", "crossed"),
                            ("M", "multi-directional"),
                            ("C", "circular"),
                            ("R", "radial"),
                            ("P", "particulate"),
                        ] {
                            ch |= ui
                                .selectable_value(&mut sf.lay, v.to_string(), if v.is_empty() { "–" } else { v })
                                .on_hover_text(tip)
                                .changed();
                        }
                    },
                );
                ui.end_row();
                ui.label(crate::i18n::tr("Rotation °"));
                let mut r = sf.rotation.to_degrees();
                if ui.add(egui::DragValue::new(&mut r).speed(1.0)).changed() {
                    sf.rotation = norm_angle(r.to_radians());
                    ch = true;
                }
                ui.end_row();
            });
            ch |= ui.checkbox(&mut sf.all_around, crate::i18n::tr("All around the outline")).changed();
        }
        Shape::Weld(w) => {
            for (label, side) in [("Arrow side (below the line)", &mut w.arrow_side), ("Other side (above the line)", &mut w.other_side)] {
                ui.label(RichText::new(label).strong());
                ui.push_id(label, |ui| {
                    ui.horizontal(|ui| {
                        egui::ComboBox::from_id_salt("wt").width(140.0).selected_text(side.kind.name()).show_ui(ui, |ui| {
                            for t in WeldType::ALL {
                                ch |= ui.selectable_value(&mut side.kind, t, t.name()).changed();
                            }
                        });
                        ch |= ui.add(egui::TextEdit::singleline(&mut side.size).desired_width(48.0).hint_text("a5")).changed();
                        ch |= ui
                            .add(egui::TextEdit::singleline(&mut side.length).desired_width(72.0).hint_text(crate::i18n::tr("length")))
                            .changed();
                    });
                });
            }
            ui.horizontal(|ui| {
                ch |= ui.checkbox(&mut w.all_around, crate::i18n::tr("All around")).changed();
                ch |= ui.checkbox(&mut w.field, crate::i18n::tr("Field weld")).changed();
            });
            ui.horizontal(|ui| {
                ui.label(crate::i18n::tr("Tail"));
                ch |= ui
                    .add(egui::TextEdit::singleline(&mut w.tail).hint_text(crate::i18n::tr("process, e.g. 111")).desired_width(160.0))
                    .changed();
            });
        }
        _ => {}
    }
    ch
}

/// Open dialog: a new symbol being prepared, or an existing one being edited.
pub struct SymbolDialog {
    pub kind: SymbolKind,
    pub shape: Shape,
    pub editing: Option<u64>,
}

// ------------------------------------------------------------------ placement

/// Places a prepared symbol: the feature point first, then the symbol.
pub struct PlaceSymbolCmd {
    pub proto: Shape,
    pub first: Option<V2>,
    /// Feature control frames: Enter at the first prompt skips the leader.
    pub no_leader: bool,
}

impl PlaceSymbolCmd {
    pub fn new(proto: Shape) -> Self {
        Self { proto, first: None, no_leader: false }
    }

    fn at(&self, p: V2) -> Shape {
        let mut s = self.proto.clone();
        match &mut s {
            Shape::Tolerance(f) => {
                f.pos = p;
                f.leader = if self.no_leader { vec![] } else { self.first.into_iter().collect() };
            }
            Shape::Datum(d) => {
                d.tip = self.first.unwrap_or(p);
                d.pos = p;
            }
            Shape::Surface(sf) => {
                if let Some(tip) = self.first {
                    sf.pos = tip;
                    if p.dist(tip) > 1e-9 {
                        // Point "up" from the surface towards the cursor.
                        sf.rotation = norm_angle((p - tip).angle() - std::f64::consts::FRAC_PI_2);
                    }
                } else {
                    sf.pos = p;
                }
            }
            Shape::Weld(w) => {
                w.tip = self.first.unwrap_or(p);
                w.elbow = p;
            }
            _ => {}
        }
        s
    }

    fn waiting_first(&self) -> bool {
        self.first.is_none() && !self.no_leader
    }
}

impl Command for PlaceSymbolCmd {
    fn name(&self) -> &'static str {
        match SymbolKind::of(&self.proto) {
            Some(SymbolKind::Tolerance) => "TOLERANCE",
            Some(SymbolKind::Datum) => "DATUM",
            Some(SymbolKind::Surface) => "SURFFINISH",
            _ => "WELD",
        }
    }
    fn prompt(&self, _: &Cx) -> String {
        let first = self.waiting_first();
        match SymbolKind::of(&self.proto) {
            Some(SymbolKind::Tolerance) if first => "Specify leader arrow point on the feature <no leader>:".into(),
            Some(SymbolKind::Tolerance) => "Specify tolerance location:".into(),
            Some(SymbolKind::Datum) if first => "Specify point on the datum feature:".into(),
            Some(SymbolKind::Datum) => "Specify symbol location:".into(),
            Some(SymbolKind::Surface) if first => "Specify point on the surface:".into(),
            Some(SymbolKind::Surface) => "Specify direction away from the surface <upright>:".into(),
            _ if first => "Specify arrow point on the joint:".into(),
            _ => "Specify reference line location:".into(),
        }
    }
    fn wants(&self) -> Want {
        Want::Point
    }
    fn base(&self) -> Option<V2> {
        self.first
    }
    fn point(&mut self, cx: &mut Cx, p: V2) -> Flow {
        if self.waiting_first() {
            self.first = Some(p);
            return Flow::Continue;
        }
        cx.checkpoint();
        let layer = dim_layer(cx);
        let s = self.at(p);
        cx.doc().add_on(&layer, s);
        Flow::Done
    }
    fn enter(&mut self, cx: &mut Cx) -> Flow {
        match (SymbolKind::of(&self.proto), self.first) {
            (Some(SymbolKind::Tolerance), None) => {
                self.no_leader = true;
                Flow::Continue
            }
            // Surface symbol: upright at the picked point.
            (Some(SymbolKind::Surface), Some(tip)) => self.point(cx, tip),
            _ => Flow::Done,
        }
    }
    fn preview(&self, _: &Document, c: V2) -> Vec<Shape> {
        if self.waiting_first() && SymbolKind::of(&self.proto) != Some(SymbolKind::Tolerance) {
            return vec![];
        }
        vec![self.at(c)]
    }
}

impl App {
    /// Open the symbol dialog for a new symbol.
    pub fn new_symbol(&mut self, kind: SymbolKind) {
        let used: Vec<String> = self.tabs[self.active]
            .doc
            .entities
            .iter()
            .filter_map(|e| if let Shape::Datum(d) = &e.shape { Some(d.letter.clone()) } else { None })
            .collect();
        let next = ('A'..='Z').map(|c| c.to_string()).find(|l| !used.contains(l)).unwrap_or_else(|| "A".into());
        self.dlg.symbol = Some(SymbolDialog { kind, shape: kind.prototype(&next), editing: None });
    }

    /// Edit an existing symbol (double-click). Returns false for other shapes.
    pub fn edit_symbol_of(&mut self, id: u64) -> bool {
        let Some(e) = self.tabs[self.active].doc.get(id) else { return false };
        let Some(kind) = SymbolKind::of(&e.shape) else { return false };
        self.dlg.symbol = Some(SymbolDialog { kind, shape: e.shape.clone(), editing: Some(id) });
        true
    }

    pub fn symbol_dialog(&mut self, ctx: &egui::Context) {
        let Some(mut d) = self.dlg.symbol.take() else { return };
        let mut open = true;
        let mut done = false;
        let mut cancel = false;
        egui::Window::new(d.kind.title()).open(&mut open).collapsible(false).resizable(false).show(ctx, |ui| {
            symbol_editor(ui, &mut d.shape);
            ui.add_space(6.0);
            // Live preview of the symbol.
            let ds = &self.tabs[self.active].doc.dimstyle;
            let mut s = d.shape.clone();
            match &mut s {
                Shape::Tolerance(f) => {
                    f.pos = V2::ZERO;
                    f.leader.clear();
                }
                Shape::Datum(x) => {
                    x.pos = V2::new(0.0, 8.0);
                    x.tip = V2::ZERO;
                }
                Shape::Surface(x) => {
                    x.pos = V2::ZERO;
                    x.rotation = 0.0;
                }
                Shape::Weld(x) => {
                    x.tip = V2::ZERO;
                    x.elbow = V2::new(8.0, 8.0);
                }
                _ => {}
            }
            let ctx_d = icad_core::Ctx::new(ds, 1.0);
            let prims = s.prims(&ctx_d);
            crate::canvas::preview_prims(ui, &prims, egui::vec2(320.0, 110.0));
            ui.separator();
            ui.horizontal(|ui| {
                let label = if d.editing.is_some() { "OK" } else { "Place in drawing" };
                if ui.button(RichText::new(label).strong()).clicked() {
                    done = true;
                }
                if ui.button(crate::i18n::tr("Cancel")).clicked() {
                    cancel = true;
                }
            });
        });
        if done {
            match d.editing {
                Some(id) => {
                    let t = &mut self.tabs[self.active];
                    t.checkpoint();
                    if let Some(e) = t.doc.get_mut(id) {
                        e.shape = d.shape;
                    }
                }
                None => {
                    self.cmd = Some(Box::new(PlaceSymbolCmd::new(d.shape)));
                    self.log.push(format!("Command: {}", self.cmd.as_ref().map(|c| c.name()).unwrap_or("")));
                }
            }
            return;
        }
        if open && !cancel {
            self.dlg.symbol = Some(d);
        }
    }
}
