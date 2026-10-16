// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Proteen Das

//! External references: another drawing shown as a block that is reloaded
//! from its file. Its layers and blocks are kept apart as `NAME|LAYER` and
//! `NAME|BLOCK`; binding turns it into an ordinary block (`NAME$0$LAYER`).

use crate::document::Document;
use crate::entity::*;
use crate::geom::V2;

fn dependent(xref: &str, name: &str) -> bool {
    name.len() > xref.len() + 1 && name[..xref.len()].eq_ignore_ascii_case(xref) && name.as_bytes()[xref.len()] == b'|'
}

/// Rename layers and block references of `e` into the xref's namespace.
fn adopt(e: &Entity, xref: &str) -> Entity {
    let mut n = e.clone();
    if n.layer != "0" {
        n.layer = format!("{xref}|{}", n.layer);
    }
    if let Shape::Insert(i) = &mut n.shape {
        i.name = format!("{xref}|{}", i.name);
    }
    n
}

impl Document {
    /// External references of this drawing.
    pub fn xrefs(&self) -> Vec<&BlockDef> {
        self.blocks.iter().filter(|b| b.xref.is_some()).collect()
    }

    /// Load `other` as external reference `name` from file `path` (attach or reload).
    pub fn attach_xref(&mut self, name: &str, path: &str, other: &Document) {
        self.layers.retain(|l| !dependent(name, &l.name));
        self.blocks.retain(|b| !dependent(name, &b.name) && !b.name.eq_ignore_ascii_case(name));
        for l in other.layers.iter().filter(|l| l.name != "0") {
            let mut l = l.clone();
            l.name = format!("{name}|{}", l.name);
            l.locked = false;
            self.layers.push(l);
        }
        for b in &other.blocks {
            let entities = b.entities.iter().map(|e| adopt(e, name)).collect();
            self.blocks.push(BlockDef {
                name: format!("{name}|{}", b.name),
                base: b.base,
                entities,
                description: String::new(),
                xref: None,
            });
        }
        let entities = other.entities.iter().map(|e| adopt(e, name)).collect();
        self.blocks.push(BlockDef {
            name: name.to_string(),
            base: V2::ZERO,
            entities,
            description: format!("External reference: {path}"),
            xref: Some(path.to_string()),
        });
    }

    /// Remove an external reference with its references, layers and blocks.
    pub fn detach_xref(&mut self, name: &str) {
        self.entities.retain(|e| !matches!(&e.shape, Shape::Insert(i) if i.name.eq_ignore_ascii_case(name)));
        self.layers.retain(|l| !dependent(name, &l.name));
        self.blocks.retain(|b| !dependent(name, &b.name) && !b.name.eq_ignore_ascii_case(name));
    }

    /// Make an external reference part of this drawing (an ordinary block).
    pub fn bind_xref(&mut self, name: &str) {
        let bound = |s: &str| format!("{name}$0${}", &s[name.len() + 1..]);
        for l in &mut self.layers {
            if dependent(name, &l.name) {
                l.name = bound(&l.name);
            }
        }
        for b in &mut self.blocks {
            if dependent(name, &b.name) {
                b.name = bound(&b.name);
            }
            if b.name.eq_ignore_ascii_case(name) {
                b.xref = None;
            }
            for e in &mut b.entities {
                if dependent(name, &e.layer) {
                    e.layer = bound(&e.layer);
                }
                if let Shape::Insert(i) = &mut e.shape {
                    if dependent(name, &i.name) {
                        i.name = bound(&i.name);
                    }
                }
            }
        }
    }

    /// Whether a layer or block belongs to an external reference.
    pub fn is_xref_dependent(&self, name: &str) -> bool {
        self.xrefs().iter().any(|x| dependent(&x.name, name))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::style::{Layer, LineType};

    #[test]
    fn attach_reload_bind_detach() {
        let mut part = Document::new();
        part.ensure_layer(Layer::new("WELDS", 3, LineType::Continuous, 0.35));
        let id = part.add_on("WELDS", Shape::Circle { c: V2::ZERO, r: 5.0 });
        let _ = id;
        let mut host = Document::new();
        host.attach_xref("BASEPLATE", "/parts/baseplate.dwg", &part);
        assert!(host.layer("BASEPLATE|WELDS").is_some());
        assert_eq!(host.xrefs().len(), 1);
        host.add(Shape::Insert(Insert::new("BASEPLATE", V2::new(100.0, 0.0))));
        // Reloading replaces the contents.
        part.add_on("WELDS", Shape::Line { a: V2::ZERO, b: V2::new(10.0, 0.0) });
        host.attach_xref("BASEPLATE", "/parts/baseplate.dwg", &part);
        assert_eq!(host.block("BASEPLATE").unwrap().entities.len(), 2);
        assert_eq!(host.blocks.len(), 1);
        assert!(host.is_xref_dependent("BASEPLATE|WELDS"));
        let mut bound = host.clone();
        bound.bind_xref("BASEPLATE");
        assert!(bound.xrefs().is_empty() && bound.layer("BASEPLATE$0$WELDS").is_some());
        assert_eq!(bound.block("BASEPLATE").unwrap().entities[0].layer, "BASEPLATE$0$WELDS");
        host.detach_xref("BASEPLATE");
        assert!(host.xrefs().is_empty() && host.layer("BASEPLATE|WELDS").is_none() && host.entities.is_empty());
    }
}
