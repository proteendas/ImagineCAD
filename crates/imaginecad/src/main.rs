// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Proteen Das

//! ImagineCAD — AI-assisted 2D CAD for mechanical drawings.
#![cfg_attr(all(windows, not(debug_assertions)), windows_subsystem = "windows")]

mod app;
mod attributes;
mod blockedit_ui;
mod canvas;
mod chat;
mod cli;
mod commands;
mod commands_dim;
mod commands_ext;
mod commands_markup;
mod commands_region;
mod constraints_ui;
mod dialogs;
mod dialogs2;
mod icons;
mod layouts_ui;
#[cfg(target_os = "macos")]
mod macos_open;
mod menus;
mod ribbon;
mod secrets;
mod settings;
mod sheetset_ui;
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
    // Every file the file manager passed (several when a selection is opened).
    let open: Vec<std::path::PathBuf> = args.iter().filter(|a| !a.starts_with('-')).map(|a| launch_path(a)).collect();

    #[cfg(target_os = "macos")]
    macos_open::register();
    // IMAGINECAD_WINDOW=WxH sets the start size (visual tests).
    let size = std::env::var("IMAGINECAD_WINDOW")
        .ok()
        .and_then(|s| s.split_once('x').and_then(|(w, h)| Some([w.parse().ok()?, h.parse().ok()?])))
        .unwrap_or([1500.0, 920.0]);
    let mut viewport = egui::ViewportBuilder::default()
        .with_title("ImagineCAD")
        .with_inner_size(size)
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

/// The file a file manager asked to open: a plain path (Windows Explorer,
/// most Linux desktops) or a `file://` URI (desktop entries using %U).
fn launch_path(arg: &str) -> std::path::PathBuf {
    let Some(rest) = arg.strip_prefix("file://") else { return std::path::PathBuf::from(arg) };
    // Drop an optional host ("file://localhost/…") and decode %XX escapes.
    let path = if rest.starts_with('/') { rest } else { rest.find('/').map(|i| &rest[i..]).unwrap_or(rest) };
    let b = path.as_bytes();
    let mut out = Vec::with_capacity(b.len());
    let mut i = 0;
    while i < b.len() {
        if b[i] == b'%' && i + 2 < b.len() {
            if let Ok(v) = u8::from_str_radix(&path[i + 1..i + 3], 16) {
                out.push(v);
                i += 3;
                continue;
            }
        }
        out.push(b[i]);
        i += 1;
    }
    std::path::PathBuf::from(String::from_utf8_lossy(&out).into_owned())
}

#[cfg(test)]
mod launch_tests {
    #[test]
    fn file_uris_and_paths() {
        assert_eq!(super::launch_path("/home/a/part 1.dxf"), std::path::PathBuf::from("/home/a/part 1.dxf"));
        assert_eq!(super::launch_path("file:///home/a/part%201.dxf"), std::path::PathBuf::from("/home/a/part 1.dxf"));
        assert_eq!(super::launch_path("file://localhost/tmp/x.dwg"), std::path::PathBuf::from("/tmp/x.dwg"));
        assert_eq!(super::launch_path(r"C:\Users\a\x.dwg"), std::path::PathBuf::from(r"C:\Users\a\x.dwg"));
    }
}
