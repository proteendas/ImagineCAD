// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Proteen Das

//! Block attributes: ATTDEF (define), ATTEDIT (values of a block reference,
//! also after INSERT and on double-click) and ATTEXPORT (CSV).

use crate::app::App;
use crate::commands::{Command, Cx, Flow, Want};
use eframe::egui::{self, RichText};
use icad_core::entity::*;
use icad_core::geom::*;
use icad_core::prim::Text;
use icad_core::Document;

pub fn create(name: &str) -> Option<Box<dyn Command>> {
    let c: Box<dyn Command> = match name {
        "ATTDEF" | "ATT" => Box::new(AttDefCmd::default()),
        "ATTEDIT" | "ATE" | "EATTEDIT" => Box::new(AttEditCmd),
        other => return crate::commands_region::create(other),
    };
    Some(c)
}

// ------------------------------------------------------------------ ATTDEF

#[derive(Default)]
struct AttDefCmd {
    tag: Option<String>,
    prompt: Option<String>,
    default: Option<String>,
}

impl AttDefCmd {
    fn def(&self, cx_h: f64, at: V2) -> Option<AttDef> {
        Some(AttDef {
            tag: self.tag.clone()?,
            prompt: self.prompt.clone().unwrap_or_default(),
            default: self.default.clone().unwrap_or_default(),
            text: Text::new(at, cx_h, ""),
            invisible: false,
        })
    }
}

impl Command for AttDefCmd {
    fn name(&self) -> &'static str {
        "ATTDEF"
    }
    fn prompt(&self, _: &Cx) -> String {
        match (&self.tag, &self.prompt, &self.default) {
            (None, ..) => "Enter attribute tag (no spaces):".into(),
            (Some(_), None, _) => "Enter attribute prompt <same as tag>:".into(),
            (Some(_), Some(_), None) => "Enter default value <none>:".into(),
            _ => "Specify start point of the attribute text:".into(),
        }
    }
    fn wants(&self) -> Want {
        if self.default.is_none() {
            Want::Text
        } else {
            Want::Point
        }
    }
    fn value(&mut self, cx: &mut Cx, s: &str) -> Flow {
        if self.tag.is_none() {
            let t = s.trim().replace(' ', "_").to_ascii_uppercase();
            if t.is_empty() {
                cx.msg("The tag cannot be empty.");
            } else {
                self.tag = Some(t);
            }
        } else if self.prompt.is_none() {
            self.prompt = Some(s.trim().to_string());
        } else if self.default.is_none() {
            self.default = Some(s.to_string());
        }
        Flow::Continue
    }
    fn enter(&mut self, _: &mut Cx) -> Flow {
        match (&self.tag, &self.prompt, &self.default) {
            (None, ..) => Flow::Done,
            (Some(t), None, _) => {
                self.prompt = Some(t.clone());
                Flow::Continue
            }
            (Some(_), Some(_), None) => {
                self.default = Some(String::new());
                Flow::Continue
            }
            _ => Flow::Done,
        }
    }
    fn point(&mut self, cx: &mut Cx, p: V2) -> Flow {
        let h = cx.mem.text_h.unwrap_or(cx.tab.doc.dimstyle.text_height * cx.tab.doc.ascale());
        if let Some(d) = self.def(h, p) {
            cx.checkpoint();
            cx.doc().add(Shape::AttDef(d));
            cx.msg("Attribute defined. Include it when you create the block (BLOCK).");
        }
        Flow::Done
    }
    fn preview(&self, doc: &Document, c: V2) -> Vec<Shape> {
        let h = doc.dimstyle.text_height * doc.ascale();
        self.def(h, c).map(|d| vec![Shape::AttDef(d)]).unwrap_or_default()
    }
}

// ------------------------------------------------------------------ ATTEDIT

struct AttEditCmd;

impl Command for AttEditCmd {
    fn name(&self) -> &'static str {
        "ATTEDIT"
    }
    fn start(&mut self, cx: &mut Cx) -> Flow {
        let sel = cx.take_selection();
        if let Some(id) = sel.into_iter().find(|id| has_attributes(&cx.tab.doc, *id)) {
            cx.mem.pending_attedit = Some(id);
            return Flow::Done;
        }
        Flow::Continue
    }
    fn prompt(&self, _: &Cx) -> String {
        "Select block reference:".into()
    }
    fn wants(&self) -> Want {
        Want::Entity
    }
    fn entity(&mut self, cx: &mut Cx, id: u64, _: V2) -> Flow {
        if has_attributes(&cx.tab.doc, id) {
            cx.mem.pending_attedit = Some(id);
            Flow::Done
        } else {
            cx.msg("That object is not a block with attributes.");
            Flow::Continue
        }
    }
}

/// True for a block reference whose block defines attributes.
pub fn has_attributes(doc: &Document, id: u64) -> bool {
    match doc.get(id).map(|e| &e.shape) {
        Some(Shape::Insert(i)) => doc.block(&i.name).is_some_and(|b| b.attdefs().next().is_some()),
        _ => false,
    }
}

/// Attribute values being edited: (tag, prompt, value) of one block reference.
pub struct AttEditDialog {
    pub id: u64,
    pub block: String,
    pub rows: Vec<(String, String, String)>,
}

impl App {
    /// Open the attribute editor for a block reference (false if it has none).
    pub fn edit_attributes_of(&mut self, id: u64) -> bool {
        let t = &self.tabs[self.active];
        if !has_attributes(&t.doc, id) {
            return false;
        }
        let Some(Shape::Insert(ins)) = t.doc.get(id).map(|e| &e.shape) else { return false };
        let b = t.doc.block(&ins.name).unwrap();
        let rows = b
            .attdefs()
            .map(|a| {
                let prompt = if a.prompt.is_empty() { a.tag.clone() } else { a.prompt.clone() };
                (a.tag.clone(), prompt, ins.value_of(&a.tag).unwrap_or(&a.default).to_string())
            })
            .collect();
        self.dlg.attedit = Some(AttEditDialog { id, block: ins.name.clone(), rows });
        true
    }

    pub fn attedit_dialog(&mut self, ctx: &egui::Context) {
        if let Some(id) = self.mem.pending_attedit.take() {
            self.edit_attributes_of(id);
        }
        let Some(mut d) = self.dlg.attedit.take() else { return };
        let mut open = true;
        let mut ok = false;
        let mut cancel = false;
        egui::Window::new("Edit Attributes").open(&mut open).collapsible(false).default_width(380.0).show(ctx, |ui| {
            ui.label(RichText::new(format!("Block: {}", d.block)).strong());
            ui.add_space(4.0);
            egui::Grid::new("att_rows").num_columns(2).spacing([10.0, 6.0]).show(ui, |ui| {
                for (tag, prompt, value) in &mut d.rows {
                    ui.label(prompt.as_str()).on_hover_text(format!("Tag {tag}"));
                    ui.add(egui::TextEdit::singleline(value).desired_width(220.0));
                    ui.end_row();
                }
            });
            ui.add_space(6.0);
            ui.horizontal(|ui| {
                if ui.button(RichText::new("OK").strong()).clicked() {
                    ok = true;
                }
                if ui.button("Cancel").clicked() {
                    cancel = true;
                }
            });
        });
        if ok {
            let t = self.tab();
            t.checkpoint();
            if let Some(Shape::Insert(ins)) = t.doc.get_mut(d.id).map(|e| &mut e.shape) {
                for (tag, _, value) in &d.rows {
                    ins.set_value(tag, value.clone());
                }
            }
            return;
        }
        if open && !cancel {
            self.dlg.attedit = Some(d);
        }
    }

    /// ATTEXPORT: block attributes of the drawing as CSV (a parts list).
    pub fn export_attributes(&mut self) {
        let doc = &self.tabs[self.active].doc;
        let csv = attributes_csv(doc);
        if csv.lines().count() <= 1 {
            self.log.push("No block references with attributes in this drawing.".into());
            return;
        }
        let Some(path) = rfd::FileDialog::new().add_filter("CSV", &["csv"]).set_file_name("attributes.csv").save_file() else { return };
        match std::fs::write(&path, csv) {
            Ok(()) => self.log.push(format!("Attributes exported to {}", path.display())),
            Err(e) => self.log.push(format!("Cannot write {}: {e}", path.display())),
        }
    }
}

fn csv_field(s: &str) -> String {
    if s.contains([',', '"', '\n']) {
        format!("\"{}\"", s.replace('"', "\"\""))
    } else {
        s.to_string()
    }
}

/// Block name, position and (tag, value) pairs of one block reference.
type CsvRow = (String, V2, Vec<(String, String)>);

/// One row per block reference with attributes: block, position, then each tag.
pub fn attributes_csv(doc: &Document) -> String {
    let mut tags: Vec<String> = vec![];
    let mut rows: Vec<CsvRow> = vec![];
    for e in &doc.entities {
        let Shape::Insert(ins) = &e.shape else { continue };
        let Some(b) = doc.block(&ins.name) else { continue };
        let vals: Vec<(String, String)> =
            b.attdefs().map(|a| (a.tag.to_ascii_uppercase(), ins.value_of(&a.tag).unwrap_or(&a.default).to_string())).collect();
        if vals.is_empty() {
            continue;
        }
        for (t, _) in &vals {
            if !tags.contains(t) {
                tags.push(t.clone());
            }
        }
        rows.push((ins.name.clone(), ins.pos, vals));
    }
    let mut out = String::from("Block,X,Y");
    for t in &tags {
        out += &format!(",{}", csv_field(t));
    }
    out.push('\n');
    for (name, p, vals) in rows {
        out += &format!("{},{},{}", csv_field(&name), crate::commands::fmt(p.x), crate::commands::fmt(p.y));
        for t in &tags {
            let v = vals.iter().find(|(k, _)| k == t).map(|(_, v)| v.as_str()).unwrap_or("");
            out += &format!(",{}", csv_field(v));
        }
        out.push('\n');
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use icad_core::style::{Color, LtRef};

    #[test]
    fn attributes_render_explode_and_export() {
        let mut doc = Document::new();
        let def = |tag: &str, default: &str, y: f64| Entity {
            id: 0,
            layer: "0".into(),
            color: Color::ByLayer,
            linetype: LtRef::ByLayer,
            lineweight: None,
            shape: Shape::AttDef(AttDef {
                tag: tag.into(),
                prompt: format!("{tag}?"),
                default: default.into(),
                text: Text::new(V2::new(0.0, y), 2.5, ""),
                invisible: false,
            }),
        };
        doc.define_block(BlockDef {
            name: "BALLOON".into(),
            base: V2::ZERO,
            entities: vec![
                Entity {
                    id: 0,
                    layer: "0".into(),
                    color: Color::ByLayer,
                    linetype: LtRef::ByLayer,
                    lineweight: None,
                    shape: Shape::Circle { c: V2::ZERO, r: 5.0 },
                },
                def("ITEM", "1", 0.0),
                def("PART, NO", "X-1", -8.0),
            ],
            description: String::new(),
            xref: None,
            params: vec![],
        });
        let mut ins = Insert::new("BALLOON", V2::new(100.0, 50.0));
        ins.set_value("item", "7");
        let id = doc.add(Shape::Insert(ins));
        assert!(has_attributes(&doc, id));
        let e = doc.get(id).unwrap();
        let texts: Vec<String> =
            doc.prims_of(e).into_iter().filter_map(|p| if let icad_core::Prim::Text(t) = p { Some(t.content) } else { None }).collect();
        assert_eq!(texts, vec!["7".to_string(), "X-1".to_string()]);
        let parts = doc.explode_entity(e).unwrap();
        assert!(parts.iter().any(|p| matches!(&p.shape, Shape::Text(t) if t.content == "7" && t.pos == V2::new(100.0, 50.0))));
        let csv = attributes_csv(&doc);
        assert_eq!(csv, "Block,X,Y,ITEM,\"PART, NO\"\nBALLOON,100,50,7,X-1\n");
    }
}
