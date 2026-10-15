// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Proteen Das

//! AI back-ends for reading dimension text from sketches.
//!
//! * **Any cloud API key**: OpenAI, Anthropic, Google Gemini, OpenRouter, Groq,
//!   Mistral, xAI, Together, Hugging Face, NVIDIA, Fireworks… The provider and
//!   model are configured automatically from the key (see `providers`).
//! * **Ollama** (local, free, open-weight models), e.g. `qwen2.5vl:7b`.
//! * **OpenAI-compatible** local servers: LM Studio, llama.cpp, vLLM, LocalAI.

use crate::labels::{parse, Label};
use base64::Engine as _;
use icad_core::geom::*;
use image::GrayImage;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::time::Duration;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Provider {
    /// Computer vision only; no dimension reading by AI.
    Disabled,
    Ollama,
    OpenAiCompatible,
    Anthropic,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct AiSettings {
    pub provider: Provider,
    pub ollama_url: String,
    pub ollama_model: String,
    pub openai_url: String,
    pub openai_model: String,
    /// Cloud API key. Never written to the settings file: the application keeps
    /// it in the OS keychain and injects it at run time.
    #[serde(default, skip_serializing, alias = "openai_key")]
    pub api_key: String,
    /// Display name of the auto-detected provider (e.g. "Google Gemini").
    #[serde(default)]
    pub vendor: String,
    #[serde(default = "default_anthropic_model")]
    pub anthropic_model: String,
    /// The first-run AI setup has been completed or skipped.
    #[serde(default)]
    pub setup_done: bool,
    /// Also run Tesseract OCR when it is installed.
    pub use_tesseract: bool,
    pub timeout_secs: u64,
}

impl Default for AiSettings {
    fn default() -> Self {
        Self {
            provider: Provider::Ollama,
            ollama_url: "http://localhost:11434".into(),
            ollama_model: "qwen2.5vl:7b".into(),
            openai_url: "http://localhost:1234/v1".into(),
            openai_model: "qwen2.5-vl-7b-instruct".into(),
            api_key: String::new(),
            vendor: String::new(),
            anthropic_model: default_anthropic_model(),
            setup_done: false,
            use_tesseract: true,
            timeout_secs: 300,
        }
    }
}

fn default_anthropic_model() -> String {
    "claude-opus-5-5".into()
}

impl AiSettings {
    /// Apply an auto-detected provider configuration.
    pub fn apply_detected(&mut self, d: &crate::providers::Detected, key: &str) {
        self.provider = d.provider;
        self.vendor = d.vendor.clone();
        self.api_key = key.to_string();
        match d.provider {
            Provider::Anthropic => self.anthropic_model = d.model.clone(),
            _ => {
                self.openai_url = d.base_url.clone();
                self.openai_model = d.model.clone();
            }
        }
        self.setup_done = true;
    }

    /// Human-readable engine description.
    pub fn describe(&self) -> String {
        match self.provider {
            Provider::Disabled => "AI off (geometry only)".into(),
            Provider::Ollama => format!("Ollama · {}", self.ollama_model),
            Provider::Anthropic => format!("Anthropic · {}", self.anthropic_model),
            Provider::OpenAiCompatible => {
                let v = if self.vendor.is_empty() { "OpenAI-compatible".to_string() } else { self.vendor.clone() };
                format!("{v} · {}", self.openai_model)
            }
        }
    }

    pub fn model(&self) -> &str {
        match self.provider {
            Provider::Ollama => &self.ollama_model,
            Provider::Anthropic => &self.anthropic_model,
            _ => &self.openai_model,
        }
    }
}

pub struct LlmReading {
    pub labels: Vec<Label>,
    pub title: Option<String>,
    /// Parametric part description, when the model recognised one.
    pub part: Option<crate::parts::PartSpec>,
    /// Set when the configured model could not be used and another was used instead.
    pub switched_model: Option<String>,
    /// Why the model was switched (for the user).
    pub switch_reason: Option<String>,
    /// The answer was cut off and completed at its last whole value.
    pub cut_off: bool,
    pub raw: String,
}

fn agent(timeout: u64) -> ureq::Agent {
    ureq::AgentBuilder::new().timeout_connect(Duration::from_secs(5)).timeout(Duration::from_secs(timeout)).build()
}

/// List models installed in Ollama (also serves as a connection test).
pub fn ollama_models(url: &str) -> Result<Vec<String>, String> {
    let v: Value = agent(5)
        .get(&format!("{}/api/tags", url.trim_end_matches('/')))
        .call()
        .map_err(|e| format!("Ollama not reachable at {url}: {e}"))?
        .into_json()
        .map_err(|e| e.to_string())?;
    Ok(v["models"].as_array().map(|a| a.iter().filter_map(|m| m["name"].as_str().map(String::from)).collect()).unwrap_or_default())
}

/// List models on an OpenAI-compatible server.
pub fn openai_models(url: &str, key: &str) -> Result<Vec<String>, String> {
    let mut req = agent(8).get(&format!("{}/models", url.trim_end_matches('/')));
    if !key.is_empty() {
        req = req.set("Authorization", &format!("Bearer {key}"));
    }
    let v: Value = req.call().map_err(|e| format!("Server not reachable at {url}: {e}"))?.into_json().map_err(|e| e.to_string())?;
    Ok(v["data"].as_array().map(|a| a.iter().filter_map(|m| m["id"].as_str().map(String::from)).collect()).unwrap_or_default())
}

pub fn prompt(w: u32, h: u32) -> String {
    format!(
        "You are an expert mechanical draftsman reading a photo of a hand-drawn engineering sketch ({w}x{h} pixels).\n\
1. List EVERY dimension value written on the drawing (e.g. 50, 12.5, Ø20, 2xØ8, R5, M6, 45°, 2.5x45°). Ignore values that \
are crossed out or scribbled over. For each give the exact text and its bounding box [x1, y1, x2, y2] normalised to \
0-1000 on each axis (x relative to the image width, y relative to the height, origin top-left).\n\
2. Describe the part parametrically in \"part\" when possible, using only the written values:\n\
 - Turned shaft/axle drawn in side view (stepped cylinder along a centre line): {{\"type\": \"shaft\", \"total_length\": T or null, \
\"segments\": [{{\"length\": L, \"diameter\": D}} ... from left to right], \"keyways\": [{{\"start\": distance from the left end, \
\"length\": L, \"width\": W or 0 if not written}}], \"chamfers\": [{{\"at\": boundary index (0 = left end, k = between segment k-1 and k, \
n = right end), \"size\": C}}]}}. Each segment is a stretch of constant diameter: if one length dimension covers several \
diameters (e.g. 50 + rest + 50), split it so every segment has its own length and diameter. The segment lengths must add up to \
the written total length when one is given.\n\
 - Rectangular plate: {{\"type\": \"plate\", \"width\": W, \"height\": H, \"holes\": [{{\"x\": X, \"y\": Y, \"diameter\": D}}]}} \
(hole positions from the bottom-left corner).\n\
 - Anything else: null.\n\
3. The part title if one is written.\n\
Answer with JSON only, no prose:\n\
{{\"title\": \"TITLE or null\", \"part\": {{...}} or null, \"dimensions\": [{{\"text\": \"Ø20\", \"bbox\": [x1, y1, x2, y2]}}]}}"
    )
}

/// Longest side of the image sent to the model.
const SEND_MAX: u32 = 2000;

/// PNG of the image (aspect preserved, longest side ≤ SEND_MAX).
fn send_png(img: &GrayImage) -> Result<(Vec<u8>, u32, u32), String> {
    let (w, h) = (img.width(), img.height());
    let k = (SEND_MAX as f64 / w.max(h) as f64).min(1.0);
    let (nw, nh) = (((w as f64 * k).round() as u32).max(1), ((h as f64 * k).round() as u32).max(1));
    let small = if k < 1.0 { image::imageops::resize(img, nw, nh, image::imageops::FilterType::Triangle) } else { img.clone() };
    let mut bytes = Vec::new();
    image::DynamicImage::ImageLuma8(small)
        .write_to(&mut std::io::Cursor::new(&mut bytes), image::ImageFormat::Png)
        .map_err(|e| e.to_string())?;
    Ok((bytes, nw, nh))
}

/// Extract the first JSON object from a model reply (tolerates code fences / prose).
pub fn extract_json(s: &str) -> Option<Value> {
    parse_reply(s).map(|(v, _)| v)
}

/// Parse the JSON object in a model's answer, tolerating code fences, text
/// around it, comments and trailing commas. An answer that was cut off is
/// completed at its last whole value; the flag tells whether that happened.
pub fn parse_reply(s: &str) -> Option<(Value, bool)> {
    if let Ok(v) = serde_json::from_str::<Value>(s.trim()) {
        return Some((v, false));
    }
    let body = clean_json(&s[s.find('{')?..]);
    if let Some(end) = object_end(&body) {
        if let Ok(v) = serde_json::from_str(&body[..end]) {
            return Some((v, false));
        }
    }
    complete_json(&body).map(|v| (v, true))
}

/// Remove comments and trailing commas outside strings.
fn clean_json(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let (mut in_str, mut esc) = (false, false);
    let mut it = s.chars().peekable();
    while let Some(c) = it.next() {
        if in_str {
            out.push(c);
            match (esc, c) {
                (true, _) => esc = false,
                (false, '\\') => esc = true,
                (false, '"') => in_str = false,
                _ => {}
            }
            continue;
        }
        match c {
            '"' => {
                in_str = true;
                out.push(c);
            }
            '/' if it.peek() == Some(&'/') => {
                for d in it.by_ref() {
                    if d == '\n' {
                        out.push('\n');
                        break;
                    }
                }
            }
            '/' if it.peek() == Some(&'*') => {
                it.next();
                let mut prev = ' ';
                for d in it.by_ref() {
                    if prev == '*' && d == '/' {
                        break;
                    }
                    prev = d;
                }
            }
            '}' | ']' => {
                let keep = out.trim_end().len();
                out.truncate(keep);
                if out.ends_with(',') {
                    out.pop();
                }
                out.push(c);
            }
            _ => out.push(c),
        }
    }
    out
}

/// Walk `s` outside strings, calling `f(byte index, char, depth before the char)`.
fn scan(s: &str, mut f: impl FnMut(usize, char, &[char]) -> bool) -> Option<Vec<char>> {
    let mut stack: Vec<char> = vec![];
    let (mut in_str, mut esc) = (false, false);
    for (i, c) in s.char_indices() {
        if in_str {
            match (esc, c) {
                (true, _) => esc = false,
                (false, '\\') => esc = true,
                (false, '"') => in_str = false,
                _ => {}
            }
            continue;
        }
        if !f(i, c, &stack) {
            return None;
        }
        match c {
            '"' => in_str = true,
            '{' => stack.push('}'),
            '[' => stack.push(']'),
            '}' | ']' => {
                stack.pop();
            }
            _ => {}
        }
    }
    (!in_str).then_some(stack)
}

/// Byte index just past the object that starts at 0, if it is complete.
fn object_end(s: &str) -> Option<usize> {
    let mut end = None;
    scan(s, |i, c, st| {
        if (c == '}' || c == ']') && st.len() == 1 {
            end = Some(i + 1);
            return false;
        }
        true
    });
    end
}

/// Complete a cut-off object: drop the unfinished trailing value and close
/// the open brackets. Cuts are made only between array items or top-level
/// keys, so a half-written item (a segment without its diameter) is dropped
/// whole instead of kept incomplete.
fn complete_json(s: &str) -> Option<Value> {
    let mut cuts = vec![];
    scan(s, |i, c, st| {
        if c == ',' && (st.last() == Some(&']') || st.len() == 1) {
            cuts.push(i);
        }
        true
    });
    cuts.into_iter().rev().take(400).find_map(|p| {
        let head = s[..p].trim_end();
        let open = scan(head, |_, _, _| true)?;
        let tail: String = open.iter().rev().collect();
        serde_json::from_str(&format!("{head}{tail}")).ok()
    })
}

/// Keep the raw answer for troubleshooting; returns where it was saved.
fn save_reply(raw: &str) -> String {
    let p = std::env::temp_dir().join("imaginecad-last-ai-reply.txt");
    match std::fs::write(&p, raw) {
        Ok(()) => p.display().to_string(),
        Err(_) => "nowhere (temporary folder not writable)".into(),
    }
}

fn bbox_of(v: &Value) -> Option<[f64; 4]> {
    let b = v.get("bbox").or_else(|| v.get("bbox_2d")).or_else(|| v.get("box"))?;
    let nums: Vec<f64> = match b {
        Value::Array(a) => a
            .iter()
            .flat_map(|x| match x {
                Value::Array(inner) => inner.iter().filter_map(|y| y.as_f64()).collect::<Vec<_>>(),
                other => other.as_f64().into_iter().collect(),
            })
            .collect(),
        Value::Object(o) => ["x1", "y1", "x2", "y2"].iter().filter_map(|k| o.get(*k).and_then(|x| x.as_f64())).collect(),
        _ => vec![],
    };
    (nums.len() == 4).then(|| [nums[0], nums[1], nums[2], nums[3]])
}

/// Convert a model's JSON answer into labels in y-up processed-image pixels.
/// Boxes may be normalised 0–1000 (as requested), 0–1, or absolute pixels of
/// the sent image (some models insist); the convention is detected per reply.
pub fn labels_from_json(v: &Value, img_w: f64, img_h: f64, sent_w: f64, sent_h: f64) -> (Vec<Label>, Option<String>) {
    let title = v["title"].as_str().map(str::trim).filter(|t| !t.is_empty() && !t.eq_ignore_ascii_case("null")).map(String::from);
    let list = v["dimensions"].as_array().or_else(|| v["labels"].as_array()).cloned().unwrap_or_default();
    let boxes: Vec<[f64; 4]> = list.iter().filter_map(bbox_of).collect();
    let all_unit = !boxes.is_empty() && boxes.iter().all(|b| b.iter().all(|x| *x <= 1.0));
    let absolute = boxes.iter().any(|b| b[0].max(b[2]) > 1000.0 || b[1].max(b[3]) > 1000.0);
    let (sx, sy) = if all_unit {
        (img_w, img_h)
    } else if absolute {
        (img_w / sent_w, img_h / sent_h)
    } else {
        (img_w / 1000.0, img_h / 1000.0)
    };
    let mut out = vec![];
    for d in list {
        let text = match d.get("text").or_else(|| d.get("value")) {
            Some(Value::String(s)) => s.clone(),
            Some(Value::Number(n)) => n.to_string(),
            _ => continue,
        };
        let Some((kind, value)) = parse(&text) else { continue };
        let Some(b) = bbox_of(&d) else { continue };
        let x0 = b[0].min(b[2]) * sx;
        let x1 = b[0].max(b[2]) * sx;
        let y0 = b[1].min(b[3]) * sy;
        let y1 = b[1].max(b[3]) * sy;
        if x1 < 0.0 || y1 < 0.0 || x0 > img_w || y0 > img_h {
            continue;
        }
        out.push(Label {
            text,
            kind,
            value,
            bbox: BBox::from_pts(V2::new(x0.max(0.0), img_h - y1.min(img_h)), V2::new(x1.min(img_w), img_h - y0.max(0.0))),
            source: "ai",
        });
    }
    (out, title)
}

/// True when a provider error says the model cannot take image input.
fn image_unsupported(code: u16, body: &str) -> bool {
    let b = body.to_ascii_lowercase();
    (code == 404 || code == 400 || code == 422)
        && (b.contains("image") || b.contains("vision") || b.contains("modalit") || b.contains("multimodal") || b.contains("no endpoints"))
}

fn openai_request(s: &AiSettings, model: &str, b64: &str, w: u32, h: u32) -> Result<String, (u16, String)> {
    let mut body = json!({
        "model": model,
        "messages": [{"role": "user", "content": [
            {"type": "text", "text": prompt(w, h)},
            {"type": "image_url", "image_url": {"url": format!("data:image/png;base64,{b64}")}}
        ]}],
    });
    // Local servers: deterministic sampling. Cloud models may reject
    // non-default temperatures, so leave theirs alone.
    if s.api_key.is_empty() {
        body["temperature"] = json!(0);
    }
    // OpenRouter reserves credit for the model's whole output limit when none is
    // given, which fails with 402 on a small balance; the answer needs far less.
    // Reasoning models spend part of that budget thinking; cap it so the
    // JSON answer always has room.
    if s.openai_url.contains("openrouter.ai") {
        body["max_tokens"] = json!(8192);
        body["reasoning"] = json!({"max_tokens": 2048});
    }
    let mut req = agent(s.timeout_secs).post(&format!("{}/chat/completions", s.openai_url.trim_end_matches('/')));
    if !s.api_key.is_empty() {
        req = req.set("Authorization", &format!("Bearer {}", s.api_key));
    }
    let v: Value = match req.send_json(body) {
        Ok(r) => r.into_json().map_err(|e| (0, e.to_string()))?,
        Err(ureq::Error::Status(code, r)) => return Err((code, r.into_string().unwrap_or_default())),
        Err(e) => return Err((0, e.to_string())),
    };
    if let Some(err) = v.get("error") {
        return Err((0, err.to_string()));
    }
    Ok(v["choices"][0]["message"]["content"].as_str().unwrap_or("").to_string())
}

/// The provider's error message (from a JSON body when possible), shortened.
fn short(body: &str) -> String {
    let msg = serde_json::from_str::<Value>(body)
        .ok()
        .and_then(|v| v["error"]["message"].as_str().or_else(|| v["message"].as_str()).map(String::from))
        .unwrap_or_else(|| body.to_string());
    msg.chars().take(300).collect()
}

/// Ask the configured vision model for dimension labels.
pub fn read_labels(s: &AiSettings, img: &GrayImage) -> Result<LlmReading, String> {
    if s.provider != Provider::Disabled && s.model().trim().is_empty() {
        return Err("No AI model is selected. Choose one in AI Settings.".into());
    }
    let (png, sw, sh) = send_png(img)?;
    let b64 = base64::engine::general_purpose::STANDARD.encode(&png);
    let mut switched_model = None;
    let mut switch_reason = None;
    let raw = match s.provider {
        Provider::Disabled => return Err("AI disabled".into()),
        Provider::Ollama => {
            let body = json!({
                "model": s.ollama_model,
                "stream": false,
                "format": "json",
                "options": {"temperature": 0, "num_ctx": 8192},
                "messages": [{"role": "user", "content": prompt(sw, sh), "images": [b64]}],
            });
            let v: Value = agent(s.timeout_secs)
                .post(&format!("{}/api/chat", s.ollama_url.trim_end_matches('/')))
                .send_json(body)
                .map_err(|e| format!("Ollama request failed: {e}. Is Ollama running and is `{}` pulled?", s.ollama_model))?
                .into_json()
                .map_err(|e| e.to_string())?;
            v["message"]["content"].as_str().unwrap_or("").to_string()
        }
        Provider::Anthropic => {
            let mut body = json!({
                "model": s.anthropic_model,
                "max_tokens": 16000,
                "messages": [{"role": "user", "content": [
                    {"type": "image", "source": {"type": "base64", "media_type": "image/png", "data": b64}},
                    {"type": "text", "text": prompt(sw, sh)}
                ]}],
            });
            let mut req = agent(s.timeout_secs)
                .post(&format!("{}/messages", crate::providers::anthropic_base()))
                .set("x-api-key", &s.api_key)
                .set("anthropic-version", "2023-06-01");
            // Server-side refusal fallback on the models that support it.
            if ["claude-opus-5-5", "claude-opus-5", "claude-fable-5-1", "claude-sonnet-5-5"].contains(&s.anthropic_model.as_str()) {
                body["fallbacks"] = json!("default");
                req = req.set("anthropic-beta", "server-side-fallback-2026-07-01");
            }
            let v: Value = req
                .send_json(body)
                .map_err(|e| match e {
                    ureq::Error::Status(code, r) => format!("Anthropic API error {code}: {}", r.into_string().unwrap_or_default()),
                    e => format!("Anthropic API request failed: {e}"),
                })?
                .into_json()
                .map_err(|e| e.to_string())?;
            if v["stop_reason"] == "refusal" {
                return Err("The model declined this request.".into());
            }
            v["content"]
                .as_array()
                .and_then(|blocks| blocks.iter().find(|b| b["type"] == "text").and_then(|b| b["text"].as_str()))
                .unwrap_or("")
                .to_string()
        }
        Provider::OpenAiCompatible => match openai_request(s, &s.openai_model, &b64, sw, sh) {
            Ok(t) => t,
            Err((code, body)) if image_unsupported(code, &body) => {
                // The configured model cannot read images: pick a vision model
                // from the same provider and try once more.
                let ids = crate::providers::vision_models(&s.openai_url, &s.api_key).unwrap_or_default();
                let vendors = crate::providers::VENDORS;
                let prefer: Vec<&str> = match vendors.iter().find(|v| s.openai_url.starts_with(v.base)) {
                    Some(v) => v.prefer.to_vec(),
                    None => vendors.iter().flat_map(|v| v.prefer.iter().copied()).collect(),
                };
                let Some(alt) = crate::providers::choose_model(&ids, &prefer, None).filter(|m| *m != s.openai_model) else {
                    return Err(format!(
                        "`{}` cannot read images and no vision model was found ({code}: {}).",
                        s.openai_model,
                        short(&body)
                    ));
                };
                let t = openai_request(s, &alt, &b64, sw, sh).map_err(|(c, b)| format!("AI request failed ({c}): {}", short(&b)))?;
                switched_model = Some(alt);
                switch_reason = Some("it cannot read images".to_string());
                t
            }
            Err((402, body)) => {
                // Not enough credit for a paid model: use a free vision model when the
                // provider has one (OpenRouter `:free` models need no credit).
                let ids = crate::providers::vision_models(&s.openai_url, &s.api_key).unwrap_or_default();
                let Some(free) = crate::providers::free_vision(&ids).filter(|m| *m != s.openai_model) else {
                    return Err(format!(
                        "The AI account has no credit for `{}` (402: {}). Add credit with the provider{}, or choose a free model in AI Settings.",
                        s.openai_model,
                        short(&body),
                        if s.openai_url.contains("openrouter.ai") { " (openrouter.ai/settings/credits)" } else { "" }
                    ));
                };
                let t = openai_request(s, &free, &b64, sw, sh).map_err(|(c, b)| format!("AI request failed ({c}): {}", short(&b)))?;
                switched_model = Some(free);
                switch_reason = Some(format!("the account has no credit for it ({})", short(&body)));
                t
            }
            Err((code, body)) => return Err(format!("AI request failed ({code}): {}", short(&body))),
        },
    };
    let (v, cut_off) = parse_reply(&raw).ok_or_else(|| {
        let preview: String = raw.split_whitespace().collect::<Vec<_>>().join(" ").chars().take(120).collect();
        format!("The model's answer was not usable JSON (\"{preview}…\"); full answer saved to {}.", save_reply(&raw))
    })?;
    if cut_off {
        save_reply(&raw);
    }
    let (labels, title) = labels_from_json(&v, img.width() as f64, img.height() as f64, sw as f64, sh as f64);
    let part = crate::parts::PartSpec::from_json(&v);
    Ok(LlmReading { labels, title, part, switched_model, switch_reason, cut_off, raw })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lenient_reply_parsing() {
        // Fenced, with prose, a comment and trailing commas.
        let r = "Here it is:\n```json\n{\"title\": null, // none written\n \"dimensions\": [{\"text\": \"50\", \"bbox\": [1,2,3,4],},],}\n```\nDone {ok}";
        let (v, cut) = parse_reply(r).unwrap();
        assert!(!cut && v["dimensions"][0]["text"] == "50");
        // Cut off mid-way: completed at the last whole value.
        let r = "```json\n{\"title\": null, \"part\": {\"type\": \"shaft\", \"total_length\": 2836, \"segments\": [{\"length\": 155, \"diameter\": 160}, {\"length\": 330, \"diam";
        let (v, cut) = parse_reply(r).unwrap();
        assert!(cut);
        assert_eq!(v["part"]["segments"].as_array().unwrap().len(), 1);
        assert_eq!(v["part"]["total_length"], 2836);
        // Braces and commas inside strings do not confuse the scanner.
        let (v, _) = parse_reply("{\"title\": \"A {b}, c\", \"x\": [1, 2").unwrap();
        assert_eq!(v["title"], "A {b}, c");
        assert!(parse_reply("no json here").is_none());
    }

    #[test]
    fn parses_model_reply_and_maps_coordinates() {
        let reply = "```json\n{\"title\": \"BRACKET\", \"dimensions\": [{\"text\": \"Ø20\", \"bbox\": [100, 300, 160, 330]}, {\"text\": \"abc\", \"bbox\": [0,0,1,1]}]}\n```";
        let v = extract_json(reply).unwrap();
        // Normalised 0–1000 boxes on a 2000×1000 image.
        let (labels, title) = labels_from_json(&v, 2000.0, 1000.0, 2000.0, 1000.0);
        assert_eq!(title.as_deref(), Some("BRACKET"));
        assert_eq!(labels.len(), 1);
        let b = labels[0].bbox;
        assert!((b.min.x - 200.0).abs() < 1e-9 && (b.max.x - 320.0).abs() < 1e-9);
        // y 300..330 ‰ of 1000 px → 300..330 px from the top → y-up 670..700.
        assert!((b.min.y - 670.0).abs() < 1e-9 && (b.max.y - 700.0).abs() < 1e-9);
        // Absolute pixel boxes (any coordinate > 1000) are detected too.
        let abs = serde_json::json!({"dimensions": [{"text": "50", "bbox": [1500, 100, 1600, 150]}]});
        let (l, _) = labels_from_json(&abs, 1000.0, 500.0, 2000.0, 1000.0);
        assert!((l[0].bbox.min.x - 750.0).abs() < 1e-9);
    }
}
