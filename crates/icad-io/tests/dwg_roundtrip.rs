// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Proteen Das

//! DWG save → open round trip through the DWG converter. Runs when a
//! converter is available (CI sets IMAGINECAD_LIBREDWG to the bundled build).

use icad_core::entity::*;
use icad_core::geom::V2;
use icad_core::Document;

#[test]
fn dwg_save_and_reopen() {
    if icad_io::dwg::find_converter().is_none() {
        eprintln!("skipping: no DWG converter available");
        return;
    }
    let mut d = Document::new();
    d.add(Shape::Polyline { pts: vec![V2::new(0., 0.), V2::new(160., 0.), V2::new(160., 100.), V2::new(0., 100.)], closed: true });
    d.add(Shape::Circle { c: V2::new(60., 50.), r: 20. });
    d.add(Shape::Ellipse { c: V2::new(120., 50.), major: V2::new(15., 0.), ratio: 0.5, start: 0.0, end: std::f64::consts::TAU });
    d.add(Shape::Dim(Dimension::new(DimKind::Horizontal, V2::new(0., 0.), V2::new(160., 0.), V2::new(80., -15.))));
    let dir = std::env::temp_dir().join(format!("icad-dwg-test-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("part.dwg");
    icad_io::save(&d, &path, &Default::default()).expect("save DWG");
    assert!(std::fs::metadata(&path).unwrap().len() > 1000);
    let r = icad_io::open(&path).expect("open DWG");
    let kinds: Vec<&str> = r.entities.iter().map(|e| e.shape.type_name()).collect();
    assert!(kinds.contains(&"Circle") && kinds.contains(&"Ellipse") && kinds.contains(&"Polyline"), "{kinds:?}");
    let dim = r.entities.iter().find_map(|e| if let Shape::Dim(d) = &e.shape { Some(d) } else { None }).expect("dimension survives DWG");
    assert!((dim.measurement() - 160.0).abs() < 1e-6);
    let c = r.entities.iter().find_map(|e| if let Shape::Circle { r, .. } = e.shape { Some(r) } else { None }).unwrap();
    assert!((c - 20.0).abs() < 1e-9);
    let _ = std::fs::remove_dir_all(&dir);
}
