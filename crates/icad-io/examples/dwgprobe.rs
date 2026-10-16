// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Proteen Das

//! Read DWG files with the built-in reader and summarise what came out:
//! `cargo run -p icad-io --example dwgprobe -- FILE.dwg…`

use std::collections::BTreeMap;

fn summary(doc: &icad_core::Document) -> BTreeMap<String, usize> {
    let mut m = BTreeMap::new();
    for e in &doc.entities {
        *m.entry(e.shape.type_name().to_string()).or_insert(0) += 1;
    }
    m
}

fn main() {
    for path in std::env::args().skip(1) {
        if path.ends_with(".dxf") {
            let doc = icad_io::dxf::read_dxf(&String::from_utf8_lossy(&std::fs::read(&path).expect("read"))).expect("dxf");
            println!("{path} (DXF): layers {} blocks {} entities {}", doc.layers.len(), doc.blocks.len(), doc.entities.len());
            println!("  kinds {:?}", summary(&doc));
            let e = doc.extents();
            println!("  extents {:.1},{:.1} – {:.1},{:.1}", e.min.x, e.min.y, e.max.x, e.max.y);
            for e in doc.entities.iter().take(0) {
                println!("  {:?}", e.shape);
            }
            continue;
        }
        let bytes = std::fs::read(&path).expect("read");
        let t = std::time::Instant::now();
        match icad_io::dwgnative::read(&bytes) {
            Ok((doc, rep)) => {
                println!("{path}: {} in {:?}", rep.version, t.elapsed());
                println!("  layers {} blocks {} entities {}", doc.layers.len(), doc.blocks.len(), doc.entities.len());
                println!("  kinds {:?}", summary(&doc));
                println!("  skipped {:?} damaged {}", rep.skipped, rep.damaged);
                for e in &doc.entities {
                    if let icad_core::entity::Shape::Dim(d) = &e.shape {
                        if d.kind == icad_core::entity::DimKind::ArcLength {
                            println!("  arc dim {:?} value {:.3}", d, d.measurement());
                        }
                    }
                }
                let e = doc.extents();
                println!("  extents {:.1},{:.1} – {:.1},{:.1}", e.min.x, e.min.y, e.max.x, e.max.y);
            }
            Err(e) => println!("{path}: error {e}"),
        }
    }
}
