// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Proteen Das

//! Headless command-line interface:
//!
//! ```text
//! imaginecad convert sketch.jpg -o part.dxf [--no-ai] [--model qwen2.5vl:7b] ...
//! imaginecad export part.icad -o part.pdf
//! imaginecad ask part.icad "make the bore 12 mm" -o part.icad
//! ```

use icad_vision::llm::Provider;
use std::path::PathBuf;

const HELP: &str = "ImagineCAD — AI-assisted 2D CAD

USAGE:
  imaginecad [FILE]                      Start the editor (optionally opening a drawing or sketch image)
  imaginecad convert IMAGE -o OUT [opts] Convert a sketch image to CAD (OUT: .dxf .dwg .pdf .svg .icad)
  imaginecad export DRAWING -o OUT       Convert a drawing between formats (.icad/.dxf/.dwg → .dxf/.dwg/.pdf/.svg/.icad)
  imaginecad ask DRAWING \"CHANGE\" -o OUT  Let the AI edit a drawing as described (or answer a question about it);
                                         takes the same AI options as convert
  imaginecad publish SET.icadset -o OUT.pdf
                                         Plot every sheet of a sheet set to one PDF, numbered as in the set
  imaginecad --version | --help

CONVERT OPTIONS:
  --no-ai                 Geometry only (computer vision); no AI dimension reading
  --ollama-url URL        Ollama server (default http://localhost:11434)
  --model NAME            Ollama vision model (default qwen2.5vl:7b)
  --openai-url URL        Use an OpenAI-compatible server instead (LM Studio, llama.cpp, ...)
  --openai-model NAME     Model name on that server
  --api-key KEY           Any AI provider's API key (OpenAI, Anthropic, Gemini, OpenRouter, Groq,
                          Mistral, xAI, …); provider and model are detected automatically.
                          The key saved in the app (or IMAGINECAD_API_KEY) is used otherwise.
  --no-ocr                Do not use Tesseract even if installed
  --size MM               Overall size when no dimensions are found (default 100)
  --title TEXT            Title block: drawing title
  --company TEXT          Title block: company name
  --drawn-by TEXT         Title block: drawn by
";

fn take(args: &[String], flag: &str) -> Option<String> {
    args.iter().position(|a| a == flag).and_then(|i| args.get(i + 1).cloned())
}

/// Returns Some(exit code) when a CLI action ran, None to start the GUI.
pub fn run(args: &[String]) -> Option<i32> {
    let first = args.first()?.as_str();
    match first {
        "--help" | "-h" | "help" => {
            println!("{HELP}");
            Some(0)
        }
        "--version" | "-V" => {
            println!("imaginecad {}\n{}", env!("CARGO_PKG_VERSION"), crate::LEGAL_NOTICE);
            Some(0)
        }
        "convert" => Some(convert(&args[1..])),
        "export" => Some(export(&args[1..])),
        "ask" => Some(ask(&args[1..])),
        "publish" => Some(publish(&args[1..])),
        _ => None,
    }
}

fn out_path(args: &[String]) -> Option<PathBuf> {
    take(args, "-o").or_else(|| take(args, "--output")).map(PathBuf::from)
}

fn convert(args: &[String]) -> i32 {
    let Some(input) = args.first().filter(|a| !a.starts_with('-')) else {
        eprintln!("usage: imaginecad convert IMAGE -o OUT");
        return 2;
    };
    let Some(out) = out_path(args) else {
        eprintln!("missing -o OUT");
        return 2;
    };
    let mut opts = crate::settings::Settings::load().convert;
    if let Err(code) = ai_options(args, &mut opts.ai) {
        return code;
    }
    if let Some(s) = take(args, "--size").and_then(|s| s.parse().ok()) {
        opts.fallback_size_mm = s;
    }
    let bytes = match std::fs::read(input) {
        Ok(b) => b,
        Err(e) => {
            eprintln!("cannot read {input}: {e}");
            return 1;
        }
    };
    let conv = match icad_vision::convert(&bytes, &opts, &mut |f, s| eprintln!("[{:>3.0}%] {s}", f * 100.0)) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("conversion failed: {e}");
            return 1;
        }
    };
    for l in &conv.log {
        eprintln!("  {l}");
    }
    for w in &conv.warnings {
        eprintln!("  warning: {w}");
    }
    let s = crate::settings::Settings::load();
    let mut doc = icad_core::Document::new();
    doc.sheet.title.company =
        take(args, "--company").unwrap_or_else(|| if s.company.is_empty() { doc.sheet.title.company.clone() } else { s.company.clone() });
    doc.sheet.title.drawn_by = take(args, "--drawn-by").unwrap_or(s.drawn_by);
    icad_vision::apply(&conv, &mut doc);
    if let Some(t) = take(args, "--title") {
        doc.sheet.title.title = t;
    }
    write(&doc, &out)
}

fn export(args: &[String]) -> i32 {
    let Some(input) = args.first().filter(|a| !a.starts_with('-')) else {
        eprintln!("usage: imaginecad export DRAWING -o OUT");
        return 2;
    };
    let Some(out) = out_path(args) else {
        eprintln!("missing -o OUT");
        return 2;
    };
    match icad_io::open(std::path::Path::new(input)) {
        Ok(doc) => write(&doc, &out),
        Err(e) => {
            eprintln!("cannot open {input}: {e}");
            1
        }
    }
}

/// AI engine from the saved settings, adjusted by command-line options.
fn ai_options(args: &[String], ai: &mut icad_vision::llm::AiSettings) -> Result<(), i32> {
    if args.iter().any(|a| a == "--no-ai") {
        ai.provider = Provider::Disabled;
    }
    if args.iter().any(|a| a == "--no-ocr") {
        ai.use_tesseract = false;
    }
    if let Some(u) = take(args, "--ollama-url") {
        ai.ollama_url = u;
        ai.provider = Provider::Ollama;
    }
    if let Some(m) = take(args, "--model") {
        ai.ollama_model = m;
        ai.provider = Provider::Ollama;
    }
    if let Some(u) = take(args, "--openai-url") {
        ai.openai_url = u;
        ai.provider = Provider::OpenAiCompatible;
    }
    if let Some(m) = take(args, "--openai-model") {
        ai.openai_model = m;
    }
    if let Some(k) = crate::secrets::load() {
        ai.api_key = k;
    }
    if let Some(k) = take(args, "--api-key") {
        // Any provider's key: detect the provider and pick a vision model.
        match icad_vision::providers::detect(&k, take(args, "--openai-url").as_deref()) {
            Ok(d) => {
                ai.apply_detected(&d, &k);
                // An explicitly chosen model wins over the detected default.
                if let Some(m) = take(args, "--openai-model") {
                    ai.openai_model = m;
                }
                eprintln!("AI: {} · {}", d.vendor, ai.openai_model);
            }
            Err(e) => {
                eprintln!("API key: {e}");
                return Err(1);
            }
        }
    }
    Ok(())
}

fn publish(args: &[String]) -> i32 {
    let (Some(set_path), Some(out)) = (args.first().filter(|a| !a.starts_with('-')), out_path(args)) else {
        eprintln!("usage: imaginecad publish SET.icadset -o OUT.pdf");
        return 2;
    };
    let set_path = std::path::PathBuf::from(set_path);
    let set = match icad_core::sheetset::SheetSet::load(&set_path) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("{e}");
            return 1;
        }
    };
    let (pages, problems) = crate::sheetset_ui::set_pages(&set, Some(&set_path), icad_io::open);
    for p in &problems {
        eprintln!("skipped {p}");
    }
    match std::fs::write(&out, icad_io::pdf::write_pages(&pages, &set.name)) {
        Ok(()) => {
            eprintln!("wrote {} ({} sheet(s))", out.display(), pages.len());
            if problems.is_empty() {
                0
            } else {
                1
            }
        }
        Err(e) => {
            eprintln!("cannot write {}: {e}", out.display());
            1
        }
    }
}

fn ask(args: &[String]) -> i32 {
    let usage = "usage: imaginecad ask DRAWING \"describe the change\" -o OUT";
    let positional = |i: usize| args.get(i).filter(|a| !a.starts_with('-'));
    let (Some(input), Some(message)) = (positional(0), positional(1)) else {
        eprintln!("{usage}");
        return 2;
    };
    let out = out_path(args);
    let mut ai = crate::settings::Settings::load().convert.ai;
    if let Err(code) = ai_options(args, &mut ai) {
        return code;
    }
    let mut doc = match icad_io::open(std::path::Path::new(input.as_str())) {
        Ok(d) => d,
        Err(e) => {
            eprintln!("cannot open {input}: {e}");
            return 1;
        }
    };
    let turn = icad_vision::llm::Turn { user: true, text: message.to_string() };
    let answer = match icad_vision::assistant::ask(&ai, &[turn], &doc, &[]) {
        Ok(a) => a,
        Err(e) => {
            eprintln!("AI: {e}");
            return 1;
        }
    };
    println!("{}", answer.reply);
    let r = icad_vision::assistant::apply(&mut doc, &answer.actions);
    eprintln!("{} change(s) applied", r.applied);
    for e in &r.errors {
        eprintln!("  skipped: {e}");
    }
    match out {
        Some(out) => write(&doc, &out),
        None if r.applied == 0 => 0,
        None => {
            eprintln!("add -o OUT to save the changed drawing");
            0
        }
    }
}

fn write(doc: &icad_core::Document, out: &std::path::Path) -> i32 {
    match icad_io::save(doc, out, &Default::default()) {
        Ok(()) => {
            eprintln!("wrote {}", out.display());
            0
        }
        Err(e) => {
            eprintln!("cannot write {}: {e}", out.display());
            1
        }
    }
}
