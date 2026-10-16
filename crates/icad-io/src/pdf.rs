// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Proteen Das

//! Vector PDF plotting at true scale (one page = the drawing sheet).
//! Hand-written PDF 1.4 so there are no heavyweight dependencies; PDF/A-2b
//! for archiving (text as outlines, an embedded sRGB profile and XMP
//! metadata).

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
    write_pages_with(&[sheet_page(doc, opts)], &doc.sheet.title.title, opts.pdfa)
}

/// Text replaced by its outlines (PDF/A needs every font embedded; outlines
/// need none). Text with no font file to draw it from stays text.
fn outline_text(items: &[PlotItem]) -> Vec<PlotItem> {
    let mut out = Vec::with_capacity(items.len());
    for it in items {
        let fills = match &it.prim {
            Prim::Text(t) => icad_core::fonts::outline_always(t),
            _ => None,
        };
        match fills {
            Some(f) => out.extend(f.into_iter().map(|prim| PlotItem { prim, rgb: it.rgb, dash: vec![], width: it.width })),
            None => out.push(PlotItem { prim: it.prim.clone(), rgb: it.rgb, dash: it.dash.clone(), width: it.width }),
        }
    }
    out
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
    write_pages_with(pages, title, false)
}

/// The characters of `s` as the WinAnsi font shows them (for metadata that
/// must match the document information exactly).
fn winansi_text(s: &str) -> String {
    s.chars()
        .map(|c| match c {
            '⌀' | '∅' => 'Ø',
            c if (c as u32) < 0x100 && !c.is_control() => c,
            _ => '?',
        })
        .collect()
}

fn xml_escape(s: &str) -> String {
    s.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;").replace('"', "&quot;")
}

/// XMP metadata identifying the file as PDF/A-2b (matching the Info entries).
fn xmp(title: &str, date: &str) -> String {
    format!(
        r#"<?xpacket begin="" id="W5M0MpCehiHzreSzNTczkc9d"?>
<x:xmpmeta xmlns:x="adobe:ns:meta/">
<rdf:RDF xmlns:rdf="http://www.w3.org/1999/02/22-rdf-syntax-ns#">
<rdf:Description rdf:about="" xmlns:dc="http://purl.org/dc/elements/1.1/" xmlns:xmp="http://ns.adobe.com/xap/1.0/" xmlns:pdf="http://ns.adobe.com/pdf/1.3/" xmlns:pdfaid="http://www.aiim.org/pdfa/ns/id/">
<dc:format>application/pdf</dc:format>{}
<xmp:CreatorTool>ImagineCAD</xmp:CreatorTool>
<xmp:CreateDate>{date}</xmp:CreateDate>
<xmp:ModifyDate>{date}</xmp:ModifyDate>
<pdf:Producer>ImagineCAD</pdf:Producer>
<pdfaid:part>2</pdfaid:part>
<pdfaid:conformance>B</pdfaid:conformance>
</rdf:Description>
</rdf:RDF>
</x:xmpmeta>
<?xpacket end="w"?>"#,
        if title.is_empty() {
            String::new()
        } else {
            format!(
                "\n<dc:title><rdf:Alt><rdf:li xml:lang=\"x-default\">{}</rdf:li></rdf:Alt></dc:title>",
                xml_escape(&winansi_text(title))
            )
        }
    )
}

/// A minimal sRGB (IEC 61966-2.1) ICC v2 display profile, D50-adapted, for
/// the PDF/A output intent.
pub fn srgb_icc() -> Vec<u8> {
    fn s15(v: f64) -> [u8; 4] {
        ((v * 65536.0).round() as i32).to_be_bytes()
    }
    fn xyz(x: f64, y: f64, z: f64) -> Vec<u8> {
        let mut t = b"XYZ \0\0\0\0".to_vec();
        for v in [x, y, z] {
            t.extend(s15(v));
        }
        t
    }
    let desc = {
        let name = b"sRGB IEC61966-2.1\0";
        let mut t = b"desc\0\0\0\0".to_vec();
        t.extend((name.len() as u32).to_be_bytes());
        t.extend(name);
        t.extend([0u8; 4 + 4 + 2 + 1 + 67]);
        t
    };
    let cprt = b"text\0\0\0\0No copyright, use freely\0".to_vec();
    let trc = {
        let mut t = b"curv\0\0\0\0".to_vec();
        let n = 1024u32;
        t.extend(n.to_be_bytes());
        for i in 0..n {
            let v = i as f64 / (n - 1) as f64;
            let lin = if v <= 0.04045 { v / 12.92 } else { ((v + 0.055) / 1.055).powf(2.4) };
            t.extend(((lin * 65535.0).round() as u16).to_be_bytes());
        }
        t
    };
    let tags: Vec<(&[u8; 4], Vec<u8>)> = vec![
        (b"desc", desc),
        (b"cprt", cprt),
        (b"wtpt", xyz(0.9642, 1.0, 0.8249)),
        (b"rXYZ", xyz(0.4361, 0.2225, 0.0139)),
        (b"gXYZ", xyz(0.3851, 0.7169, 0.0971)),
        (b"bXYZ", xyz(0.1431, 0.0606, 0.7141)),
        (b"rTRC", trc.clone()),
        (b"gTRC", trc.clone()),
        (b"bTRC", trc),
    ];
    let table_len = 4 + 12 * tags.len();
    let mut data = vec![];
    let mut entries = vec![];
    for (sig, body) in &tags {
        let off = 128 + table_len + data.len();
        entries.push((*sig, off as u32, body.len() as u32));
        data.extend(body);
        while data.len() % 4 != 0 {
            data.push(0);
        }
    }
    let size = (128 + table_len + data.len()) as u32;
    let mut h = vec![];
    h.extend(size.to_be_bytes());
    h.extend([0u8; 4]); // CMM
    h.extend([2u8, 0x10, 0, 0]); // version 2.1
    h.extend(b"mntrRGB XYZ ");
    h.extend([0x07u8, 0xEA, 0, 1, 0, 1, 0, 0, 0, 0, 0, 0]); // 2026-01-01
    h.extend(b"acsp");
    h.extend([0u8; 4 + 4 + 4 + 4 + 8 + 4]); // platform, flags, manufacturer, model, attributes, intent
    for v in [0.9642, 1.0, 0.8249] {
        h.extend(s15(v));
    }
    h.extend([0u8; 4 + 16 + 28]);
    debug_assert_eq!(h.len(), 128);
    h.extend((tags.len() as u32).to_be_bytes());
    for (sig, off, len) in entries {
        h.extend(sig);
        h.extend(off.to_be_bytes());
        h.extend(len.to_be_bytes());
    }
    h.extend(data);
    h
}

/// A multi-page PDF; `pdfa` writes PDF/A-2b.
pub fn write_pages_with(pages: &[Page], title: &str, pdfa: bool) -> Vec<u8> {
    let outlined: Vec<Page>;
    let pages = if pdfa {
        outlined = pages.iter().map(|p| Page { w: p.w, h: p.h, items: outline_text(&p.items) }).collect();
        &outlined[..]
    } else {
        pages
    };
    let mut images: Vec<(String, u8)> = vec![];
    let contents: Vec<String> = pages.iter().map(|p| page_content(p, &mut images)).collect();
    let [y, mo, d, hh, mi, ss] = icad_core::sheet::utc_now();
    let pdf_date = format!("D:{y:04}{mo:02}{d:02}{hh:02}{mi:02}{ss:02}Z");
    let xmp_date = format!("{y:04}-{mo:02}-{d:02}T{hh:02}:{mi:02}:{ss:02}Z");
    let raw_title = title;
    let title = if raw_title.is_empty() { String::new() } else { format!("/Title {} ", pdf_str(raw_title)) };
    // Objects: 1 catalog, 2 pages, 3 font, 4 info, then images, then page + content pairs.
    let mut objs: Vec<Vec<u8>> =
        vec![vec![], vec![], b"<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica /Encoding /WinAnsiEncoding >>".to_vec()];
    objs.push(
        format!("<< {title}/Producer (ImagineCAD) /Creator (ImagineCAD) /CreationDate ({pdf_date}) /ModDate ({pdf_date}) >>").into_bytes(),
    );
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
        // PDF/A: only fonts that are used may be listed (and outlined pages use none).
        let font = if !pdfa || content.contains(" Tf ") { "/Font << /F1 3 0 R >> " } else { "" };
        objs.push(
            format!(
                "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 {} {}] /Resources << {font}/XObject << {xnames}>> >> /Contents {} 0 R >>",
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
    if pdfa && !contents.iter().any(|c| c.contains(" Tf ")) {
        // No page uses the (unembedded) standard font: leave it out.
        objs[2] = b"<< >>".to_vec();
    }
    if pdfa {
        let icc = miniz_oxide::deflate::compress_to_vec_zlib(&srgb_icc(), 6);
        let mut obj = format!("<< /N 3 /Filter /FlateDecode /Length {} >>\nstream\n", icc.len()).into_bytes();
        obj.extend_from_slice(&icc);
        obj.extend_from_slice(b"\nendstream");
        objs.push(obj);
        let icc_no = objs.len();
        let meta = xmp(raw_title, &xmp_date);
        objs.push(format!("<< /Type /Metadata /Subtype /XML /Length {} >>\nstream\n{meta}\nendstream", meta.len()).into_bytes());
        let meta_no = objs.len();
        objs[0] = format!(
            "<< /Type /Catalog /Pages 2 0 R /Metadata {meta_no} 0 R /OutputIntents [<< /Type /OutputIntent /S /GTS_PDFA1 \
             /OutputConditionIdentifier (sRGB IEC61966-2.1) /Info (sRGB IEC61966-2.1) /DestOutputProfile {icc_no} 0 R >>] >>"
        )
        .into_bytes();
    }
    objs[1] = format!("<< /Type /Pages /Kids [{}] /Count {} >>", kids.join(" "), kids.len()).into_bytes();
    let version: &[u8] = if pdfa { b"%PDF-1.7\n" } else { b"%PDF-1.4\n" };
    let mut out: Vec<u8> = version.to_vec();
    out.extend_from_slice(b"%\xE2\xE3\xCF\xD3\n");
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
    // A file identifier (required by PDF/A): a hash of the body.
    let id = {
        let mut h: u64 = 0xcbf2_9ce4_8422_2325;
        for b in &out {
            h = (h ^ *b as u64).wrapping_mul(0x100_0000_01b3);
        }
        format!("{h:016x}{:016x}", h.rotate_left(29) ^ out.len() as u64)
    };
    let _ = write!(x, "trailer\n<< /Size {} /Root 1 0 R /Info 4 0 R /ID [<{id}> <{id}>] >>\nstartxref\n{}\n%%EOF\n", objs.len() + 1, xref);
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
    fn pdfa_and_plot_stamp() {
        let mut d = Document::new();
        d.add(Shape::Line { a: V2::new(0., 0.), b: V2::new(100., 0.) });
        let mut fields = PlotStamp { on: true, custom: "PRELIMINARY".into(), ..Default::default() };
        fields.scale = true;
        let stamp = Stamp { fields, drawing: "bracket.icad".into(), user: "pd".into(), when: "2026-10-16 10:00 UTC".into() };
        assert_eq!(
            stamp.text("Layout1", &d.sheet),
            "bracket.icad  |  Layout1  |  2026-10-16 10:00 UTC  |  pd  |  Scale 1:1  |  PRELIMINARY"
        );
        let opts = PlotOptions { stamp: Some(stamp), pdfa: true, ..Default::default() };
        let page = sheet_page(&d, &opts);
        // In the bottom margin, below the frame.
        let Some(Prim::Text(t)) =
            page.items.iter().map(|i| &i.prim).find(|p| matches!(p, Prim::Text(t) if t.content.contains("PRELIMINARY")))
        else {
            panic!()
        };
        assert!(t.pos.y < d.sheet.frame_mm().min.y && t.pos.x == d.sheet.frame_mm().min.x);
        let pdf = write_pdf(&d, &opts);
        let s = String::from_utf8_lossy(&pdf);
        assert!(s.starts_with("%PDF-1.7"));
        assert!(s.contains("/OutputIntents") && s.contains("/GTS_PDFA1") && s.contains("<pdfaid:part>2</pdfaid:part>"));
        assert!(s.contains("/ID [<") && s.contains("/CreationDate (D:"));
        let icc = srgb_icc();
        assert_eq!(u32::from_be_bytes(icc[0..4].try_into().unwrap()) as usize, icc.len());
        assert_eq!(&icc[36..40], b"acsp");
        // When a font file is available the text is outlined and no font is listed.
        if icad_core::fonts::locate("").is_some() {
            assert!(!s.contains("/Font << /F1"), "text should be outlines");
        }
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
