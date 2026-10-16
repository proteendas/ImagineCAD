// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Proteen Das

//! Dialog windows.

use crate::app::App;
use crate::commands::{self, ViewReq};
use crate::settings::Settings;
use crate::tab::Tab;
use crate::theme;
use eframe::egui::{self, vec2, Color32, RichText};
use icad_core::sheet::{parse_scale, scale_label, PaperSize, Projection, STANDARD_SCALES};
use icad_core::style::{aci_name, Layer, LineType, ACI_NAMES, LINEWEIGHTS};
use icad_vision::llm::{self, Provider};
use icad_vision::providers::{Cost, ModelInfo};
use icad_vision::{Conversion, ConvertOptions};
use std::path::{Path, PathBuf};
use std::sync::mpsc::{channel, Receiver};
use std::sync::Arc;

pub enum AiMsg {
    Progress(f32, String),
    Done(Box<Result<Conversion, String>>),
}

pub struct AiDialog {
    pub path: Option<PathBuf>,
    pub bytes: Option<Arc<Vec<u8>>>,
    pub texture: Option<egui::TextureHandle>,
    pub img_size: [usize; 2],
    pub load_error: Option<String>,
    pub opts: ConvertOptions,
    pub rx: Option<Receiver<AiMsg>>,
    pub progress: f32,
    pub stage: String,
    pub result: Option<Result<Conversion, String>>,
    pub open: bool,
}

impl AiDialog {
    pub fn new(s: &Settings) -> Self {
        Self {
            path: None,
            bytes: None,
            texture: None,
            img_size: [0, 0],
            load_error: None,
            opts: s.convert.clone(),
            rx: None,
            progress: 0.0,
            stage: String::new(),
            result: None,
            open: true,
        }
    }

    pub fn with_image(p: &Path, s: &Settings) -> Self {
        let mut d = Self::new(s);
        d.set_image(p);
        d
    }

    fn set_image(&mut self, p: &Path) {
        self.texture = None;
        self.result = None;
        self.load_error = None;
        match std::fs::read(p) {
            Ok(b) => {
                self.bytes = Some(Arc::new(b));
                self.path = Some(p.to_path_buf());
            }
            Err(e) => self.load_error = Some(e.to_string()),
        }
    }

    fn ensure_texture(&mut self, ctx: &egui::Context) {
        if self.texture.is_some() {
            return;
        }
        let Some(bytes) = &self.bytes else { return };
        match image::load_from_memory(bytes) {
            Ok(img) => {
                let img = img.thumbnail(900, 900).to_rgba8();
                let size = [img.width() as usize, img.height() as usize];
                let ci = egui::ColorImage::from_rgba_unmultiplied(size, img.as_raw());
                self.img_size = size;
                self.texture = Some(ctx.load_texture("sketch_preview", ci, egui::TextureOptions::LINEAR));
            }
            Err(e) => {
                self.load_error = Some(format!("Not a supported image: {e}"));
                self.bytes = None;
            }
        }
    }

    fn start(&mut self, ctx: &egui::Context) {
        let Some(bytes) = self.bytes.clone() else { return };
        let (tx, rx) = channel();
        let mut opts = self.opts.clone();
        if opts.ai.api_key.is_empty() {
            if let Some(k) = crate::secrets::load() {
                opts.ai.api_key = k;
            }
        }
        let ctx = ctx.clone();
        self.rx = Some(rx);
        self.result = None;
        self.progress = 0.0;
        self.stage = "Starting…".into();
        std::thread::spawn(move || {
            let tx2 = tx.clone();
            let ctx2 = ctx.clone();
            let mut prog = move |f: f32, s: &str| {
                let _ = tx2.send(AiMsg::Progress(f, s.to_string()));
                ctx2.request_repaint();
            };
            let r = icad_vision::convert(&bytes, &opts, &mut prog);
            let _ = tx.send(AiMsg::Done(Box::new(r)));
            ctx.request_repaint();
        });
    }

    fn poll(&mut self) {
        let Some(rx) = &self.rx else { return };
        while let Ok(m) = rx.try_recv() {
            match m {
                AiMsg::Progress(f, s) => {
                    self.progress = f;
                    self.stage = s;
                }
                AiMsg::Done(r) => {
                    self.result = Some(*r);
                    self.rx = None;
                    return;
                }
            }
        }
    }
}

/// First-use / change-key AI setup.
#[derive(Default)]
pub struct AiSetup {
    pub key: String,
    pub show_key: bool,
    pub custom_url: String,
    pub rx: Option<Receiver<Result<icad_vision::providers::Detected, String>>>,
    pub result: Option<Result<icad_vision::providers::Detected, String>>,
    pub model: String,
    pub filter: ModelFilter,
    /// Open the Image-to-CAD dialog when finished.
    pub then_convert: bool,
}

/// Search text and free-only switch of a model dropdown.
#[derive(Default)]
pub struct ModelFilter {
    pub text: String,
    pub free_only: bool,
}

/// A FREE / PAID badge (nothing when the provider does not say).
fn cost_tag(ui: &mut egui::Ui, cost: Cost) {
    let (t, c) = match cost {
        Cost::Free => ("FREE", Color32::from_rgb(40, 140, 80)),
        Cost::Paid => ("PAID", Color32::from_rgb(176, 112, 28)),
        Cost::Unknown => return,
    };
    ui.label(RichText::new(format!(" {t} ")).size(10.5).strong().color(Color32::WHITE).background_color(c));
}

/// Model dropdown with search, a free-only filter and FREE/PAID badges.
fn model_picker(ui: &mut egui::Ui, id: &str, model: &mut String, models: &[ModelInfo], f: &mut ModelFilter) {
    let current = models.iter().find(|m| m.id == *model).map(|m| m.cost);
    let shown = if model.is_empty() { "Choose a model…".to_string() } else { model.clone() };
    egui::ComboBox::from_id_salt(id)
        .selected_text(shown)
        .width(340.0)
        .height(380.0)
        .close_behavior(egui::PopupCloseBehavior::CloseOnClickOutside)
        .show_ui(ui, |ui| {
            ui.horizontal(|ui| {
                ui.add(egui::TextEdit::singleline(&mut f.text).hint_text("Search models").desired_width(190.0));
                ui.checkbox(&mut f.free_only, "Free only");
            });
            ui.separator();
            let q = f.text.to_ascii_lowercase();
            let mut shown = 0;
            for m in models.iter().filter(|m| (!f.free_only || m.cost == Cost::Free) && m.id.to_ascii_lowercase().contains(&q)) {
                ui.horizontal(|ui| {
                    if ui.selectable_label(*model == m.id, &m.id).clicked() {
                        *model = m.id.clone();
                        ui.memory_mut(|mem| mem.close_popup());
                    }
                    cost_tag(ui, m.cost);
                });
                shown += 1;
            }
            if shown == 0 {
                ui.label(RichText::new("No matching model").color(theme::TEXT_DIM));
            }
        });
    if let Some(c) = current {
        cost_tag(ui, c);
    }
}

/// Quick Select criteria.
#[derive(Default)]
pub struct QSelect {
    pub kind: String,
    pub layer: String,
    pub color: Option<u8>,
    pub append: bool,
}

#[derive(Default)]
pub struct Dialogs {
    pub layers: bool,
    pub page: bool,
    pub dimstyle: bool,
    pub ai_settings: bool,
    pub ai_help: bool,
    pub help: bool,
    pub about: bool,
    pub export: bool,
    pub blocks: bool,
    pub units: bool,
    pub message: Option<(String, String)>,
    pub close_tab: Option<usize>,
    pub confirm_quit: bool,
    pub quit_requested: bool,
    pub context_at: Option<egui::Pos2>,
    pub ai: Option<AiDialog>,
    pub ai_setup: Option<AiSetup>,
    pub qselect: Option<QSelect>,
    pub text_edit: Option<(u64, String)>,
    pub ai_check: Option<Receiver<Result<Vec<ModelInfo>, String>>>,
    pub ai_models: Vec<ModelInfo>,
    pub symbol: Option<crate::symbol_ui::SymbolDialog>,
    pub attedit: Option<crate::attributes::AttEditDialog>,
    pub table: Option<crate::tables::TableDialog>,
    pub table_edit: Option<crate::tables::TableEditor>,
    pub mleader_styles: bool,
    pub style: Option<crate::text_ui::StyleDialog>,
    pub spell: Option<crate::spell::SpellDialog>,
    pub layout_rename: Option<(usize, String)>,
    pub publish: Option<crate::layouts_ui::PublishDialog>,
    pub xref: bool,
    pub sheetset: bool,
    pub parameters: bool,
    /// Which engine `ai_models` was loaded for.
    pub ai_models_for: String,
    pub model_filter: ModelFilter,
    pub ai_status: Option<Result<String, String>>,
    pub new_layer: String,
}

fn local_ai_ui(ui: &mut egui::Ui, o: &mut ConvertOptions, models: &[ModelInfo], filter: &mut ModelFilter) {
    let ai = &mut o.ai;
    match ai.provider {
        Provider::Ollama => {
            egui::Grid::new("ollama").num_columns(2).show(ui, |ui| {
                ui.label("Server URL");
                ui.text_edit_singleline(&mut ai.ollama_url);
                ui.end_row();
                ui.label("Vision model");
                ui.horizontal(|ui| {
                    if models.is_empty() {
                        ui.text_edit_singleline(&mut ai.ollama_model);
                    } else {
                        model_picker(ui, "ollama_models", &mut ai.ollama_model, models, filter);
                    }
                });
                ui.end_row();
            });
            ui.label(
                RichText::new("Free and private: install from ollama.com, then run  `ollama pull qwen2.5vl:7b`.")
                    .size(11.5)
                    .color(theme::TEXT_DIM),
            );
        }
        Provider::OpenAiCompatible if ai.api_key.is_empty() => {
            egui::Grid::new("oai").num_columns(2).show(ui, |ui| {
                ui.label("Base URL");
                ui.text_edit_singleline(&mut ai.openai_url);
                ui.end_row();
                ui.label("Model");
                ui.horizontal(|ui| {
                    if models.is_empty() {
                        ui.text_edit_singleline(&mut ai.openai_model);
                    } else {
                        model_picker(ui, "local_models", &mut ai.openai_model, models, filter);
                    }
                });
                ui.end_row();
            });
            ui.horizontal_wrapped(|ui| {
                ui.label(RichText::new("Presets:").size(11.5));
                if ui.small_button("LM Studio").clicked() {
                    ai.openai_url = "http://localhost:1234/v1".into();
                }
                if ui.small_button("llama.cpp server").clicked() {
                    ai.openai_url = "http://localhost:8080/v1".into();
                }
            });
        }
        _ => {}
    }
    ui.add_space(4.0);
    ui.checkbox(&mut ai.use_tesseract, "Also use Tesseract OCR when installed (good for neatly printed numbers)");
    match icad_vision::ocr::find_tesseract() {
        Some(p) => ui.label(RichText::new(format!("✔ Tesseract found: {}", p.display())).size(11.5).color(Color32::from_rgb(90, 210, 120))),
        None => ui.label(
            RichText::new(
                "Tesseract not found (optional): brew install tesseract · apt install tesseract-ocr · UB-Mannheim installer on Windows",
            )
            .size(11.5)
            .color(theme::TEXT_DIM),
        ),
    };
    ui.horizontal(|ui| {
        ui.label("Timeout (s)");
        ui.add(egui::DragValue::new(&mut ai.timeout_secs).range(10..=3600));
    });
}

impl App {
    pub fn dialogs(&mut self, ctx: &egui::Context) {
        self.ai_setup_dialog(ctx);
        self.ai_dialog(ctx);
        self.ai_settings_dialog(ctx);
        self.layers_dialog(ctx);
        self.page_dialog(ctx);
        self.dimstyle_dialog(ctx);
        self.export_dialog(ctx);
        self.blocks_dialog(ctx);
        self.qselect_dialog(ctx);
        self.text_edit_dialog(ctx);
        self.units_dialog(ctx);
        self.symbol_dialog(ctx);
        self.attedit_dialog(ctx);
        self.table_dialogs(ctx);
        self.style_dialog(ctx);
        self.spell_dialog(ctx);
        self.layout_dialogs(ctx);
        self.xref_palette(ctx);
        self.sheet_set_palette(ctx);
        self.parameters_dialog(ctx);
        self.help_dialogs(ctx);
        self.confirm_dialogs(ctx);
        self.poll_ai_check();
    }

    /// Open Image-to-CAD, running the AI setup first on first use.
    pub fn open_image_to_cad(&mut self, image: Option<&std::path::Path>) {
        if !self.settings.convert.ai.setup_done {
            self.dlg.ai_setup = Some(AiSetup { then_convert: true, ..Default::default() });
            if let Some(p) = image {
                self.dlg.ai = Some(AiDialog::with_image(p, &self.settings));
                if let Some(d) = self.dlg.ai.as_mut() {
                    d.open = false;
                }
            }
            return;
        }
        match (image, self.dlg.ai.as_mut()) {
            (Some(p), _) => self.dlg.ai = Some(AiDialog::with_image(p, &self.settings)),
            (None, Some(d)) => d.open = true,
            (None, None) => self.dlg.ai = Some(AiDialog::new(&self.settings)),
        }
    }

    fn ai_setup_dialog(&mut self, ctx: &egui::Context) {
        let Some(mut st) = self.dlg.ai_setup.take() else { return };
        if let Some(rx) = &st.rx {
            if let Ok(r) = rx.try_recv() {
                if let Ok(d) = &r {
                    st.model = d.model.clone();
                }
                st.result = Some(r);
                st.rx = None;
            }
        }
        let mut open = true;
        let mut action: Option<&str> = None;
        egui::Window::new("Connect an AI model").open(&mut open).collapsible(false).resizable(false).default_width(560.0).anchor(egui::Align2::CENTER_CENTER, vec2(0.0, 0.0)).show(ctx, |ui| {
            ui.label(RichText::new("Image to CAD uses an AI vision model to read the dimension values on your sketch.").strong());
            ui.label("Paste an API key from any provider: OpenAI, Anthropic, Google Gemini, OpenRouter, Groq, Mistral, xAI, Together, Hugging Face, NVIDIA, Fireworks… ImagineCAD recognises the provider and chooses the best vision model automatically.");
            ui.add_space(6.0);
            ui.horizontal(|ui| {
                ui.label("API key");
                ui.add(egui::TextEdit::singleline(&mut st.key).password(!st.show_key).desired_width(330.0).hint_text("sk-…, sk-ant-…, AIza…, gsk_…"));
                ui.checkbox(&mut st.show_key, "show");
            });
            egui::CollapsingHeader::new("Other OpenAI-compatible endpoint (optional)").show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.label("Base URL");
                    ui.add(egui::TextEdit::singleline(&mut st.custom_url).hint_text("https://example.com/v1").desired_width(330.0));
                });
            });
            ui.add_space(4.0);
            let busy = st.rx.is_some();
            ui.horizontal(|ui| {
                if ui.add_enabled(!busy && (!st.key.trim().is_empty() || !st.custom_url.trim().is_empty()), egui::Button::new("Detect provider & model")).clicked() {
                    action = Some("detect");
                }
                if busy {
                    ui.spinner();
                    ui.label("Checking the key…");
                }
            });
            match &st.result {
                Some(Ok(d)) => {
                    ui.label(RichText::new(format!("✔ {} — {} model(s) available", d.vendor, d.models.len())).color(Color32::from_rgb(90, 210, 120)));
                    ui.horizontal(|ui| {
                        ui.label("Vision model");
                        model_picker(ui, "setup_model", &mut st.model, &d.models, &mut st.filter);
                    });
                    if ui.add(egui::Button::new(RichText::new("Save & continue").strong().color(Color32::WHITE)).fill(Color32::from_rgb(38, 96, 160))).clicked() {
                        action = Some("save");
                    }
                }
                Some(Err(e)) => {
                    ui.label(RichText::new(format!("✖ {e}")).color(theme::ERR));
                }
                None => {}
            }
            ui.label(RichText::new("The key is stored securely (macOS Keychain / Windows Credential Manager / a private file on Linux) and only sent to the provider it belongs to. Sketches are sent to that provider for analysis.").size(11.0).color(theme::TEXT_DIM));
            ui.separator();
            ui.horizontal(|ui| {
                if ui.button("Use a free local model (Ollama) instead").clicked() {
                    action = Some("ollama");
                }
                if ui.button("Skip — geometry only").clicked() {
                    action = Some("skip");
                }
            });
        });
        match action {
            Some("detect") => {
                let (tx, rx) = channel();
                let key = st.key.trim().to_string();
                let url = st.custom_url.trim().to_string();
                let ctx2 = ctx.clone();
                std::thread::spawn(move || {
                    let r = icad_vision::providers::detect(&key, if url.is_empty() { None } else { Some(url.as_str()) });
                    let _ = tx.send(r);
                    ctx2.request_repaint();
                });
                st.rx = Some(rx);
                st.result = None;
            }
            Some("save") => {
                if let Some(Ok(mut d)) = st.result.take() {
                    d.model = st.model.clone();
                    let key = st.key.trim().to_string();
                    let ai = &mut self.settings.convert.ai;
                    ai.apply_detected(&d, &key);
                    let where_ = if key.is_empty() { Ok("(no key)") } else { crate::secrets::save(&key) };
                    match where_ {
                        Ok(w) => self.log.push(format!("AI connected: {} (key stored in {w}).", self.settings.convert.ai.describe())),
                        Err(e) => self.error(format!("Could not store the API key: {e}")),
                    }
                    self.ai_ready = true;
                    self.settings.save();
                    self.sync_ai_dialog();
                    if st.then_convert {
                        self.open_image_to_cad(None);
                    }
                }
                return;
            }
            Some("ollama") => {
                self.settings.convert.ai.provider = Provider::Ollama;
                self.settings.convert.ai.setup_done = true;
                self.settings.save();
                self.sync_ai_dialog();
                self.dlg.ai_settings = true;
                if st.then_convert {
                    self.open_image_to_cad(None);
                }
                return;
            }
            Some("skip") => {
                self.settings.convert.ai.provider = Provider::Disabled;
                self.settings.convert.ai.setup_done = true;
                self.settings.save();
                self.sync_ai_dialog();
                if st.then_convert {
                    self.open_image_to_cad(None);
                }
                return;
            }
            _ => {}
        }
        if open {
            self.dlg.ai_setup = Some(st);
        }
    }

    /// Push the current AI settings into an open Image-to-CAD dialog.
    fn sync_ai_dialog(&mut self) {
        if let Some(d) = self.dlg.ai.as_mut() {
            d.opts.ai = self.settings.convert.ai.clone();
        }
    }

    fn start_ai_check(&mut self, ctx: &egui::Context) {
        let ai = self.settings.convert.ai.clone();
        let (tx, rx) = channel();
        let ctx = ctx.clone();
        std::thread::spawn(move || {
            use icad_vision::providers as p;
            let tag = |ids: Vec<String>, cost: Cost| ids.into_iter().map(|id| ModelInfo::new(id, cost)).collect::<Vec<_>>();
            let r = match ai.provider {
                // Local models cost nothing.
                Provider::Ollama => llm::ollama_models(&ai.ollama_url).map(|m| tag(m, Cost::Free)),
                // Only offer models that accept images; fall back to the full list when the
                // server publishes no capability metadata.
                Provider::OpenAiCompatible => match p::vision_models_priced(&ai.openai_url, &ai.api_key) {
                    Ok(m) if !m.is_empty() => Ok(m),
                    _ => llm::openai_models(&ai.openai_url, &ai.api_key).map(|ids| {
                        ids.into_iter()
                            .map(|id| ModelInfo { cost: p::model_cost(&ai.openai_url, &id, &serde_json::Value::Null), id })
                            .collect()
                    }),
                },
                Provider::Anthropic => p::list_anthropic(&ai.api_key).map(|r| tag(r.1, Cost::Paid)),
                Provider::Disabled => Ok(vec![]),
            };
            let _ = tx.send(r);
            ctx.request_repaint();
        });
        self.dlg.ai_check = Some(rx);
        self.dlg.ai_status = Some(Ok("Checking…".into()));
    }

    fn poll_ai_check(&mut self) {
        let Some(rx) = &self.dlg.ai_check else { return };
        if let Ok(r) = rx.try_recv() {
            self.dlg.ai_check = None;
            match r {
                Ok(models) => {
                    // No model chosen yet (or the field was cleared): take the vendor's best.
                    if self.settings.convert.ai.model().trim().is_empty() && !models.is_empty() {
                        let ai = &mut self.settings.convert.ai;
                        let ids: Vec<String> = models.iter().map(|m| m.id.clone()).collect();
                        let prefer = icad_vision::providers::VENDORS.iter().find(|v| v.name == ai.vendor).map(|v| v.prefer).unwrap_or(&[]);
                        let best = icad_vision::providers::choose_model(&ids, prefer, None).unwrap_or_else(|| ids[0].clone());
                        match ai.provider {
                            Provider::Ollama => ai.ollama_model = best,
                            Provider::Anthropic => ai.anthropic_model = best,
                            _ => ai.openai_model = best,
                        }
                        self.settings.save();
                        self.sync_ai_dialog();
                    }
                    let want = self.settings.convert.ai.model().to_string();
                    let has = models
                        .iter()
                        .map(|m| m.id.as_str())
                        .any(|m| m == want || m.starts_with(&format!("{want}:")) || want.starts_with(m));
                    self.ai_ready = has;
                    self.dlg.ai_status = Some(if has {
                        Ok(format!("Connected. {} model(s) available; `{want}` is ready.", models.len()))
                    } else if self.settings.convert.ai.provider == Provider::Ollama {
                        Err(format!("Connected, but `{want}` is not installed. Run: ollama pull {want}"))
                    } else {
                        Err(format!("Connected, but `{want}` is not offered by this provider."))
                    });
                    self.dlg.ai_models = models;
                }
                Err(e) => {
                    self.ai_ready = false;
                    self.dlg.ai_status = Some(Err(e));
                }
            }
        }
    }

    fn ai_settings_dialog(&mut self, ctx: &egui::Context) {
        if !self.dlg.ai_settings {
            return;
        }
        let mut open = true;
        let mut test = false;
        let mut change_key = false;
        let mut remove_key = false;
        // Load the model list for the selected engine so the dropdown is ready.
        {
            let ai = &self.settings.convert.ai;
            let engine = format!("{:?}|{}|{}|{}", ai.provider, ai.openai_url, ai.ollama_url, crate::secrets::mask(&ai.api_key));
            if ai.provider != Provider::Disabled && self.dlg.ai_models_for != engine && self.dlg.ai_check.is_none() {
                self.dlg.ai_models_for = engine;
                self.dlg.ai_models.clear();
                self.start_ai_check(ctx);
            }
        }
        egui::Window::new("AI Settings").open(&mut open).default_width(580.0).collapsible(false).show(ctx, |ui| {
            let ai = &mut self.settings.convert.ai;
            ui.label(RichText::new(format!("Current engine: {}", ai.describe())).strong());
            ui.add_space(4.0);
            ui.label(RichText::new("Engine").strong());
            let has_key = !ai.api_key.is_empty();
            ui.add_enabled_ui(has_key, |ui| {
                let cloud = if ai.vendor == "Anthropic" { Provider::Anthropic } else { Provider::OpenAiCompatible };
                let label = if has_key {
                    format!(
                        "Cloud API — {} (key {})",
                        if ai.vendor.is_empty() { "custom" } else { &ai.vendor },
                        crate::secrets::mask(&ai.api_key)
                    )
                } else {
                    "Cloud API — no key configured".into()
                };
                if ui
                    .radio(matches!(ai.provider, Provider::Anthropic) || (ai.provider == Provider::OpenAiCompatible && has_key), label)
                    .clicked()
                {
                    ai.provider = cloud;
                }
            });
            ui.radio_value(&mut ai.provider, Provider::Ollama, "Ollama — free local open-weight model");
            if ui
                .radio(ai.provider == Provider::OpenAiCompatible && !has_key, "Local OpenAI-compatible server (LM Studio, llama.cpp, vLLM)")
                .clicked()
            {
                ai.provider = Provider::OpenAiCompatible;
            }
            ui.radio_value(&mut ai.provider, Provider::Disabled, "Off — geometry only (computer vision, no AI)");
            ui.add_space(6.0);
            if matches!(ai.provider, Provider::Anthropic) || (ai.provider == Provider::OpenAiCompatible && has_key) {
                ui.horizontal(|ui| {
                    ui.label("Model");
                    let field = if ai.provider == Provider::Anthropic { &mut ai.anthropic_model } else { &mut ai.openai_model };
                    if !self.dlg.ai_models.is_empty() {
                        model_picker(ui, "cloud_models", field, &self.dlg.ai_models, &mut self.dlg.model_filter);
                    } else if self.dlg.ai_check.is_some() {
                        ui.spinner();
                        ui.label(RichText::new("Loading models…").color(theme::TEXT_DIM));
                    } else {
                        ui.text_edit_singleline(field);
                    }
                });
            }
            ui.horizontal(|ui| {
                if ui.button(if has_key { "Change API key…" } else { "Add an API key…" }).clicked() {
                    change_key = true;
                }
                if has_key && ui.button("Remove key").clicked() {
                    remove_key = true;
                }
            });
            ui.separator();
            local_ai_ui(ui, &mut self.settings.convert, &self.dlg.ai_models, &mut self.dlg.model_filter);
            ui.separator();
            ui.horizontal(|ui| {
                if ui.button("Test connection").clicked() {
                    test = true;
                }
                match &self.dlg.ai_status {
                    Some(Ok(s)) => {
                        ui.label(RichText::new(s).color(Color32::from_rgb(90, 210, 120)));
                    }
                    Some(Err(e)) => {
                        ui.label(RichText::new(e).color(theme::ERR));
                    }
                    None => {}
                }
            });
        });
        if test {
            self.start_ai_check(ctx);
        }
        if change_key {
            self.dlg.ai_setup = Some(AiSetup::default());
        }
        if remove_key {
            crate::secrets::delete();
            let ai = &mut self.settings.convert.ai;
            ai.api_key.clear();
            ai.vendor.clear();
            if ai.provider != Provider::Ollama {
                ai.provider = Provider::Disabled;
            }
            self.log.push("API key removed.".into());
        }
        if !open || test || change_key || remove_key {
            self.settings.convert.ai.setup_done = true;
            self.settings.save();
            self.sync_ai_dialog();
        }
        if !open {
            self.dlg.ai_settings = false;
        }
    }

    fn ai_dialog(&mut self, ctx: &egui::Context) {
        let Some(mut d) = self.dlg.ai.take() else { return };
        d.ensure_texture(ctx);
        d.poll();
        // The AI switched to a vision-capable model after the configured one rejected the
        // image: remember it so the next conversion uses it straight away.
        if let Some(Ok(Conversion { switched_model: Some(m), .. })) = &d.result {
            if self.settings.convert.ai.openai_model != *m {
                self.settings.convert.ai.openai_model = m.clone();
                self.settings.save();
                d.opts.ai.openai_model = m.clone();
                self.log.push(format!("AI Settings: model set to {m}, which can read images."));
            }
        }
        let mut open = d.open;
        let mut action: Option<&str> = None;
        let mut open_settings = false;
        egui::Window::new("Image to CAD").open(&mut open).default_width(860.0).default_height(620.0).collapsible(false).show(ctx, |ui| {
            ui.horizontal(|ui| {
                ui.vertical(|ui| {
                    ui.set_width(440.0);
                    if ui.button("📂  Choose sketch image…").clicked() {
                        if let Some(p) =
                            rfd::FileDialog::new().add_filter("Images", &["png", "jpg", "jpeg", "bmp", "tif", "tiff", "webp"]).pick_file()
                        {
                            d.set_image(&p);
                        }
                    }
                    if let Some(p) = &d.path {
                        ui.label(RichText::new(p.display().to_string()).size(11.0).color(theme::TEXT_DIM));
                    }
                    if let Some(e) = &d.load_error {
                        ui.label(RichText::new(e).color(theme::ERR));
                    }
                    let avail = vec2(430.0, 380.0);
                    match &d.texture {
                        Some(tex) => {
                            let [w, h] = d.img_size;
                            let k = (avail.x / w as f32).min(avail.y / h as f32).min(1.0);
                            ui.image(egui::load::SizedTexture::new(tex.id(), vec2(w as f32 * k, h as f32 * k)));
                        }
                        None => {
                            let (r, _) = ui.allocate_exact_size(avail, egui::Sense::hover());
                            ui.painter().rect_stroke(r, 6.0, egui::Stroke::new(1.5f32, theme::TEXT_DIM), egui::StrokeKind::Middle);
                            ui.painter().text(
                                r.center(),
                                egui::Align2::CENTER_CENTER,
                                "Choose an image or drop it on the window\n(photo or scan of a hand-drawn mechanical sketch)",
                                egui::FontId::proportional(13.0),
                                theme::TEXT_DIM,
                            );
                        }
                    }
                });
                ui.separator();
                ui.vertical(|ui| {
                    egui::ScrollArea::vertical().max_height(470.0).show(ui, |ui| {
                        ui.label(RichText::new("Conversion").strong());
                        egui::Grid::new("conv").num_columns(2).show(ui, |ui| {
                            ui.label("Straighten lines within");
                            ui.add(egui::Slider::new(&mut d.opts.snap_deg, 0.0..=15.0).suffix("°"));
                            ui.end_row();
                            ui.label("Sketch smoothing");
                            ui.add(egui::Slider::new(&mut d.opts.tolerance, 0.3..=3.0));
                            ui.end_row();
                            ui.label("Size if no dimensions");
                            ui.add(egui::DragValue::new(&mut d.opts.fallback_size_mm).suffix(" mm").range(1.0..=100000.0));
                            ui.end_row();
                            ui.label("Processing resolution");
                            ui.add(egui::DragValue::new(&mut d.opts.max_dim).suffix(" px").range(600..=4000));
                            ui.end_row();
                        });
                        ui.checkbox(&mut d.opts.center_marks, "Add centre lines to holes");
                        ui.checkbox(&mut d.opts.reconstruct_parts, "Let AI rebuild the part exactly from the written dimensions")
                            .on_hover_text("The AI describes the part (shaft, ring, flange, pulley, gear, spring, plate, or any profile) from its dimensions; the description is checked against the written values and its own geometry before it is drawn, otherwise the sketch is traced.");
                        ui.add_space(6.0);
                        ui.horizontal(|ui| {
                            ui.label(RichText::new(format!("AI engine: {}", d.opts.ai.describe())).color(theme::AI_ACCENT));
                            if ui.small_button("Change…").clicked() {
                                open_settings = true;
                            }
                        });
                    });
                    ui.add_space(6.0);
                    let busy = d.rx.is_some();
                    ui.horizontal(|ui| {
                        let btn = egui::Button::new(
                            RichText::new(if busy { "Converting…" } else { "Convert to CAD" }).strong().color(Color32::WHITE),
                        )
                        .fill(Color32::from_rgb(110, 70, 200))
                        .min_size(vec2(160.0, 30.0));
                        if ui.add_enabled(!busy && d.bytes.is_some(), btn).clicked() {
                            d.start(ui.ctx());
                        }
                    });
                    if busy {
                        ui.add(egui::ProgressBar::new(d.progress).text(&d.stage).animate(true));
                    }
                    match &d.result {
                        Some(Ok(c)) => {
                            ui.add_space(4.0);
                            ui.label(
                                RichText::new(format!(
                                    "✔ {} objects, {} dimension(s) read, {} applied",
                                    c.shapes.len(),
                                    c.labels_found,
                                    c.dims_applied
                                ))
                                .color(Color32::from_rgb(90, 210, 120)),
                            );
                            if let Some(p) = &c.reconstructed {
                                ui.label(RichText::new(format!("Rebuilt from dimensions: {p}")).size(12.0).color(theme::AI_ACCENT));
                            }
                            for l in &c.log {
                                ui.label(RichText::new(format!("• {l}")).size(11.5).color(theme::TEXT_DIM));
                            }
                            for w in &c.warnings {
                                ui.label(RichText::new(format!("⚠ {w}")).size(11.5).color(theme::WARN));
                            }
                            if c.underlay.is_some() {
                                ui.horizontal(|ui| {
                                    ui.checkbox(&mut d.opts.overlay_sketch, "Overlay the sketch to review the result")
                                        .on_hover_text("The photo goes under the drawing on layer AI-SKETCH-IMAGE (not plotted); turn the layer off or change Fade in Properties when done");
                                    ui.add_enabled(d.opts.overlay_sketch, egui::Slider::new(&mut d.opts.overlay_fade, 0..=90).text("fade %"));
                                });
                            }
                            ui.horizontal(|ui| {
                                if ui.button("Open as new drawing").clicked() {
                                    action = Some("new");
                                }
                                if ui.button("Insert into current drawing").clicked() {
                                    action = Some("insert");
                                }
                            });
                            ui.label(
                                RichText::new("Review magenta dashed items on layer AI-REVIEW. Everything is fully editable.")
                                    .size(11.0)
                                    .color(theme::TEXT_DIM),
                            );
                        }
                        Some(Err(e)) => {
                            ui.label(RichText::new(format!("✖ {e}")).color(theme::ERR));
                        }
                        None => {}
                    }
                });
            });
        });
        d.open = open;
        if open_settings {
            self.dlg.ai_settings = true;
        }
        if let Some(a) = action {
            if let Some(Ok(conv)) = d.result.take() {
                let name = d.path.as_ref().and_then(|p| p.file_stem()).and_then(|s| s.to_str()).unwrap_or("Sketch").to_string();
                if a == "new" {
                    let mut doc = self.new_document();
                    icad_vision::apply(&conv, &mut doc);
                    let mut t = Tab::new(doc, name);
                    t.dirty = true;
                    t.layout = true;
                    self.push_tab(t);
                    self.view_req = Some(ViewReq::Sheet);
                } else {
                    let t = self.tab();
                    t.checkpoint();
                    icad_vision::apply(&conv, &mut t.doc);
                    self.view_req = Some(ViewReq::Extents);
                }
                if d.opts.overlay_sketch {
                    if let Some(p) = &d.path {
                        let (fade, doc) = (d.opts.overlay_fade, &mut self.tabs[self.active].doc);
                        if icad_vision::overlay(&conv, doc, &p.display().to_string(), fade) {
                            self.log.push("The sketch is overlaid on layer AI-SKETCH-IMAGE for review (not plotted).".into());
                        }
                    }
                }
                self.log.push(format!("Image to CAD: inserted {} objects ({} dimensions applied).", conv.shapes.len(), conv.dims_applied));
                // Remember conversion preferences.
                self.settings.convert = d.opts.clone();
                self.settings.save();
                d.open = false;
            }
        }
        if d.open || d.rx.is_some() {
            self.dlg.ai = Some(d);
        }
    }

    fn layers_dialog(&mut self, ctx: &egui::Context) {
        if !self.dlg.layers {
            return;
        }
        let mut open = true;
        let new_layer = &mut self.dlg.new_layer;
        let t = &mut self.tabs[self.active];
        egui::Window::new("Layer Properties Manager").open(&mut open).default_width(760.0).show(ctx, |ui| {
            ui.horizontal(|ui| {
                ui.add(egui::TextEdit::singleline(new_layer).hint_text("new layer name").desired_width(180.0));
                if ui.button("➕ New Layer").clicked() {
                    let mut name = new_layer.trim().to_string();
                    if name.is_empty() {
                        name = format!("Layer{}", t.doc.layers.len());
                    }
                    if t.doc.layer(&name).is_none() {
                        t.checkpoint();
                        t.doc.layers.push(Layer::new(&name, 7, LineType::Continuous, 0.25));
                    }
                    new_layer.clear();
                }
            });
            ui.separator();
            let mut delete = None;
            let mut make_current = None;
            let mut changed = false;
            let cur = t.doc.current_layer.clone();
            let counts: Vec<usize> = t.doc.layers.iter().map(|l| t.doc.entities.iter().filter(|e| e.layer == l.name).count()).collect();
            egui::ScrollArea::vertical().max_height(420.0).show(ui, |ui| {
                egui::Grid::new("layers").striped(true).num_columns(9).spacing([10.0, 4.0]).show(ui, |ui| {
                    for h in ["Status", "Name", "On", "Lock", "Plot", "Color", "Linetype", "Lineweight", ""] {
                        ui.label(RichText::new(h).strong());
                    }
                    ui.end_row();
                    for (i, l) in t.doc.layers.iter_mut().enumerate() {
                        if ui.selectable_label(l.name == cur, if l.name == cur { "✔" } else { "  " }).on_hover_text("Set current").clicked()
                        {
                            make_current = Some(l.name.clone());
                        }
                        ui.label(format!("{}  ({})", l.name, counts[i]));
                        changed |= ui.checkbox(&mut l.visible, "").changed();
                        changed |= ui.checkbox(&mut l.locked, "").changed();
                        changed |= ui.checkbox(&mut l.plot, "").changed();
                        egui::ComboBox::from_id_salt(("lc", i)).selected_text(aci_name(l.color)).width(100.0).show_ui(ui, |ui| {
                            for (c, n) in ACI_NAMES {
                                changed |= ui.selectable_value(&mut l.color, c, RichText::new(n).color(theme::aci(c, false))).changed();
                            }
                            ui.horizontal(|ui| {
                                ui.label("Index");
                                changed |= ui.add(egui::DragValue::new(&mut l.color).range(1..=255)).changed();
                            });
                        });
                        egui::ComboBox::from_id_salt(("lt", i)).selected_text(l.linetype.name()).width(110.0).show_ui(ui, |ui| {
                            for lt in LineType::ALL {
                                changed |=
                                    ui.selectable_value(&mut l.linetype, lt, format!("{} — {}", lt.name(), lt.description())).changed();
                            }
                        });
                        egui::ComboBox::from_id_salt(("lw", i)).selected_text(format!("{:.2} mm", l.lineweight)).width(90.0).show_ui(
                            ui,
                            |ui| {
                                for w in LINEWEIGHTS {
                                    changed |= ui.selectable_value(&mut l.lineweight, w, format!("{w:.2} mm")).changed();
                                }
                            },
                        );
                        if l.name != "0" && ui.small_button("🗑").on_hover_text("Delete layer (must be empty)").clicked() {
                            delete = Some(i);
                        } else if l.name == "0" {
                            ui.label("");
                        }
                        ui.end_row();
                    }
                });
            });
            if changed {
                t.dirty = true;
            }
            if let Some(n) = make_current {
                t.doc.current_layer = n;
            }
            if let Some(i) = delete {
                let name = t.doc.layers[i].name.clone();
                if counts[i] > 0 {
                    ui.label(RichText::new(format!("Layer {name} is not empty.")).color(theme::ERR));
                } else if name == t.doc.current_layer {
                    ui.label(RichText::new("Cannot delete the current layer.").color(theme::ERR));
                } else {
                    t.checkpoint();
                    t.doc.layers.remove(i);
                }
            }
        });
        self.dlg.layers = open;
    }

    /// Page Setup edits the sheet on screen: the main sheet, or the shown layout's.
    fn page_dialog(&mut self, ctx: &egui::Context) {
        let t = &mut self.tabs[self.active];
        let paper = t.paper.filter(|li| *li < t.doc.layouts.len());
        if let Some(li) = paper {
            let d = &mut t.doc;
            std::mem::swap(&mut d.sheet, &mut d.layouts[li].sheet);
        }
        self.page_dialog_sheet(ctx);
        if let Some(li) = paper {
            let d = &mut self.tabs[self.active].doc;
            std::mem::swap(&mut d.sheet, &mut d.layouts[li].sheet);
        }
    }

    fn page_dialog_sheet(&mut self, ctx: &egui::Context) {
        if !self.dlg.page {
            return;
        }
        let mut open = true;
        let mut fit = false;
        let mut save_defaults = false;
        let mut make_template = false;
        let t = &mut self.tabs[self.active];
        let before = t.doc.sheet.clone();
        // Blocks with attributes can serve as the title block.
        let tb_blocks: Vec<String> = t.doc.blocks.iter().filter(|b| b.attdefs().next().is_some()).map(|b| b.name.clone()).collect();
        egui::Window::new("Page Setup & Title Block").open(&mut open).default_width(520.0).show(ctx, |ui| {
            let sh = &mut t.doc.sheet;
            ui.label(RichText::new("Sheet").strong());
            egui::Grid::new("sheet").num_columns(2).spacing([10.0, 6.0]).show(ui, |ui| {
                ui.label("Paper size");
                egui::ComboBox::from_id_salt("paper").selected_text(sh.paper.name()).show_ui(ui, |ui| {
                    for p in PaperSize::ALL {
                        let (w, h) = p.mm();
                        ui.selectable_value(&mut sh.paper, p, format!("{}  ({}×{} mm)", p.name(), w, h));
                    }
                });
                ui.end_row();
                ui.label("Orientation");
                ui.horizontal(|ui| {
                    ui.radio_value(&mut sh.landscape, true, "Landscape");
                    ui.radio_value(&mut sh.landscape, false, "Portrait");
                });
                ui.end_row();
                ui.label("Plot scale");
                ui.horizontal(|ui| {
                    egui::ComboBox::from_id_salt("scale").selected_text(sh.scale_label()).show_ui(ui, |ui| {
                        for s in STANDARD_SCALES {
                            ui.selectable_value(&mut sh.scale, s, scale_label(s));
                        }
                    });
                    let mut txt = sh.scale_label();
                    if ui.add(egui::TextEdit::singleline(&mut txt).desired_width(70.0)).on_hover_text("Custom, e.g. 1:3 or 3:1").changed() {
                        if let Some(v) = parse_scale(&txt) {
                            sh.scale = v;
                        }
                    }
                });
                ui.end_row();
                ui.label("Show border & title");
                ui.checkbox(&mut sh.visible, "");
                ui.end_row();
            });
            if ui.button("Fit sheet to drawing (choose paper & scale)").clicked() {
                fit = true;
            }
            ui.separator();
            ui.label(RichText::new("Title block").strong());
            ui.horizontal(|ui| {
                ui.label("Layout");
                let shown = sh.title_block.clone().unwrap_or_else(|| "Built-in (ISO 7200)".into());
                egui::ComboBox::from_id_salt("tb_block").width(200.0).selected_text(shown).show_ui(ui, |ui| {
                    ui.selectable_value(&mut sh.title_block, None, "Built-in (ISO 7200)");
                    for n in &tb_blocks {
                        ui.selectable_value(&mut sh.title_block, Some(n.clone()), n);
                    }
                });
                if ui
                    .small_button("Create editable copy")
                    .on_hover_text(
                        "Adds the built-in title block as a block with attributes and uses it. Insert and explode it, change it, \
                         and redefine it with BLOCK under the same name. Attribute tags name the fields that fill it in.",
                    )
                    .clicked()
                {
                    make_template = true;
                }
            });
            let tb = &mut sh.title;
            egui::Grid::new("tb").num_columns(2).spacing([10.0, 5.0]).show(ui, |ui| {
                for (label, v) in [
                    ("Company", &mut tb.company),
                    ("Title", &mut tb.title),
                    ("Drawing no.", &mut tb.drawing_no),
                    ("Revision", &mut tb.revision),
                    ("Drawn by", &mut tb.drawn_by),
                    ("Checked by", &mut tb.checked_by),
                    ("Approved by", &mut tb.approved_by),
                    ("Material", &mut tb.material),
                    ("Finish", &mut tb.finish),
                    ("Sheet", &mut tb.sheet_no),
                    ("Units", &mut tb.units),
                    ("General tolerance", &mut tb.tolerance),
                ] {
                    ui.label(label);
                    ui.add(egui::TextEdit::singleline(v).desired_width(300.0));
                    ui.end_row();
                }
                ui.label("Date");
                ui.horizontal(|ui| {
                    ui.add(egui::TextEdit::singleline(&mut tb.date).desired_width(120.0));
                    if ui.small_button("Today").clicked() {
                        tb.date = icad_core::sheet::today();
                    }
                });
                ui.end_row();
                ui.label("Projection");
                ui.horizontal(|ui| {
                    ui.radio_value(&mut tb.projection, Projection::FirstAngle, "First angle (ISO)");
                    ui.radio_value(&mut tb.projection, Projection::ThirdAngle, "Third angle (ANSI)");
                });
                ui.end_row();
            });
            ui.add_space(4.0);
            if ui.button("Use company & drawn-by as defaults for new drawings").clicked() {
                save_defaults = true;
            }
        });
        if make_template {
            let mut name = "TITLE_BLOCK".to_string();
            let mut k = 2;
            while t.doc.block(&name).is_some() {
                name = format!("TITLE_BLOCK_{k}");
                k += 1;
            }
            t.checkpoint();
            let entities = t
                .doc
                .sheet
                .title_block_template()
                .into_iter()
                .map(|(shape, lw)| icad_core::Entity {
                    id: t.doc.alloc_id(),
                    layer: "0".into(),
                    color: Default::default(),
                    linetype: Default::default(),
                    lineweight: Some(lw),
                    shape,
                })
                .collect();
            t.doc.define_block(icad_core::BlockDef {
                name: name.clone(),
                base: icad_core::V2::ZERO,
                entities,
                description: "Title block: base point at its bottom-right corner; attribute tags name the title fields".into(),
                xref: None,
            });
            t.doc.sheet.title_block = Some(name.clone());
            self.log.push(format!("Title block block {name} created and in use."));
        }
        if fit {
            let ext = t.doc.extents();
            if !ext.is_empty() {
                let pad = ext.width().max(ext.height()) * 0.12;
                t.doc.sheet.fit_to(&ext.expand(pad));
                self.view_req = Some(ViewReq::Sheet);
            }
        }
        if t.doc.sheet != before {
            // Coalesce edits: one undo step per dialog session.
            if !self.prop_editing {
                let mut snapshot = t.doc.clone();
                snapshot.sheet = before;
                t.undo_stack.push(snapshot);
                t.redo_stack.clear();
                self.prop_editing = true;
            }
            t.dirty = true;
        }
        if save_defaults {
            self.settings.company = self.tabs[self.active].doc.sheet.title.company.clone();
            self.settings.drawn_by = self.tabs[self.active].doc.sheet.title.drawn_by.clone();
            self.settings.save();
        }
        self.dlg.page = open;
    }

    fn dimstyle_dialog(&mut self, ctx: &egui::Context) {
        if !self.dlg.dimstyle {
            return;
        }
        let mut open = true;
        let t = &mut self.tabs[self.active];
        let before = (t.doc.dimstyle.clone(), t.doc.ltscale);
        egui::Window::new("Dimension Style").open(&mut open).default_width(360.0).show(ctx, |ui| {
            let ds = &mut t.doc.dimstyle;
            ui.label(RichText::new("Sizes are in paper millimetres; they scale with the sheet scale.").size(11.5).color(theme::TEXT_DIM));
            egui::Grid::new("ds").num_columns(2).spacing([10.0, 6.0]).show(ui, |ui| {
                let row = |ui: &mut egui::Ui, l: &str, v: &mut f64| {
                    ui.label(l);
                    ui.add(egui::DragValue::new(v).speed(0.05).range(0.0..=100.0).suffix(" mm"));
                    ui.end_row();
                };
                row(ui, "Text height", &mut ds.text_height);
                row(ui, "Arrow size", &mut ds.arrow_size);
                row(ui, "Extension line offset", &mut ds.ext_offset);
                row(ui, "Extend beyond dim line", &mut ds.ext_beyond);
                row(ui, "Text gap", &mut ds.text_gap);
                ui.label("Precision (decimals)");
                ui.add(egui::DragValue::new(&mut ds.decimals).range(0..=6));
                ui.end_row();
                ui.label("Suppress trailing zeros");
                ui.checkbox(&mut ds.suppress_trailing_zeros, "");
                ui.end_row();
            });
            ui.separator();
            ui.horizontal(|ui| {
                ui.label("Linetype scale (LTSCALE)");
                ui.add(egui::DragValue::new(&mut t.doc.ltscale).speed(0.05).range(0.01..=1000.0));
            });
        });
        if (t.doc.dimstyle.clone(), t.doc.ltscale) != before {
            if !self.prop_editing {
                let mut snap = t.doc.clone();
                snap.dimstyle = before.0;
                snap.ltscale = before.1;
                t.undo_stack.push(snap);
                t.redo_stack.clear();
                self.prop_editing = true;
            }
            t.dirty = true;
        }
        self.dlg.dimstyle = open;
    }

    fn export_dialog(&mut self, ctx: &egui::Context) {
        if !self.dlg.export {
            return;
        }
        let mut open = true;
        let mut go = None;
        egui::Window::new("Export").open(&mut open).default_width(380.0).show(ctx, |ui| {
            ui.horizontal(|ui| {
                ui.label("PDF/SVG plot style");
                let t = &mut self.tabs[self.active];
                let sheet = match t.paper {
                    Some(li) => &mut t.doc.layouts[li].sheet,
                    None => &mut t.doc.sheet,
                };
                egui::ComboBox::from_id_salt("exp_style").selected_text(sheet.plot_style.name()).show_ui(ui, |ui| {
                    for s in icad_core::layout::PlotStyle::ALL {
                        if ui.selectable_value(&mut sheet.plot_style, s, s.name()).changed() {
                            t.dirty = true;
                        }
                    }
                });
            });
            ui.checkbox(&mut self.settings.explode_dims_dxf, "DXF: explode dimensions into lines and text");
            ui.separator();
            ui.horizontal(|ui| {
                for (l, e) in [("PDF", "pdf"), ("DXF", "dxf"), ("DWG", "dwg"), ("SVG", "svg")] {
                    if ui.button(format!("Export {l}…")).clicked() {
                        go = Some(e);
                    }
                }
            });
            let conv = icad_io::dwg::find_converter();
            ui.label(
                RichText::new(match &conv {
                    Some(c) => format!("DWG converter: {}", c.name()),
                    None => "DWG needs the free ODA File Converter or LibreDWG (not found).".into(),
                })
                .size(11.5)
                .color(theme::TEXT_DIM),
            );
        });
        if let Some(e) = go {
            self.settings.save();
            self.export(e);
        }
        self.dlg.export = open;
    }

    fn help_dialogs(&mut self, ctx: &egui::Context) {
        if self.dlg.help {
            let mut open = true;
            egui::Window::new("ImagineCAD Help").open(&mut open).default_width(620.0).default_height(520.0).show(ctx, |ui| {
                egui::ScrollArea::vertical().show(ui, |ui| {
                    ui.label(RichText::new("Commands (type the name or alias, then Enter/Space)").strong());
                    egui::Grid::new("help_cmds").striped(true).num_columns(2).show(ui, |ui| {
                        for (n, a) in commands::COMMANDS {
                            ui.label(RichText::new(*n).monospace());
                            ui.label(RichText::new(*a).monospace().color(theme::TEXT_DIM));
                            ui.end_row();
                        }
                    });
                    ui.add_space(8.0);
                    ui.label(RichText::new("Coordinate input").strong());
                    ui.label("x,y  absolute   ·   @dx,dy  relative   ·   @dist<angle  polar   ·   a number = distance along the cursor direction");
                    ui.add_space(8.0);
                    ui.label(RichText::new("Mouse").strong());
                    ui.label("Wheel: zoom at cursor · Middle-drag or Alt+drag: pan · Double-click middle: zoom extents · Right-click: Enter / context menu");
                    ui.label("Click-click or drag left→right: window selection (fully inside) · right→left: crossing selection · Shift+click: remove from selection");
                    ui.label("Click a blue grip of a selected object to stretch it.");
                    ui.add_space(8.0);
                    ui.label(RichText::new("Keys").strong());
                    ui.label("Esc cancel · Enter/Space repeat or confirm · Delete erase · F3 OSNAP · F7 grid · F8 ortho · F9 snap · F12 dynamic input");
                    ui.label("Ctrl+Z/Y undo/redo · Ctrl+S save · Ctrl+O open · Ctrl+N new · Ctrl+P plot · Ctrl+I image-to-CAD · Ctrl+C/V copy/paste · Ctrl+A select all");
                });
            });
            self.dlg.help = open;
        }
        if self.dlg.ai_help {
            let mut open = true;
            egui::Window::new("Tips for best sketch conversion").open(&mut open).default_width(520.0).show(ctx, |ui| {
                for tip in [
                    "Use dark pen or a soft pencil on plain white paper. Squared paper works if the grid is faint.",
                    "Photograph straight-on with even light (no strong shadows), or scan at 200–300 dpi.",
                    "Draw the part outline as closed shapes; leave a small gap between extension lines and the part.",
                    "Write each dimension value clearly next to its dimension line: 50, 12.5, Ø20, R5, 2x Ø8.",
                    "Dimensions do not need to be to scale — ImagineCAD re-proportions the drawing so every dimension is exact.",
                    "Horizontal and vertical dimensions are enforced exactly; slanted ones are drawn but not yet enforced.",
                    "After conversion, check layer AI-REVIEW (magenta dashed) for strokes the AI was unsure about.",
                    "For handwriting, an AI vision model (any cloud API key, or local Ollama + qwen2.5vl) reads numbers far better than OCR.",
                ] {
                    ui.label(format!("• {tip}"));
                }
            });
            self.dlg.ai_help = open;
        }
        if self.dlg.about {
            let mut open = true;
            egui::Window::new("About ImagineCAD").open(&mut open).default_width(460.0).show(ctx, |ui| {
                ui.heading("ImagineCAD");
                ui.label(format!("Version {}", env!("CARGO_PKG_VERSION")));
                ui.label("AI-assisted 2D CAD for mechanical drawings. Built in Rust.");
                ui.add_space(6.0);
                ui.label(RichText::new(crate::LEGAL_NOTICE).size(11.5).color(theme::TEXT_DIM));
                ui.add_space(6.0);
                ui.horizontal(|ui| {
                    if let Ok(exe) = std::env::current_exe() {
                        if let Some(dir) = exe.parent() {
                            for (label, file) in [("License", "LICENSE"), ("Third-party licenses", "THIRD_PARTY_LICENSES.html")] {
                                let cands =
                                    [dir.join(file), dir.join("../Resources").join(file), dir.join("../share/doc/imaginecad").join(file)];
                                if let Some(f) = cands.iter().find(|p| p.is_file()) {
                                    ui.hyperlink_to(label, format!("file://{}", f.display()));
                                }
                            }
                        }
                    }
                    ui.hyperlink_to("GNU GPL v3", "https://www.gnu.org/licenses/gpl-3.0.html");
                });
            });
            self.dlg.about = open;
        }
        if let Some((title, msg)) = self.dlg.message.clone() {
            let mut close = false;
            egui::Window::new(title).collapsible(false).resizable(false).anchor(egui::Align2::CENTER_CENTER, vec2(0.0, 0.0)).show(
                ctx,
                |ui| {
                    ui.set_max_width(460.0);
                    ui.label(msg);
                    if ui.button("OK").clicked() {
                        close = true;
                    }
                },
            );
            if close {
                self.dlg.message = None;
            }
        }
    }

    fn confirm_dialogs(&mut self, ctx: &egui::Context) {
        if let Some(i) = self.dlg.close_tab {
            if i >= self.tabs.len() {
                self.dlg.close_tab = None;
                return;
            }
            let name = self.tabs[i].name.clone();
            let mut choice = None;
            egui::Window::new("Save changes?")
                .collapsible(false)
                .resizable(false)
                .anchor(egui::Align2::CENTER_CENTER, vec2(0.0, 0.0))
                .show(ctx, |ui| {
                    ui.label(format!("Save changes to {name}?"));
                    ui.horizontal(|ui| {
                        if ui.button("Save").clicked() {
                            choice = Some(0);
                        }
                        if ui.button("Don't save").clicked() {
                            choice = Some(1);
                        }
                        if ui.button("Cancel").clicked() {
                            choice = Some(2);
                        }
                    });
                });
            match choice {
                Some(0) => {
                    self.active = i;
                    if self.save(false) {
                        self.close_tab(i);
                    }
                    self.dlg.close_tab = None;
                }
                Some(1) => {
                    self.close_tab(i);
                    self.dlg.close_tab = None;
                }
                Some(_) => self.dlg.close_tab = None,
                None => {}
            }
        }
        if self.dlg.confirm_quit {
            let dirty: Vec<String> = self.tabs.iter().filter(|t| t.dirty).map(|t| t.name.clone()).collect();
            let mut choice = None;
            egui::Window::new("Quit ImagineCAD?")
                .collapsible(false)
                .resizable(false)
                .anchor(egui::Align2::CENTER_CENTER, vec2(0.0, 0.0))
                .show(ctx, |ui| {
                    ui.label(format!("Unsaved changes in: {}", dirty.join(", ")));
                    ui.horizontal(|ui| {
                        if ui.button("Quit without saving").clicked() {
                            choice = Some(true);
                        }
                        if ui.button("Cancel").clicked() {
                            choice = Some(false);
                        }
                    });
                });
            match choice {
                Some(true) => {
                    self.allow_close = true;
                    self.dlg.confirm_quit = false;
                    self.settings.save();
                    ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                }
                Some(false) => self.dlg.confirm_quit = false,
                None => {}
            }
        }
    }
}
