// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Proteen Das

//! Cloud AI providers and API-key auto-configuration.
//!
//! The user pastes any API key; ImagineCAD recognises the provider from the
//! key format, verifies it by listing that provider's models, and picks the
//! best vision-capable model automatically.

use crate::llm::Provider;
use serde_json::Value;
use std::time::Duration;

/// Wire protocol spoken by a vendor.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Api {
    /// `POST {base}/chat/completions` with bearer auth.
    OpenAi,
    /// Anthropic Messages API.
    Anthropic,
}

pub struct Vendor {
    pub name: &'static str,
    pub base: &'static str,
    pub api: Api,
    /// Key prefixes that identify this vendor.
    pub prefixes: &'static [&'static str],
    /// Preferred vision models, best first (exact id or id prefix).
    pub prefer: &'static [&'static str],
    /// Endpoint used to validate a key when `/models` is public.
    pub validate: Option<&'static str>,
}

pub const ANTHROPIC_BASE: &str = "https://api.anthropic.com/v1";

/// Anthropic API base (overridable with IMAGINECAD_ANTHROPIC_BASE for testing/proxies).
pub fn anthropic_base() -> String {
    std::env::var("IMAGINECAD_ANTHROPIC_BASE").unwrap_or_else(|_| ANTHROPIC_BASE.to_string())
}

pub const VENDORS: &[Vendor] = &[
    Vendor {
        name: "Anthropic",
        base: ANTHROPIC_BASE,
        api: Api::Anthropic,
        prefixes: &["sk-ant-"],
        prefer: &["claude-opus-5-5"],
        validate: None,
    },
    Vendor {
        name: "OpenRouter",
        base: "https://openrouter.ai/api/v1",
        api: Api::OpenAi,
        prefixes: &["sk-or-"],
        prefer: &["google/gemini-2.5-flash", "openai/gpt-4.1-mini", "openai/gpt-4o-mini", "qwen/qwen2.5-vl-72b-instruct"],
        validate: Some("https://openrouter.ai/api/v1/key"),
    },
    Vendor {
        name: "Google Gemini",
        base: "https://generativelanguage.googleapis.com/v1beta/openai",
        api: Api::OpenAi,
        prefixes: &["AIza"],
        prefer: &["gemini-2.5-flash", "gemini-2.5-pro", "gemini-2.0-flash", "gemini-flash-latest", "gemini-"],
        validate: None,
    },
    Vendor {
        name: "Groq",
        base: "https://api.groq.com/openai/v1",
        api: Api::OpenAi,
        prefixes: &["gsk_"],
        prefer: &["meta-llama/llama-4-maverick", "meta-llama/llama-4-scout", "llama-4"],
        validate: None,
    },
    Vendor {
        name: "xAI",
        base: "https://api.x.ai/v1",
        api: Api::OpenAi,
        prefixes: &["xai-"],
        prefer: &["grok-4", "grok-2-vision", "grok-vision"],
        validate: None,
    },
    Vendor {
        name: "Hugging Face",
        base: "https://router.huggingface.co/v1",
        api: Api::OpenAi,
        prefixes: &["hf_"],
        prefer: &["Qwen/Qwen2.5-VL-72B-Instruct", "Qwen/Qwen2.5-VL-7B-Instruct", "meta-llama/Llama-4-Scout"],
        validate: None,
    },
    Vendor {
        name: "NVIDIA",
        base: "https://integrate.api.nvidia.com/v1",
        api: Api::OpenAi,
        prefixes: &["nvapi-"],
        prefer: &["meta/llama-4-maverick", "meta/llama-3.2-90b-vision", "nvidia/llama-3.1-nemotron-nano-vl"],
        validate: None,
    },
    Vendor {
        name: "Fireworks",
        base: "https://api.fireworks.ai/inference/v1",
        api: Api::OpenAi,
        prefixes: &["fw_"],
        prefer: &["accounts/fireworks/models/llama4-maverick", "accounts/fireworks/models/qwen2p5-vl"],
        validate: None,
    },
    Vendor {
        name: "OpenAI",
        base: "https://api.openai.com/v1",
        api: Api::OpenAi,
        prefixes: &["sk-proj-", "sk-svcacct-", "sk-"],
        prefer: &["gpt-5", "gpt-4.1", "gpt-4o"],
        validate: None,
    },
    Vendor {
        name: "DeepSeek",
        base: "https://api.deepseek.com/v1",
        api: Api::OpenAi,
        prefixes: &["sk-"],
        prefer: &["vl", "vision"],
        validate: None,
    },
    Vendor {
        name: "Mistral",
        base: "https://api.mistral.ai/v1",
        api: Api::OpenAi,
        prefixes: &[],
        prefer: &["mistral-medium-latest", "pixtral-large-latest", "mistral-small-latest", "pixtral-12b"],
        validate: None,
    },
    Vendor {
        name: "Together AI",
        base: "https://api.together.xyz/v1",
        api: Api::OpenAi,
        prefixes: &[],
        prefer: &["meta-llama/Llama-4-Maverick", "Qwen/Qwen2.5-VL-72B-Instruct", "meta-llama/Llama-4-Scout"],
        validate: None,
    },
];

/// Words in model ids that indicate non-chat or non-vision models.
/// Free OpenRouter vision models, best first.
const FREE_PREFER: &[&str] = &[
    "google/gemma-4-31b-it:free",
    "nvidia/nemotron-3.5-super-vl:free",
    "google/gemma-4-26b-a4b-it:free",
    "qwen/qwen2.5-vl-72b-instruct:free",
];

/// A free (`:free`) model from a list of vision models.
pub fn free_vision(vision: &[String]) -> Option<String> {
    let free: Vec<String> = vision.iter().filter(|m| m.ends_with(":free")).cloned().collect();
    choose_model(&free, FREE_PREFER, None).or_else(|| free.iter().find(|m| usable(m)).cloned())
}

/// OpenRouter key info: true when the account cannot pay for paid models.
fn no_credit(info: &Value) -> bool {
    let d = &info["data"];
    d["is_free_tier"] == true || d["limit_remaining"].as_f64().is_some_and(|x| x <= 0.0)
}

const EXCLUDE: &[&str] = &[
    "safety",
    "guard",
    "embed",
    "audio",
    "realtime",
    "tts",
    "transcribe",
    "whisper",
    "dall-e",
    "image-gen",
    "moderation",
    "search",
    "guard",
    "rerank",
    "-image",
];
/// Words that indicate vision capability for unknown vendors.
const VISION_HINTS: &[&str] = &[
    "vl",
    "vision",
    "pixtral",
    "gemini",
    "gpt-4o",
    "gpt-4.1",
    "gpt-5",
    "claude",
    "llama-4",
    "llama4",
    "llava",
    "minicpm-v",
    "gemma-3",
    "gemma3",
    "grok-4",
    "qwen2.5-vl",
    "qwen3-vl",
];

#[derive(Clone, Debug)]
pub struct Detected {
    pub vendor: String,
    pub provider: Provider,
    pub base_url: String,
    pub model: String,
    pub models: Vec<ModelInfo>,
}

/// Whether using a model costs money, as far as the provider tells.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Cost {
    Free,
    Paid,
    Unknown,
}

#[derive(Clone, Debug, PartialEq)]
pub struct ModelInfo {
    pub id: String,
    pub cost: Cost,
}

impl ModelInfo {
    pub fn new(id: impl Into<String>, cost: Cost) -> Self {
        Self { id: id.into(), cost }
    }
}

/// Price tier of `id` on the endpoint `base`, from the listing `raw` when it
/// publishes prices (OpenRouter), otherwise from what is known of the provider.
pub fn model_cost(base: &str, id: &str, raw: &Value) -> Cost {
    if id.ends_with(":free") {
        return Cost::Free;
    }
    let entry = raw.as_array().and_then(|a| a.iter().find(|m| m["id"].as_str().map(|i| i.trim_start_matches("models/")) == Some(id)));
    if let Some(p) = entry.map(|m| &m["pricing"]).filter(|p| p.is_object()) {
        let num = |k: &str| p[k].as_str().and_then(|s| s.parse::<f64>().ok()).or_else(|| p[k].as_f64());
        if let (Some(a), Some(b)) = (num("prompt"), num("completion")) {
            return if a == 0.0 && b == 0.0 { Cost::Free } else { Cost::Paid };
        }
    }
    let b = base.to_ascii_lowercase();
    if b.contains("localhost") || b.contains("127.0.0.1") || b.contains("[::1]") {
        Cost::Free
    } else if b.contains("api.openai.com") || b.contains("anthropic.com") || b.contains("api.x.ai") {
        Cost::Paid
    } else {
        Cost::Unknown
    }
}

/// Vision models of an OpenAI-compatible endpoint with their price tier.
pub fn vision_models_priced(base: &str, key: &str) -> Result<Vec<ModelInfo>, String> {
    let (ids, raw) = list_openai(base, key)?;
    Ok(priced(base, &filter_vision(&ids, &raw), &raw))
}

fn priced(base: &str, ids: &[String], raw: &Value) -> Vec<ModelInfo> {
    ids.iter().map(|id| ModelInfo::new(id.clone(), model_cost(base, id, raw))).collect()
}

fn agent(secs: u64) -> ureq::Agent {
    ureq::AgentBuilder::new().timeout_connect(Duration::from_secs(5)).timeout(Duration::from_secs(secs)).build()
}

fn http_err(e: ureq::Error) -> String {
    match e {
        ureq::Error::Status(401 | 403, _) => "the key was rejected (401/403)".into(),
        ureq::Error::Status(code, r) => {
            format!("HTTP {code}: {}", r.into_string().unwrap_or_default().chars().take(200).collect::<String>())
        }
        ureq::Error::Transport(t) => t.to_string(),
    }
}

/// List models for an OpenAI-compatible endpoint. Returns (ids, raw json).
pub fn list_openai(base: &str, key: &str) -> Result<(Vec<String>, Value), String> {
    let mut req = agent(12).get(&format!("{}/models", base.trim_end_matches('/')));
    if !key.is_empty() {
        req = req.set("Authorization", &format!("Bearer {key}"));
    }
    let v: Value = req.call().map_err(http_err)?.into_json().map_err(|e| e.to_string())?;
    let arr = v["data"].as_array().or_else(|| v.as_array()).cloned().unwrap_or_default();
    let ids = arr
        .iter()
        .filter_map(|m| m["id"].as_str().or_else(|| m["name"].as_str()))
        .map(|s| s.trim_start_matches("models/").to_string())
        .collect();
    Ok((ids, Value::Array(arr)))
}

/// List Anthropic models; returns (ids, vision-capable ids) newest first.
pub fn list_anthropic(key: &str) -> Result<(Vec<String>, Vec<String>), String> {
    let v: Value = agent(12)
        .get(&format!("{}/models?limit=1000", anthropic_base()))
        .set("x-api-key", key)
        .set("anthropic-version", "2023-06-01")
        .call()
        .map_err(http_err)?
        .into_json()
        .map_err(|e| e.to_string())?;
    let arr = v["data"].as_array().cloned().unwrap_or_default();
    let ids: Vec<String> = arr.iter().filter_map(|m| m["id"].as_str().map(String::from)).collect();
    let vision: Vec<String> = arr
        .iter()
        .filter(|m| m["capabilities"]["image_input"]["supported"].as_bool().unwrap_or(true))
        .filter_map(|m| m["id"].as_str().map(String::from))
        .collect();
    Ok((ids, vision))
}

/// Whether a model entry's metadata says it accepts images: `Some(bool)` when
/// the provider publishes modalities/capabilities, `None` when unknown.
fn meta_vision(m: &Value) -> Option<bool> {
    if let Some(a) = m["architecture"]["input_modalities"].as_array() {
        return Some(a.iter().any(|x| x == "image"));
    }
    if let Some(b) = m["capabilities"]["vision"].as_bool() {
        return Some(b);
    }
    if let Some(b) = m["capabilities"]["image_input"]["supported"].as_bool() {
        return Some(b);
    }
    None
}

/// Models of an OpenAI-compatible endpoint that can read images. Uses the
/// provider's metadata when published; otherwise returns every usable model.
pub fn vision_models(base: &str, key: &str) -> Result<Vec<String>, String> {
    let (ids, raw) = list_openai(base, key)?;
    Ok(filter_vision(&ids, &raw))
}

fn filter_vision(ids: &[String], raw: &Value) -> Vec<String> {
    let arr = raw.as_array().cloned().unwrap_or_default();
    let known: Vec<(String, bool)> =
        arr.iter().filter_map(|m| Some((m["id"].as_str()?.trim_start_matches("models/").to_string(), meta_vision(m)?))).collect();
    if known.is_empty() {
        return ids.iter().filter(|i| usable(i)).cloned().collect();
    }
    known.into_iter().filter(|(id, v)| *v && usable(id)).map(|(id, _)| id).collect()
}

fn usable(id: &str) -> bool {
    let l = id.to_ascii_lowercase();
    !EXCLUDE.iter().any(|x| l.contains(x))
}

/// Pick the best model id from a list given preference patterns.
pub fn choose_model(ids: &[String], prefer: &[&str], raw: Option<&Value>) -> Option<String> {
    let usable_ids: Vec<&String> = ids.iter().filter(|i| usable(i)).collect();
    for p in prefer {
        if let Some(exact) = usable_ids.iter().find(|i| i.eq_ignore_ascii_case(p)) {
            return Some((*exact).clone());
        }
        let mut matches: Vec<&&String> =
            usable_ids.iter().filter(|i| i.to_ascii_lowercase().starts_with(&p.to_ascii_lowercase())).collect();
        if matches.is_empty() {
            matches = usable_ids.iter().filter(|i| i.to_ascii_lowercase().contains(&p.to_ascii_lowercase())).collect();
        }
        // Prefer the plain model over -nano/-preview/dated snapshots.
        matches.sort_by_key(|i| rank(i));
        if let Some(m) = matches.first() {
            return Some((**m).clone());
        }
    }
    // Capability metadata (OpenRouter: architecture.input_modalities, Mistral: capabilities.vision).
    if let Some(Value::Array(arr)) = raw {
        for m in arr {
            let img = m["architecture"]["input_modalities"].as_array().is_some_and(|a| a.iter().any(|x| x == "image"))
                || m["capabilities"]["vision"].as_bool() == Some(true);
            if img {
                if let Some(id) = m["id"].as_str().filter(|i| usable(i)) {
                    return Some(id.trim_start_matches("models/").to_string());
                }
            }
        }
    }
    let mut hinted: Vec<&&String> = usable_ids.iter().filter(|i| VISION_HINTS.iter().any(|h| i.to_ascii_lowercase().contains(h))).collect();
    hinted.sort_by_key(|i| rank(i));
    hinted.first().map(|s| (**s).clone())
}

/// Lower is better: full models before nano/mini/lite/preview snapshots.
fn rank(id: &str) -> (u8, u8, usize) {
    let l = id.to_ascii_lowercase();
    let small = ["nano", "mini", "lite", "small", "tiny"].iter().filter(|w| l.contains(*w)).count() as u8;
    let unstable = ["preview", "exp", "beta"].iter().filter(|w| l.contains(*w)).count() as u8;
    (small, unstable, id.len())
}

fn candidates(key: &str) -> Vec<&'static Vendor> {
    let k = key.trim();
    let mut by_prefix: Vec<&Vendor> = VENDORS.iter().filter(|v| v.prefixes.iter().any(|p| k.starts_with(p))).collect();
    // Longest matching prefix first (sk-ant- before sk-).
    by_prefix.sort_by_key(|v| std::cmp::Reverse(v.prefixes.iter().filter(|p| k.starts_with(**p)).map(|p| p.len()).max().unwrap_or(0)));
    if by_prefix.is_empty() {
        // Keys without a recognisable prefix: try the vendors that issue such keys.
        VENDORS.iter().filter(|v| v.prefixes.is_empty() || v.name == "OpenAI").collect()
    } else {
        by_prefix
    }
}

/// Recognise the provider of `key` (or use `custom_base`), verify the key and
/// choose a vision model.
pub fn detect(key: &str, custom_base: Option<&str>) -> Result<Detected, String> {
    let key = key.trim();
    if key.is_empty() && custom_base.is_none() {
        return Err("Enter an API key.".into());
    }
    if let Some(base) = custom_base.filter(|b| !b.trim().is_empty()) {
        let (ids, raw) = list_openai(base, key).map_err(|e| format!("{base}: {e}"))?;
        let vision = filter_vision(&ids, &raw);
        let model = choose_model(&vision, &[], Some(&raw)).ok_or("The server lists no vision-capable model.")?;
        return Ok(Detected {
            vendor: "Custom endpoint".into(),
            provider: Provider::OpenAiCompatible,
            base_url: base.into(),
            model,
            models: priced(base, &vision, &raw),
        });
    }
    let mut errors = vec![];
    for v in candidates(key) {
        match v.api {
            Api::Anthropic => match list_anthropic(key) {
                Ok((_, vision)) => {
                    let model =
                        choose_model(&vision, v.prefer, None).or_else(|| vision.first().cloned()).ok_or("No vision model available.")?;
                    return Ok(Detected {
                        vendor: v.name.into(),
                        provider: Provider::Anthropic,
                        base_url: v.base.into(),
                        model,
                        models: vision.into_iter().map(|id| ModelInfo::new(id, Cost::Paid)).collect(),
                    });
                }
                Err(e) => errors.push(format!("{}: {e}", v.name)),
            },
            Api::OpenAi => {
                let mut free_only = false;
                if let Some(url) = v.validate {
                    match agent(10).get(url).set("Authorization", &format!("Bearer {key}")).call().map_err(http_err) {
                        Ok(r) => free_only = r.into_json::<Value>().is_ok_and(|info| no_credit(&info)),
                        Err(e) => {
                            errors.push(format!("{}: {e}", v.name));
                            continue;
                        }
                    }
                }
                match list_openai(v.base, key) {
                    Ok((ids, raw)) => match free_only
                        .then(|| free_vision(&filter_vision(&ids, &raw)))
                        .flatten()
                        .or_else(|| choose_model(&filter_vision(&ids, &raw), v.prefer, Some(&raw)))
                    {
                        Some(model) => {
                            return Ok(Detected {
                                vendor: v.name.into(),
                                provider: Provider::OpenAiCompatible,
                                base_url: v.base.into(),
                                model,
                                models: priced(v.base, &filter_vision(&ids, &raw), &raw),
                            })
                        }
                        None => errors.push(format!("{}: key works, but this provider offers no vision model", v.name)),
                    },
                    Err(e) => errors.push(format!("{}: {e}", v.name)),
                }
            }
        }
    }
    Err(format!("Could not use this key. {}", errors.join("; ")))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn prefixes_route_to_vendors() {
        assert_eq!(candidates("sk-ant-api03-xyz")[0].name, "Anthropic");
        assert_eq!(candidates("sk-or-v1-abc")[0].name, "OpenRouter");
        assert_eq!(candidates("AIzaSyD")[0].name, "Google Gemini");
        assert_eq!(candidates("gsk_123")[0].name, "Groq");
        assert_eq!(candidates("sk-proj-123")[0].name, "OpenAI");
        let unknown: Vec<&str> = candidates("0123456789abcdef").iter().map(|v| v.name).collect();
        assert!(unknown.contains(&"Mistral") && unknown.contains(&"Together AI"));
    }

    #[test]
    fn text_only_models_are_filtered_out() {
        let raw = serde_json::json!([
            {"id": "z-ai/glm-5.3", "architecture": {"input_modalities": ["text"]}},
            {"id": "z-ai/glm-5.3-flash", "architecture": {"input_modalities": ["text", "image", "video"]}},
            {"id": "google/gemini-2.5-flash", "architecture": {"input_modalities": ["file", "image", "text"]}}
        ]);
        let ids: Vec<String> = ["z-ai/glm-5.3", "z-ai/glm-5.3-flash", "google/gemini-2.5-flash"].iter().map(|s| s.to_string()).collect();
        let v = filter_vision(&ids, &raw);
        assert!(!v.contains(&"z-ai/glm-5.3".to_string()) && v.len() == 2);
        assert_eq!(choose_model(&v, VENDORS[1].prefer, Some(&raw)).as_deref(), Some("google/gemini-2.5-flash"));
    }

    #[test]
    fn no_credit_falls_back_to_free_vision_model() {
        let v: Vec<String> = ["google/gemini-2.5-flash", "nvidia/nemotron-3.5-content-safety:free", "google/gemma-4-31b-it:free"]
            .iter()
            .map(|s| s.to_string())
            .collect();
        assert_eq!(free_vision(&v).as_deref(), Some("google/gemma-4-31b-it:free"));
        assert_eq!(free_vision(&v[..2]), None, "safety classifiers are not drawing readers");
        assert!(no_credit(&serde_json::json!({"data": {"is_free_tier": true, "limit_remaining": null}})));
        assert!(no_credit(&serde_json::json!({"data": {"is_free_tier": false, "limit_remaining": 0}})));
        assert!(!no_credit(&serde_json::json!({"data": {"is_free_tier": false, "limit_remaining": null}})));
    }

    #[test]
    fn price_tags() {
        let raw = serde_json::json!([
            {"id": "google/gemini-2.5-flash", "pricing": {"prompt": "0.0000003", "completion": "0.0000025"}},
            {"id": "google/gemma-4-31b-it:free", "pricing": {"prompt": "0", "completion": "0"}},
            {"id": "acme/promo", "pricing": {"prompt": "0", "completion": "0"}},
            {"id": "acme/unpriced"}
        ]);
        let or = "https://openrouter.ai/api/v1";
        assert_eq!(model_cost(or, "google/gemini-2.5-flash", &raw), Cost::Paid);
        assert_eq!(model_cost(or, "google/gemma-4-31b-it:free", &raw), Cost::Free);
        assert_eq!(model_cost(or, "acme/promo", &raw), Cost::Free);
        assert_eq!(model_cost(or, "acme/unpriced", &raw), Cost::Unknown);
        assert_eq!(model_cost("https://api.openai.com/v1", "gpt-5", &Value::Null), Cost::Paid);
        assert_eq!(model_cost("http://localhost:1234/v1", "qwen2.5-vl", &Value::Null), Cost::Free);
    }

    #[test]
    fn model_choice_prefers_vision_flagship() {
        let ids: Vec<String> = ["text-embedding-3-large", "gpt-4o-mini", "gpt-5-nano", "gpt-5", "gpt-4o-realtime-preview", "dall-e-3"]
            .iter()
            .map(|s| s.to_string())
            .collect();
        assert_eq!(choose_model(&ids, &["gpt-5", "gpt-4o"], None).as_deref(), Some("gpt-5"));
        let g: Vec<String> = ["gemini-2.5-flash-lite", "gemini-2.5-flash", "text-embedding-004"].iter().map(|s| s.to_string()).collect();
        assert_eq!(choose_model(&g, &["gemini-2.5-flash"], None).as_deref(), Some("gemini-2.5-flash"));
        let custom: Vec<String> = ["text-embedding-3-large", "gpt-5-nano", "gpt-5"].iter().map(|s| s.to_string()).collect();
        assert_eq!(choose_model(&custom, &[], None).as_deref(), Some("gpt-5"));
        let raw = serde_json::json!([{"id": "a/text"}, {"id": "b/vis", "architecture": {"input_modalities": ["text", "image"]}}]);
        assert_eq!(choose_model(&["a/text".into(), "b/vis".into()], &[], Some(&raw)).as_deref(), Some("b/vis"));
    }
}
