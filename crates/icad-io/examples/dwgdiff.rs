// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Proteen Das

//! Compare the built-in DWG reader with a DXF of the same drawing:
//! `cargo run -p icad-io --example dwgdiff -- FILE.dwg FILE.dxf`

fn key(doc: &icad_core::Document, e: &icad_core::Entity) -> String {
    let b = doc.entity_bbox(e);
    format!("{:<22} {:>12.3} {:>12.3} {:>12.3} {:>12.3} {}", e.shape.type_name(), b.min.x, b.min.y, b.max.x, b.max.y, e.layer)
}

fn main() {
    let a: Vec<String> = std::env::args().collect();
    let (dwg, _) = icad_io::dwgnative::read(&std::fs::read(&a[1]).unwrap()).unwrap();
    let dxf = icad_io::dxf::read_dxf(&String::from_utf8_lossy(&std::fs::read(&a[2]).unwrap())).unwrap();
    let mut x: Vec<String> = dwg.entities.iter().map(|e| key(&dwg, e)).collect();
    let mut y: Vec<String> = dxf.entities.iter().map(|e| key(&dxf, e)).collect();
    x.sort();
    y.sort();
    for k in &x {
        if !y.contains(k) {
            println!("only DWG: {k}");
        }
    }
    for k in &y {
        if !x.contains(k) {
            println!("only DXF: {k}");
        }
    }
    let texts = |d: &icad_core::Document| -> Vec<String> {
        let mut v: Vec<String> = d
            .entities
            .iter()
            .filter_map(|e| match &e.shape {
                icad_core::entity::Shape::Text(t) => Some(t.content.clone()),
                icad_core::entity::Shape::MText(t) => Some(t.content.clone()),
                icad_core::entity::Shape::Dim(dm) => Some(format!("dim {:?}", dm.text)),
                _ => None,
            })
            .collect();
        v.sort();
        v
    };
    for (n, d) in [("DWG", &dwg), ("DXF", &dxf)] {
        for e in &d.entities {
            if let icad_core::entity::Shape::Dim(dm) = &e.shape {
                if matches!(dm.kind, icad_core::entity::DimKind::Angular) {
                    println!("{n} angular {:?}", dm);
                }
            }
        }
    }
    for (n, d) in [("DWG", &dwg), ("DXF", &dxf)] {
        for e in &d.entities {
            if let icad_core::entity::Shape::Tolerance(t) = &e.shape {
                println!("{n} tolerance {:?}", t);
            }
        }
    }
    println!("scale DWG {} DXF {}  dimstyle DWG {:?}\n DXF {:?}", dwg.sheet.scale, dxf.sheet.scale, dwg.dimstyle, dxf.dimstyle);
    println!("texts DWG {:?}\ntexts DXF {:?}", texts(&dwg), texts(&dxf));
    println!("layers DWG {:?}", dwg.layers.iter().map(|l| (&l.name, l.color, l.linetype, l.visible)).collect::<Vec<_>>());
    println!("layers DXF {:?}", dxf.layers.iter().map(|l| (&l.name, l.color, l.linetype, l.visible)).collect::<Vec<_>>());
    println!("blocks DWG {:?}", dwg.blocks.iter().map(|b| (&b.name, b.entities.len())).collect::<Vec<_>>());
    println!("blocks DXF {:?}", dxf.blocks.iter().map(|b| (&b.name, b.entities.len())).collect::<Vec<_>>());
}
