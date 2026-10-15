// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Proteen Das

//! AI chat panel: change the drawing by describing the change. Each answer is
//! applied as one undo step; manual editing stays available alongside.

use crate::app::App;
use crate::commands::ViewReq;
use crate::theme;
use eframe::egui::{self, Color32, RichText};
use icad_vision::assistant::{self, Answer, Report};
use icad_vision::llm::{Provider, Turn};
use std::sync::mpsc::{channel, Receiver};

/// Example requests shown in an empty conversation.
const EXAMPLES: [&str; 5] = [
    "Change the company name to ACME ENGINEERING and the material to C45",
    "Add a 10 mm hole at the centre of the plate and dimension it",
    "Make the longest segment 100 mm longer",
    "Put all dimensions on a new layer DIMS-CHECK in red",
    "What is the overall length?",
];

/// How many earlier messages are sent with each request.
const HISTORY: usize = 12;

pub struct ChatMsg {
    pub user: bool,
    pub text: String,
    /// What happened to the drawing, for assistant messages.
    pub report: Option<Report>,
    /// Tab index and undo depth right after this answer was applied (for Undo).
    pub undo_mark: Option<(usize, usize)>,
    pub error: bool,
}

#[derive(Default)]
pub struct ChatPanel {
    pub messages: Vec<ChatMsg>,
    pub input: String,
    pub rx: Option<Receiver<Result<Answer, String>>>,
    /// Tab the pending request was made for.
    pub tab: usize,
}

impl App {
    fn send_chat(&mut self, ctx: &egui::Context) {
        let text = self.chat.input.trim().to_string();
        if text.is_empty() || self.chat.rx.is_some() {
            return;
        }
        self.chat.input.clear();
        self.chat.messages.push(ChatMsg { user: true, text, report: None, undo_mark: None, error: false });
        // Recent messages, starting with the user and alternating (merge repeats).
        let ok: Vec<&ChatMsg> = self.chat.messages.iter().filter(|m| !m.error).collect();
        let mut turns: Vec<Turn> = vec![];
        for m in &ok[ok.len().saturating_sub(HISTORY)..] {
            match turns.last_mut() {
                Some(t) if t.user == m.user => t.text = format!("{}\n\n{}", t.text, m.text),
                None if !m.user => {}
                _ => turns.push(Turn { user: m.user, text: m.text.clone() }),
            }
        }
        let ai = self.settings.convert.ai.clone();
        let t = &self.tabs[self.active];
        let (doc, sel) = (t.doc.clone(), t.selection.clone());
        let (tx, rx) = channel();
        let ctx = ctx.clone();
        std::thread::spawn(move || {
            let _ = tx.send(assistant::ask(&ai, &turns, &doc, &sel));
            ctx.request_repaint();
        });
        self.chat.rx = Some(rx);
        self.chat.tab = self.active;
    }

    fn poll_chat(&mut self) {
        let Some(rx) = &self.chat.rx else { return };
        let Ok(result) = rx.try_recv() else { return };
        self.chat.rx = None;
        match result {
            Ok(a) => {
                if let Some(m) = &a.switched_model {
                    self.settings.convert.ai.openai_model = m.clone();
                    self.settings.save();
                }
                let mut report = None;
                let mut undo_mark = None;
                if !a.actions.is_empty() && self.chat.tab < self.tabs.len() {
                    let t = &mut self.tabs[self.chat.tab];
                    let before = t.doc.clone();
                    let r = assistant::apply(&mut t.doc, &a.actions);
                    if t.doc != before {
                        // One undo step for the whole answer.
                        t.undo_stack.push(before);
                        t.redo_stack.clear();
                        t.dirty = true;
                        undo_mark = Some((self.chat.tab, t.undo_stack.len()));
                    }
                    t.selection.retain(|id| t.doc.get(*id).is_some());
                    if !r.select.is_empty() {
                        t.selection = r.select.clone();
                    }
                    if r.zoom {
                        self.view_req = Some(ViewReq::Extents);
                    }
                    self.log.push(format!("AI Chat: {} change(s) applied.", r.applied));
                    report = Some(r);
                }
                let mut text = if a.reply.is_empty() { "Done.".to_string() } else { a.reply };
                if let Some(m) = a.switched_model {
                    text += &format!("\n(Answered by the free model {m}: the selected model has no credit.)");
                }
                self.chat.messages.push(ChatMsg { user: false, text, report, undo_mark, error: false });
            }
            Err(e) => self.chat.messages.push(ChatMsg { user: false, text: e, report: None, undo_mark: None, error: true }),
        }
    }

    pub fn chat_panel(&mut self, ctx: &egui::Context) {
        self.poll_chat();
        if !self.settings.show_chat {
            return;
        }
        let mut send = false;
        let mut undo: Option<usize> = None;
        let mut open_settings = false;
        egui::SidePanel::right("ai_chat")
            .resizable(true)
            .default_width(340.0)
            .min_width(260.0)
            .frame(egui::Frame::NONE.fill(theme::PANEL).inner_margin(egui::Margin::same(8)))
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    ui.label(RichText::new("AI CHAT").strong().color(theme::AI_ACCENT));
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui.add(egui::Button::new("×").frame(false)).on_hover_text("Close (AICHAT)").clicked() {
                            self.settings.show_chat = false;
                        }
                        if !self.chat.messages.is_empty() && ui.small_button("Clear").on_hover_text("Start a new conversation").clicked() {
                            self.chat.messages.clear();
                        }
                    });
                });
                let ai = &self.settings.convert.ai;
                ui.horizontal(|ui| {
                    ui.label(RichText::new(ai.describe()).size(11.0).color(theme::TEXT_DIM));
                    if ui.small_button("Change…").clicked() {
                        open_settings = true;
                    }
                });
                ui.separator();
                let off = ai.provider == Provider::Disabled;
                let input_h = 110.0;
                egui::ScrollArea::vertical()
                    .auto_shrink([false, false])
                    .max_height((ui.available_height() - input_h).max(80.0))
                    .stick_to_bottom(true)
                    .show(ui, |ui| {
                        if self.chat.messages.is_empty() {
                            ui.label(
                                RichText::new(
                                    "Describe a change and the AI edits the drawing. Every answer is one undo step (Ctrl+Z). \
                                     Select objects first to talk about \"this\" or \"these\".",
                                )
                                .size(12.0)
                                .color(theme::TEXT_DIM),
                            );
                            ui.add_space(6.0);
                            for ex in EXAMPLES {
                                if ui.add(egui::Button::new(RichText::new(ex).size(11.5)).wrap()).clicked() {
                                    self.chat.input = ex.to_string();
                                }
                            }
                        }
                        let last_applied = self.chat.messages.iter().rposition(|m| m.undo_mark.is_some());
                        for (i, m) in self.chat.messages.iter().enumerate() {
                            let (bg, fg) = if m.user {
                                (theme::ACTIVE, Color32::WHITE)
                            } else if m.error {
                                (Color32::from_rgb(80, 44, 48), theme::ERR)
                            } else {
                                (Color32::from_rgb(60, 70, 86), theme::TEXT)
                            };
                            let layout = if m.user {
                                egui::Layout::right_to_left(egui::Align::Min)
                            } else {
                                egui::Layout::left_to_right(egui::Align::Min)
                            };
                            ui.with_layout(layout, |ui| {
                                egui::Frame::NONE.fill(bg).corner_radius(6.0).inner_margin(egui::Margin::symmetric(8, 6)).show(ui, |ui| {
                                    ui.set_max_width(ui.available_width() * 0.88);
                                    ui.add(egui::Label::new(RichText::new(&m.text).color(fg)).wrap());
                                    if let Some(r) = &m.report {
                                        ui.horizontal_wrapped(|ui| {
                                            let ok = format!("✔ {} change(s) applied", r.applied);
                                            ui.label(RichText::new(ok).size(11.0).color(Color32::from_rgb(90, 210, 120)));
                                            let current = m.undo_mark.is_some_and(|(tab, depth)| {
                                                Some(i) == last_applied && tab == self.active && self.tabs[tab].undo_stack.len() == depth
                                            });
                                            if current && ui.small_button("Undo").on_hover_text("Revert this answer's changes").clicked() {
                                                undo = Some(i);
                                            }
                                        });
                                        for e in &r.errors {
                                            ui.label(RichText::new(format!("⚠ skipped: {e}")).size(11.0).color(theme::WARN));
                                        }
                                    }
                                });
                            });
                            ui.add_space(4.0);
                        }
                        if self.chat.rx.is_some() {
                            ui.horizontal(|ui| {
                                ui.spinner();
                                ui.label(RichText::new("Thinking…").color(theme::TEXT_DIM));
                            });
                        }
                    });
                ui.separator();
                if off {
                    ui.label(RichText::new("AI is off. Connect a model to use the chat.").color(theme::WARN));
                    if ui.button("Open AI Settings").clicked() {
                        open_settings = true;
                    }
                    return;
                }
                let id = egui::Id::new("ai_chat_input");
                let focused = ui.memory(|m| m.has_focus(id));
                // Enter sends; Shift+Enter starts a new line.
                if focused && ui.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::Enter)) {
                    send = true;
                }
                ui.add(
                    egui::TextEdit::multiline(&mut self.chat.input)
                        .id(id)
                        .desired_rows(3)
                        .desired_width(f32::INFINITY)
                        .hint_text("Describe a change… (Enter to send, Shift+Enter for a new line)"),
                );
                ui.horizontal(|ui| {
                    let busy = self.chat.rx.is_some();
                    let btn = egui::Button::new(RichText::new("Send").strong().color(Color32::WHITE)).fill(Color32::from_rgb(110, 70, 200));
                    if ui.add_enabled(!busy && !self.chat.input.trim().is_empty(), btn).clicked() {
                        send = true;
                    }
                    let n = self.tabs[self.active].selection.len();
                    if n > 0 {
                        ui.label(RichText::new(format!("{n} selected object(s) included")).size(11.0).color(theme::TEXT_DIM));
                    }
                });
            });
        if send {
            self.send_chat(ctx);
        }
        if let Some(i) = undo {
            if let Some((tab, _)) = self.chat.messages[i].undo_mark.take() {
                if self.tabs[tab].undo() {
                    self.log.push("AI Chat: change undone.".into());
                    self.chat.messages[i].text += "\n(Undone.)";
                }
            }
        }
        if open_settings {
            self.dlg.ai_settings = true;
        }
    }
}
