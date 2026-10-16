// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Proteen Das

//! Spell check (SPELL) of the drawing's text: single and multiline text,
//! leaders, dimension overrides, attribute values and table cells.
//!
//! The dictionary is a public-domain English word list (Webster's Second
//! International) plus drafting terms, extended by the user's own words.

use crate::app::App;
use crate::theme;
use eframe::egui::{self, RichText};
use icad_core::entity::Shape;
use icad_core::Document;
use std::collections::HashSet;
use std::path::PathBuf;
use std::sync::{Mutex, OnceLock};

static BUNDLED: &[u8] = include_bytes!("../../../assets/dict/en-words.txt.z");

fn user_file() -> Option<PathBuf> {
    Some(dirs::config_dir()?.join("ImagineCAD").join("user-dictionary.txt"))
}

fn dictionary() -> &'static Mutex<HashSet<String>> {
    static DICT: OnceLock<Mutex<HashSet<String>>> = OnceLock::new();
    DICT.get_or_init(|| {
        let raw = miniz_oxide::inflate::decompress_to_vec_zlib(BUNDLED).unwrap_or_default();
        let mut set: HashSet<String> = String::from_utf8_lossy(&raw).lines().map(str::to_string).collect();
        if let Some(f) = user_file() {
            set.extend(std::fs::read_to_string(f).unwrap_or_default().lines().map(|w| w.trim().to_lowercase()).filter(|w| !w.is_empty()));
        }
        Mutex::new(set)
    })
}

/// Add a word to the user's dictionary (kept between sessions).
pub fn learn(word: &str) {
    let w = word.to_lowercase();
    dictionary().lock().unwrap().insert(w.clone());
    if let Some(f) = user_file() {
        if let Some(d) = f.parent() {
            let _ = std::fs::create_dir_all(d);
        }
        use std::io::Write as _;
        if let Ok(mut file) = std::fs::OpenOptions::new().create(true).append(true).open(f) {
            let _ = writeln!(file, "{w}");
        }
    }
}

/// Regular English inflections back to a base form found in `has`.
fn known_with(word: &str, has: &dyn Fn(&str) -> bool) -> bool {
    let w = word.to_lowercase();
    let w = w.trim_end_matches("'s").trim_end_matches('\'');
    if has(w) {
        return true;
    }
    let try_base = |stem: &str| !stem.is_empty() && (has(stem) || has(&format!("{stem}e")));
    for (suffix, repl) in [("ies", "y"), ("ied", "y"), ("ier", "y"), ("iest", "y"), ("ily", "y")] {
        if let Some(s) = w.strip_suffix(suffix) {
            if has(&format!("{s}{repl}")) {
                return true;
            }
        }
    }
    for suffix in ["s", "es", "ed", "d", "ing", "er", "ers", "est", "ly", "ness", "ment", "ments", "able", "ings"] {
        if let Some(s) = w.strip_suffix(suffix) {
            if try_base(s) {
                return true;
            }
            // Doubled final consonant: stopped → stop, milling → mill is direct.
            let b = s.as_bytes();
            if b.len() >= 3 && b[b.len() - 1] == b[b.len() - 2] && has(&s[..s.len() - 1]) {
                return true;
            }
        }
    }
    false
}

pub fn is_known(word: &str) -> bool {
    let d = dictionary().lock().unwrap();
    known_with(word, &|w: &str| d.contains(w))
}

/// Words worth checking: letters only, not acronyms, part numbers or fields.
pub fn words(text: &str) -> Vec<(usize, &str)> {
    let mut out = vec![];
    let mut skip_field = false;
    let bytes = text.as_bytes();
    let mut i = 0;
    while i < text.len() {
        if text[i..].starts_with("{{") {
            skip_field = true;
        }
        if skip_field {
            if text[i..].starts_with("}}") {
                skip_field = false;
                i += 2;
            } else {
                i += text[i..].chars().next().map(|c| c.len_utf8()).unwrap_or(1);
            }
            continue;
        }
        let c = text[i..].chars().next().unwrap();
        if c.is_alphabetic() {
            let start = i;
            let mut end = i;
            for (k, ch) in text[i..].char_indices() {
                if ch.is_alphanumeric() || (ch == '\'' && text[i + k + 1..].chars().next().is_some_and(char::is_alphabetic)) {
                    end = i + k + ch.len_utf8();
                } else {
                    break;
                }
            }
            let w = &text[start..end];
            let has_digit = w.chars().any(|c| c.is_ascii_digit());
            let acronym = w.chars().all(|c| !c.is_lowercase()) && w.chars().count() <= 6;
            // Words glued to digits or symbols (M6, R5, 2x) are part references.
            let glued = start > 0 && bytes[start - 1].is_ascii_digit();
            // Mixed case inside a word (mK, kN) marks a code or unit.
            let mixed = w.chars().skip(1).any(char::is_uppercase) && w.chars().any(char::is_lowercase);
            if !has_digit && !acronym && !glued && !mixed && w.chars().count() >= 2 && w.is_ascii() {
                out.push((start, w));
            }
            i = end.max(i + 1);
        } else {
            i += c.len_utf8();
        }
    }
    out
}

/// Close dictionary words (one or two edits), best first.
pub fn suggest(word: &str) -> Vec<String> {
    let w = word.to_lowercase();
    let letters = "abcdefghijklmnopqrstuvwxyz";
    let edits1 = |w: &str| {
        let mut v: Vec<String> = vec![];
        let chars: Vec<char> = w.chars().collect();
        for i in 0..=chars.len() {
            let (a, b) = (&chars[..i], &chars[i..]);
            let a: String = a.iter().collect();
            if !b.is_empty() {
                v.push(format!("{a}{}", b[1..].iter().collect::<String>()));
            }
            if b.len() > 1 {
                v.push(format!("{a}{}{}{}", b[1], b[0], b[2..].iter().collect::<String>()));
            }
            for c in letters.chars() {
                if !b.is_empty() {
                    v.push(format!("{a}{c}{}", b[1..].iter().collect::<String>()));
                }
                v.push(format!("{a}{c}{}", b.iter().collect::<String>()));
            }
        }
        v
    };
    let d = dictionary().lock().unwrap();
    let mut out: Vec<String> = vec![];
    let one = edits1(&w);
    for c in &one {
        if d.contains(c.as_str()) && !out.contains(c) {
            out.push(c.clone());
        }
    }
    if out.len() < 3 && w.len() <= 12 {
        for c in one.iter().take(4000) {
            for c2 in edits1(c) {
                if d.contains(c2.as_str()) && !out.contains(&c2) {
                    out.push(c2);
                }
                if out.len() >= 8 {
                    break;
                }
            }
            if out.len() >= 8 {
                break;
            }
        }
    }
    // Keep the capitalisation of the original.
    let upper = word.chars().all(|c| !c.is_lowercase());
    let title = word.chars().next().is_some_and(char::is_uppercase);
    out.truncate(8);
    out.into_iter()
        .map(|s| {
            if upper {
                s.to_uppercase()
            } else if title {
                let mut c = s.chars();
                c.next().map(|f| f.to_uppercase().collect::<String>() + c.as_str()).unwrap_or_default()
            } else {
                s
            }
        })
        .collect()
}

/// Where a checked text lives.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Slot {
    Main,
    Attrib(usize),
    Cell(usize, usize),
}

fn texts_of(s: &Shape) -> Vec<(Slot, String)> {
    match s {
        Shape::Text(t) => vec![(Slot::Main, t.content.clone())],
        Shape::MText(m) => vec![(Slot::Main, m.content.clone())],
        Shape::Leader(l) => vec![(Slot::Main, l.text.clone())],
        Shape::Dim(d) => d.text.clone().map(|t| vec![(Slot::Main, t)]).unwrap_or_default(),
        Shape::Insert(i) => i.attribs.iter().enumerate().map(|(k, a)| (Slot::Attrib(k), a.value.clone())).collect(),
        Shape::Table(t) => t
            .cells
            .iter()
            .enumerate()
            .flat_map(|(r, row)| row.iter().enumerate().map(move |(c, x)| (Slot::Cell(r, c), x.text.clone())))
            .collect(),
        _ => vec![],
    }
}

fn text_mut(s: &mut Shape, slot: Slot) -> Option<&mut String> {
    match (s, slot) {
        (Shape::Text(t), Slot::Main) => Some(&mut t.content),
        (Shape::MText(m), Slot::Main) => Some(&mut m.content),
        (Shape::Leader(l), Slot::Main) => Some(&mut l.text),
        (Shape::Dim(d), Slot::Main) => d.text.as_mut(),
        (Shape::Insert(i), Slot::Attrib(k)) => i.attribs.get_mut(k).map(|a| &mut a.value),
        (Shape::Table(t), Slot::Cell(r, c)) => t.cells.get_mut(r).and_then(|row| row.get_mut(c)).map(|x| &mut x.text),
        _ => None,
    }
}

#[derive(Clone, Debug)]
pub struct Issue {
    pub id: u64,
    pub slot: Slot,
    pub word: String,
    pub context: String,
}

/// Unknown words in the given entities (all text entities when `ids` is empty).
pub fn check(doc: &Document, ids: &[u64]) -> Vec<Issue> {
    let mut out = vec![];
    for e in doc.entities.iter().filter(|e| ids.is_empty() || ids.contains(&e.id)) {
        for (slot, text) in texts_of(&e.shape) {
            for (_, w) in words(&text) {
                if !is_known(w) {
                    out.push(Issue { id: e.id, slot, word: w.to_string(), context: text.replace('\n', " ") });
                }
            }
        }
    }
    out
}

/// Replace whole-word `from` with `to` in `text`; returns whether anything changed.
fn replace_word(text: &mut String, from: &str, to: &str) -> bool {
    let positions: Vec<usize> = words(text).into_iter().filter(|(_, w)| *w == from).map(|(i, _)| i).collect();
    for i in positions.iter().rev() {
        text.replace_range(*i..*i + from.len(), to);
    }
    !positions.is_empty()
}

pub struct SpellDialog {
    pub issues: Vec<Issue>,
    pub at: usize,
    pub replacement: String,
    pub suggestions: Vec<String>,
    pub ignored: HashSet<String>,
    pub changed: usize,
}

impl SpellDialog {
    fn current(&self) -> Option<&Issue> {
        self.issues.get(self.at)
    }
    /// Move to the next issue that is still relevant.
    fn advance(&mut self, doc: &Document) {
        while let Some(i) = self.issues.get(self.at) {
            let still = doc
                .get(i.id)
                .is_some_and(|e| texts_of(&e.shape).iter().any(|(s, t)| *s == i.slot && words(t).iter().any(|(_, w)| *w == i.word)));
            if still && !self.ignored.contains(&i.word.to_lowercase()) && !is_known(&i.word) {
                break;
            }
            self.at += 1;
        }
        self.suggestions = self.current().map(|i| suggest(&i.word)).unwrap_or_default();
        self.replacement = self.suggestions.first().cloned().unwrap_or_else(|| self.current().map(|i| i.word.clone()).unwrap_or_default());
    }
}

impl App {
    pub fn start_spell(&mut self) {
        let t = &self.tabs[self.active];
        let ids = t.selection.clone();
        let issues = check(&t.doc, &ids);
        if issues.is_empty() {
            self.log.push(
                if ids.is_empty() { "Spelling check complete: no unknown words." } else { "No unknown words in the selection." }.into(),
            );
            return;
        }
        let mut d = SpellDialog { issues, at: 0, replacement: String::new(), suggestions: vec![], ignored: HashSet::new(), changed: 0 };
        d.advance(&t.doc);
        self.dlg.spell = Some(d);
    }

    pub fn spell_dialog(&mut self, ctx: &egui::Context) {
        let Some(mut d) = self.dlg.spell.take() else { return };
        let mut open = true;
        let mut action: Option<&str> = None;
        egui::Window::new("Check Spelling").open(&mut open).collapsible(false).default_width(420.0).show(ctx, |ui| {
            let Some(issue) = d.current().cloned() else {
                ui.label(format!("Spelling check complete. {} change(s) made.", d.changed));
                if ui.button("Close").clicked() {
                    action = Some("close");
                }
                return;
            };
            ui.label(RichText::new(format!("Not in dictionary:  {}", issue.word)).strong().color(theme::WARN));
            ui.label(
                RichText::new(format!("Context: {}", issue.context.chars().take(120).collect::<String>()))
                    .size(11.5)
                    .color(theme::TEXT_DIM),
            );
            ui.add_space(4.0);
            ui.horizontal(|ui| {
                ui.label("Change to");
                ui.text_edit_singleline(&mut d.replacement);
            });
            ui.label(RichText::new("Suggestions").size(11.5));
            egui::ScrollArea::vertical().max_height(120.0).show(ui, |ui| {
                if d.suggestions.is_empty() {
                    ui.label(RichText::new("(none)").color(theme::TEXT_DIM));
                }
                for s in d.suggestions.clone() {
                    if ui.selectable_label(d.replacement == s, &s).clicked() {
                        d.replacement = s;
                    }
                }
            });
            ui.add_space(6.0);
            ui.horizontal_wrapped(|ui| {
                for (label, a) in [
                    ("Change", "change"),
                    ("Change All", "change_all"),
                    ("Ignore", "ignore"),
                    ("Ignore All", "ignore_all"),
                    ("Add to Dictionary", "add"),
                ] {
                    if ui.button(label).clicked() {
                        action = Some(a);
                    }
                }
            });
            ui.label(RichText::new(format!("{} of {}", d.at + 1, d.issues.len())).size(11.0).color(theme::TEXT_DIM));
        });
        let tab = self.active;
        match action {
            Some("close") => return,
            Some(a) => {
                let issue = d.current().cloned();
                if let Some(issue) = issue {
                    match a {
                        "change" | "change_all" => {
                            let t = &mut self.tabs[tab];
                            t.checkpoint();
                            let targets: Vec<Issue> = if a == "change" {
                                vec![issue.clone()]
                            } else {
                                d.issues[d.at..].iter().filter(|i| i.word == issue.word).cloned().collect()
                            };
                            for i in targets {
                                if let Some(text) = t.doc.get_mut(i.id).and_then(|e| text_mut(&mut e.shape, i.slot)) {
                                    if replace_word(text, &i.word, &d.replacement) {
                                        d.changed += 1;
                                    }
                                }
                            }
                            if a == "change_all" {
                                d.ignored.insert(issue.word.to_lowercase());
                            }
                            d.at += 1;
                        }
                        "ignore" => d.at += 1,
                        "ignore_all" => {
                            d.ignored.insert(issue.word.to_lowercase());
                            d.at += 1;
                        }
                        _ => {
                            learn(&issue.word);
                            d.at += 1;
                        }
                    }
                }
                d.advance(&self.tabs[tab].doc);
            }
            None => {}
        }
        if open {
            self.dlg.spell = Some(d);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn checks_words_but_not_codes() {
        let w: Vec<&str> =
            words("Break all edges, M6 thru, 2x Ø8 {{TITLE}} ISO 2768-mK deburred chamfre").into_iter().map(|x| x.1).collect();
        assert_eq!(w, vec!["Break", "all", "edges", "thru", "deburred", "chamfre"]);
        for ok in ["Break", "edges", "holes", "milling", "deburred", "assemblies", "stopped", "Chamfered", "keyway"] {
            assert!(is_known(ok), "{ok}");
        }
        assert!(!is_known("chamfre"));
        assert!(suggest("chamfre").contains(&"chamfer".to_string()));
        assert_eq!(suggest("Wahser").first().map(String::as_str), Some("Washer"));
        let mut s = "a chamfre and chamfres".to_string();
        assert!(replace_word(&mut s, "chamfre", "chamfer"));
        assert_eq!(s, "a chamfer and chamfres");
    }
}
