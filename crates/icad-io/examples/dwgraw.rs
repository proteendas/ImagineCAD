// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Proteen Das

//! Print the decoded entities of one type from a DWG file:
//! `cargo run -p icad-io --example dwgraw -- FILE.dwg TYPE`

use icad_io::dwgnative::{container, objects};

fn main() {
    let a: Vec<String> = std::env::args().collect();
    let file = std::fs::read(&a[1]).unwrap();
    let ty: u16 = a[2].parse().unwrap();
    let c = container::read(&file).unwrap();
    if ty == 0 {
        let mut v: Vec<_> = icad_io::dwgnative::classes::read(&c).into_iter().collect();
        v.sort();
        println!("{v:?}");
        return;
    }
    for &(h, off) in &c.map {
        if let Some(mut o) = objects::open(&c, off) {
            if o.ty == ty {
                println!("{h:X}: {:?}", objects::entity(&mut o));
            }
        }
    }
}
