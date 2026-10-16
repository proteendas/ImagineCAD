// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Proteen Das

//! Vector PDF plotting at true scale (one page = the drawing sheet).
//! Hand-written PDF 1.4 so there are no heavyweight dependencies.

use crate::plot::*;
use icad_core::document::Document;
use icad_core::geom::*;
use icad_core::prim::*;
use std::fmt::Write as _;

const PT: f64 = 72.0 / 25.4;

fn n(v: f64) -> String {
    let v = if v.is_finite() { v } else { 0.0 };
    let s = format!("{:.3}", v);
    let s = s.trim_end_matches('0').trim_end_matches('.');
    if s == "-0" {
        "0".into()
    } else {
        s.to_string()
    }
}

/// Encode a string for a WinAnsi-encoded Type1 font as a PDF literal.
fn pdf_str(s: &str) -> String {
    let mut o = String::from("(");
    for c in s.chars() {
        let b: u32 = match c {
            '⌀' | '∅' => 0xD8,
            c if (c as u32) < 0x100 => c as u32,
            _ => '?' as u32,
        };
        match b {
            0x28 | 0x29 | 0x5C => {
                o.push('\\');
                o.push(char::from_u32(b).unwrap());
            }
            0x20..=0x7E => o.push(char::from_u32(b).unwrap()),
            _ => {
                let _ = write!(o, "\\{:03o}", b);
            }
        }
    }
    o.push(')');
    o
}

fn path_arc(s: &mut String, c: V2, r: f64, start: f64, end: f64, move_to: bool) {
    let sweep = arc_sweep(start, end);
    let segs = (sweep / std::f64::consts::FRAC_PI_2).ceil().max(1.0) as usize;
    let step = sweep / segs as f64;
    let p0 = c + V2::polar(r, start);
    if move_to {
        let _ = writeln!(s, "{} {} m", n(p0.x * PT), n(p0.y * PT));
    }
    for i in 0..segs {
        let a1 = start + step * i as f64;
        let a2 = a1 + step;
        let k = 4.0 / 3.0 * (step / 4.0).tan() * r;
        let q0 = c + V2::polar(r, a1);
        let q3 = c + V2::polar(r, a2);
        let q1 = q0 + V2::new(-a1.sin(), a1.cos()) * k;
        let q2 = q3 - V2::new(-a2.sin(), a2.cos()) * k;
        let _ = writeln!(s, "{} {} {} {} {} {} c", n(q1.x * PT), n(q1.y * PT), n(q2.x * PT), n(q2.y * PT), n(q3.x * PT), n(q3.y * PT));
    }
}

fn poly_path(s: &mut String, pts: &[V2], closed: bool) {
    for (i, p) in pts.iter().enumerate() {
        let _ = writeln!(s, "{} {} {}", n(p.x * PT), n(p.y * PT), if i == 0 { "m" } else { "l" });
    }
    if closed {
        s.push_str("h\n");
    }
}

/// One page: paper size in millimetres and what is drawn on it.
pub struct Page {
    pub w: f64,
    pub h: f64,
    pub items: Vec<PlotItem>,
}

/// The document's main sheet as a page.
pub fn sheet_page(doc: &Document, opts: &PlotOptions) -> Page {
    let (w, h) = doc.sheet.paper_mm();
    Page { w, h, items: plot_items(doc, opts) }
}

/// Paper-space layout `index` as a page.
pub fn layout_page(doc: &Document, index: usize, opts: &PlotOptions) -> Option<Page> {
    let l = doc.layouts.get(index)?;
    let (w, h) = l.sheet.paper_mm();
    Some(Page { w, h, items: crate::plot::layout_plot_items(doc, index, opts) })
}

/// Render the document's sheet to a single-page PDF.
pub fn write_pdf(doc: &Document, opts: &PlotOptions) -> Vec<u8> {
    write_pages(&[sheet_page(doc, opts)], &doc.sheet.title.title)
}

/// Content stream of one page; images are added to `images` (shared by all pages).
fn page_content(page: &Page, images: &mut Vec<(String, u8)>) -> String {
    let (w, h, items) = (page.w, page.h, &page.items);
    let mut c = String::with_capacity(256 * 1024);
    c.push_str("1 J 1 j\n");
    // Clip to the page.
    let _ = writeln!(c, "0 0 {} {} re W n", n(w * PT), n(h * PT));
    // Runs of same-coloured fills (text outlines, solid hatches) are filled
    // as one path, so anti-aliasing leaves no seams between the pieces.
    let same_fill = |a: &crate::plot::PlotItem, b: &crate::plot::PlotItem| {
        matches!((&a.prim, &b.prim), (Prim::Fill(_), Prim::Fill(_))) && a.rgb == b.rgb
    };
    for (i, it) in items.iter().enumerate() {
        let continues = i > 0 && same_fill(&items[i - 1], it);
        let run_ends = !items.get(i + 1).is_some_and(|nx| same_fill(it, nx));
        if continues {
            if let Prim::Fill(pts) = &it.prim {
                poly_path(&mut c, pts, true);
                if run_ends {
                    c.push_str("f\n");
                }
            }
            continue;
        }
        let [r, g, b] = it.rgb.map(|v| v as f64 / 255.0);
        let _ = writeln!(c, "{} {} {} RG {} {} {} rg {} w", n(r), n(g), n(b), n(r), n(g), n(b), n(it.width * PT));
        if it.dash.is_empty() {
            c.push_str("[] 0 d\n");
        } else {
            let d: Vec<String> = it.dash.iter().map(|x| n((x * PT).max(0.01))).collect();
            let _ = writeln!(c, "[{}] 0 d", d.join(" "));
        }
        match &it.prim {
            Prim::Line(a, b2) => {
                let _ = writeln!(c, "{} {} m {} {} l S", n(a.x * PT), n(a.y * PT), n(b2.x * PT), n(b2.y * PT));
            }
            Prim::Poly(pts, closed) => {
                poly_path(&mut c, pts, *closed);
                c.push_str("S\n");
            }
            Prim::Fill(pts) => {
                poly_path(&mut c, pts, true);
                if run_ends {
                    c.push_str("f\n");
                }
            }
            Prim::Circle(ctr, r) => {
                path_arc(&mut c, *ctr, *r, 0.0, std::f64::consts::TAU, true);
                c.push_str("h S\n");
            }
            Prim::Arc { c: ctr, r, start, end } => {
                path_arc(&mut c, *ctr, *r, *start, *end, true);
                c.push_str("S\n");
            }
            Prim::Image(img) => {
                let key = (img.path.clone(), img.fade);
                let idx = images.iter().position(|k| *k == key).unwrap_or_else(|| {
                    images.push(key);
                    images.len() - 1
                });
                let _ =
                    writeln!(c, "q {} 0 0 {} {} {} cm /Im{idx} Do Q", n(img.w * PT), n(img.h * PT), n(img.pos.x * PT), n(img.pos.y * PT));
            }
            Prim::Text(t) => {
                let size = t.height / 0.72 * PT;
                let o = t.pos + t.local_origin().rotate(t.rotation);
                let (s, co) = t.rotation.sin_cos();
                let _ = writeln!(
                    c,
                    "BT /F1 {} Tf {} {} {} {} {} {} Tm {} Tj ET",
                    n(size),
                    n(co),
                    n(s),
                    n(-s),
                    n(co),
                    n(o.x * PT),
                    n(o.y * PT),
                    pdf_str(&t.content)
                );
            }
        }
    }

    c
}

/// A multi-page PDF (batch plot); each page keeps its own paper size.
pub fn write_pages(pages: &[Page], title: &str) -> Vec<u8> {
    let mut images: Vec<(String, u8)> = vec![];
    let contents: Vec<String> = pages.iter().map(|p| page_content(p, &mut images)).collect();
    let title = pdf_str(title);
    // Objects: 1 catalog, 2 pages, 3 font, 4 info, then images, then page + content pairs.
    let mut objs: Vec<Vec<u8>> =
        vec![vec![], vec![], b"<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica /Encoding /WinAnsiEncoding >>".to_vec()];
    objs.push(format!("<< /Title {title} /Producer (ImagineCAD) /Creator (ImagineCAD) >>").into_bytes());
    // Image XObjects (RGB, Flate-compressed); unreadable images are skipped.
    let mut xnames = String::new();
    for (i, (path, fade)) in images.iter().enumerate() {
        let Ok(img) = image::open(path) else { continue };
        let rgb = img.to_rgb8();
        let (iw, ih) = rgb.dimensions();
        let k = *fade as f32 / 100.0;
        let raw: Vec<u8> = rgb.into_raw().into_iter().map(|v| (v as f32 + (255.0 - v as f32) * k) as u8).collect();
        let data = miniz_oxide::deflate::compress_to_vec_zlib(&raw, 6);
        let mut obj = format!(
            "<< /Type /XObject /Subtype /Image /Width {iw} /Height {ih} /ColorSpace /DeviceRGB /BitsPerComponent 8 /Filter /FlateDecode /Length {} >>\nstream\n",
            data.len()
        )
        .into_bytes();
        obj.extend_from_slice(&data);
        obj.extend_from_slice(b"\nendstream");
        objs.push(obj);
        let _ = write!(xnames, "/Im{i} {} 0 R ", objs.len());
    }
    let mut kids = vec![];
    for (page, content) in pages.iter().zip(&contents) {
        let page_no = objs.len() + 1;
        kids.push(format!("{page_no} 0 R"));
        objs.push(
            format!(
                "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 {} {}] /Resources << /Font << /F1 3 0 R >> /XObject << {xnames}>> >> /Contents {} 0 R >>",
                n(page.w * PT),
                n(page.h * PT),
                page_no + 1
            )
            .into_bytes(),
        );
        let data = miniz_oxide::deflate::compress_to_vec_zlib(content.as_bytes(), 6);
        let mut stream = format!("<< /Length {} /Filter /FlateDecode >>\nstream\n", data.len()).into_bytes();
        stream.extend_from_slice(&data);
        stream.extend_from_slice(b"\nendstream");
        objs.push(stream);
    }
    objs[0] = b"<< /Type /Catalog /Pages 2 0 R >>".to_vec();
    objs[1] = format!("<< /Type /Pages /Kids [{}] /Count {} >>", kids.join(" "), kids.len()).into_bytes();
    let mut out: Vec<u8> = b"%PDF-1.4\n%\xE2\xE3\xCF\xD3\n".to_vec();
    let mut offsets = Vec::new();
    for (i, o) in objs.iter().enumerate() {
        offsets.push(out.len());
        out.extend_from_slice(format!("{} 0 obj\n", i + 1).as_bytes());
        out.extend_from_slice(o);
        out.extend_from_slice(b"\nendobj\n");
    }
    let xref = out.len();
    let mut x = format!("xref\n0 {}\n0000000000 65535 f \n", objs.len() + 1);
    for o in offsets {
        let _ = writeln!(x, "{:010} 00000 n ", o);
    }
    let _ = write!(x, "trailer\n<< /Size {} /Root 1 0 R /Info 4 0 R >>\nstartxref\n{}\n%%EOF\n", objs.len() + 1, xref);
    out.extend_from_slice(x.as_bytes());
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use icad_core::entity::Shape;

    #[test]
    fn multi_page_batch() {
        let mut d = Document::new();
        d.add(Shape::Line { a: V2::new(0., 0.), b: V2::new(100., 0.) });
        d.layouts.push(icad_core::layout::Layout::new("Detail", d.sheet.clone(), &d.extents()));
        d.layouts[0].sheet.paper = icad_core::sheet::PaperSize::A4;
        let opts = PlotOptions::default();
        let pages = vec![sheet_page(&d, &opts), layout_page(&d, 0, &opts).unwrap()];
        let pdf = write_pages(&pages, "set");
        let s = String::from_utf8_lossy(&pdf);
        assert_eq!(s.matches("/Type /Page ").count(), 2);
        assert!(s.contains("/Count 2"));
        // A3 then A4 landscape.
        let boxes: Vec<&str> = s.match_indices("/MediaBox [").map(|(i, _)| &s[i..i + 40]).collect();
        assert!(s.contains("/MediaBox [0 0 1190.551 841.89]") && s.contains("/MediaBox [0 0 841.89 595.276]"), "{boxes:?}");
    }

    #[test]
    fn writes_valid_structure() {
        let mut d = Document::new();
        d.add(Shape::Circle { c: V2::new(100., 100.), r: 20. });
        d.add(Shape::Text(Text::new(V2::new(50., 50.), 5.0, "Ø10 (test)")));
        let pdf = write_pdf(&d, &PlotOptions::default());
        let s = String::from_utf8_lossy(&pdf);
        assert!(s.starts_with("%PDF-1.4") && s.trim_end().ends_with("%%EOF"));
        // Inflate the content stream and check the text encoding.
        let start = pdf.windows(7).position(|w| w == b"stream\n").unwrap() + 7;
        let end = pdf.windows(10).position(|w| w == b"\nendstream").unwrap();
        let content = miniz_oxide::inflate::decompress_to_vec_zlib(&pdf[start..end]).unwrap();
        let cs = String::from_utf8_lossy(&content);
        assert!(cs.contains("\\330") && cs.contains("\\(test\\)"));
        // xref offset points at "xref".
        let sx = s.rfind("startxref\n").unwrap();
        let off: usize = s[sx + 10..].lines().next().unwrap().parse().unwrap();
        assert_eq!(&pdf[off..off + 4], b"xref");
    }
}
