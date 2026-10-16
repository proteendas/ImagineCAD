// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Proteen Das

//! External references: XATTACH, the XREF palette (reload, bind, detach,
//! open) and reloading when a drawing is opened.

use crate::app::App;
use crate::theme;
use eframe::egui::{self, RichText};
use std::path::{Path, PathBuf};

/// Where an external reference's file is: as stored, or next to the drawing.
pub fn resolve(stored: &str, drawing: Option<&Path>) -> Option<PathBuf> {
    let p = PathBuf::from(stored);
    if p.is_file() {
        return Some(p);
    }
    let dir = drawing?.parent()?;
    let near = dir.join(&p);
    if near.is_file() {
        return Some(near);
    }
    let by_name = dir.join(p.file_name()?);
    by_name.is_file().then_some(by_name)
}

impl App {
    /// Load (or reload) every external reference of the active drawing.
    /// Returns the names whose file was not found.
    pub fn reload_xrefs(&mut self) -> Vec<String> {
        let t = &self.tabs[self.active];
        let refs: Vec<(String, String)> = t.doc.xrefs().iter().map(|b| (b.name.clone(), b.xref.clone().unwrap_or_default())).collect();
        let here = t.path.clone();
        let mut missing = vec![];
        for (name, stored) in refs {
            match resolve(&stored, here.as_deref()).and_then(|p| icad_io::open(&p).ok().map(|d| (p, d))) {
                Some((p, other)) => self.tabs[self.active].doc.attach_xref(&name, &p.display().to_string(), &other),
                None => missing.push(name),
            }
        }
        if !missing.is_empty() {
            self.log.push(format!("External reference(s) not found: {} (XREF to fix the path)", missing.join(", ")));
        }
        missing
    }

    /// XATTACH: choose a drawing and place it as an external reference.
    pub fn xattach(&mut self) {
        let Some(path) = rfd::FileDialog::new().add_filter("Drawings", &["dwg", "dxf", "icad"]).pick_file() else { return };
        let other = match icad_io::open(&path) {
            Ok(d) => d,
            Err(e) => return self.error(format!("Cannot attach {}: {e}", path.display())),
        };
        let t = self.tab();
        let stem = path.file_stem().and_then(|s| s.to_str()).unwrap_or("XREF").replace(['|', ' '], "_").to_uppercase();
        let mut name = stem.clone();
        let mut k = 2;
        while t.doc.block(&name).is_some_and(|b| b.xref.as_deref() != Some(path.display().to_string().as_str())) {
            name = format!("{stem}_{k}");
            k += 1;
        }
        t.checkpoint();
        t.doc.attach_xref(&name, &path.display().to_string(), &other);
        self.log.push(format!("External reference {name} attached; place it (scale and rotation as for INSERT)."));
        self.run(&format!("INSERT {name}"));
    }

    pub fn xref_palette(&mut self, ctx: &egui::Context) {
        if !self.dlg.xref {
            return;
        }
        let mut open = true;
        let mut action: Option<(&str, String)> = None;
        let rows: Vec<(String, String, bool, usize)> = {
            let t = &self.tabs[self.active];
            t.doc
                .xrefs()
                .iter()
                .map(|b| {
                    let stored = b.xref.clone().unwrap_or_default();
                    let found = resolve(&stored, t.path.as_deref()).is_some();
                    let uses = t
                        .doc
                        .entities
                        .iter()
                        .filter(|e| matches!(&e.shape, icad_core::Shape::Insert(i) if i.name.eq_ignore_ascii_case(&b.name)))
                        .count();
                    (b.name.clone(), stored, found, uses)
                })
                .collect()
        };
        egui::Window::new(crate::i18n::tr("External References")).open(&mut open).default_width(560.0).show(ctx, |ui| {
            ui.horizontal(|ui| {
                if ui.button(crate::i18n::tr("Attach DWG / DXF…")).clicked() {
                    action = Some(("attach", String::new()));
                }
                if ui.add_enabled(!rows.is_empty(), egui::Button::new("Reload all")).clicked() {
                    action = Some(("reload_all", String::new()));
                }
            });
            ui.separator();
            if rows.is_empty() {
                ui.label(
                    RichText::new(crate::i18n::tr(
                        "No external references. Attach a drawing to show it here, kept up to date with its file.",
                    ))
                    .color(theme::TEXT_DIM),
                );
            }
            egui::Grid::new("xrefs").num_columns(4).striped(true).spacing([10.0, 6.0]).show(ui, |ui| {
                for (name, path, found, uses) in &rows {
                    ui.label(RichText::new(name).strong());
                    ui.label(if *found {
                        RichText::new(format!("Loaded · {uses} reference(s)")).color(egui::Color32::from_rgb(90, 210, 120))
                    } else {
                        RichText::new(crate::i18n::tr("Not found")).color(theme::ERR)
                    });
                    ui.label(RichText::new(path).size(11.0).color(theme::TEXT_DIM)).on_hover_text(path);
                    ui.horizontal(|ui| {
                        if ui.small_button(crate::i18n::tr("Reload")).clicked() {
                            action = Some(("reload", name.clone()));
                        }
                        if ui.small_button(crate::i18n::tr("Path…")).on_hover_text(crate::i18n::tr("Choose the file again")).clicked() {
                            action = Some(("path", name.clone()));
                        }
                        if ui.small_button(crate::i18n::tr("Open")).clicked() {
                            action = Some(("open", path.clone()));
                        }
                        if ui.small_button(crate::i18n::tr("Bind")).on_hover_text(crate::i18n::tr("Make it part of this drawing")).clicked()
                        {
                            action = Some(("bind", name.clone()));
                        }
                        if ui.small_button(crate::i18n::tr("Detach")).clicked() {
                            action = Some(("detach", name.clone()));
                        }
                    });
                    ui.end_row();
                }
            });
        });
        self.dlg.xref = open;
        let Some((a, arg)) = action else { return };
        match a {
            "attach" => self.xattach(),
            "reload_all" => {
                self.tab().checkpoint();
                if self.reload_xrefs().is_empty() {
                    self.log.push("External references reloaded.".into());
                }
            }
            "reload" | "path" => {
                let t = &self.tabs[self.active];
                let stored = t.doc.block(&arg).and_then(|b| b.xref.clone()).unwrap_or_default();
                let file = if a == "path" {
                    rfd::FileDialog::new().add_filter("Drawings", &["dwg", "dxf", "icad"]).pick_file()
                } else {
                    resolve(&stored, t.path.as_deref())
                };
                match file.and_then(|p| icad_io::open(&p).ok().map(|d| (p, d))) {
                    Some((p, other)) => {
                        let t = self.tab();
                        t.checkpoint();
                        t.doc.attach_xref(&arg, &p.display().to_string(), &other);
                        self.log.push(format!("{arg} reloaded from {}", p.display()));
                    }
                    None => self.log.push(format!("{arg}: file not found or unreadable.")),
                }
            }
            "open" => {
                let t = &self.tabs[self.active];
                match resolve(&arg, t.path.as_deref()) {
                    Some(p) => self.open_path(&p),
                    None => self.log.push(format!("{arg} not found.")),
                }
            }
            "bind" => {
                let t = self.tab();
                t.checkpoint();
                t.doc.bind_xref(&arg);
                self.log.push(format!("{arg} bound into this drawing."));
            }
            _ => {
                let t = self.tab();
                t.checkpoint();
                t.doc.detach_xref(&arg);
                self.log.push(format!("{arg} detached."));
            }
        }
    }
}
