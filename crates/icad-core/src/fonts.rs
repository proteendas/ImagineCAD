// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Proteen Das

//! TrueType / OpenType text for text styles: system font discovery, glyph
//! outlines and metrics. Styled text is drawn from the font's outlines as
//! filled shapes, so it looks the same on screen, in PDF and in SVG, with the
//! style's width factor and oblique angle applied.

use crate::geom::*;
use crate::prim::{Prim, Text};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, OnceLock};

/// A named text style.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct TextStyle {
    pub name: String,
    /// Font file (path, or a file name found in the system font folders;
    /// `#n` selects face n of a collection); empty = the standard drafting font.
    #[serde(default)]
    pub file: String,
    /// Display name of the font.
    #[serde(default)]
    pub font: String,
    #[serde(default = "one")]
    pub width_factor: f64,
    /// Oblique (slant) angle in degrees.
    #[serde(default)]
    pub oblique: f64,
    /// Fixed text height (0 = chosen per text).
    #[serde(default)]
    pub height: f64,
}

fn one() -> f64 {
    1.0
}

impl TextStyle {
    pub fn standard() -> Self {
        Self { name: "Standard".into(), file: String::new(), font: String::new(), width_factor: 1.0, oblique: 0.0, height: 0.0 }
    }
}

/// An installed font face.
#[derive(Clone, Debug, PartialEq)]
pub struct FontInfo {
    /// "Family" or "Family Style".
    pub name: String,
    pub path: PathBuf,
    pub index: u32,
}

fn font_dirs() -> Vec<PathBuf> {
    let mut v = vec![];
    let home = std::env::var_os("HOME").or_else(|| std::env::var_os("USERPROFILE")).map(PathBuf::from);
    if cfg!(target_os = "macos") {
        v.extend(["/System/Library/Fonts", "/System/Library/Fonts/Supplemental", "/Library/Fonts"].map(PathBuf::from));
        if let Some(h) = &home {
            v.push(h.join("Library/Fonts"));
        }
    } else if cfg!(windows) {
        let win = std::env::var_os("WINDIR").map(PathBuf::from).unwrap_or_else(|| PathBuf::from("C:\\Windows"));
        v.push(win.join("Fonts"));
        if let Some(l) = std::env::var_os("LOCALAPPDATA") {
            v.push(PathBuf::from(l).join("Microsoft\\Windows\\Fonts"));
        }
    } else {
        v.extend(["/usr/share/fonts", "/usr/local/share/fonts"].map(PathBuf::from));
        if let Some(h) = &home {
            v.push(h.join(".local/share/fonts"));
            v.push(h.join(".fonts"));
        }
    }
    v
}

fn font_files() -> Vec<PathBuf> {
    let mut out = vec![];
    let mut stack = font_dirs();
    while let Some(d) = stack.pop() {
        let Ok(rd) = std::fs::read_dir(&d) else { continue };
        for e in rd.flatten() {
            let p = e.path();
            if p.is_dir() {
                stack.push(p);
            } else if p
                .extension()
                .and_then(|x| x.to_str())
                .is_some_and(|x| matches!(x.to_ascii_lowercase().as_str(), "ttf" | "otf" | "ttc"))
            {
                out.push(p);
            }
        }
    }
    out
}

fn face_name(face: &ttf_parser::Face) -> Option<String> {
    let get = |id: u16| {
        face.names().into_iter().filter(|n| n.name_id == id && n.is_unicode()).find_map(|n| n.to_string()).filter(|s| !s.trim().is_empty())
    };
    let family = get(16).or_else(|| get(1))?;
    let sub = get(17).or_else(|| get(2)).unwrap_or_default();
    Some(if sub.is_empty() || sub.eq_ignore_ascii_case("regular") { family } else { format!("{family} {sub}") })
}

static FONTS: OnceLock<Vec<FontInfo>> = OnceLock::new();

/// The installed fonts if they have been listed already (see `system_fonts`).
pub fn system_fonts_if_ready() -> Option<&'static [FontInfo]> {
    FONTS.get().map(|v| v.as_slice())
}

/// Installed fonts, sorted by name. Slow the first time (reads every font
/// file); call it off the UI thread.
pub fn system_fonts() -> &'static [FontInfo] {
    FONTS.get_or_init(|| {
        let mut v: Vec<FontInfo> = vec![];
        for path in font_files() {
            let Ok(data) = std::fs::read(&path) else { continue };
            let n = ttf_parser::fonts_in_collection(&data).unwrap_or(1);
            for index in 0..n {
                if let Ok(face) = ttf_parser::Face::parse(&data, index) {
                    if let Some(name) = face_name(&face) {
                        if !v.iter().any(|f| f.name == name) {
                            v.push(FontInfo { name, path: path.clone(), index });
                        }
                    }
                }
            }
        }
        v.sort_by_key(|f| f.name.to_lowercase());
        v
    })
}

/// A sans font present on most systems, used for styled text without a font.
fn fallback_file() -> Option<PathBuf> {
    let candidates: &[&str] = if cfg!(target_os = "macos") {
        &["/System/Library/Fonts/Supplemental/Arial.ttf", "/System/Library/Fonts/Helvetica.ttc", "/System/Library/Fonts/Geneva.ttf"]
    } else if cfg!(windows) {
        &["C:\\Windows\\Fonts\\arial.ttf", "C:\\Windows\\Fonts\\segoeui.ttf"]
    } else {
        &[
            "/usr/share/fonts/truetype/dejavu/DejaVuSans.ttf",
            "/usr/share/fonts/TTF/DejaVuSans.ttf",
            "/usr/share/fonts/dejavu/DejaVuSans.ttf",
            "/usr/share/fonts/truetype/liberation/LiberationSans-Regular.ttf",
        ]
    };
    candidates.iter().map(PathBuf::from).find(|p| p.exists())
}

impl FontInfo {
    /// The `file` value selecting this face.
    pub fn file_ref(&self) -> String {
        if self.index == 0 {
            self.path.display().to_string()
        } else {
            format!("{}#{}", self.path.display(), self.index)
        }
    }
}

/// Split `path#index`.
fn split_index(file: &str) -> (&str, u32) {
    match file.rsplit_once('#') {
        Some((p, i)) if i.parse::<u32>().is_ok() => (p, i.parse().unwrap()),
        _ => (file, 0),
    }
}

/// Find a font file: a path, or a file name in the system font folders.
pub fn locate(file: &str) -> Option<PathBuf> {
    let file = split_index(file).0;
    if file.trim().is_empty() {
        return fallback_file();
    }
    let p = Path::new(file);
    if p.is_file() {
        return Some(p.to_path_buf());
    }
    static BY_NAME: OnceLock<Mutex<HashMap<String, Option<PathBuf>>>> = OnceLock::new();
    let cache = BY_NAME.get_or_init(Default::default);
    let key = p.file_name().and_then(|n| n.to_str()).unwrap_or(file).to_ascii_lowercase();
    if let Some(hit) = cache.lock().unwrap().get(&key) {
        return hit.clone();
    }
    let found = font_files().into_iter().find(|f| f.file_name().and_then(|n| n.to_str()).is_some_and(|n| n.eq_ignore_ascii_case(&key)));
    cache.lock().unwrap().insert(key, found.clone());
    found
}

/// Memoised lookups (None = not available).
type Cache<K, V> = Mutex<HashMap<K, Option<Arc<V>>>>;

struct Face {
    data: Vec<u8>,
    index: u32,
}

/// Text outline, scaled so the font's cap height is 1.
pub struct Shaped {
    /// Filled areas (convex quads, even–odd rule already applied).
    pub fills: Vec<[V2; 4]>,
    pub advance: f64,
}

struct Pen {
    loops: Vec<Vec<V2>>,
    cur: Vec<V2>,
    at: V2,
    k: f64,
    dx: f64,
}

impl Pen {
    fn pt(&self, x: f32, y: f32) -> V2 {
        V2::new(x as f64 * self.k + self.dx, y as f64 * self.k)
    }
    fn close_loop(&mut self) {
        if self.cur.len() >= 3 {
            self.loops.push(std::mem::take(&mut self.cur));
        } else {
            self.cur.clear();
        }
    }
}

impl ttf_parser::OutlineBuilder for Pen {
    fn move_to(&mut self, x: f32, y: f32) {
        self.close_loop();
        self.at = self.pt(x, y);
        self.cur.push(self.at);
    }
    fn line_to(&mut self, x: f32, y: f32) {
        self.at = self.pt(x, y);
        self.cur.push(self.at);
    }
    fn quad_to(&mut self, x1: f32, y1: f32, x: f32, y: f32) {
        let (p0, c, p1) = (self.at, self.pt(x1, y1), self.pt(x, y));
        for i in 1..=6 {
            let t = i as f64 / 6.0;
            self.cur.push(p0 * ((1.0 - t) * (1.0 - t)) + c * (2.0 * t * (1.0 - t)) + p1 * (t * t));
        }
        self.at = p1;
    }
    fn curve_to(&mut self, x1: f32, y1: f32, x2: f32, y2: f32, x: f32, y: f32) {
        let (p0, c1, c2, p1) = (self.at, self.pt(x1, y1), self.pt(x2, y2), self.pt(x, y));
        for i in 1..=8 {
            let t = i as f64 / 8.0;
            let u = 1.0 - t;
            self.cur.push(p0 * (u * u * u) + c1 * (3.0 * u * u * t) + c2 * (3.0 * u * t * t) + p1 * (t * t * t));
        }
        self.at = p1;
    }
    fn close(&mut self) {
        self.close_loop();
    }
}

fn load(path: &Path, index: u32) -> Option<Arc<Face>> {
    static FACES: OnceLock<Cache<(PathBuf, u32), Face>> = OnceLock::new();
    let cache = FACES.get_or_init(Default::default);
    let key = (path.to_path_buf(), index);
    if let Some(f) = cache.lock().unwrap().get(&key) {
        return f.clone();
    }
    let face = std::fs::read(path).ok().filter(|d| ttf_parser::Face::parse(d, index).is_ok()).map(|data| Arc::new(Face { data, index }));
    cache.lock().unwrap().insert(key, face.clone());
    face
}

/// Outline of `text` in `file` (cached).
pub fn shape(file: &str, text: &str) -> Option<Arc<Shaped>> {
    static SHAPED: OnceLock<Cache<(String, String), Shaped>> = OnceLock::new();
    let cache = SHAPED.get_or_init(Default::default);
    let key = (file.to_string(), text.to_string());
    if let Some(s) = cache.lock().unwrap().get(&key) {
        return s.clone();
    }
    let shaped = locate(file).and_then(|p| load(&p, split_index(file).1)).and_then(|f| {
        let face = ttf_parser::Face::parse(&f.data, f.index).ok()?;
        let upm = face.units_per_em() as f64;
        let cap = face.capital_height().filter(|c| *c > 0).map(|c| c as f64).unwrap_or(upm * 0.7);
        let k = 1.0 / cap;
        let mut pen = Pen { loops: vec![], cur: vec![], at: V2::ZERO, k, dx: 0.0 };
        let fallback = face.glyph_index('?');
        for ch in text.chars() {
            let gid = face.glyph_index(ch).or(fallback)?;
            face.outline_glyph(gid, &mut pen);
            pen.close_loop();
            pen.dx += face.glyph_hor_advance(gid).unwrap_or(0) as f64 * k;
        }
        Some(Arc::new(Shaped { fills: crate::hatch::solid_trapezoids(&pen.loops), advance: pen.dx }))
    });
    cache.lock().unwrap().insert(key, shaped.clone());
    shaped
}

/// Advance width of `text` in `file` with cap height 1.
pub fn advance(file: &str, text: &str) -> Option<f64> {
    shape(file, text).map(|s| s.advance)
}

/// Styled text as filled outlines; None for plain text (drawn as text).
pub fn outline(t: &Text) -> Option<Vec<Prim>> {
    if !t.styled() {
        return None;
    }
    outline_always(t)
}

/// Any text as filled outlines (plain text in the standard sans font); None
/// when no font file is available.
pub fn outline_always(t: &Text) -> Option<Vec<Prim>> {
    if t.content.is_empty() {
        return None;
    }
    let s = shape(&t.font, &t.content)?;
    let o = t.local_origin();
    let slant = t.oblique.tan();
    let map = |p: V2| {
        let local = V2::new((p.x * t.width_factor + p.y * slant) * t.height, p.y * t.height) + o;
        t.pos + local.rotate(t.rotation)
    };
    Some(s.fills.iter().map(|q| Prim::Fill(q.iter().map(|p| map(*p)).collect())).collect())
}

/// Replace styled text in `prims` with its outlines.
pub fn expand(prims: Vec<Prim>) -> Vec<Prim> {
    let mut out = Vec::with_capacity(prims.len());
    for p in prims {
        match &p {
            Prim::Text(t) => match outline(t) {
                Some(f) => out.extend(f),
                None => out.push(p),
            },
            _ => out.push(p),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn styled_text_becomes_outlines_when_a_font_is_available() {
        let mut t = Text::new(V2::new(10.0, 5.0), 3.5, "AB");
        assert!(outline(&t).is_none(), "plain text stays text");
        t.width_factor = 0.8;
        if locate("").is_none() {
            eprintln!("no system font; skipping");
            return;
        }
        let fills = outline(&t).expect("outlines");
        assert!(fills.len() > 4);
        let b = fills.iter().fold(BBox::empty(), |b, p| b.union(&p.bbox()));
        // Cap height 3.5; the text starts at the insertion point.
        assert!((b.height() - 3.5).abs() < 0.4, "{b:?}");
        assert!((b.min.x - 10.0).abs() < 0.6 && (b.min.y - 5.0).abs() < 0.3, "{b:?}");
        let wide = {
            let mut w = t.clone();
            w.width_factor = 1.6;
            w.width()
        };
        assert!((wide / t.width() - 2.0).abs() < 1e-9);
    }
}
