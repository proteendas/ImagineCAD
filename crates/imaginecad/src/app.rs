// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Proteen Das

//! The ImagineCAD main window.

use crate::canvas::{self, RenderOpts};
use crate::commands::{self, Command, Cx, Flow, GripCmd, Memory, PasteCmd, ViewReq, Want};
use crate::dialogs::Dialogs;
use crate::icons::{self, Icon};
use crate::ribbon::{self, RibbonState};
use crate::settings::Settings;
use crate::tab::Tab;
use crate::theme;
use eframe::egui::{self, pos2, vec2, Align2, Color32, CursorIcon, FontId, Key, PointerButton, Rect, RichText, Sense, Stroke};
use icad_core::entity::{Entity, Shape, SnapKind};
use icad_core::geom::*;
use icad_core::style::{aci_name, Color, LineType, LtRef, ACI_NAMES};
use icad_core::Document;
use std::path::{Path, PathBuf};

pub struct App {
    pub tabs: Vec<Tab>,
    pub active: usize,
    pub settings: Settings,
    pub cmd: Option<Box<dyn Command>>,
    pub last_cmd: Option<String>,
    pub mem: Memory,
    pub log: Vec<String>,
    pub input: String,
    pub ribbon_tab: usize,
    pub cursor: V2,
    pub snap: Option<(SnapKind, V2)>,
    pub box_start: Option<V2>,
    pub clipboard: Vec<Entity>,
    pub last_point: Option<V2>,
    pub view_req: Option<ViewReq>,
    pub pan_mode: bool,
    pub prop_editing: bool,
    pub dlg: Dialogs,
    pub untitled_counter: usize,
    pub window_title: String,
    pub allow_close: bool,
    pub ai_ready: bool,
    pub images: canvas::ImageCache,
    /// Angle (degrees) while the cursor is snapped by polar tracking.
    pub polar_hit: Option<f64>,
    /// Self-screenshot hook (docs / visual tests): IMAGINECAD_SCREENSHOT=out.png,
    /// optional IMAGINECAD_RUN="CMD;CMD" executed first (see `screenshot_hook`).
    pub shot: Option<(PathBuf, u32)>,
    pub chat: crate::chat::ChatPanel,
}

impl App {
    pub fn new(cc: &eframe::CreationContext<'_>, open: Option<PathBuf>) -> Self {
        theme::apply(&cc.egui_ctx);
        let mut settings = Settings::load();
        if let Some(k) = crate::secrets::load() {
            settings.convert.ai.api_key = k;
        }
        let mut app = Self {
            tabs: vec![],
            active: 0,
            settings,
            cmd: None,
            last_cmd: None,
            mem: Memory::default(),
            log: vec![
                "Welcome to ImagineCAD. Type a command (e.g. LINE or L), use the ribbon, or open the AI tab to convert a sketch.".into()
            ],
            input: String::new(),
            ribbon_tab: 0,
            cursor: V2::ZERO,
            snap: None,
            box_start: None,
            clipboard: vec![],
            last_point: None,
            view_req: None,
            pan_mode: false,
            prop_editing: false,
            dlg: Dialogs::default(),
            untitled_counter: 0,
            window_title: String::new(),
            allow_close: false,
            ai_ready: false,
            images: canvas::ImageCache::default(),
            polar_hit: None,
            shot: std::env::var_os("IMAGINECAD_SCREENSHOT").map(|p| (PathBuf::from(p), 0)),
            chat: Default::default(),
        };
        app.new_drawing();
        if let Some(p) = open {
            app.open_path(&p);
        }
        app
    }

    pub fn tab(&mut self) -> &mut Tab {
        &mut self.tabs[self.active]
    }

    pub fn new_document(&self) -> Document {
        let mut d = Document::new();
        if !self.settings.company.is_empty() {
            d.sheet.title.company = self.settings.company.clone();
        }
        d.sheet.title.drawn_by = self.settings.drawn_by.clone();
        d.sheet.title.date = icad_core::sheet::today();
        d
    }

    pub fn new_drawing(&mut self) {
        self.untitled_counter += 1;
        let doc = self.new_document();
        let mut t = Tab::new(doc, format!("Drawing{}", self.untitled_counter));
        t.dirty = false;
        self.tabs.push(t);
        self.active = self.tabs.len() - 1;
        self.view_req = Some(ViewReq::Sheet);
    }

    pub fn push_tab(&mut self, tab: Tab) {
        // Replace a pristine empty drawing instead of piling up tabs.
        if self.tabs.len() == 1 && !self.tabs[0].dirty && self.tabs[0].doc.entities.is_empty() && self.tabs[0].path.is_none() {
            self.tabs.clear();
        }
        self.tabs.push(tab);
        self.active = self.tabs.len() - 1;
    }

    pub fn open_path(&mut self, p: &Path) {
        let ext = p.extension().and_then(|e| e.to_str()).unwrap_or("").to_ascii_lowercase();
        if ["png", "jpg", "jpeg", "bmp", "tif", "tiff", "webp"].contains(&ext.as_str()) {
            self.open_image_to_cad(Some(p));
            return;
        }
        match icad_io::open(p) {
            Ok(doc) => {
                let name = p.file_stem().and_then(|s| s.to_str()).unwrap_or("Drawing").to_string();
                let mut t = Tab::new(doc, name);
                if ["icad", "dxf", "dwg"].contains(&ext.as_str()) {
                    t.path = Some(p.to_path_buf());
                }
                self.push_tab(t);
                self.settings.push_recent(p.to_path_buf());
                self.settings.save();
                self.log.push(format!("Opened {}", p.display()));
            }
            Err(e) => self.error(format!("Could not open {}: {e}", p.display())),
        }
    }

    pub fn error(&mut self, msg: String) {
        self.log.push(format!("Error: {msg}"));
        self.dlg.message = Some(("ImagineCAD".into(), msg));
    }

    // ------------------------------------------------------------ commands

    fn with_cmd(&mut self, f: impl FnOnce(&mut dyn Command, &mut Cx) -> Flow) {
        let Some(mut cmd) = self.cmd.take() else { return };
        let flow = {
            let mut cx = Cx { tab: &mut self.tabs[self.active], log: &mut self.log, mem: &mut self.mem, view_req: &mut self.view_req };
            f(cmd.as_mut(), &mut cx)
        };
        if flow == Flow::Continue {
            self.cmd = Some(cmd);
        } else {
            self.box_start = None;
        }
    }

    pub fn cancel(&mut self) {
        if self.cmd.take().is_some() {
            self.log.push("*Cancel*".into());
        } else {
            self.tab().selection.clear();
        }
        self.box_start = None;
        self.pan_mode = false;
    }

    /// Run a command line: first token is the command, the rest are fed as inputs.
    pub fn run(&mut self, line: &str) {
        let mut parts = line.split_whitespace();
        let Some(name) = parts.next() else { return };
        let rest: Vec<String> = parts.map(String::from).collect();
        let up = name.trim_start_matches('_').to_ascii_uppercase();
        if self.app_command(&up, &rest) {
            return;
        }
        let created: Option<Box<dyn Command>> = if up == "PASTECLIP" {
            if self.clipboard.is_empty() {
                self.log.push("Clipboard is empty.".into());
                return;
            }
            Some(Box::new(PasteCmd { items: self.clipboard.clone() }))
        } else {
            commands::create(&up)
        };
        match created {
            Some(c) => {
                self.cmd = None;
                self.log.push(format!("Command: {}", c.name()));
                self.last_cmd = Some(up.clone());
                self.cmd = Some(c);
                // Only entities that may be edited take part in noun-verb selection.
                let t = &mut self.tabs[self.active];
                let ed: Vec<u64> = t.selection.iter().copied().filter(|id| t.editable(*id)).collect();
                t.selection = ed;
                self.with_cmd(|c, cx| c.start(cx));
                for r in rest {
                    if self.cmd.is_none() {
                        break;
                    }
                    // `<ENTER>` lets scripts and command macros press Enter.
                    self.submit(if r.eq_ignore_ascii_case("<ENTER>") { "" } else { &r });
                }
            }
            None => self.log.push(format!("Unknown command \"{name}\". Press F1 for the command list.")),
        }
    }

    /// Commands handled by the application rather than the editor.
    fn app_command(&mut self, up: &str, args: &[String]) -> bool {
        let arg = args.first().map(|s| s.to_ascii_uppercase()).unwrap_or_default();
        match up {
            "NEW" => self.new_drawing(),
            "OPEN" => self.open_dialog(),
            "SAVE" | "QSAVE" => {
                self.save(false);
            }
            "SAVEAS" => {
                self.save(true);
            }
            "CLOSE" => self.request_close_tab(self.active),
            "EXPORT" | "EXP" => match arg.as_str() {
                "DXF" | "DWG" | "SVG" | "PDF" => self.export(&arg.to_lowercase()),
                _ => self.dlg.export = true,
            },
            "PLOT" | "PRINT" => self.export("pdf"),
            "PAGESETUP" | "TITLEBLOCK" | "TB" => self.dlg.page = true,
            "DIMSTYLE" | "D" | "DDIM" => self.dlg.dimstyle = true,
            "IMAGETOCAD" | "AI" | "SKETCH" => self.open_image_to_cad(None),
            "AISETUP" => self.dlg.ai_setup = Some(Default::default()),
            "BLOCKS" | "BLOCKSPALETTE" => self.dlg.blocks = true,
            "QSELECT" => self.dlg.qselect = Some(Default::default()),
            "UNITS" | "UN" => self.dlg.units = true,
            "TEXTEDIT" | "ED" | "DDEDIT" => {
                let sel = self.tabs[self.active].selection.first().copied();
                if !sel.is_some_and(|id| self.edit_text_of(id)) {
                    self.log.push("Select a text, multiline text, leader or dimension first, or double-click it.".into());
                }
            }
            "PURGE" | "PU" => {
                let t = self.tab();
                t.checkpoint();
                let (l, b) = t.doc.purge();
                self.log.push(format!("Purged {l} unused layer(s) and {b} unused block(s)."));
            }
            "OOPS" => {
                let items = std::mem::take(&mut self.mem.last_erased);
                if items.is_empty() {
                    self.log.push("Nothing to restore.".into());
                } else {
                    let t = self.tab();
                    t.checkpoint();
                    let n = items.len();
                    t.doc.entities.extend(items);
                    self.log.push(format!("{n} object(s) restored."));
                }
            }
            "LAYUNISO" => {
                let backup = std::mem::take(&mut self.mem.layiso_backup);
                let t = self.tab();
                t.checkpoint();
                for (name, vis) in backup {
                    if let Some(l) = t.doc.layer_mut(&name) {
                        l.visible = vis;
                    }
                }
            }
            "LAYON" => {
                let t = self.tab();
                t.checkpoint();
                t.doc.layers.iter_mut().for_each(|l| l.visible = true);
            }
            "POLAR" => self.settings.polar = !self.settings.polar,
            "IMAGEATTACH" | "IAT" | "ATTACH" => {
                if let Some(p) =
                    rfd::FileDialog::new().add_filter("Images", &["png", "jpg", "jpeg", "bmp", "tif", "tiff", "webp"]).pick_file()
                {
                    let aspect = image::image_dimensions(&p).map(|(w, h)| h as f64 / w.max(1) as f64).unwrap_or(0.75);
                    self.cmd = Some(Box::new(crate::commands_ext::ImagePlaceCmd { path: p.display().to_string(), aspect, pos: None }));
                    self.log.push("Command: IMAGEATTACH".into());
                }
            }
            "AISETTINGS" => self.dlg.ai_settings = true,
            "AICHAT" | "CHAT" | "ASSISTANT" => self.settings.show_chat = !self.settings.show_chat,
            "AIHELP" => self.dlg.ai_help = true,
            "LAYER" | "LA" => self.dlg.layers = true,
            "PROPERTIES" | "PR" | "PROPS" => {
                self.settings.show_properties = !self.settings.show_properties;
                self.settings.save();
            }
            "UNDO" | "U" => {
                let n = num_or(args, 1.0) as usize;
                for _ in 0..n.max(1) {
                    if !self.tab().undo() {
                        self.log.push("Nothing to undo.".into());
                        break;
                    }
                }
            }
            "REDO" => {
                if !self.tab().redo() {
                    self.log.push("Nothing to redo.".into());
                }
            }
            "SELECTALL" | "AI_SELALL" => {
                let t = self.tab();
                t.selection = t.doc.entities.iter().filter(|e| t.doc.is_visible(e) && !t.doc.is_locked(e)).map(|e| e.id).collect();
            }
            "GRID" => self.settings.grid = !self.settings.grid,
            "SNAP" => self.settings.snap = !self.settings.snap,
            "ORTHO" => self.settings.ortho = !self.settings.ortho,
            "OSNAP" => self.settings.osnap = !self.settings.osnap,
            "LWT" | "LWDISPLAY" => self.settings.lineweights = !self.settings.lineweights,
            "DYNMODE" => self.settings.dyn_input = !self.settings.dyn_input,
            "SHEET" => {
                let t = self.tab();
                t.doc.sheet.visible = !t.doc.sheet.visible;
                t.dirty = true;
            }
            "LAYOUT" => {
                let t = self.tab();
                t.layout = !t.layout;
                if t.layout {
                    self.view_req = Some(ViewReq::Sheet);
                }
            }
            "MODEL" => self.tab().layout = false,
            "REGEN" | "RE" | "REGENALL" => {}
            "HELP" | "?" => self.dlg.help = true,
            "ABOUT" => self.dlg.about = true,
            "COPYCLIP" => self.copy_clip(),
            "CUTCLIP" => {
                self.copy_clip();
                self.run("ERASE");
            }
            "PANINFO" | "PAN" | "P" => {
                self.pan_mode = true;
                self.log.push(
                    "Pan: drag with the left mouse button. Press Esc or Enter to exit. (Middle-drag or Alt+drag pans any time.)".into(),
                );
            }
            "LTSCALE" | "LTS" => {
                if let Some(v) = args.first().and_then(|a| commands::num(a)).filter(|v| *v > 0.0) {
                    let t = self.tab();
                    t.checkpoint();
                    t.doc.ltscale = v;
                } else {
                    let v = self.tabs[self.active].doc.ltscale;
                    self.log.push(format!("LTSCALE = {v}. Usage: LTSCALE 0.5"));
                }
            }
            "EXIT" | "QUIT" => {
                self.dlg.quit_requested = true;
            }
            _ => return false,
        }
        true
    }

    /// Submit a typed command-line entry (or Enter when empty).
    pub fn submit(&mut self, text: &str) {
        let text = text.trim();
        if self.cmd.is_none() {
            if text.is_empty() {
                if let Some(c) = self.last_cmd.clone() {
                    self.run(&c);
                }
            } else {
                self.run(text);
            }
            return;
        }
        if text.is_empty() {
            if self.cmd.as_ref().map(|c| c.wants()) == Some(Want::Select) {
                let ids = std::mem::take(&mut self.tab().selection);
                self.with_cmd(|c, cx| c.selected(cx, ids));
            } else {
                self.with_cmd(|c, cx| c.enter(cx));
            }
            return;
        }
        let wants = self.cmd.as_ref().map(|c| c.wants()).unwrap();
        if wants == Want::Point {
            let base = self.cmd.as_ref().and_then(|c| c.base());
            let last = base.or(self.last_point);
            if let Some(p) = commands::parse_point(text, last) {
                self.feed_point(p);
                return;
            }
            if let (Some(b), Some(d)) = (base, commands::num(text)) {
                // Direct distance entry along the cursor direction.
                let mut dir = (self.cursor - b).norm();
                if self.settings.ortho {
                    dir = ortho_dir(dir);
                }
                if dir == V2::ZERO {
                    dir = V2::new(1.0, 0.0);
                }
                self.feed_point(b + dir * d);
                return;
            }
        }
        if wants == Want::Select && text.eq_ignore_ascii_case("ALL") {
            self.app_command("SELECTALL", &[]);
            return;
        }
        let t = text.to_string();
        self.with_cmd(|c, cx| c.value(cx, &t));
    }

    fn feed_point(&mut self, p: V2) {
        self.last_point = Some(p);
        self.with_cmd(|c, cx| c.point(cx, p));
    }

    fn copy_clip(&mut self) {
        let t = &self.tabs[self.active];
        self.clipboard = t.selection.iter().filter_map(|id| t.doc.get(*id).cloned()).collect();
        self.log.push(format!("{} object(s) copied to clipboard", self.clipboard.len()));
    }

    // ------------------------------------------------------------ files

    pub fn open_dialog(&mut self) {
        let f = rfd::FileDialog::new()
            .add_filter("Drawings", &["icad", "dxf", "dwg"])
            .add_filter("Sketch images", &["png", "jpg", "jpeg", "bmp", "tif", "tiff", "webp"])
            .pick_file();
        if let Some(p) = f {
            self.open_path(&p);
        }
    }

    pub fn save(&mut self, save_as: bool) -> bool {
        let t = &self.tabs[self.active];
        let path = match (&t.path, save_as) {
            (Some(p), false) => Some(p.clone()),
            _ => rfd::FileDialog::new()
                .set_file_name(format!("{}.icad", t.name))
                .add_filter("ImagineCAD drawing", &["icad"])
                .add_filter("DWG drawing", &["dwg"])
                .add_filter("DXF drawing", &["dxf"])
                .save_file(),
        };
        let Some(mut path) = path else { return false };
        if path.extension().is_none() {
            path.set_extension("icad");
        }
        let opts = self.export_options();
        match icad_io::save(&self.tabs[self.active].doc, &path, &opts) {
            Ok(()) => {
                let t = self.tab();
                t.path = Some(path.clone());
                t.name = path.file_stem().and_then(|s| s.to_str()).unwrap_or("Drawing").to_string();
                t.dirty = false;
                self.settings.push_recent(path.clone());
                self.settings.save();
                self.log.push(format!("Saved {}", path.display()));
                true
            }
            Err(e) => {
                self.error(format!("Save failed: {e}"));
                false
            }
        }
    }

    pub fn export(&mut self, ext: &str) {
        if ext == "dwg" && icad_io::dwg::find_converter().is_none() {
            self.dlg.message = Some((
                "DWG export".into(),
                "DWG is a closed format. ImagineCAD writes DWG through a free converter, and none was found.\n\n\
                 • Install the free ODA File Converter (opendesign.com → ODA File Converter), or\n\
                 • install LibreDWG (provides dxf2dwg).\n\n\
                 Alternatively export DXF: every major CAD program opens DXF files directly."
                    .into(),
            ));
            return;
        }
        let t = &self.tabs[self.active];
        let label = match ext {
            "pdf" => "PDF",
            "dxf" => "DXF drawing",
            "dwg" => "DWG drawing",
            _ => "SVG image",
        };
        let Some(mut path) = rfd::FileDialog::new().set_file_name(format!("{}.{ext}", t.name)).add_filter(label, &[ext]).save_file() else {
            return;
        };
        if path.extension().is_none() {
            path.set_extension(ext);
        }
        let opts = self.export_options();
        match icad_io::save(&self.tabs[self.active].doc, &path, &opts) {
            Ok(()) => self.log.push(format!("Exported {}", path.display())),
            Err(e) => self.error(format!("Export failed: {e}")),
        }
    }

    pub fn export_options(&self) -> icad_io::ExportOptions {
        icad_io::ExportOptions {
            dxf: icad_io::DxfOptions {
                include_sheet: true,
                explode_dimensions: self.settings.explode_dims_dxf,
                version: if self.settings.dxf_r12 { icad_io::DxfVersion::R12 } else { icad_io::DxfVersion::R2000 },
            },
            plot: icad_io::PlotOptions { monochrome: self.settings.monochrome_plot, include_sheet: true, lineweights: true },
        }
    }

    pub fn request_close_tab(&mut self, i: usize) {
        if self.tabs[i].dirty {
            self.dlg.close_tab = Some(i);
        } else {
            self.close_tab(i);
        }
    }

    pub fn close_tab(&mut self, i: usize) {
        self.cmd = None;
        self.tabs.remove(i);
        if self.tabs.is_empty() {
            self.new_drawing();
        }
        self.active = self.active.min(self.tabs.len() - 1);
    }

    // ------------------------------------------------------------ input

    fn keyboard(&mut self, ctx: &egui::Context) {
        let typing_elsewhere = ctx.wants_keyboard_input();
        let events = ctx.input(|i| i.events.clone());
        let wants_text = self.cmd.as_ref().map(|c| c.wants()) == Some(Want::Text);
        for ev in events {
            match ev {
                egui::Event::Key { key, pressed: true, modifiers, .. } => {
                    if modifiers.command {
                        match key {
                            // In a text box these undo the typing, not the drawing.
                            Key::Z | Key::Y if typing_elsewhere => false,
                            Key::Z if modifiers.shift => self.app_command("REDO", &[]),
                            Key::Z => self.app_command("UNDO", &[]),
                            Key::Y => self.app_command("REDO", &[]),
                            Key::S if modifiers.shift => {
                                self.save(true);
                                true
                            }
                            Key::S => {
                                self.save(false);
                                true
                            }
                            Key::O => self.app_command("OPEN", &[]),
                            Key::N => self.app_command("NEW", &[]),
                            Key::P => self.app_command("PLOT", &[]),
                            Key::A if !typing_elsewhere => self.app_command("SELECTALL", &[]),
                            Key::I => self.app_command("IMAGETOCAD", &[]),
                            _ => false,
                        };
                        continue;
                    }
                    match key {
                        Key::F1 => self.dlg.help = true,
                        Key::F3 => self.settings.osnap = !self.settings.osnap,
                        Key::F7 => self.settings.grid = !self.settings.grid,
                        Key::F8 => self.settings.ortho = !self.settings.ortho,
                        Key::F9 => self.settings.snap = !self.settings.snap,
                        Key::F12 => self.settings.dyn_input = !self.settings.dyn_input,
                        Key::F10 => self.settings.polar = !self.settings.polar,
                        _ => {}
                    }
                    if typing_elsewhere {
                        continue;
                    }
                    match key {
                        Key::Escape => {
                            if !self.input.is_empty() {
                                self.input.clear();
                            } else {
                                self.cancel();
                            }
                        }
                        Key::Enter => {
                            let s = std::mem::take(&mut self.input);
                            if self.pan_mode && s.is_empty() {
                                self.pan_mode = false;
                            } else {
                                self.submit(&s);
                            }
                        }
                        Key::Space if !wants_text => {
                            let s = std::mem::take(&mut self.input);
                            self.submit(&s);
                        }
                        Key::Backspace => {
                            self.input.pop();
                        }
                        Key::Delete if self.cmd.is_none() && !self.tabs[self.active].selection.is_empty() => {
                            self.run("ERASE");
                        }
                        Key::Tab => {
                            if let Some(s) = self.suggestions().first() {
                                self.input = s.0.to_string();
                            }
                        }
                        _ => {}
                    }
                }
                egui::Event::Text(t) if !typing_elsewhere => {
                    if t == " " && !wants_text {
                        continue;
                    }
                    self.input.push_str(&t);
                }
                egui::Event::Copy if !typing_elsewhere => self.copy_clip(),
                egui::Event::Cut if !typing_elsewhere => {
                    self.copy_clip();
                    self.run("ERASE");
                }
                egui::Event::Paste(_) if !typing_elsewhere => self.run("PASTECLIP"),
                _ => {}
            }
        }
    }

    fn suggestions(&self) -> Vec<(&'static str, &'static str)> {
        if self.cmd.is_some() || self.input.is_empty() {
            return vec![];
        }
        let q = self.input.to_ascii_uppercase();
        let mut v: Vec<_> = commands::COMMANDS.iter().filter(|(n, a)| *a == q || n.starts_with(&q)).copied().collect();
        v.sort_by_key(|(n, a)| (*a != q, n.len()));
        v.truncate(8);
        v
    }

    // ------------------------------------------------------------ canvas

    fn canvas(&mut self, ui: &mut egui::Ui) {
        let (resp, painter) = ui.allocate_painter(ui.available_size(), Sense::click_and_drag());
        let rect = resp.rect;
        let ctx = ui.ctx().clone();
        {
            let t = &mut self.tabs[self.active];
            t.view.rect = rect;
            if t.needs_fit > 0 {
                t.needs_fit -= 1;
                let ext = t.doc.extents();
                if ext.is_empty() {
                    let b = t.doc.sheet.bounds();
                    t.view.fit(&b);
                } else {
                    t.view.fit(&ext.union(&if t.doc.sheet.visible { t.doc.sheet.bounds() } else { ext }));
                }
            }
        }
        let settling = self.tabs[self.active].needs_fit > 0;
        if self.view_req.is_some() && !settling {
            let t = &mut self.tabs[self.active];
            t.view_history.push((t.view.center, t.view.zoom));
            if t.view_history.len() > 50 {
                t.view_history.remove(0);
            }
        }
        let req = if settling { self.view_req.clone() } else { self.view_req.take() };
        if let Some(req) = req {
            let t = &mut self.tabs[self.active];
            match req {
                ViewReq::Extents => {
                    let mut e = t.doc.extents();
                    if e.is_empty() || t.layout {
                        e = e.union(&t.doc.sheet.bounds());
                    }
                    t.view.fit(&e);
                }
                ViewReq::Sheet => {
                    let b = t.doc.sheet.bounds().union(&t.doc.extents());
                    t.view.fit(&b);
                }
                ViewReq::Window(b) => t.view.fit(&b),
                ViewReq::Previous => {
                    // The current view was just pushed; drop it and restore the one before.
                    t.view_history.pop();
                    match t.view_history.pop() {
                        Some((c, z)) => {
                            t.view.center = c;
                            t.view.zoom = z;
                        }
                        None => self.log.push("No previous view.".into()),
                    }
                }
            }
        }

        // Navigation: wheel zoom, pinch, middle/Alt/pan-mode drag.
        let hover = resp.hover_pos();
        if let Some(h) = hover {
            let (scroll, zoom_d, middle_double) =
                ctx.input(|i| (i.smooth_scroll_delta, i.zoom_delta(), i.pointer.button_double_clicked(PointerButton::Middle)));
            let t = &mut self.tabs[self.active];
            if scroll.y.abs() > 0.0 {
                t.view.zoom_at(h, (1.0025f64).powf(scroll.y as f64));
            }
            if scroll.x.abs() > 0.0 {
                t.view.pan_pixels(scroll.x, 0.0);
            }
            if (zoom_d - 1.0).abs() > 1e-4 {
                t.view.zoom_at(h, zoom_d as f64);
            }
            if middle_double {
                self.view_req = Some(ViewReq::Extents);
            }
        }
        let (middle, alt, delta, primary_down) =
            ctx.input(|i| (i.pointer.middle_down(), i.modifiers.alt, i.pointer.delta(), i.pointer.primary_down()));
        let panning = resp.hovered() && (middle || ((alt || self.pan_mode) && primary_down));
        if panning || (resp.dragged_by(PointerButton::Middle)) {
            self.tabs[self.active].view.pan_pixels(delta.x, delta.y);
        }

        let wants = self.cmd.as_ref().map(|c| c.wants());
        let base = self.cmd.as_ref().and_then(|c| c.base());
        let layout = self.tabs[self.active].layout;

        // Cursor → snapped world point.
        self.snap = None;
        if let Some(h) = hover {
            let t = &self.tabs[self.active];
            let raw = t.view.to_world(h);
            let mut p = raw;
            if self.settings.osnap && wants == Some(Want::Point) {
                self.snap = canvas::find_snap(&t.doc, &t.view, raw, base, &self.settings.osnap_modes, 12.0);
            }
            if let Some((_, s)) = self.snap {
                p = s;
            } else {
                if self.settings.snap {
                    let sp = self.settings.snap_spacing.max(1e-9);
                    p = V2::new((p.x / sp).round() * sp, (p.y / sp).round() * sp);
                }
                self.polar_hit = None;
                if let (true, false, Some(b)) = (self.settings.polar, self.settings.ortho, base) {
                    if let Some((q, a)) = canvas::polar_track(&t.view, b, p, self.settings.polar_angle.max(1.0), 8.0) {
                        p = q;
                        self.polar_hit = Some(a);
                    }
                }
                if let (true, Some(b)) = (self.settings.ortho, base) {
                    let d = p - b;
                    p = if d.x.abs() >= d.y.abs() { V2::new(p.x, b.y) } else { V2::new(b.x, p.y) };
                }
            }
            self.cursor = p;
        }

        // ---- Draw.
        painter.rect_filled(rect, 0.0, if layout { theme::PAPER_SURROUND } else { theme::CANVAS });
        {
            let t = &self.tabs[self.active];
            if self.settings.grid {
                canvas::draw_grid(&painter, &t.view, layout);
            }
            canvas::draw_sheet(&painter, &t.view, &t.doc, layout, self.settings.lineweights);
        }
        let raw = hover.map(|h| self.tabs[self.active].view.to_world(h));
        let hover_id = match (raw, wants) {
            (Some(r), None | Some(Want::Select) | Some(Want::Entity)) if !panning => {
                let t = &self.tabs[self.active];
                t.doc.pick(r, t.view.px(5.0)).filter(|id| t.editable(*id))
            }
            _ => None,
        };
        {
            let t = &self.tabs[self.active];
            let o = RenderOpts { on_paper: layout, lineweights: self.settings.lineweights };
            canvas::draw_document(&painter, &t.view, &t.doc, &t.selection, hover_id, &o, &mut self.images);
            if let Some(c) = &self.cmd {
                if hover.is_some() {
                    canvas::draw_preview(&painter, &t.view, &t.doc, &c.preview(&t.doc, self.cursor), layout);
                }
            }
            if self.cmd.is_none() {
                canvas::draw_grips(&painter, &t.view, &t.doc, &t.selection, None);
            }
        }

        // Selection window.
        if let (Some(start), Some(r)) = (self.box_start, raw) {
            let t = &self.tabs[self.active];
            let a = t.view.to_screen(start);
            let b = t.view.to_screen(r);
            let crossing = r.x < start.x;
            let rr = Rect::from_two_pos(a, b);
            painter.rect_filled(rr, 0.0, if crossing { theme::CROSSING_FILL } else { theme::WINDOW_FILL });
            let col = if crossing { Color32::from_rgb(80, 220, 120) } else { Color32::from_rgb(90, 160, 255) };
            if crossing {
                let mut shapes = vec![];
                let st = canvas::Style { color: col, width: 1.0, dash: vec![4.0 / t.view.zoom, -3.0 / t.view.zoom] };
                let poly = icad_core::Prim::Poly(BBox::from_pts(start, r).corners().to_vec(), true);
                canvas::prim_shapes(&t.view, &poly, &st, &mut shapes);
                painter.extend(shapes);
            } else {
                painter.rect_stroke(rr, 0.0, Stroke::new(1.0f32, col), egui::StrokeKind::Middle);
            }
        }

        if let Some(h) = hover {
            let t = &self.tabs[self.active];
            if let Some((k, p)) = self.snap {
                canvas::draw_snap_marker(&painter, t.view.to_screen(p), k);
            }
            if let (Some(a), Some(b)) = (self.polar_hit, base) {
                canvas::draw_tracking_ray(&painter, &t.view, b, a, rect);
                let d = self.cursor.dist(b);
                painter.text(
                    t.view.to_screen(self.cursor) + vec2(14.0, -22.0),
                    Align2::LEFT_BOTTOM,
                    format!("Polar: {} < {}°", commands::fmt(d), commands::fmt(a)),
                    FontId::proportional(11.5),
                    Color32::from_rgb(120, 255, 140),
                );
            }
            if self.pan_mode || panning {
                ctx.set_cursor_icon(CursorIcon::Grabbing);
            } else {
                ctx.set_cursor_icon(CursorIcon::None);
                let cs = if wants == Some(Want::Point) { t.view.to_screen(self.cursor) } else { h };
                canvas::draw_crosshair(&painter, rect, cs, layout, wants != Some(Want::Point));
            }
            // Dynamic input tooltip.
            if self.settings.dyn_input && !panning {
                let mut lines = vec![];
                if let Some(c) = &self.cmd {
                    let cx_prompt = {
                        let cx = Cx { tab: &mut self.tabs[self.active], log: &mut vec![], mem: &mut self.mem, view_req: &mut None };
                        c.prompt(&cx)
                    };
                    lines.push(cx_prompt);
                }
                if let Some(b) = base {
                    let d = self.cursor - b;
                    lines.push(format!("{}  <  {}°", commands::fmt(d.len()), commands::fmt(deg(norm_angle(d.angle())))));
                } else if wants == Some(Want::Point) {
                    lines.push(format!("{}, {}", commands::fmt(self.cursor.x), commands::fmt(self.cursor.y)));
                }
                if !self.input.is_empty() && self.cmd.is_some() {
                    lines.push(format!("› {}", self.input));
                }
                if !lines.is_empty() {
                    let txt = lines.join("\n");
                    let galley = painter.layout_no_wrap(txt, FontId::proportional(12.0), theme::TEXT);
                    let pos = h + vec2(18.0, 18.0);
                    let r = Rect::from_min_size(pos, galley.size() + vec2(10.0, 6.0));
                    painter.rect_filled(r, 3.0, Color32::from_rgba_unmultiplied(40, 48, 60, 235));
                    painter.rect_stroke(r, 3.0, Stroke::new(1.0f32, Color32::from_rgb(80, 96, 120)), egui::StrokeKind::Middle);
                    painter.galley(pos + vec2(5.0, 3.0), galley, theme::TEXT);
                }
            }
        }
        canvas::draw_ucs(&painter, rect, layout);
        let t = &self.tabs[self.active];
        painter.text(
            rect.left_top() + vec2(8.0, 6.0),
            Align2::LEFT_TOP,
            if layout { "[Layout] [Paper]" } else { "[Model] [Top] [2D Wireframe]" },
            FontId::proportional(11.5),
            if layout { Color32::from_gray(40) } else { theme::TEXT_DIM },
        );
        let _ = t;

        // ---- Clicks.
        if panning || self.pan_mode {
            return;
        }
        if resp.clicked_by(PointerButton::Secondary) {
            if self.cmd.is_some() {
                self.submit("");
            } else {
                self.dlg.context_at = hover;
            }
            return;
        }
        let Some(r) = raw else { return };
        let shift = ctx.input(|i| i.modifiers.shift);
        let drag_started = resp.drag_started_by(PointerButton::Primary);
        let drag_stopped = resp.drag_stopped_by(PointerButton::Primary);
        let clicked = resp.clicked_by(PointerButton::Primary);
        if wants.is_none() && resp.double_clicked() {
            if let Some(id) = hover_id {
                if self.edit_text_of(id) {
                    return;
                }
            }
        }
        match wants {
            Some(Want::Point) => {
                if clicked {
                    self.feed_point(self.cursor);
                }
            }
            Some(Want::Entity) => {
                if clicked {
                    if let Some(id) = hover_id {
                        self.with_cmd(|c, cx| c.entity(cx, id, r));
                    }
                }
            }
            Some(Want::Select) | None => {
                let idle = wants.is_none();
                if self.box_start.is_some() && (clicked || drag_stopped) {
                    let start = self.box_start.take().unwrap();
                    if start.dist(r) * self.tabs[self.active].view.zoom > 3.0 {
                        self.finish_box(start, r, shift);
                        return;
                    }
                }
                if clicked || drag_started {
                    if idle && clicked {
                        let t = &self.tabs[self.active];
                        if let Some((id, gi, gp)) =
                            canvas::grip_at(&t.doc, &t.view, &t.selection, resp.interact_pointer_pos().unwrap_or(pos2(0.0, 0.0)))
                        {
                            self.cmd = Some(Box::new(GripCmd { id, grip: gi, from: gp }));
                            return;
                        }
                    }
                    match hover_id {
                        Some(id) if clicked => {
                            let sel = &mut self.tabs[self.active].selection;
                            if shift {
                                sel.retain(|x| *x != id);
                            } else if !sel.contains(&id) {
                                sel.push(id);
                            }
                        }
                        _ => {
                            if self.box_start.is_none() {
                                self.box_start = Some(r);
                            }
                        }
                    }
                }
            }
            _ => {}
        }
    }

    fn finish_box(&mut self, a: V2, b: V2, remove: bool) {
        let t = &mut self.tabs[self.active];
        let crossing = b.x < a.x;
        t.last_window = if crossing { Some(BBox::from_pts(a, b)) } else { None };
        let ids: Vec<u64> = t.doc.select_box(&BBox::from_pts(a, b), crossing).into_iter().filter(|id| t.editable(*id)).collect();
        if remove {
            t.selection.retain(|x| !ids.contains(x));
        } else {
            for id in ids {
                if !t.selection.contains(&id) {
                    t.selection.push(id);
                }
            }
        }
    }

    // ------------------------------------------------------------ panels

    fn title_bar(&mut self, ctx: &egui::Context) {
        egui::TopBottomPanel::top("titlebar")
            .exact_height(30.0)
            .frame(egui::Frame::NONE.fill(theme::TITLE_BAR).inner_margin(egui::Margin::symmetric(6, 3)))
            .show(ctx, |ui| {
                ui.horizontal_centered(|ui| {
                    let menu_btn = egui::Button::new(RichText::new(" IC ").strong().color(Color32::WHITE).size(14.0))
                        .fill(theme::BRAND)
                        .corner_radius(3);
                    egui::menu::menu_custom_button(ui, menu_btn, |ui| {
                        ui.set_min_width(220.0);
                        let run = |ui: &mut egui::Ui, label: &str, cmd: &str, me: &mut App| {
                            if ui.button(label).clicked() {
                                me.run(cmd);
                                ui.close_menu();
                            }
                        };
                        run(ui, "New drawing        Ctrl+N", "NEW", self);
                        run(ui, "Open…              Ctrl+O", "OPEN", self);
                        run(ui, "Save               Ctrl+S", "SAVE", self);
                        run(ui, "Save As…", "SAVEAS", self);
                        ui.separator();
                        run(ui, "Image to CAD (AI)…  Ctrl+I", "IMAGETOCAD", self);
                        ui.separator();
                        run(ui, "Plot to PDF…       Ctrl+P", "PLOT", self);
                        run(ui, "Export DXF…", "EXPORT DXF", self);
                        run(ui, "Export DWG…", "EXPORT DWG", self);
                        run(ui, "Export SVG…", "EXPORT SVG", self);
                        ui.separator();
                        let recent = self.settings.recent.clone();
                        ui.menu_button("Recent drawings", |ui| {
                            if recent.is_empty() {
                                ui.label("(none)");
                            }
                            for p in recent {
                                if ui
                                    .button(p.file_name().and_then(|n| n.to_str()).unwrap_or("?"))
                                    .on_hover_text(p.display().to_string())
                                    .clicked()
                                {
                                    self.open_path(&p);
                                    ui.close_menu();
                                }
                            }
                        });
                        ui.separator();
                        run(ui, "About ImagineCAD", "ABOUT", self);
                        run(ui, "Close drawing", "CLOSE", self);
                        run(ui, "Exit", "EXIT", self);
                    });
                    ui.add_space(6.0);
                    for (icon, tip, cmd) in [
                        (Icon::New, "New (Ctrl+N)", "NEW"),
                        (Icon::Open, "Open (Ctrl+O)", "OPEN"),
                        (Icon::Save, "Save (Ctrl+S)", "SAVE"),
                        (Icon::Undo, "Undo (Ctrl+Z)", "UNDO"),
                        (Icon::Redo, "Redo (Ctrl+Y)", "REDO"),
                        (Icon::Pdf, "Plot to PDF (Ctrl+P)", "PLOT"),
                    ] {
                        let (r, resp) = ui.allocate_exact_size(vec2(24.0, 22.0), Sense::click());
                        if resp.hovered() {
                            ui.painter().rect_filled(r, 3.0, theme::HOVER);
                        }
                        icons::draw(ui.painter(), r.shrink(3.0), icon, theme::TEXT);
                        if resp.on_hover_text(tip).clicked() {
                            self.run(cmd);
                        }
                    }
                    let title = format!("ImagineCAD  —  {}", self.tabs[self.active].title());
                    let center = ui.max_rect().center();
                    ui.painter().text(pos2(center.x, center.y), Align2::CENTER_CENTER, title, FontId::proportional(13.0), theme::TEXT);
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui.add(egui::Button::new("?").frame(false)).on_hover_text("Help (F1)").clicked() {
                            self.dlg.help = true;
                        }
                        if ui.add(egui::Button::new(RichText::new("Image to CAD").color(theme::AI_ACCENT)).frame(false)).clicked() {
                            self.run("IMAGETOCAD");
                        }
                    });
                });
            });
    }

    fn ribbon(&mut self, ctx: &egui::Context) {
        egui::TopBottomPanel::top("ribbon")
            .frame(egui::Frame::NONE.fill(theme::RIBBON_TABS).inner_margin(egui::Margin { left: 4, right: 4, top: 3, bottom: 0 }))
            .show(ctx, |ui| {
                let mut out = vec![];
                let (grid, lwt, props, ai_ready) =
                    (self.settings.grid, self.settings.lineweights, self.settings.show_properties, self.ai_ready);
                let t = &mut self.tabs[self.active];
                let (sheet, layout) = (t.doc.sheet.visible, t.layout);
                let mut layer_ui = |ui: &mut egui::Ui| layer_combo(ui, t);
                let st = RibbonState { grid, lwt, props, sheet, layout, ai_ready, layer_ui: &mut layer_ui };
                ribbon::show(ui, &mut self.ribbon_tab, st, &mut out);
                for c in out {
                    self.run(&c);
                }
            });
    }

    fn file_tabs(&mut self, ctx: &egui::Context) {
        egui::TopBottomPanel::top("filetabs")
            .exact_height(26.0)
            .frame(egui::Frame::NONE.fill(theme::TITLE_BAR).inner_margin(egui::Margin::symmetric(4, 2)))
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing.x = 2.0;
                    let mut close = None;
                    for i in 0..self.tabs.len() {
                        let sel = i == self.active;
                        let fill = if sel { theme::CANVAS } else { theme::RIBBON_TABS };
                        egui::Frame::NONE
                            .fill(fill)
                            .corner_radius(egui::CornerRadius { nw: 3, ne: 3, sw: 0, se: 0 })
                            .inner_margin(egui::Margin::symmetric(8, 2))
                            .show(ui, |ui| {
                                ui.horizontal(|ui| {
                                    let lab = ui.add(
                                        egui::Label::new(RichText::new(self.tabs[i].title()).color(if sel {
                                            Color32::WHITE
                                        } else {
                                            theme::TEXT_DIM
                                        }))
                                        .sense(Sense::click()),
                                    );
                                    if lab.clicked() && !sel {
                                        self.cmd = None;
                                        self.active = i;
                                    }
                                    if ui
                                        .add(egui::Button::new(RichText::new("×").size(12.0)).frame(false))
                                        .on_hover_text("Close")
                                        .clicked()
                                    {
                                        close = Some(i);
                                    }
                                });
                            });
                    }
                    if ui.add(egui::Button::new(RichText::new(" + ").strong()).frame(false)).on_hover_text("New drawing").clicked() {
                        self.new_drawing();
                    }
                    if let Some(i) = close {
                        self.request_close_tab(i);
                    }
                });
            });
    }

    fn command_line(&mut self, ctx: &egui::Context) {
        egui::TopBottomPanel::bottom("cmdline")
            .exact_height(84.0)
            .frame(egui::Frame::NONE.fill(theme::CMDLINE).inner_margin(egui::Margin::symmetric(8, 4)))
            .show(ctx, |ui| {
                let start = self.log.len().saturating_sub(3);
                for line in &self.log[start..] {
                    let col = if line.starts_with("Error") { theme::ERR } else { theme::TEXT_DIM };
                    ui.label(RichText::new(line).color(col).monospace().size(12.0));
                }
                let prompt = match &self.cmd {
                    Some(c) => {
                        let name = c.name().to_string();
                        let cx = Cx { tab: &mut self.tabs[self.active], log: &mut vec![], mem: &mut self.mem, view_req: &mut None };
                        format!("{name} {}", c.prompt(&cx))
                    }
                    None => "Type a command".into(),
                };
                let frame = egui::Frame::NONE
                    .fill(Color32::from_rgb(26, 31, 38))
                    .stroke(Stroke::new(1.0f32, Color32::from_rgb(70, 84, 104)))
                    .corner_radius(2)
                    .inner_margin(egui::Margin::symmetric(6, 3));
                let r = frame.show(ui, |ui| {
                    ui.set_width(ui.available_width());
                    ui.horizontal(|ui| {
                        ui.label(RichText::new("›_").color(theme::ACCENT).monospace());
                        ui.label(
                            RichText::new(&prompt)
                                .color(if self.cmd.is_some() { Color32::from_rgb(140, 200, 255) } else { theme::TEXT_DIM })
                                .monospace(),
                        );
                        let blink = (ctx.input(|i| i.time) * 2.0) as i64 % 2 == 0;
                        ui.label(
                            RichText::new(format!("{}{}", self.input, if blink { "▏" } else { " " })).color(Color32::WHITE).monospace(),
                        );
                    });
                });
                // Autocomplete list above the command line.
                let sugg = self.suggestions();
                if !sugg.is_empty() {
                    let pos = r.response.rect.left_top() - vec2(0.0, 8.0 + 20.0 * sugg.len() as f32);
                    egui::Area::new(egui::Id::new("cmd_suggest")).fixed_pos(pos).order(egui::Order::Foreground).show(ctx, |ui| {
                        egui::Frame::popup(ui.style()).show(ui, |ui| {
                            for (n, a) in sugg {
                                let label = if a.is_empty() { n.to_string() } else { format!("{n}  ({a})") };
                                if ui.selectable_label(false, RichText::new(label).monospace()).clicked() {
                                    self.input.clear();
                                    self.run(n);
                                }
                            }
                        });
                    });
                }
            });
    }

    fn status_bar(&mut self, ctx: &egui::Context) {
        egui::TopBottomPanel::bottom("status")
            .exact_height(26.0)
            .frame(egui::Frame::NONE.fill(theme::STATUS).inner_margin(egui::Margin::symmetric(6, 2)))
            .show(ctx, |ui| {
                ui.horizontal_centered(|ui| {
                    let layout = self.tabs[self.active].layout;
                    if ui.selectable_label(!layout, "Model").clicked() {
                        self.tabs[self.active].layout = false;
                    }
                    if ui.selectable_label(layout, "Layout").clicked() {
                        self.run("LAYOUT");
                        self.tabs[self.active].layout = true;
                    }
                    ui.separator();
                    ui.label(
                        RichText::new(format!("{:>10}, {:>10}", commands::fmt(self.cursor.x), commands::fmt(self.cursor.y)))
                            .monospace()
                            .color(theme::TEXT),
                    );
                    ui.separator();
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        let sc = self.tabs[self.active].doc.sheet.scale_label();
                        if ui.button(format!("Scale {sc}")).on_hover_text("Sheet / annotation scale — click for Page Setup").clicked() {
                            self.dlg.page = true;
                        }
                        let chip = |ui: &mut egui::Ui, label: &str, on: bool, tip: &str| -> egui::Response {
                            let txt = RichText::new(label).size(11.5).color(if on { Color32::WHITE } else { theme::TEXT_DIM });
                            ui.add(egui::Button::new(txt).fill(if on { theme::ACTIVE } else { Color32::TRANSPARENT }).corner_radius(2))
                                .on_hover_text(tip)
                        };
                        if chip(ui, "DYN", self.settings.dyn_input, "Dynamic input tooltip (F12)").clicked() {
                            self.settings.dyn_input ^= true;
                        }
                        if chip(ui, "LWT", self.settings.lineweights, "Show lineweights").clicked() {
                            self.settings.lineweights ^= true;
                        }
                        let os = chip(ui, "OSNAP", self.settings.osnap, "Object snap (F3) — right-click for modes");
                        if os.clicked() {
                            self.settings.osnap ^= true;
                        }
                        os.context_menu(|ui| {
                            let m = &mut self.settings.osnap_modes;
                            ui.checkbox(&mut m.end, "Endpoint");
                            ui.checkbox(&mut m.mid, "Midpoint");
                            ui.checkbox(&mut m.center, "Center");
                            ui.checkbox(&mut m.quadrant, "Quadrant");
                            ui.checkbox(&mut m.intersection, "Intersection");
                            ui.checkbox(&mut m.perpendicular, "Perpendicular");
                            ui.checkbox(&mut m.node, "Node");
                            ui.checkbox(&mut m.nearest, "Nearest");
                        });
                        let pl = chip(ui, "POLAR", self.settings.polar, "Polar tracking (F10) — right-click for the angle increment");
                        if pl.clicked() {
                            self.settings.polar ^= true;
                        }
                        pl.context_menu(|ui| {
                            for a in [90.0, 45.0, 30.0, 22.5, 15.0, 10.0, 5.0] {
                                if ui.radio(self.settings.polar_angle == a, format!("{a}°")).clicked() {
                                    self.settings.polar_angle = a;
                                    ui.close_menu();
                                }
                            }
                        });
                        if chip(ui, "ORTHO", self.settings.ortho, "Ortho mode (F8)").clicked() {
                            self.settings.ortho ^= true;
                        }
                        let sn = chip(ui, "SNAP", self.settings.snap, "Grid snap (F9) — right-click for spacing");
                        if sn.clicked() {
                            self.settings.snap ^= true;
                        }
                        sn.context_menu(|ui| {
                            ui.horizontal(|ui| {
                                ui.label("Snap spacing");
                                ui.add(egui::DragValue::new(&mut self.settings.snap_spacing).speed(0.5).range(0.001..=1e6));
                            });
                        });
                        if chip(ui, "GRID", self.settings.grid, "Grid display (F7)").clicked() {
                            self.settings.grid ^= true;
                        }
                    });
                });
            });
    }

    fn properties(&mut self, ctx: &egui::Context) {
        if !self.settings.show_properties {
            return;
        }
        egui::SidePanel::left("properties")
            .resizable(true)
            .default_width(250.0)
            .min_width(200.0)
            .frame(egui::Frame::NONE.fill(theme::PANEL).inner_margin(egui::Margin::same(8)))
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    ui.label(RichText::new("PROPERTIES").strong().color(theme::TEXT));
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui.add(egui::Button::new("×").frame(false)).clicked() {
                            self.settings.show_properties = false;
                        }
                    });
                });
                ui.separator();
                egui::ScrollArea::vertical().show(ui, |ui| {
                    let changed = properties_ui(ui, &mut self.tabs[self.active], &mut self.prop_editing);
                    if changed {
                        ui.ctx().request_repaint();
                    }
                });
            });
        let idle = ctx.input(|i| !i.pointer.any_down()) && !ctx.wants_keyboard_input();
        if idle {
            self.prop_editing = false;
        }
    }

    fn context_menu(&mut self, ctx: &egui::Context) {
        let Some(at) = self.dlg.context_at else { return };
        let mut close = false;
        let area = egui::Area::new(egui::Id::new("canvas_ctx")).fixed_pos(at).order(egui::Order::Foreground).show(ctx, |ui| {
            egui::Frame::popup(ui.style()).show(ui, |ui| {
                ui.set_min_width(180.0);
                let has_sel = !self.tabs[self.active].selection.is_empty();
                let mut item = |ui: &mut egui::Ui, label: &str, cmd: &str, enabled: bool, me: &mut App| {
                    if ui.add_enabled(enabled, egui::Button::new(label).frame(false)).clicked() {
                        me.run(cmd);
                        close = true;
                    }
                };
                if let Some(last) = self.last_cmd.clone() {
                    item(ui, &format!("Repeat {last}"), &last, true, self);
                    ui.separator();
                }
                item(ui, "Move", "MOVE", has_sel, self);
                item(ui, "Copy", "COPY", has_sel, self);
                item(ui, "Rotate", "ROTATE", has_sel, self);
                item(ui, "Erase", "ERASE", has_sel, self);
                ui.separator();
                item(ui, "Copy to clipboard", "COPYCLIP", has_sel, self);
                item(ui, "Paste", "PASTECLIP", !self.clipboard.is_empty(), self);
                ui.separator();
                item(ui, "Undo", "UNDO", true, self);
                item(ui, "Select all", "SELECTALL", true, self);
                item(ui, "Zoom extents", "ZOOM E", true, self);
                item(ui, "Properties", "PROPERTIES", true, self);
            });
        });
        let clicked_elsewhere = ctx.input(|i| i.pointer.any_pressed()) && !area.response.hovered() && !area.response.contains_pointer();
        if close || clicked_elsewhere || ctx.input(|i| i.key_pressed(Key::Escape)) {
            self.dlg.context_at = None;
        }
    }

    fn handle_drops(&mut self, ctx: &egui::Context) {
        let dropped = ctx.input(|i| i.raw.dropped_files.clone());
        for f in dropped {
            if let Some(p) = f.path {
                self.open_path(&p);
            }
        }
    }

    fn handle_close(&mut self, ctx: &egui::Context) {
        let req = ctx.input(|i| i.viewport().close_requested()) || std::mem::take(&mut self.dlg.quit_requested);
        if req {
            if self.tabs.iter().any(|t| t.dirty) && !self.allow_close {
                ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
                self.dlg.confirm_quit = true;
            } else {
                self.settings.save();
                ctx.send_viewport_cmd(egui::ViewportCommand::Close);
            }
        }
    }
}

impl App {
    fn screenshot_hook(&mut self, ctx: &egui::Context) {
        let Some((path, frame)) = self.shot.clone() else { return };
        self.shot = Some((path.clone(), frame + 1));
        if frame == 3 {
            if let Ok(cmds) = std::env::var("IMAGINECAD_RUN") {
                for c in cmds.split(';') {
                    self.run(c.trim());
                }
            }
        }
        // IMAGINECAD_SCREENSHOT_FRAMES: frames to wait (e.g. for a model list to load).
        let at = std::env::var("IMAGINECAD_SCREENSHOT_FRAMES").ok().and_then(|s| s.parse().ok()).unwrap_or(30);
        if frame == at {
            ctx.send_viewport_cmd(egui::ViewportCommand::Screenshot(Default::default()));
        }
        ctx.request_repaint();
        let shot = ctx.input(|i| {
            i.raw.events.iter().find_map(|e| if let egui::Event::Screenshot { image, .. } = e { Some(image.clone()) } else { None })
        });
        if let Some(img) = shot {
            let [w, h] = img.size;
            let bytes: Vec<u8> = img.pixels.iter().flat_map(|c| c.to_array()).collect();
            if let Some(buf) = image::RgbaImage::from_raw(w as u32, h as u32, bytes) {
                let _ = buf.save(&path);
            }
            self.allow_close = true;
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
        }
    }
}

fn num_or(args: &[String], d: f64) -> f64 {
    args.first().and_then(|a| commands::num(a)).unwrap_or(d)
}

fn ortho_dir(d: V2) -> V2 {
    if d.x.abs() >= d.y.abs() {
        V2::new(d.x.signum(), 0.0)
    } else {
        V2::new(0.0, d.y.signum())
    }
}

fn color_swatch(ui: &mut egui::Ui, aci: u8) {
    let (r, _) = ui.allocate_exact_size(vec2(12.0, 12.0), Sense::hover());
    ui.painter().rect_filled(r, 1.0, theme::aci(aci, false));
    ui.painter().rect_stroke(r, 1.0, Stroke::new(1.0f32, Color32::from_gray(20)), egui::StrokeKind::Middle);
}

/// Layer drop-down in the ribbon: sets the current layer, or moves the selection.
fn layer_combo(ui: &mut egui::Ui, t: &mut Tab) {
    let cur = t.doc.current_layer.clone();
    let shown = if t.selection.is_empty() {
        cur.clone()
    } else {
        let layers: std::collections::BTreeSet<String> =
            t.selection.iter().filter_map(|id| t.doc.get(*id)).map(|e| e.layer.clone()).collect();
        if layers.len() == 1 {
            layers.into_iter().next().unwrap()
        } else {
            "*VARIES*".into()
        }
    };
    ui.horizontal(|ui| {
        if let Some(l) = t.doc.layer(&shown) {
            color_swatch(ui, l.color);
        }
        egui::ComboBox::from_id_salt("layer_combo").width(170.0).selected_text(&shown).show_ui(ui, |ui| {
            let names: Vec<(String, u8, bool)> = t.doc.layers.iter().map(|l| (l.name.clone(), l.color, l.visible)).collect();
            for (name, color, vis) in names {
                ui.horizontal(|ui| {
                    color_swatch(ui, color);
                    let label = format!("{}{name}", if vis { "" } else { "(off) " });
                    if ui.selectable_label(name == shown, label).clicked() {
                        if t.selection.is_empty() {
                            t.doc.current_layer = name.clone();
                        } else {
                            t.checkpoint();
                            let sel = t.selection.clone();
                            for id in sel {
                                if let Some(e) = t.doc.get_mut(id) {
                                    e.layer = name.clone();
                                }
                            }
                        }
                    }
                });
            }
        });
    });
    ui.horizontal(|ui| {
        ui.label(RichText::new("Current:").size(11.0).color(theme::TEXT_DIM));
        ui.label(RichText::new(&cur).size(11.0));
    });
}

/// Properties palette. Returns true when something changed.
fn properties_ui(ui: &mut egui::Ui, t: &mut Tab, editing: &mut bool) -> bool {
    let sel: Vec<u64> = t.selection.iter().copied().filter(|id| t.doc.get(*id).is_some()).collect();
    if sel.is_empty() {
        ui.label(RichText::new("No selection").color(theme::TEXT_DIM));
        ui.add_space(6.0);
        egui::Grid::new("doc_info").num_columns(2).spacing([8.0, 4.0]).show(ui, |ui| {
            ui.label("Current layer");
            ui.label(&t.doc.current_layer);
            ui.end_row();
            ui.label("Objects");
            ui.label(t.doc.entities.len().to_string());
            ui.end_row();
            ui.label("Sheet");
            ui.label(format!(
                "{} {} @ {}",
                t.doc.sheet.paper.name(),
                if t.doc.sheet.landscape { "landscape" } else { "portrait" },
                t.doc.sheet.scale_label()
            ));
            ui.end_row();
            ui.label("Units");
            ui.label(&t.doc.sheet.title.units);
            ui.end_row();
        });
        ui.add_space(10.0);
        ui.label(RichText::new("Tip: select objects to edit their properties and exact coordinates.").size(11.5).color(theme::TEXT_DIM));
        return false;
    }
    let first = t.doc.get(sel[0]).unwrap().clone();
    let same_type = sel.iter().all(|id| t.doc.get(*id).map(|e| e.shape.type_name()) == Some(first.shape.type_name()));
    ui.label(
        RichText::new(if sel.len() == 1 {
            first.shape.type_name().to_string()
        } else if same_type {
            format!("{} ({})", first.shape.type_name(), sel.len())
        } else {
            format!("All ({})", sel.len())
        })
        .strong(),
    );
    ui.add_space(4.0);

    let mut proto = first.clone();
    let mut geom_changed = false;
    let mut gen_changed = false;
    ui.collapsing("General", |ui| {
        egui::Grid::new("gen").num_columns(2).spacing([8.0, 4.0]).show(ui, |ui| {
            ui.label("Layer");
            egui::ComboBox::from_id_salt("p_layer").selected_text(&proto.layer).width(140.0).show_ui(ui, |ui| {
                for l in &t.doc.layers {
                    if ui.selectable_value(&mut proto.layer, l.name.clone(), &l.name).changed() {
                        gen_changed = true;
                    }
                }
            });
            ui.end_row();
            ui.label("Color");
            let ctext = match proto.color {
                Color::ByLayer => "ByLayer".to_string(),
                Color::Aci(c) => aci_name(c),
            };
            egui::ComboBox::from_id_salt("p_color").selected_text(ctext).width(140.0).show_ui(ui, |ui| {
                if ui.selectable_value(&mut proto.color, Color::ByLayer, "ByLayer").changed() {
                    gen_changed = true;
                }
                for (i, n) in ACI_NAMES {
                    ui.horizontal(|ui| {
                        color_swatch(ui, i);
                        if ui.selectable_value(&mut proto.color, Color::Aci(i), n).changed() {
                            gen_changed = true;
                        }
                    });
                }
            });
            ui.end_row();
            ui.label("Linetype");
            let lt = match proto.linetype {
                LtRef::ByLayer => "ByLayer".to_string(),
                LtRef::Lt(l) => l.name().to_string(),
            };
            egui::ComboBox::from_id_salt("p_lt").selected_text(lt).width(140.0).show_ui(ui, |ui| {
                if ui.selectable_value(&mut proto.linetype, LtRef::ByLayer, "ByLayer").changed() {
                    gen_changed = true;
                }
                for l in LineType::ALL {
                    if ui.selectable_value(&mut proto.linetype, LtRef::Lt(l), l.name()).changed() {
                        gen_changed = true;
                    }
                }
            });
            ui.end_row();
        });
    });

    let gflag = std::cell::Cell::new(false);
    if sel.len() == 1 {
        ui.collapsing("Geometry", |ui| {
            egui::Grid::new("geo").num_columns(2).spacing([8.0, 4.0]).show(ui, |ui| {
                let dv = |ui: &mut egui::Ui, label: &str, v: &mut f64| {
                    ui.label(label);
                    if ui.add(egui::DragValue::new(v).speed(0.1).max_decimals(4)).changed() {
                        gflag.set(true);
                    }
                    ui.end_row();
                };
                match &mut proto.shape {
                    Shape::Line { a, b } => {
                        dv(ui, "Start X", &mut a.x);
                        dv(ui, "Start Y", &mut a.y);
                        dv(ui, "End X", &mut b.x);
                        dv(ui, "End Y", &mut b.y);
                        let mut len = a.dist(*b);
                        let before = len;
                        dv(ui, "Length", &mut len);
                        if (len - before).abs() > 1e-12 && before > 0.0 {
                            *b = *a + (*b - *a).norm() * len.max(1e-9);
                        }
                        let mut ang = deg(norm_angle((*b - *a).angle()));
                        let before_a = ang;
                        dv(ui, "Angle °", &mut ang);
                        if (ang - before_a).abs() > 1e-12 {
                            *b = *a + V2::polar(a.dist(*b), rad(ang));
                        }
                    }
                    Shape::Circle { c, r } => {
                        dv(ui, "Center X", &mut c.x);
                        dv(ui, "Center Y", &mut c.y);
                        dv(ui, "Radius", r);
                        let mut d = *r * 2.0;
                        let bd = d;
                        dv(ui, "Diameter", &mut d);
                        if (d - bd).abs() > 1e-12 {
                            *r = (d / 2.0).max(1e-9);
                        }
                        *r = r.max(1e-9);
                    }
                    Shape::Arc { c, r, start, end } => {
                        dv(ui, "Center X", &mut c.x);
                        dv(ui, "Center Y", &mut c.y);
                        dv(ui, "Radius", r);
                        let mut s = deg(*start);
                        let mut e = deg(*end);
                        dv(ui, "Start angle °", &mut s);
                        dv(ui, "End angle °", &mut e);
                        *start = norm_angle(rad(s));
                        *end = norm_angle(rad(e));
                        *r = r.max(1e-9);
                    }
                    Shape::Polyline { pts, closed } => {
                        ui.label("Closed");
                        if ui.checkbox(closed, "").changed() {
                            gflag.set(true);
                        }
                        ui.end_row();
                        for (i, p) in pts.iter_mut().enumerate().take(64) {
                            dv(ui, &format!("Vertex {} X", i + 1), &mut p.x);
                            dv(ui, &format!("Vertex {} Y", i + 1), &mut p.y);
                        }
                    }
                    Shape::Text(tx) => {
                        ui.label("Contents");
                        if ui.text_edit_singleline(&mut tx.content).changed() {
                            gflag.set(true);
                        }
                        ui.end_row();
                        dv(ui, "Position X", &mut tx.pos.x);
                        dv(ui, "Position Y", &mut tx.pos.y);
                        dv(ui, "Height", &mut tx.height);
                        let mut r = deg(tx.rotation);
                        dv(ui, "Rotation °", &mut r);
                        tx.rotation = norm_angle(rad(r));
                        tx.height = tx.height.max(1e-6);
                    }
                    Shape::Dim(d) => {
                        ui.label("Measurement");
                        ui.label(t.doc.dimstyle.format(d.measurement()));
                        ui.end_row();
                        ui.label("Text override");
                        let mut s = d.text.clone().unwrap_or_default();
                        if ui
                            .text_edit_singleline(&mut s)
                            .on_hover_text("Leave empty for the measured value; <> inserts it, e.g. 2x <>")
                            .changed()
                        {
                            d.text = if s.is_empty() { None } else { Some(s) };
                            gflag.set(true);
                        }
                        ui.end_row();
                        dv(ui, "Point 1 X", &mut d.p1.x);
                        dv(ui, "Point 1 Y", &mut d.p1.y);
                        dv(ui, "Point 2 X", &mut d.p2.x);
                        dv(ui, "Point 2 Y", &mut d.p2.y);
                        dv(ui, "Text X", &mut d.loc.x);
                        dv(ui, "Text Y", &mut d.loc.y);
                    }
                    Shape::Ellipse { c, major, ratio, .. } => {
                        dv(ui, "Center X", &mut c.x);
                        dv(ui, "Center Y", &mut c.y);
                        let mut a = major.len();
                        let b0 = a;
                        dv(ui, "Major radius", &mut a);
                        if (a - b0).abs() > 1e-12 && a > 0.0 {
                            *major = major.norm() * a;
                        }
                        let mut minor = major.len() * *ratio;
                        let m0 = minor;
                        dv(ui, "Minor radius", &mut minor);
                        if (minor - m0).abs() > 1e-12 && minor > 0.0 {
                            *ratio = (minor / major.len().max(1e-9)).clamp(1e-6, 1.0);
                        }
                        let mut ang = deg(major.angle());
                        let a0 = ang;
                        dv(ui, "Rotation °", &mut ang);
                        if (ang - a0).abs() > 1e-12 {
                            *major = V2::polar(major.len(), rad(ang));
                        }
                    }
                    Shape::Spline { pts, closed } => {
                        ui.label("Closed");
                        if ui.checkbox(closed, "").changed() {
                            gflag.set(true);
                        }
                        ui.end_row();
                        for (i, p) in pts.iter_mut().enumerate().take(64) {
                            dv(ui, &format!("Fit point {} X", i + 1), &mut p.x);
                            dv(ui, &format!("Fit point {} Y", i + 1), &mut p.y);
                        }
                    }
                    Shape::Point { p } => {
                        dv(ui, "X", &mut p.x);
                        dv(ui, "Y", &mut p.y);
                    }
                    Shape::XLine { p, dir } | Shape::Ray { p, dir } => {
                        dv(ui, "Base X", &mut p.x);
                        dv(ui, "Base Y", &mut p.y);
                        let mut ang = deg(norm_angle(dir.angle()));
                        let a0 = ang;
                        dv(ui, "Angle °", &mut ang);
                        if (ang - a0).abs() > 1e-12 {
                            *dir = V2::polar(1.0, rad(ang));
                        }
                    }
                    Shape::MText(m) => {
                        ui.label("Contents");
                        if ui.add(egui::TextEdit::multiline(&mut m.content).desired_rows(3).desired_width(160.0)).changed() {
                            gflag.set(true);
                        }
                        ui.end_row();
                        dv(ui, "Position X", &mut m.pos.x);
                        dv(ui, "Position Y", &mut m.pos.y);
                        dv(ui, "Height", &mut m.height);
                        dv(ui, "Wrap width", &mut m.width);
                        let mut r = deg(m.rotation);
                        dv(ui, "Rotation °", &mut r);
                        m.rotation = norm_angle(rad(r));
                        m.height = m.height.max(1e-6);
                        m.width = m.width.max(0.0);
                        ui.label("Justify");
                        let names = [
                            "Top left",
                            "Top center",
                            "Top right",
                            "Middle left",
                            "Middle center",
                            "Middle right",
                            "Bottom left",
                            "Bottom center",
                            "Bottom right",
                        ];
                        egui::ComboBox::from_id_salt("mt_attach").selected_text(names[(m.attach.clamp(1, 9) - 1) as usize]).show_ui(
                            ui,
                            |ui| {
                                for (i, n) in names.iter().enumerate() {
                                    if ui.selectable_value(&mut m.attach, i as u8 + 1, *n).changed() {
                                        gflag.set(true);
                                    }
                                }
                            },
                        );
                        ui.end_row();
                    }
                    Shape::Hatch(h) => {
                        ui.label("Pattern");
                        egui::ComboBox::from_id_salt("h_pat").selected_text(h.pattern.clone()).show_ui(ui, |ui| {
                            for (n, d) in icad_core::hatch::PATTERNS {
                                if ui.selectable_value(&mut h.pattern, n.to_string(), *n).on_hover_text(*d).changed() {
                                    gflag.set(true);
                                }
                            }
                        });
                        ui.end_row();
                        dv(ui, "Scale", &mut h.scale);
                        h.scale = h.scale.max(1e-6);
                        let mut a = deg(h.angle);
                        dv(ui, "Angle °", &mut a);
                        h.angle = rad(a);
                        ui.label("Area");
                        ui.label(commands::fmt(h.area()));
                        ui.end_row();
                    }
                    Shape::Insert(ins) => {
                        ui.label("Block");
                        ui.label(&ins.name);
                        ui.end_row();
                        dv(ui, "Position X", &mut ins.pos.x);
                        dv(ui, "Position Y", &mut ins.pos.y);
                        dv(ui, "Scale X", &mut ins.sx);
                        dv(ui, "Scale Y", &mut ins.sy);
                        let mut r = deg(ins.rotation);
                        dv(ui, "Rotation °", &mut r);
                        ins.rotation = norm_angle(rad(r));
                    }
                    Shape::Leader(l) => {
                        ui.label("Text");
                        if ui.add(egui::TextEdit::multiline(&mut l.text).desired_rows(2).desired_width(160.0)).changed() {
                            gflag.set(true);
                        }
                        ui.end_row();
                        for (i, p) in l.pts.iter_mut().enumerate().take(16) {
                            dv(ui, &format!("Vertex {} X", i + 1), &mut p.x);
                            dv(ui, &format!("Vertex {} Y", i + 1), &mut p.y);
                        }
                    }
                    Shape::Image(img) => {
                        ui.label("File");
                        ui.label(RichText::new(&img.path).size(11.0));
                        ui.end_row();
                        dv(ui, "Position X", &mut img.pos.x);
                        dv(ui, "Position Y", &mut img.pos.y);
                        let aspect = img.h / img.w.max(1e-9);
                        let w0 = img.w;
                        dv(ui, "Width", &mut img.w);
                        if (img.w - w0).abs() > 1e-12 {
                            img.w = img.w.max(1e-6);
                            img.h = img.w * aspect;
                        }
                        ui.label("Fade %");
                        if ui.add(egui::Slider::new(&mut img.fade, 0..=90)).changed() {
                            gflag.set(true);
                        }
                        ui.end_row();
                    }
                }
            });
        });
        if let Some(l) = proto.shape.length() {
            ui.label(RichText::new(format!("Length: {}", commands::fmt(l))).color(theme::TEXT_DIM));
        }
        if let Some(a) = proto.shape.area() {
            ui.label(RichText::new(format!("Area: {}", commands::fmt(a))).color(theme::TEXT_DIM));
        }
    }

    geom_changed |= gflag.get();
    if gen_changed || geom_changed {
        if !*editing {
            t.checkpoint();
            *editing = true;
        }
        t.dirty = true;
        for id in &sel {
            if let Some(e) = t.doc.get_mut(*id) {
                if gen_changed {
                    e.layer = proto.layer.clone();
                    e.color = proto.color;
                    e.linetype = proto.linetype;
                }
                if geom_changed && sel.len() == 1 {
                    e.shape = proto.shape.clone();
                }
            }
        }
        return true;
    }
    false
}

impl eframe::App for App {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.screenshot_hook(ctx);
        self.handle_drops(ctx);
        self.handle_close(ctx);
        self.keyboard(ctx);
        self.title_bar(ctx);
        self.ribbon(ctx);
        self.file_tabs(ctx);
        self.status_bar(ctx);
        self.command_line(ctx);
        self.properties(ctx);
        self.chat_panel(ctx);
        egui::CentralPanel::default().frame(egui::Frame::NONE.fill(theme::CANVAS)).show(ctx, |ui| self.canvas(ui));
        self.context_menu(ctx);
        self.dialogs(ctx);
        let title = format!("ImagineCAD - {}", self.tabs[self.active].title());
        if title != self.window_title {
            ctx.send_viewport_cmd(egui::ViewportCommand::Title(title.clone()));
            self.window_title = title;
        }
        // Keep the blinking command-line cursor alive at a low rate.
        ctx.request_repaint_after(std::time::Duration::from_millis(500));
    }

    fn on_exit(&mut self, _gl: Option<&eframe::glow::Context>) {
        self.settings.save();
    }
}
