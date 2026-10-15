// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Proteen Das

//! Tesseract OCR (free, Apache-2.0) via its command-line tool. Good for
//! neatly printed dimension text; handwriting is better served by a local
//! vision-language model (see `llm`).

use crate::labels::{parse, Label};
use icad_core::geom::*;
use image::GrayImage;
use std::path::PathBuf;
use std::process::Command;

pub fn find_tesseract() -> Option<PathBuf> {
    let mut cands: Vec<PathBuf> = vec![];
    if let Some(paths) = std::env::var_os("PATH") {
        for d in std::env::split_paths(&paths) {
            cands.push(d.join(if cfg!(windows) { "tesseract.exe" } else { "tesseract" }));
        }
    }
    cands.extend(
        [
            "/opt/homebrew/bin/tesseract",
            "/usr/local/bin/tesseract",
            "/usr/bin/tesseract",
            "C:\\Program Files\\Tesseract-OCR\\tesseract.exe",
            "C:\\Program Files (x86)\\Tesseract-OCR\\tesseract.exe",
        ]
        .map(PathBuf::from),
    );
    cands.into_iter().find(|p| p.is_file())
}

struct Word {
    key: (u32, u32, u32),
    x: f64,
    y: f64,
    w: f64,
    h: f64,
    text: String,
}

fn run_once(exe: &PathBuf, img: &GrayImage) -> Result<Vec<Word>, String> {
    let tmp = std::env::temp_dir().join(format!(
        "imaginecad-ocr-{}-{}.png",
        std::process::id(),
        std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_nanos()).unwrap_or(0)
    ));
    img.save(&tmp).map_err(|e| e.to_string())?;
    let out = Command::new(exe)
        .arg(&tmp)
        .arg("stdout")
        .args(["--psm", "11", "-c", "tessedit_char_whitelist=0123456789.,RrØøMxX°±"])
        .arg("tsv")
        .output();
    let _ = std::fs::remove_file(&tmp);
    let out = out.map_err(|e| format!("tesseract failed to start: {e}"))?;
    if !out.status.success() {
        return Err(format!("tesseract error: {}", String::from_utf8_lossy(&out.stderr)));
    }
    let tsv = String::from_utf8_lossy(&out.stdout);
    let mut words = vec![];
    for line in tsv.lines().skip(1) {
        let c: Vec<&str> = line.split('\t').collect();
        if c.len() < 12 || c[0] != "5" {
            continue;
        }
        let conf: f64 = c[10].parse().unwrap_or(-1.0);
        let text = c[11].trim();
        if conf < 30.0 || text.is_empty() || !text.chars().any(|ch| ch.is_ascii_digit()) {
            continue;
        }
        let num = |i: usize| c[i].parse::<f64>().unwrap_or(0.0);
        words.push(Word {
            key: (num(2) as u32, num(3) as u32, num(4) as u32),
            x: num(6),
            y: num(7),
            w: num(8),
            h: num(9),
            text: text.to_string(),
        });
    }
    Ok(words)
}

fn group(words: Vec<Word>) -> Vec<(String, f64, f64, f64, f64)> {
    let mut out: Vec<(String, f64, f64, f64, f64)> = vec![];
    let mut last_key = None;
    for w in words {
        let x1 = w.x + w.w;
        let y1 = w.y + w.h;
        if last_key == Some(w.key) {
            if let Some(l) = out.last_mut() {
                if w.x - l.3 < w.h * 1.2 {
                    l.0.push(' ');
                    l.0.push_str(&w.text);
                    l.1 = l.1.min(w.x);
                    l.2 = l.2.min(w.y);
                    l.3 = l.3.max(x1);
                    l.4 = l.4.max(y1);
                    continue;
                }
            }
        }
        last_key = Some(w.key);
        out.push((w.text, w.x, w.y, x1, y1));
    }
    out
}

/// OCR dimension labels in a clean black-on-white image (top-left pixel coords
/// are converted to y-up). Reads horizontal and vertical (rotated) text.
pub fn read_labels(exe: &PathBuf, img: &GrayImage) -> Result<Vec<Label>, String> {
    let h = img.height() as f64;
    let mut labels = vec![];
    for (text, x0, y0, x1, y1) in group(run_once(exe, img)?) {
        if let Some((kind, value)) = parse(&text) {
            labels.push(Label { text, kind, value, bbox: BBox::from_pts(V2::new(x0, h - y1), V2::new(x1, h - y0)), source: "tesseract" });
        }
    }
    // Vertical text: rotate 90° clockwise, read, map boxes back.
    let rot = image::imageops::rotate90(img);
    for (text, rx0, ry0, rx1, ry1) in group(run_once(exe, &rot)?) {
        if let Some((kind, value)) = parse(&text) {
            // Rotated pixel (rx, ry) came from original (x = ry, y = h - 1 - rx).
            let ox0 = ry0;
            let ox1 = ry1;
            let oy0 = h - rx1;
            let oy1 = h - rx0;
            let bbox = BBox::from_pts(V2::new(ox0, h - oy1), V2::new(ox1, h - oy0));
            if labels.iter().any(|l: &Label| l.bbox.intersects(&bbox)) {
                continue;
            }
            labels.push(Label { text, kind, value, bbox, source: "tesseract" });
        }
    }
    Ok(labels)
}
