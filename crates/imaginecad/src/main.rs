// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Proteen Das

//! ImagineCAD — AI-assisted 2D CAD for mechanical drawings.
#![cfg_attr(all(windows, not(debug_assertions)), windows_subsystem = "windows")]

mod app;
mod attributes;
mod canvas;
mod chat;
mod cli;
mod commands;
mod commands_dim;
mod commands_ext;
mod commands_region;
mod dialogs;
mod dialogs2;
mod icons;
mod layouts_ui;
mod ribbon;
mod secrets;
mod settings;
mod spell;
mod symbol_ui;
mod tab;
mod tables;
mod text_ui;
mod theme;
mod tracking;
mod view;
mod xref_ui;

use eframe::egui;

const ICON_PNG: &[u8] = include_bytes!("../../../assets/icon-256.png");

/// Legal notice shown in the About box and by `--version` (GPL §5(d)).
pub const LEGAL_NOTICE: &str = "Copyright (C) 2026 Proteen Das.\n\
ImagineCAD is free software: you can redistribute it and/or modify it under the terms of the \
GNU General Public License version 3. It comes with ABSOLUTELY NO WARRANTY. Commercial licenses \
are available from the copyright holder. DWG support uses GNU LibreDWG (GPL-3.0).";

fn main() -> eframe::Result<()> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if let Some(code) = cli::run(&args) {
        std::process::exit(code);
    }
    let open = args.iter().find(|a| !a.starts_with('-')).map(std::path::PathBuf::from);

    let mut viewport = egui::ViewportBuilder::default()
        .with_title("ImagineCAD")
        .with_inner_size([1500.0, 920.0])
        .with_min_inner_size([900.0, 600.0])
        .with_drag_and_drop(true);
    if let Ok(img) = image::load_from_memory(ICON_PNG) {
        let img = img.to_rgba8();
        let (w, h) = img.dimensions();
        viewport = viewport.with_icon(egui::IconData { rgba: img.into_raw(), width: w, height: h });
    }
    let options = eframe::NativeOptions { viewport, ..Default::default() };
    eframe::run_native("ImagineCAD", options, Box::new(move |cc| Ok(Box::new(app::App::new(cc, open)))))
}
