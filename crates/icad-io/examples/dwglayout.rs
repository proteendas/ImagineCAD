// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Proteen Das

//! Print the layouts of a DWG file as read: `cargo run -p icad-io --example dwglayout -- FILE.dwg`

use icad_io::dwgnative::{classes, container, objects};

fn main() {
    let path = std::env::args().nth(1).unwrap();
    let file = std::fs::read(&path).unwrap();
    let c = container::read(&file).unwrap();
    let cls = classes::read(&c);
    for &(h, off) in &c.map {
        let Some(mut o) = objects::open(&c, off) else { continue };
        if o.ty == 82 || cls.get(&o.ty).is_some_and(|x| x.0 == "LAYOUT") {
            println!("{h:X}: {:?}", objects::layout(&mut o));
        }
    }
}
