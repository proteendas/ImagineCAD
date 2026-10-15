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
use image::{GrayImage, Luma};
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

/// Side of the square image sent to the model. Using a padded 1000×1000 image
/// makes absolute-pixel and 0–1000-normalised coordinate conventions coincide.
const SIDE: u32 = 1000;

pub struct LlmReading {
    pub labels: Vec<Label>,
    pub title: Option<String>,
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

pub fn prompt() -> String {
    format!(
        "You are reading a hand-drawn mechanical engineering sketch. The image is {SIDE}x{SIDE} pixels \
(origin top-left). List EVERY dimension value written on the drawing, e.g. 50, 12.5, Ø20, 2xØ8, R5, M6, 45°. \
For each give the exact text as written and the tight bounding box of that text as [x1, y1, x2, y2] in pixels \
of this image. Do not invent values; ignore letters that are not dimensions. If a part name/title is written, \
return it. Answer with JSON only, no prose:\n\
{{\"title\": \"PART NAME or null\", \"dimensions\": [{{\"text\": \"Ø20\", \"bbox\": [x1, y1, x2, y2]}}]}}"
    )
}

/// Pad the image to a white square and scale to SIDE. Returns png bytes and
/// the factor/offset to map back: original = (p - off) / k.
fn square_png(img: &GrayImage) -> Result<(Vec<u8>, f64, V2), String> {
    let (w, h) = (img.width(), img.height());
    let side = w.max(h);
    let mut sq = GrayImage::from_pixel(side, side, Luma([255]));
    let (ox, oy) = ((side - w) / 2, (side - h) / 2);
    image::imageops::overlay(&mut sq, img, ox as i64, oy as i64);
    let sq = image::imageops::resize(&sq, SIDE, SIDE, image::imageops::FilterType::Triangle);
    let mut bytes = Vec::new();
    image::DynamicImage::ImageLuma8(sq)
        .write_to(&mut std::io::Cursor::new(&mut bytes), image::ImageFormat::Png)
        .map_err(|e| e.to_string())?;
    let k = SIDE as f64 / side as f64;
    Ok((bytes, k, V2::new(ox as f64 * k, oy as f64 * k)))
}

/// Extract the first JSON object from a model reply (tolerates code fences / prose).
pub fn extract_json(s: &str) -> Option<Value> {
    if let Ok(v) = serde_json::from_str::<Value>(s.trim()) {
        return Some(v);
    }
    let start = s.find('{')?;
    let end = s.rfind('}')?;
    serde_json::from_str(&s[start..=end]).ok()
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
pub fn labels_from_json(v: &Value, img_w: f64, img_h: f64, k: f64, off: V2) -> (Vec<Label>, Option<String>) {
    let title = v["title"].as_str().map(str::trim).filter(|t| !t.is_empty() && !t.eq_ignore_ascii_case("null")).map(String::from);
    let list = v["dimensions"].as_array().or_else(|| v["labels"].as_array()).cloned().unwrap_or_default();
    let mut out = vec![];
    for d in list {
        let text = match d.get("text").or_else(|| d.get("value")) {
            Some(Value::String(s)) => s.clone(),
            Some(Value::Number(n)) => n.to_string(),
            _ => continue,
        };
        let Some((kind, value)) = parse(&text) else { continue };
        let Some(mut b) = bbox_of(&d) else { continue };
        if b.iter().all(|x| *x <= 1.0) {
            b.iter_mut().for_each(|x| *x *= SIDE as f64);
        }
        let x0 = (b[0].min(b[2]) - off.x) / k;
        let x1 = (b[0].max(b[2]) - off.x) / k;
        let y0 = (b[1].min(b[3]) - off.y) / k;
        let y1 = (b[1].max(b[3]) - off.y) / k;
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

/// Ask the configured vision model for dimension labels.
pub fn read_labels(s: &AiSettings, img: &GrayImage) -> Result<LlmReading, String> {
    let (png, k, off) = square_png(img)?;
    let b64 = base64::engine::general_purpose::STANDARD.encode(&png);
    let raw = match s.provider {
        Provider::Disabled => return Err("AI disabled".into()),
        Provider::Ollama => {
            let body = json!({
                "model": s.ollama_model,
                "stream": false,
                "format": "json",
                "options": {"temperature": 0, "num_ctx": 8192},
                "messages": [{"role": "user", "content": prompt(), "images": [b64]}],
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
                    {"type": "text", "text": prompt()}
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
        Provider::OpenAiCompatible => {
            let mut body = json!({
                "model": s.openai_model,
                "messages": [{"role": "user", "content": [
                    {"type": "text", "text": prompt()},
                    {"type": "image_url", "image_url": {"url": format!("data:image/png;base64,{b64}")}}
                ]}],
            });
            // Local servers: deterministic sampling. Cloud models may reject
            // non-default temperatures, so leave theirs alone.
            if s.api_key.is_empty() {
                body["temperature"] = json!(0);
            }
            let mut req = agent(s.timeout_secs).post(&format!("{}/chat/completions", s.openai_url.trim_end_matches('/')));
            if !s.api_key.is_empty() {
                req = req.set("Authorization", &format!("Bearer {}", s.api_key));
            }
            let v: Value = req.send_json(body).map_err(|e| format!("AI request failed: {e}"))?.into_json().map_err(|e| e.to_string())?;
            v["choices"][0]["message"]["content"].as_str().unwrap_or("").to_string()
        }
    };
    let v = extract_json(&raw).ok_or_else(|| format!("Model did not return JSON: {}", raw.chars().take(200).collect::<String>()))?;
    let (labels, title) = labels_from_json(&v, img.width() as f64, img.height() as f64, k, off);
    Ok(LlmReading { labels, title, raw })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_model_reply_and_maps_coordinates() {
        let reply = "```json\n{\"title\": \"BRACKET\", \"dimensions\": [{\"text\": \"Ø20\", \"bbox\": [100, 300, 160, 330]}, {\"text\": \"abc\", \"bbox\": [0,0,1,1]}]}\n```";
        let v = extract_json(reply).unwrap();
        // Image 2000×1000 padded to 2000×2000 → k = 0.5, y offset 250 in model space.
        let (labels, title) = labels_from_json(&v, 2000.0, 1000.0, 0.5, V2::new(0.0, 250.0));
        assert_eq!(title.as_deref(), Some("BRACKET"));
        assert_eq!(labels.len(), 1);
        let b = labels[0].bbox;
        assert!((b.min.x - 200.0).abs() < 1e-9 && (b.max.x - 320.0).abs() < 1e-9);
        // Top-left y 300..330 → (y - 250) / 0.5 = 100..160 → y-up 840..900.
        assert!((b.min.y - 840.0).abs() < 1e-9 && (b.max.y - 900.0).abs() < 1e-9);
    }
}
