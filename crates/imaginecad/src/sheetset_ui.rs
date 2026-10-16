// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Proteen Das

//! The Sheet Set Manager (SHEETSET): build a set of sheets from drawings and
//! their layouts, number and title them, share project values across their
//! title blocks, publish the whole set to PDF and insert a sheet list.

use crate::app::App;
use crate::theme;
use eframe::egui::{self, RichText};
use icad_core::sheetset::{SheetRef, SheetSet};
use icad_core::Document;
use std::path::{Path, PathBuf};

/// The open sheet set.
pub struct SheetSetState {
    pub path: Option<PathBuf>,
    pub set: SheetSet,
    pub dirty: bool,
}

/// The set's sheets as PDF pages, each with the set's numbering and shared
/// values in its title block (the drawings themselves are not changed).
/// Returns the pages and the sheets that could not be plotted.
pub fn set_pages(
    set: &SheetSet,
    set_path: Option<&Path>,
    doc_of: impl Fn(&Path) -> Result<Document, String>,
    plot: &icad_io::PlotOptions,
) -> (Vec<icad_io::pdf::Page>, Vec<String>) {
    let mut pages = vec![];
    let mut problems = vec![];
    for (i, s) in set.sheets.iter().enumerate() {
        let p = set.resolve(set_path, s);
        let mut doc = match doc_of(&p) {
            Ok(d) => d,
            Err(e) => {
                problems.push(format!("{}: {e}", s.file));
                continue;
            }
        };
        set.apply(i, &mut doc);
        // The stamp names each sheet's own drawing.
        let mut opts = plot.clone();
        if let Some(st) = &mut opts.stamp {
            st.drawing = p.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_else(|| s.file.clone());
        }
        let page = match &s.layout {
            None => Some(icad_io::pdf::sheet_page(&doc, &opts)),
            Some(n) => doc.layouts.iter().position(|l| &l.name == n).and_then(|li| icad_io::pdf::layout_page(&doc, li, &opts)),
        };
        match page {
            Some(pg) => pages.push(pg),
            None => problems.push(format!("{}: layout {:?} not found", s.file, s.layout)),
        }
    }
    (pages, problems)
}

impl App {
    /// The document of a drawing: the open tab's when it is open, else read from disk.
    fn set_doc(&self, path: &Path) -> Result<Document, String> {
        match self.tabs.iter().find(|t| t.path.as_deref() == Some(path)) {
            Some(t) => Ok(t.doc.clone()),
            None => icad_io::open(path),
        }
    }

    pub fn open_sheet_set(&mut self, p: &Path) {
        match SheetSet::load(p) {
            Ok(set) => {
                self.sheetset = Some(SheetSetState { path: Some(p.to_path_buf()), set, dirty: false });
                self.dlg.sheetset = true;
                self.log.push(format!("Sheet set {} opened.", p.display()));
            }
            Err(e) => self.error(e),
        }
    }

    /// Every sheet of a drawing: its main sheet and each layout.
    fn sheets_of(&self, file: &Path, set_path: Option<&Path>) -> Result<Vec<SheetRef>, String> {
        let doc = self.set_doc(file)?;
        let stored = SheetSet::store_path(set_path, file);
        let stem = file.file_stem().and_then(|s| s.to_str()).unwrap_or("Sheet").to_string();
        let title = if doc.sheet.title.title.trim().is_empty() || doc.sheet.title.title == "UNTITLED PART" {
            stem
        } else {
            doc.sheet.title.title.clone()
        };
        let mut out = vec![SheetRef { file: stored.clone(), layout: None, number: String::new(), title: title.clone() }];
        for l in &doc.layouts {
            out.push(SheetRef {
                file: stored.clone(),
                layout: Some(l.name.clone()),
                number: String::new(),
                title: format!("{title} — {}", l.name),
            });
        }
        Ok(out)
    }

    pub fn sheet_set_palette(&mut self, ctx: &egui::Context) {
        if !self.dlg.sheetset {
            return;
        }
        let mut open = true;
        let mut action: Option<(&str, usize)> = None;
        let mut st = self.sheetset.take().unwrap_or(SheetSetState { path: None, set: SheetSet::default(), dirty: false });
        egui::Window::new("Sheet Set Manager").open(&mut open).default_width(620.0).show(ctx, |ui| {
            ui.horizontal(|ui| {
                for (label, a) in [("New", "new"), ("Open…", "open"), ("Save", "save"), ("Save As…", "saveas")] {
                    if ui.button(label).clicked() {
                        action = Some((a, 0));
                    }
                }
                let name = st.path.as_ref().and_then(|p| p.file_name()).and_then(|n| n.to_str()).unwrap_or("not saved");
                ui.label(RichText::new(format!("{name}{}", if st.dirty { " *" } else { "" })).color(theme::TEXT_DIM));
            });
            ui.separator();
            egui::Grid::new("set_fields").num_columns(4).spacing([8.0, 4.0]).show(ui, |ui| {
                let mut field = |ui: &mut egui::Ui, label: &str, v: &mut String| {
                    ui.label(label);
                    if ui.add(egui::TextEdit::singleline(v).desired_width(170.0).min_size(egui::vec2(170.0, 0.0))).changed() {
                        st.dirty = true;
                    }
                };
                field(ui, "Set name", &mut st.set.name);
                field(ui, "Project", &mut st.set.project);
                ui.end_row();
                field(ui, "Company", &mut st.set.company);
                field(ui, "Drawing no. prefix", &mut st.set.number_prefix);
                ui.end_row();
                field(ui, "Drawn by", &mut st.set.drawn_by);
                field(ui, "Checked by", &mut st.set.checked_by);
                ui.end_row();
                field(ui, "Approved by", &mut st.set.approved_by);
                ui.end_row();
            });
            ui.separator();
            ui.horizontal(|ui| {
                if ui.button("+ Current drawing").on_hover_text("Its main sheet and every layout").clicked() {
                    action = Some(("add_current", 0));
                }
                if ui.button("+ Drawings…").clicked() {
                    action = Some(("add_files", 0));
                }
                if ui.add_enabled(!st.set.sheets.is_empty(), egui::Button::new("Renumber")).clicked() {
                    st.set.renumber();
                    st.dirty = true;
                }
            });
            if st.set.sheets.is_empty() {
                ui.label(
                    RichText::new("No sheets yet. Add the current drawing or pick drawings; each brings its main sheet and its layouts.")
                        .color(theme::TEXT_DIM),
                );
            }
            egui::ScrollArea::vertical().max_height(300.0).show(ui, |ui| {
                egui::Grid::new("set_sheets").num_columns(5).striped(true).spacing([6.0, 3.0]).show(ui, |ui| {
                    ui.label(RichText::new("No.").strong());
                    ui.label(RichText::new("Title").strong());
                    ui.label(RichText::new("Drawing").strong());
                    ui.label(RichText::new("Sheet").strong());
                    ui.label("");
                    ui.end_row();
                    let n = st.set.sheets.len();
                    for i in 0..n {
                        let s = &mut st.set.sheets[i];
                        if ui.add(egui::TextEdit::singleline(&mut s.number).desired_width(38.0).min_size(egui::vec2(38.0, 0.0))).changed() {
                            st.dirty = true;
                        }
                        if ui.add(egui::TextEdit::singleline(&mut s.title).desired_width(200.0).min_size(egui::vec2(200.0, 0.0))).changed()
                        {
                            st.dirty = true;
                        }
                        ui.label(RichText::new(&s.file).size(11.0).color(theme::TEXT_DIM));
                        ui.label(RichText::new(s.layout.as_deref().unwrap_or("Main sheet")).size(11.0));
                        ui.horizontal(|ui| {
                            if ui.small_button("Open").clicked() {
                                action = Some(("open_sheet", i));
                            }
                            if ui.add_enabled(i > 0, egui::Button::new("▲").small()).clicked() {
                                action = Some(("up", i));
                            }
                            if ui.add_enabled(i + 1 < n, egui::Button::new("▼").small()).clicked() {
                                action = Some(("down", i));
                            }
                            if ui.small_button("×").on_hover_text("Remove from the set").clicked() {
                                action = Some(("remove", i));
                            }
                        });
                        ui.end_row();
                    }
                });
            });
            ui.separator();
            ui.horizontal(|ui| {
                let any = !st.set.sheets.is_empty();
                if ui.add_enabled(any, egui::Button::new(RichText::new("Publish set to PDF…").strong())).clicked() {
                    action = Some(("publish", 0));
                }
                if ui
                    .add_enabled(any, egui::Button::new("Update title blocks"))
                    .on_hover_text(
                        "Write sheet numbers, titles and the shared values into every sheet's title block, and save the drawings",
                    )
                    .clicked()
                {
                    action = Some(("update", 0));
                }
                if ui.add_enabled(any, egui::Button::new("Insert sheet list")).clicked() {
                    action = Some(("sheetlist", 0));
                }
            });
        });
        self.dlg.sheetset = open;
        let Some((a, i)) = action else {
            self.sheetset = Some(st);
            return;
        };
        match a {
            "new" => st = SheetSetState { path: None, set: SheetSet::default(), dirty: true },
            "open" => {
                if let Some(p) = rfd::FileDialog::new().add_filter("Sheet set", &["icadset"]).pick_file() {
                    match SheetSet::load(&p) {
                        Ok(set) => st = SheetSetState { path: Some(p), set, dirty: false },
                        Err(e) => self.error(e),
                    }
                }
            }
            "save" | "saveas" => {
                let target = if a == "save" { st.path.clone() } else { None }.or_else(|| {
                    rfd::FileDialog::new()
                        .add_filter("Sheet set", &["icadset"])
                        .set_file_name(format!("{}.icadset", st.set.name))
                        .save_file()
                });
                if let Some(p) = target {
                    // Paths become relative to the (new) set file.
                    let old = st.path.clone();
                    for s in &mut st.set.sheets {
                        let abs = SheetSet::default().resolve(old.as_deref(), s);
                        s.file = SheetSet::store_path(Some(&p), &abs);
                    }
                    match st.set.save(&p) {
                        Ok(()) => {
                            self.log.push(format!("Sheet set saved to {}", p.display()));
                            st.path = Some(p);
                            st.dirty = false;
                        }
                        Err(e) => self.error(e),
                    }
                }
            }
            "add_current" | "add_files" => {
                let files: Vec<PathBuf> = if a == "add_current" {
                    match self.tabs[self.active].path.clone() {
                        Some(p) => vec![p],
                        None => {
                            self.log.push("Save the drawing first, so the set can refer to its file.".into());
                            vec![]
                        }
                    }
                } else {
                    rfd::FileDialog::new().add_filter("Drawings", &["icad", "dwg", "dxf"]).pick_files().unwrap_or_default()
                };
                for f in files {
                    match self.sheets_of(&f, st.path.as_deref()) {
                        Ok(v) => {
                            st.set.sheets.extend(v);
                            st.dirty = true;
                        }
                        Err(e) => self.error(e),
                    }
                }
                if st.set.sheets.iter().all(|s| s.number.is_empty()) || st.set.sheets.iter().any(|s| s.number.is_empty()) {
                    st.set.renumber();
                }
            }
            "up" if i > 0 => {
                st.set.sheets.swap(i, i - 1);
                st.dirty = true;
            }
            "down" if i + 1 < st.set.sheets.len() => {
                st.set.sheets.swap(i, i + 1);
                st.dirty = true;
            }
            "remove" => {
                st.set.sheets.remove(i);
                st.dirty = true;
            }
            "open_sheet" => {
                let s = st.set.sheets[i].clone();
                let p = st.set.resolve(st.path.as_deref(), &s);
                match self.tabs.iter().position(|t| t.path.as_deref() == Some(p.as_path())) {
                    Some(k) => self.active = k,
                    None => self.open_path(&p),
                }
                let li = s.layout.as_ref().and_then(|n| self.tabs[self.active].doc.layouts.iter().position(|l| &l.name == n));
                self.show_sheet(Some(li));
            }
            "publish" => self.publish_set(&st),
            "update" => self.update_set_title_blocks(&st),
            "sheetlist" => {
                let th = 2.5 * self.tabs[self.active].doc.ascale();
                let table = st.set.sheet_list(icad_core::geom::V2::ZERO, th);
                self.cmd = Some(Box::new(crate::tables::PlaceTableCmd { table }));
                self.log.push("Command: TABLE — place the sheet list.".into());
            }
            _ => {}
        }
        self.sheetset = Some(st);
    }

    /// One PDF with every sheet of the set, numbered as in the set.
    fn publish_set(&mut self, st: &SheetSetState) {
        let Some(out) = rfd::FileDialog::new().add_filter("PDF", &["pdf"]).set_file_name(format!("{}.pdf", st.set.name)).save_file() else {
            return;
        };
        let opts = self.plot_options(None, self.active);
        let (pages, problems) = set_pages(&st.set, st.path.as_deref(), |p| self.set_doc(p), &opts);
        match std::fs::write(&out, icad_io::pdf::write_pages_with(&pages, &st.set.name, opts.pdfa)) {
            Ok(()) => self.log.push(format!("Published {} sheet(s) of {} to {}", pages.len(), st.set.name, out.display())),
            Err(e) => self.error(format!("Publishing failed: {e}")),
        }
        if !problems.is_empty() {
            self.error(format!("Some sheets were skipped:\n{}", problems.join("\n")));
        }
    }

    /// Write the set's numbering and values into every drawing's title blocks.
    fn update_set_title_blocks(&mut self, st: &SheetSetState) {
        let mut files: Vec<PathBuf> = vec![];
        for s in &st.set.sheets {
            let p = st.set.resolve(st.path.as_deref(), s);
            if !files.contains(&p) {
                files.push(p);
            }
        }
        let mut changed = 0;
        for f in files {
            let idx: Vec<usize> =
                (0..st.set.sheets.len()).filter(|i| st.set.resolve(st.path.as_deref(), &st.set.sheets[*i]) == f).collect();
            if let Some(k) = self.tabs.iter().position(|t| t.path.as_deref() == Some(f.as_path())) {
                // Open drawing: changed in place (one undo step), saved by the user.
                let t = &mut self.tabs[k];
                t.checkpoint();
                for i in &idx {
                    st.set.apply(*i, &mut t.doc);
                }
                t.dirty = true;
                changed += 1;
                continue;
            }
            match icad_io::open(&f) {
                Ok(mut doc) => {
                    for i in &idx {
                        st.set.apply(*i, &mut doc);
                    }
                    match icad_io::save(&doc, &f, &self.export_options()) {
                        Ok(()) => changed += 1,
                        Err(e) => self.error(format!("{}: {e}", f.display())),
                    }
                }
                Err(e) => self.error(format!("{}: {e}", f.display())),
            }
        }
        self.log.push(format!("Title blocks updated in {changed} drawing(s) (open drawings are marked changed; save them)."));
    }
}
