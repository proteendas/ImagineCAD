// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Proteen Das

//! Command scripts (`.scr`): commands and their input as typed on the
//! command line, one line at a time. Run in the editor with `SCRIPT`, or
//! headless with `imaginecad script` (see [`Runner`]).
//!
//! * Spaces and line ends both act as Enter, so `CIRCLE 0,0 25` and the three
//!   lines `CIRCLE`, `0,0`, `25` do the same.
//! * An empty line is Enter on its own (e.g. to end a selection).
//! * A line where text is asked for (TEXT contents…) is taken whole, spaces
//!   included.
//! * Lines starting with `;` are comments.

use crate::commands::{self, Command, Cx, Flow, Memory, ViewReq, Want};
use crate::tab::Tab;
use icad_core::geom::*;
use icad_core::Document;

/// What a script talks to: the editor, or the headless [`Runner`].
pub trait Engine {
    /// What the running command asks for (None = no command running).
    fn wants(&self) -> Option<Want>;
    /// Input to the running command (or Enter, for "").
    fn submit(&mut self, text: &str);
    /// Start a command line.
    fn run(&mut self, line: &str);
}

/// The lines of a script, without comments and line-end characters.
pub fn lines(src: &str) -> Vec<String> {
    src.lines().map(|l| l.trim_end_matches('\r')).filter(|l| !l.trim_start().starts_with(';')).map(String::from).collect()
}

/// Feed one script line.
pub fn feed_line(e: &mut impl Engine, line: &str) {
    if line.trim().is_empty() {
        e.submit("");
        return;
    }
    match e.wants() {
        None => e.run(line.trim()),
        Some(Want::Text) => e.submit(line.trim_end()),
        Some(_) => {
            let tokens: Vec<&str> = line.split_whitespace().collect();
            for (i, tok) in tokens.iter().enumerate() {
                if e.wants().is_none() {
                    // The command ended: the rest of the line is a new command.
                    e.run(&tokens[i..].join(" "));
                    return;
                }
                if e.wants() == Some(Want::Text) {
                    e.submit(&tokens[i..].join(" "));
                    return;
                }
                e.submit(if tok.eq_ignore_ascii_case("<ENTER>") { "" } else { tok });
            }
        }
    }
}

/// Run a whole script.
pub fn run_script(e: &mut impl Engine, src: &str) {
    for l in lines(src) {
        feed_line(e, &l);
    }
}

/// Pick tolerance for points typed to select an object.
pub fn pick_tolerance(doc: &Document) -> f64 {
    let ext = doc.extents();
    if ext.is_empty() {
        1.0
    } else {
        (ext.width().max(ext.height()) * 0.002).max(1e-3)
    }
}

/// The command engine without a window: drawing and editing commands only
/// (dialogs, zoom and the like are skipped, and say so).
pub struct Runner {
    pub tab: Tab,
    pub log: Vec<String>,
    mem: Memory,
    view_req: Option<ViewReq>,
    cmd: Option<Box<dyn Command>>,
    last_point: Option<V2>,
    last_cmd: Option<String>,
    /// Lines naming commands that do not exist.
    pub unknown: Vec<String>,
}

impl Runner {
    pub fn new(doc: Document, name: &str) -> Runner {
        Runner {
            tab: Tab::new(doc, name),
            log: vec![],
            mem: Memory::default(),
            view_req: None,
            cmd: None,
            last_point: None,
            last_cmd: None,
            unknown: vec![],
        }
    }

    pub fn doc(&self) -> &Document {
        &self.tab.doc
    }

    /// End a command left waiting at the end of the script (as Esc would).
    pub fn finish(&mut self) {
        if self.cmd.is_some() {
            self.submit("");
        }
        if let Some(c) = self.cmd.take() {
            self.log.push(format!("{} was still waiting for input at the end of the script; cancelled.", c.name()));
        }
    }

    fn with_cmd(&mut self, f: impl FnOnce(&mut dyn Command, &mut Cx) -> Flow) {
        let Some(mut cmd) = self.cmd.take() else { return };
        let flow = {
            let mut cx = Cx { tab: &mut self.tab, log: &mut self.log, mem: &mut self.mem, view_req: &mut self.view_req };
            f(cmd.as_mut(), &mut cx)
        };
        if flow == Flow::Continue {
            self.cmd = Some(cmd);
        }
        if let Some(ViewReq::Script(src)) = self.view_req.take() {
            run_script(self, &src);
        }
    }

    /// The few editor commands that make sense without a window.
    fn editor_command(&mut self, up: &str) -> bool {
        match up {
            "UNDO" | "U" => {
                if let Some(d) = self.tab.undo_stack.pop() {
                    self.tab.redo_stack.push(std::mem::replace(&mut self.tab.doc, d));
                }
            }
            "REDO" => {
                if let Some(d) = self.tab.redo_stack.pop() {
                    self.tab.undo_stack.push(std::mem::replace(&mut self.tab.doc, d));
                }
            }
            "SELECTALL" => {
                let d = &self.tab.doc;
                self.tab.selection = d.entities.iter().filter(|e| d.is_visible(e) && !d.is_locked(e)).map(|e| e.id).collect();
            }
            "ZOOM" | "Z" | "PAN" | "P" | "REGEN" | "RE" | "REDRAW" | "GRID" | "SNAP" | "ORTHO" | "OSNAP" => {}
            _ => return false,
        }
        true
    }
}

impl Engine for Runner {
    fn wants(&self) -> Option<Want> {
        self.cmd.as_ref().map(|c| c.wants())
    }

    fn submit(&mut self, text: &str) {
        let text = text.trim();
        let Some(wants) = self.wants() else {
            if text.is_empty() {
                if let Some(c) = self.last_cmd.clone() {
                    self.run(&c);
                }
            } else {
                self.run(text);
            }
            return;
        };
        if text.is_empty() {
            if wants == Want::Select {
                let ids = std::mem::take(&mut self.tab.selection);
                self.with_cmd(|c, cx| c.selected(cx, ids));
            } else {
                self.with_cmd(|c, cx| c.enter(cx));
            }
            return;
        }
        match wants {
            Want::Point => {
                let base = self.cmd.as_ref().and_then(|c| c.base());
                if let Some(p) = commands::parse_point(text, base.or(self.last_point)) {
                    self.last_point = Some(p);
                    self.with_cmd(|c, cx| c.point(cx, p));
                    return;
                }
                if let (Some(b), Some(d)) = (base, commands::num(text)) {
                    // Direct distance: along +x without a cursor to aim with.
                    let p = b + V2::new(d, 0.0);
                    self.last_point = Some(p);
                    self.with_cmd(|c, cx| c.point(cx, p));
                    return;
                }
            }
            Want::Select => {
                let up = text.to_ascii_uppercase();
                if up == "ALL" {
                    self.editor_command("SELECTALL");
                    return;
                }
                if up == "L" || up == "LAST" {
                    if let Some(e) = self.tab.doc.entities.last() {
                        self.tab.selection.push(e.id);
                    }
                    return;
                }
                if let Some(p) = commands::parse_point(text, self.last_point) {
                    match self.tab.doc.pick(p, pick_tolerance(&self.tab.doc)) {
                        Some(id) if !self.tab.selection.contains(&id) => self.tab.selection.push(id),
                        Some(_) => {}
                        None => self.log.push(format!("Nothing found at {text}.")),
                    }
                    return;
                }
            }
            Want::Entity => {
                if let Some(p) = commands::parse_point(text, self.last_point) {
                    match self.tab.doc.pick(p, pick_tolerance(&self.tab.doc)) {
                        Some(id) => self.with_cmd(|c, cx| c.entity(cx, id, p)),
                        None => self.log.push(format!("Nothing found at {text}.")),
                    }
                    return;
                }
            }
            _ => {}
        }
        let t = text.to_string();
        self.with_cmd(|c, cx| c.value(cx, &t));
    }

    fn run(&mut self, line: &str) {
        let mut parts = line.split_whitespace();
        let Some(name) = parts.next() else { return };
        let rest: Vec<String> = parts.map(String::from).collect();
        let up = name.trim_start_matches('_').to_ascii_uppercase();
        if self.editor_command(&up) {
            return;
        }
        match commands::create(&up) {
            Some(c) => {
                self.log.push(format!("Command: {}", c.name()));
                self.last_cmd = Some(up);
                self.cmd = Some(c);
                self.with_cmd(|c, cx| c.start(cx));
                for r in rest {
                    if self.cmd.is_none() {
                        break;
                    }
                    self.submit(if r.eq_ignore_ascii_case("<ENTER>") { "" } else { &r });
                }
            }
            None => {
                self.log.push(format!("Unknown command or not available in scripts: {name}"));
                self.unknown.push(line.to_string());
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use icad_core::entity::Shape;

    #[test]
    fn scripts_draw_and_edit() {
        let mut r = Runner::new(Document::new(), "test");
        run_script(
            &mut r,
            "; a plate with a hole\n\
             RECTANG 0,0 100,40\n\
             CIRCLE\n30,20\n5\n\
             LINE 0,50 @50,0\n\n\
             TEXT 0,60 5 0\nPLATE 100 X 40\n\
             MOVE\nL\n\n0,0 10,0\n\
             BOGUS\n",
        );
        r.finish();
        let d = r.doc();
        assert!(d.entities.iter().any(|e| matches!(e.shape, Shape::Polyline { closed: true, .. })), "{:?}", r.log);
        assert!(d.entities.iter().any(|e| e.shape == Shape::Circle { c: V2::new(30.0, 20.0), r: 5.0 }));
        // The last object (the text) was moved 10 to the right; spaces kept.
        let t = d.entities.iter().find_map(|e| if let Shape::Text(t) = &e.shape { Some(t.clone()) } else { None }).expect("text");
        assert_eq!(t.content, "PLATE 100 X 40");
        assert!((t.pos.x - 10.0).abs() < 1e-9, "{:?}", r.log);
        assert!(d.entities.iter().any(|e| e.shape == Shape::Line { a: V2::new(0.0, 50.0), b: V2::new(50.0, 50.0) }));
        assert_eq!(r.unknown, vec!["BOGUS".to_string()]);
        // UNDO takes back the move.
        r.run("UNDO");
        let t = r.doc().entities.iter().find_map(|e| if let Shape::Text(t) = &e.shape { Some(t.pos) } else { None }).unwrap();
        assert!(t.x.abs() < 1e-9);
    }
}
