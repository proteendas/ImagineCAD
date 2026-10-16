// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Proteen Das

//! The built-in DWG reader on sample drawings (see tests/data/README.md).

use icad_core::entity::Shape;
use icad_core::geom::V2;
use icad_io::dwgnative;

fn read(name: &str) -> (icad_core::Document, dwgnative::Report) {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/data").join(name);
    dwgnative::read(&std::fs::read(path).unwrap()).unwrap()
}

fn count(doc: &icad_core::Document, kind: &str) -> usize {
    doc.entities.iter().filter(|e| e.shape.type_name() == kind).count()
}

#[test]
fn reads_a_dwg_2000_drawing() {
    let (doc, rep) = read("sample_2000.dwg");
    assert_eq!(rep.version, "DWG 2000");
    assert!(rep.complete(), "{rep:?}");
    assert_eq!(doc.entities.len(), 6);
    assert_eq!((count(&doc, "Line"), count(&doc, "Circle"), count(&doc, "Polyline"), count(&doc, "Text")), (3, 1, 1, 1));
    let c = doc.entities.iter().find_map(|e| if let Shape::Circle { c, r } = e.shape { Some((c, r)) } else { None }).unwrap();
    assert!((c.0 - V2::new(199.6812627452187, 24.695759123913035)).len() < 1e-9 && c.1 == 20.0);
    assert!(doc
        .entities
        .iter()
        .any(|e| e.shape
            == Shape::Line { a: V2::new(99.6812627452187, 94.69575912391304), b: V2::new(199.6812627452187, 94.69575912391304) }));
}

#[test]
fn reads_a_dwg_2018_drawing() {
    let (doc, rep) = read("example_2018.dwg");
    assert_eq!(rep.version, "DWG 2018");
    assert_eq!(rep.damaged, 0);
    // The drafting entities are read; 3D objects, lights, multilines and a
    // few class-defined annotation objects are listed as left out.
    let known = [
        "3D face",
        "3D solid",
        "mesh vertex",
        "polyface/mesh",
        "multiline",
        "light",
        "acad_table",
        "arc_dimension",
        "multileader",
        "wipeout",
    ];
    assert!(rep.skipped.keys().all(|k| known.contains(&k.as_str())), "{rep:?}");
    for (kind, n) in [
        ("Line", 8),
        ("Arc", 2),
        ("Ellipse", 1),
        ("Spline", 2),
        ("Hatch", 2),
        ("Block Reference", 10),
        ("Rotated Dimension", 4),
        ("Ordinate Dimension", 3),
        ("Angular Dimension", 1),
        ("Aligned Dimension", 1),
        ("Tolerance", 1),
        ("MText", 1),
        ("Text", 2),
    ] {
        assert_eq!(count(&doc, kind), n, "{kind}");
    }
    let mt = doc.entities.iter().find_map(|e| if let Shape::MText(m) = &e.shape { Some(m.content.clone()) } else { None }).unwrap();
    assert_eq!(mt, "Teksto granda nur por testi.\nAlia linio.\nAlia pli.");
    assert!(doc.blocks.iter().any(|b| b.name == "bloko" && b.entities.len() == 5));
    // The drawing's own dimension style and scale.
    assert_eq!((doc.sheet.scale, doc.dimstyle.text_height, doc.dimstyle.arrow_size), (1.0, 2.5, 2.5));
    assert!(doc.layers.iter().any(|l| l.name == "Tavolo 3" && l.color == 4));
}

#[test]
fn reads_a_dwg_2007_drawing() {
    // DWG 2007 has its own container (Reed–Solomon coded, own compression).
    let (doc, rep) = read("example_2007.dwg");
    assert_eq!(rep.version, "DWG 2007");
    assert_eq!(rep.damaged, 0);
    let (d18, _) = read("example_2018.dwg");
    let kinds = |d: &icad_core::Document| {
        let mut v: Vec<&str> = d.entities.iter().map(|e| e.shape.type_name()).collect();
        v.sort();
        v
    };
    assert_eq!(kinds(&doc), kinds(&d18));
    assert_eq!(count(&doc, "Arc Length Dimension"), 1);
}
