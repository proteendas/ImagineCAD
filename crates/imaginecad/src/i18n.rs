// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Proteen Das

//! Interface languages. The ribbon, menus, palettes and command line are
//! shown in the chosen language; commands keep their English names (so
//! scripts, aliases and plugins work in every language).
//!
//! Translations are `assets/i18n/<code>.json`: English text → translation.
//! Text without a translation is shown in English.

use std::collections::HashMap;
use std::sync::RwLock;

/// (code, name in that language)
pub const LANGUAGES: &[(&str, &str)] = &[("en", "English"), ("de", "Deutsch"), ("fr", "Français"), ("es", "Español")];

fn bundled(code: &str) -> Option<&'static str> {
    match code {
        "de" => Some(include_str!("../../../assets/i18n/de.json")),
        "fr" => Some(include_str!("../../../assets/i18n/fr.json")),
        "es" => Some(include_str!("../../../assets/i18n/es.json")),
        _ => None,
    }
}

struct Table {
    code: String,
    map: HashMap<String, String>,
}

static CURRENT: RwLock<Option<Table>> = RwLock::new(None);

/// Switch the interface language ("en" or an unknown code = English).
pub fn set_language(code: &str) {
    let code = code.trim().to_ascii_lowercase();
    let map: HashMap<String, String> = bundled(&code).and_then(|s| serde_json::from_str(s).ok()).unwrap_or_default();
    *CURRENT.write().unwrap() = if map.is_empty() { None } else { Some(Table { code, map }) };
}

/// The current language code.
pub fn language() -> String {
    CURRENT.read().unwrap().as_ref().map(|t| t.code.clone()).unwrap_or_else(|| "en".into())
}

/// `s` in the current language. Two-line ribbon labels ("Plot\nto PDF") are
/// looked up as one line; a translation may give its own line break.
pub fn tr(s: &str) -> String {
    let g = CURRENT.read().unwrap();
    let Some(t) = g.as_ref() else { return s.to_string() };
    if let Some(v) = t.map.get(s) {
        return v.clone();
    }
    if s.contains('\n') {
        let one = s.replace('\n', " ");
        if let Some(v) = t.map.get(&one) {
            // Keep a two-line label two lines when the translation is long.
            return if v.contains('\n') || v.chars().count() <= 12 {
                v.clone()
            } else {
                // Break at the space nearest the middle.
                let mid = v.len() / 2;
                match v.match_indices(' ').map(|(i, _)| i).min_by_key(|i| i.abs_diff(mid)) {
                    Some(i) => format!("{}\n{}", &v[..i], &v[i + 1..]),
                    None => v.clone(),
                }
            };
        }
    }
    s.to_string()
}

/// Translate a template with one `{}`.
pub fn tr1(template: &str, arg: &str) -> String {
    tr(template).replacen("{}", arg, 1)
}

/// The system's language, if it is one of ours.
pub fn system_language() -> Option<&'static str> {
    let mut candidates: Vec<String> = ["LC_ALL", "LC_MESSAGES", "LANG"].iter().filter_map(|k| std::env::var(k).ok()).collect();
    #[cfg(target_os = "macos")]
    if let Ok(out) = std::process::Command::new("defaults").args(["read", "-g", "AppleLanguages"]).output() {
        candidates.extend(String::from_utf8_lossy(&out.stdout).split(['"', ',', '(', ')', '\n']).map(|s| s.trim().to_string()));
    }
    candidates.iter().find_map(|c| {
        let code = c.split(['_', '-', '.']).next().unwrap_or("").to_ascii_lowercase();
        LANGUAGES.iter().find(|(l, _)| *l == code && *l != "en").map(|(l, _)| *l)
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_menu_and_ribbon_label_is_translated() {
        // The labels the interface shows (menus, ribbon buttons, panels).
        let mut labels: Vec<String> = vec![];
        for (name, entries) in crate::menus::MENUS {
            labels.push(name.to_string());
            labels.extend(entries.iter().filter(|e| e.0 != "-").map(|e| e.0.to_string()));
        }
        labels.extend(crate::ribbon::TABS.iter().map(|t| t.to_string()));
        let src = include_str!("ribbon.rs");
        // The string after each `Icon::Name, "` (button and item labels) and `panel("`.
        let quoted = |rest: &str| -> String {
            let mut out = String::new();
            let mut esc = false;
            for c in rest.chars() {
                match (esc, c) {
                    (true, 'n') => out.push(' '),
                    (true, c) => out.push(c),
                    (false, '\\') => {
                        esc = true;
                        continue;
                    }
                    (false, '"') => break,
                    (false, c) => out.push(c),
                }
                esc = false;
            }
            out
        };
        for (i, _) in src.match_indices("Icon::") {
            let rest = &src[i + 6..];
            let name_end = rest.find(|c: char| !c.is_alphanumeric() && c != '_').unwrap_or(0);
            if let Some(r) = rest[name_end..].strip_prefix(',').map(str::trim_start).and_then(|r| r.strip_prefix('"')) {
                labels.push(quoted(r));
            }
        }
        for (i, _) in src.match_indices("panel(\"") {
            labels.push(quoted(&src[i + 7..]));
        }
        // Labels written in code rather than lists.
        labels.push("More".to_string());
        labels.retain(|l| !l.is_empty());
        for (code, _) in LANGUAGES.iter().filter(|l| l.0 != "en") {
            let map: HashMap<String, String> = serde_json::from_str(bundled(code).unwrap()).unwrap();
            let missing: Vec<&String> = labels.iter().filter(|l| !map.contains_key(*l)).collect();
            assert!(missing.is_empty(), "{code} lacks translations for {missing:?}");
            assert!(map.values().all(|v| !v.trim().is_empty()));
        }
        set_language("de");
        assert_eq!(tr("Layers"), "Layer");
        assert_eq!(tr("Plot\nto PDF"), "Als PDF\nplotten");
        assert_eq!(tr("Not translated"), "Not translated");
        set_language("en");
        assert_eq!(tr("Layers"), "Layers");
    }
}
