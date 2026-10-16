// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Proteen Das

//! SVG export of the drawing sheet in paper millimetres.

use crate::plot::*;
use icad_core::document::Document;
use icad_core::geom::*;
use icad_core::prim::*;
use std::fmt::Write as _;

fn esc(s: &str) -> String {
    s.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;").replace('"', "&quot;")
}

fn f(v: f64) -> String {
    let s = format!("{:.3}", v);
    s.trim_end_matches('0').trim_end_matches('.').to_string()
}

/// Embed an image file as a PNG/JPEG data URI.
fn data_uri(path: &str) -> Option<String> {
    use base64::Engine as _;
    let bytes = std::fs::read(path).ok()?;
    let lower = path.to_ascii_lowercase();
    let (mime, data) = if lower.ends_with(".png") {
        ("image/png", bytes)
    } else if lower.ends_with(".jpg") || lower.ends_with(".jpeg") {
        ("image/jpeg", bytes)
    } else {
        let img = image::load_from_memory(&bytes).ok()?;
        let mut png = Vec::new();
        img.write_to(&mut std::io::Cursor::new(&mut png), image::ImageFormat::Png).ok()?;
        ("image/png", png)
    };
    Some(format!("data:{mime};base64,{}", base64::engine::general_purpose::STANDARD.encode(data)))
}

pub fn write_svg(doc: &Document, opts: &PlotOptions) -> String {
    let (w, h) = doc.sheet.paper_mm();
    let y = |v: f64| h - v;
    let mut s = String::new();
    let _ = writeln!(
        s,
        r#"<?xml version="1.0" encoding="UTF-8"?>
<svg xmlns="http://www.w3.org/2000/svg" width="{w}mm" height="{h}mm" viewBox="0 0 {w} {h}">
<title>{}</title>
<rect x="0" y="0" width="{w}" height="{h}" fill="white"/>
<g fill="none" stroke-linecap="round" stroke-linejoin="round" font-family="Helvetica, Arial, sans-serif">"#,
        esc(&doc.sheet.title.title)
    );
    let items = plot_items(doc, opts);
    // Same-coloured fills in a row become one path (no seams between pieces).
    let mut run = String::new();
    for (i, it) in items.iter().enumerate() {
        if let Prim::Fill(p) = &it.prim {
            for (k, q) in p.iter().enumerate() {
                let _ = write!(run, "{}{} {} ", if k == 0 { "M" } else { "L" }, f(q.x), f(y(q.y)));
            }
            run.push_str("Z ");
            let next_same = items.get(i + 1).is_some_and(|nx| matches!(nx.prim, Prim::Fill(_)) && nx.rgb == it.rgb);
            if !next_same {
                let _ =
                    writeln!(s, r#"<path d="{}" fill="rgb({},{},{})" stroke="none"/>"#, run.trim_end(), it.rgb[0], it.rgb[1], it.rgb[2]);
                run.clear();
            }
            continue;
        }
        let col = format!("rgb({},{},{})", it.rgb[0], it.rgb[1], it.rgb[2]);
        let mut st = format!(r#"stroke="{col}" stroke-width="{}""#, f(it.width));
        if !it.dash.is_empty() {
            let d: Vec<String> = it.dash.iter().map(|x| f(x.max(0.01))).collect();
            let _ = write!(st, r#" stroke-dasharray="{}""#, d.join(" "));
        }
        let pts = |v: &[V2]| v.iter().map(|p| format!("{},{}", f(p.x), f(y(p.y)))).collect::<Vec<_>>().join(" ");
        match &it.prim {
            Prim::Line(a, b) => {
                let _ = writeln!(s, r#"<line x1="{}" y1="{}" x2="{}" y2="{}" {st}/>"#, f(a.x), f(y(a.y)), f(b.x), f(y(b.y)));
            }
            Prim::Poly(p, closed) => {
                let tag = if *closed { "polygon" } else { "polyline" };
                let _ = writeln!(s, r#"<{tag} points="{}" {st}/>"#, pts(p));
            }
            Prim::Fill(_) => {}
            Prim::Circle(c, r) => {
                let _ = writeln!(s, r#"<circle cx="{}" cy="{}" r="{}" {st}/>"#, f(c.x), f(y(c.y)), f(*r));
            }
            Prim::Arc { c, r, start, end } => {
                let p0 = *c + V2::polar(*r, *start);
                let p1 = *c + V2::polar(*r, *end);
                let large = if arc_sweep(*start, *end) > std::f64::consts::PI { 1 } else { 0 };
                // CCW in model space is clockwise once y is flipped → sweep-flag 0.
                let _ = writeln!(
                    s,
                    r#"<path d="M {} {} A {} {} 0 {large} 0 {} {}" {st}/>"#,
                    f(p0.x),
                    f(y(p0.y)),
                    f(*r),
                    f(*r),
                    f(p1.x),
                    f(y(p1.y))
                );
            }
            Prim::Image(img) => {
                if let Some(uri) = data_uri(&img.path) {
                    let _ = writeln!(
                        s,
                        r#"<image x="{}" y="{}" width="{}" height="{}" opacity="{}" preserveAspectRatio="none" href="{uri}"/>"#,
                        f(img.pos.x),
                        f(y(img.pos.y + img.h)),
                        f(img.w),
                        f(img.h),
                        f(1.0 - img.fade as f64 / 100.0)
                    );
                }
            }
            Prim::Text(t) => {
                let o = t.pos + t.local_origin().rotate(t.rotation);
                let _ = writeln!(
                    s,
                    r#"<text x="{x}" y="{yy}" font-size="{}" fill="{col}" stroke="none" transform="rotate({} {x} {yy})">{}</text>"#,
                    f(t.height / 0.72),
                    f(-deg(t.rotation)),
                    esc(&t.content),
                    x = f(o.x),
                    yy = f(y(o.y)),
                );
            }
        }
    }
    s.push_str("</g>\n</svg>\n");
    s
}
