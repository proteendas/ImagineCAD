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
    d.add(Shape::Arc { c: V2::new(60., 50.), r: 30., start: 0.0, end: std::f64::consts::FRAC_PI_2 });
    d.add(Shape::Dim(Dimension::ordinate(V2::ZERO, V2::new(160., 100.), V2::new(160., 120.))));
    d.add(Shape::Dim(Dimension::arc_length(V2::new(60., 50.), V2::new(90., 50.), V2::new(60., 80.), V2::new(85., 75.))));
    use icad_core::symbols::*;
    let fcf = Fcf {
        pos: V2::new(10., 130.),
        rows: vec![FcfRow {
            symbol: GdtSymbol::Flatness,
            diameter: false,
            tolerance: "0.05".into(),
            material: Material::None,
            datums: vec![],
        }],
        leader: vec![],
    };
    d.add(Shape::Tolerance(fcf.clone()));
    let weld = WeldSymbol {
        tip: V2::new(160., 50.),
        elbow: V2::new(180., 70.),
        arrow_side: WeldSide { kind: WeldType::Fillet, size: "a5".into(), length: String::new() },
        other_side: WeldSide::default(),
        all_around: true,
        field: false,
        tail: String::new(),
    };
    d.add(Shape::Weld(weld.clone()));
    let dir = std::env::temp_dir().join(format!("icad-dwg-test-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("part.dwg");
    icad_io::save(&d, &path, &Default::default()).expect("save DWG");
    assert!(std::fs::metadata(&path).unwrap().len() > 1000);
    let r = icad_io::open(&path).expect("open DWG");
    let kinds: Vec<&str> = r.entities.iter().map(|e| e.shape.type_name()).collect();
    assert!(kinds.contains(&"Circle") && kinds.contains(&"Ellipse") && kinds.contains(&"Polyline"), "{kinds:?}");
    let dims: Vec<&Dimension> = r.entities.iter().filter_map(|e| if let Shape::Dim(d) = &e.shape { Some(d) } else { None }).collect();
    let lin = dims.iter().find(|d| d.kind == DimKind::Horizontal).expect("dimension survives DWG");
    assert!((lin.measurement() - 160.0).abs() < 1e-6);
    let ord = dims.iter().find(|d| d.is_ordinate()).expect("ordinate dimension survives DWG");
    assert!((ord.measurement() - 160.0).abs() < 1e-6);
    let arc = dims.iter().find(|d| d.kind == DimKind::ArcLength).expect("arc length dimension survives DWG");
    assert!((arc.measurement() - 30.0 * std::f64::consts::FRAC_PI_2).abs() < 1e-6);
    let c = r.entities.iter().find_map(|e| if let Shape::Circle { r, .. } = e.shape { Some(r) } else { None }).unwrap();
    assert!((c - 20.0).abs() < 1e-9);
    assert!(r.entities.iter().any(|e| e.shape == Shape::Tolerance(fcf.clone())), "tolerance survives DWG");
    assert!(r.entities.iter().any(|e| e.shape == Shape::Weld(weld.clone())), "weld symbol survives DWG");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn dxf_and_dwg_keep_raster_images() {
    let dir = std::env::temp_dir().join(format!("icad-img-test-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let png = dir.join("photo.png");
    image::RgbImage::from_fn(40, 20, |x, _| image::Rgb([(x * 6) as u8, 80, 160])).save(&png).unwrap();
    let mut d = Document::new();
    d.add(Shape::Line { a: V2::new(0., 0.), b: V2::new(100., 0.) });
    d.add(Shape::Image(icad_core::ImageRef { path: png.display().to_string(), pos: V2::new(10., 5.), w: 80.0, h: 40.0, fade: 30 }));
    let check = |r: &Document, what: &str| {
        let img = r
            .entities
            .iter()
            .find_map(|e| if let Shape::Image(i) = &e.shape { Some(i.clone()) } else { None })
            .unwrap_or_else(|| panic!("image lost in {what}"));
        assert_eq!(img.path, png.display().to_string(), "{what}");
        assert!((img.w - 80.0).abs() < 1e-6 && (img.h - 40.0).abs() < 1e-6 && img.pos.dist(V2::new(10., 5.)) < 1e-6, "{what}: {img:?}");
        assert_eq!(img.fade, 30, "{what}");
    };
    let dxf = dir.join("img.dxf");
    icad_io::save(&d, &dxf, &Default::default()).unwrap();
    check(&icad_io::open(&dxf).unwrap(), "DXF");
    if icad_io::dwg::find_converter().is_some() {
        let dwg = dir.join("img.dwg");
        icad_io::save(&d, &dwg, &Default::default()).unwrap();
        check(&icad_io::open(&dwg).unwrap(), "DWG");
    }
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn dwg_keeps_external_references() {
    if icad_io::dwg::find_converter().is_none() {
        return;
    }
    let mut part = Document::new();
    part.add(Shape::Circle { c: V2::ZERO, r: 5.0 });
    let mut d = Document::new();
    d.attach_xref("BASE", "/parts/base.dwg", &part);
    d.add(Shape::Insert(Insert::new("BASE", V2::new(10., 10.))));
    let dir = std::env::temp_dir().join(format!("icad-xref-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("host.dwg");
    icad_io::save(&d, &path, &Default::default()).unwrap();
    let r = icad_io::open(&path).unwrap();
    assert_eq!(r.block("BASE").and_then(|b| b.xref.clone()).as_deref(), Some("/parts/base.dwg"));
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn dwg_keeps_block_attributes() {
    if icad_io::dwg::find_converter().is_none() {
        return;
    }
    let mut d = Document::new();
    d.define_block(BlockDef {
        name: "TAG".into(),
        base: V2::ZERO,
        entities: vec![Entity {
            id: 0,
            layer: "0".into(),
            color: Default::default(),
            linetype: Default::default(),
            lineweight: None,
            shape: Shape::AttDef(AttDef {
                tag: "NO".into(),
                prompt: "Number".into(),
                default: "0".into(),
                text: icad_core::Text::new(V2::ZERO, 3.5, ""),
                invisible: false,
            }),
        }],
        description: String::new(),
        xref: None,
        params: vec![],
    });
    let mut ins = Insert::new("TAG", V2::new(10., 10.));
    ins.set_value("NO", "42");
    d.add(Shape::Insert(ins));
    let dir = std::env::temp_dir().join(format!("icad-dwg-att-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("att.dwg");
    icad_io::save(&d, &path, &Default::default()).unwrap();
    let r = icad_io::open(&path).unwrap();
    let ins = r.entities.iter().find_map(|e| if let Shape::Insert(i) = &e.shape { Some(i.clone()) } else { None }).expect("insert");
    assert_eq!(ins.value_of("NO"), Some("42"));
    assert_eq!(r.block("TAG").unwrap().attdefs().count(), 1);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn dwg_keeps_constraints() {
    if icad_io::dwg::find_converter().is_none() {
        eprintln!("skipping: no DWG converter available");
        return;
    }
    use icad_core::constraints::{Constraint, PtRef, SegRef};
    let mut d = Document::new();
    let a = d.add(Shape::Line { a: V2::new(0., 0.), b: V2::new(100., 0.) });
    let b = d.add(Shape::Line { a: V2::new(100., 0.), b: V2::new(100., 40.) });
    d.add_constraint(Constraint::Horizontal(SegRef::line(a))).unwrap();
    d.add_constraint(Constraint::Coincident(PtRef { id: a, k: 1 }, PtRef { id: b, k: 0 })).unwrap();
    let dir = std::env::temp_dir().join(format!("icad-cons-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("constrained.dwg");
    icad_io::save(&d, &path, &Default::default()).unwrap();
    let r = icad_io::open(&path).unwrap();
    assert_eq!(r.constraints.len(), 2, "{:?}", r.constraints);
    let Constraint::Coincident(p, q) = &r.constraints[1] else { panic!() };
    assert!(r.get(p.id).is_some() && r.get(q.id).is_some() && p.id != q.id);
    let _ = std::fs::remove_dir_all(&dir);
}
