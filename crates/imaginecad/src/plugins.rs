// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Proteen Das

//! Plugins: new commands from a folder with a `plugin.json`, in any
//! language. A command either runs a command script, or runs a program that
//! reads a JSON request (the inputs it asked for, the selection and the
//! drawing) on standard input and answers on standard output with edit
//! operations, the same operations AI Chat uses. See docs/PLUGINS.md.
//!
//! Plugins are looked for in `<config>/ImagineCAD/plugins/*/plugin.json` and
//! in the folders listed in `IMAGINECAD_PLUGINS`.

use crate::commands::{num, parse_point, Command, Cx, Flow, ViewReq, Want};
use icad_core::geom::*;
use serde::Deserialize;
use serde_json::{json, Value};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::{Duration, Instant};

pub const PROTOCOL: u32 = 1;

#[derive(Clone, Debug, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum InputKind {
    Point,
    Number,
    Text,
    Select,
}

#[derive(Clone, Debug, Deserialize, PartialEq)]
pub struct InputDef {
    pub name: String,
    #[serde(default)]
    pub prompt: String,
    pub kind: InputKind,
    #[serde(default)]
    pub default: Option<Value>,
}

#[derive(Clone, Debug, Deserialize, PartialEq)]
pub struct CommandDef {
    pub name: String,
    #[serde(default)]
    pub description: String,
    /// Program and arguments, run in the plugin's folder.
    #[serde(default)]
    pub run: Vec<String>,
    /// Or: a command script in the plugin's folder.
    #[serde(default)]
    pub script: Option<String>,
    #[serde(default)]
    pub inputs: Vec<InputDef>,
    /// Seconds the program may take (default 60).
    #[serde(default)]
    pub timeout: Option<u64>,
}

#[derive(Clone, Debug, Deserialize, PartialEq)]
pub struct Plugin {
    pub name: String,
    #[serde(default)]
    pub version: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub commands: Vec<CommandDef>,
    #[serde(skip)]
    pub dir: PathBuf,
}

/// Where plugins are installed.
pub fn plugin_dir() -> Option<PathBuf> {
    Some(dirs::config_dir()?.join("ImagineCAD").join("plugins"))
}

fn search_dirs() -> Vec<PathBuf> {
    let mut v: Vec<PathBuf> = plugin_dir().into_iter().collect();
    if let Some(extra) = std::env::var_os("IMAGINECAD_PLUGINS") {
        v.extend(std::env::split_paths(&extra));
    }
    v
}

/// Read one plugin folder.
pub fn load_plugin(dir: &Path) -> Result<Plugin, String> {
    let text = std::fs::read_to_string(dir.join("plugin.json")).map_err(|e| format!("{}: {e}", dir.display()))?;
    let mut p: Plugin = serde_json::from_str(&text).map_err(|e| format!("{}/plugin.json: {e}", dir.display()))?;
    p.dir = dir.to_path_buf();
    for c in &mut p.commands {
        c.name = c.name.trim().to_ascii_uppercase();
        if c.name.is_empty() || c.name.contains(char::is_whitespace) {
            return Err(format!("{}: command names must be one word", p.name));
        }
        if c.run.is_empty() && c.script.is_none() {
            return Err(format!("{}: command {} needs \"run\" or \"script\"", p.name, c.name));
        }
    }
    Ok(p)
}

/// Every plugin found (a folder holding plugin.json, or a folder of them),
/// and the problems met.
pub fn discover() -> (Vec<Plugin>, Vec<String>) {
    let mut plugins = vec![];
    let mut problems = vec![];
    for d in search_dirs() {
        let candidates: Vec<PathBuf> = if d.join("plugin.json").is_file() {
            vec![d]
        } else {
            let mut v: Vec<PathBuf> = std::fs::read_dir(&d)
                .into_iter()
                .flatten()
                .flatten()
                .map(|e| e.path())
                .filter(|p| p.join("plugin.json").is_file())
                .collect();
            v.sort();
            v
        };
        for c in candidates {
            match load_plugin(&c) {
                Ok(p) => plugins.push(p),
                Err(e) => problems.push(e),
            }
        }
    }
    (plugins, problems)
}

static REGISTRY: Mutex<Option<(Vec<Plugin>, Vec<String>)>> = Mutex::new(None);

/// The installed plugins (read once; `reload` reads them again).
pub fn installed() -> (Vec<Plugin>, Vec<String>) {
    let mut r = REGISTRY.lock().unwrap();
    r.get_or_insert_with(discover).clone()
}

pub fn reload() -> (Vec<Plugin>, Vec<String>) {
    let found = discover();
    *REGISTRY.lock().unwrap() = Some(found.clone());
    found
}

/// The plugin command named `name`, if one is installed.
pub fn create(name: &str) -> Option<Box<dyn Command>> {
    let (plugins, _) = installed();
    for p in plugins {
        if let Some(c) = p.commands.iter().find(|c| c.name == name) {
            return Some(Box::new(PluginCmd {
                def: c.clone(),
                plugin: p.name.clone(),
                dir: p.dir.clone(),
                values: vec![],
                selection: vec![],
            }));
        }
    }
    None
}

/// Run a plugin program with `request` on its standard input; its answer
/// from standard output.
pub fn call(dir: &Path, run: &[String], request: &Value, timeout: Duration) -> Result<Value, String> {
    let (prog, args) = run.split_first().ok_or("no program given")?;
    // A program named with a path inside the plugin is run from there.
    let local = dir.join(prog);
    let program = if prog.contains('/') || prog.contains('\\') || local.is_file() { local } else { PathBuf::from(prog) };
    let mut child = std::process::Command::new(&program)
        .args(args)
        .current_dir(dir)
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .map_err(|e| format!("cannot start {}: {e}", program.display()))?;
    let input = serde_json::to_vec(request).map_err(|e| e.to_string())?;
    let mut stdin = child.stdin.take().ok_or("no stdin")?;
    let writer = std::thread::spawn(move || {
        let _ = stdin.write_all(&input);
    });
    let mut stdout = child.stdout.take().ok_or("no stdout")?;
    let reader = std::thread::spawn(move || {
        let mut s = String::new();
        let _ = stdout.read_to_string(&mut s);
        s
    });
    let mut stderr = child.stderr.take().ok_or("no stderr")?;
    let err_reader = std::thread::spawn(move || {
        let mut s = String::new();
        let _ = stderr.read_to_string(&mut s);
        s
    });
    let start = Instant::now();
    let status = loop {
        if let Some(st) = child.try_wait().map_err(|e| e.to_string())? {
            break st;
        }
        if start.elapsed() > timeout {
            let _ = child.kill();
            let _ = child.wait();
            return Err(format!("{} did not finish within {} s", program.display(), timeout.as_secs()));
        }
        std::thread::sleep(Duration::from_millis(10));
    };
    let _ = writer.join();
    let out = reader.join().unwrap_or_default();
    let err = err_reader.join().unwrap_or_default();
    if !status.success() {
        let why = err.trim().lines().last().unwrap_or("").to_string();
        return Err(format!("{} failed ({status}){}", program.display(), if why.is_empty() { String::new() } else { format!(": {why}") }));
    }
    serde_json::from_str(out.trim()).map_err(|e| format!("the plugin's answer is not JSON ({e})"))
}

/// The request sent to a plugin program.
pub fn request(command: &str, inputs: Value, doc: &icad_core::Document, selection: &[u64]) -> Value {
    json!({
        "protocol": PROTOCOL,
        "command": command,
        "inputs": inputs,
        "selection": selection,
        "units": "mm",
        "description": icad_vision::assistant::describe(doc, selection),
        "document": serde_json::from_str::<Value>(&doc.to_json()).unwrap_or(Value::Null),
    })
}

struct PluginCmd {
    def: CommandDef,
    plugin: String,
    dir: PathBuf,
    values: Vec<Value>,
    selection: Vec<u64>,
}

impl PluginCmd {
    fn current(&self) -> Option<&InputDef> {
        self.def.inputs.get(self.values.len())
    }

    fn next(&mut self, cx: &mut Cx, v: Value) -> Flow {
        self.values.push(v);
        if self.current().is_some() {
            Flow::Continue
        } else {
            self.finish(cx)
        }
    }

    fn finish(&mut self, cx: &mut Cx) -> Flow {
        if let Some(scr) = &self.def.script {
            match std::fs::read_to_string(self.dir.join(scr)) {
                Ok(src) => *cx.view_req = Some(ViewReq::Script(src)),
                Err(e) => cx.msg(format!("{}: cannot read {scr}: {e}", self.plugin)),
            }
            return Flow::Done;
        }
        let inputs: serde_json::Map<String, Value> =
            self.def.inputs.iter().map(|i| i.name.clone()).zip(self.values.iter().cloned()).collect();
        let selection = if self.selection.is_empty() { cx.tab.selection.clone() } else { self.selection.clone() };
        let req = request(&self.def.name, Value::Object(inputs), &cx.tab.doc, &selection);
        let timeout = Duration::from_secs(self.def.timeout.unwrap_or(60).clamp(1, 3600));
        match call(&self.dir, &self.def.run, &req, timeout) {
            Ok(answer) => {
                let actions = answer["actions"].as_array().cloned().unwrap_or_default();
                if !actions.is_empty() {
                    cx.checkpoint();
                    let rep = icad_vision::assistant::apply(cx.doc(), &actions);
                    for e in &rep.errors {
                        cx.msg(format!("{}: {e}", self.def.name));
                    }
                    if !rep.select.is_empty() {
                        cx.tab.selection = rep.select.clone();
                    }
                    if rep.zoom {
                        *cx.view_req = Some(ViewReq::Extents);
                    }
                }
                if let Some(m) = answer["message"].as_str().filter(|m| !m.trim().is_empty()) {
                    cx.msg(m.trim().to_string());
                }
                if let Some(s) = answer["script"].as_str().filter(|s| !s.trim().is_empty()) {
                    *cx.view_req = Some(ViewReq::Script(s.to_string()));
                }
            }
            Err(e) => cx.msg(format!("{} ({}): {e}", self.def.name, self.plugin)),
        }
        Flow::Done
    }

    fn default_value(&self) -> Option<Value> {
        self.current().and_then(|i| i.default.clone())
    }
}

impl Command for PluginCmd {
    fn name(&self) -> &'static str {
        "PLUGIN"
    }
    fn prompt(&self, _: &Cx) -> String {
        match self.current() {
            Some(i) => {
                let p = if i.prompt.is_empty() { i.name.clone() } else { i.prompt.clone() };
                match (&i.default, &i.kind) {
                    (_, InputKind::Select) => format!("{p} <selection>:"),
                    (Some(d), _) => format!("{p} <{}>:", d.as_str().map(String::from).unwrap_or_else(|| d.to_string())),
                    (None, _) => format!("{p}:"),
                }
            }
            None => format!("{}…", self.def.name),
        }
    }
    fn wants(&self) -> Want {
        match self.current().map(|i| &i.kind) {
            Some(InputKind::Point) => Want::Point,
            Some(InputKind::Select) => Want::Select,
            Some(InputKind::Text) => Want::Text,
            _ => Want::Value,
        }
    }
    fn start(&mut self, cx: &mut Cx) -> Flow {
        if !self.def.description.is_empty() {
            cx.msg(format!("{}: {}", self.def.name, self.def.description));
        }
        if self.def.inputs.is_empty() {
            return self.finish(cx);
        }
        Flow::Continue
    }
    fn point(&mut self, cx: &mut Cx, p: V2) -> Flow {
        self.next(cx, json!([p.x, p.y]))
    }
    fn selected(&mut self, cx: &mut Cx, ids: Vec<u64>) -> Flow {
        self.selection = ids.clone();
        self.next(cx, json!(ids))
    }
    fn value(&mut self, cx: &mut Cx, s: &str) -> Flow {
        let Some(kind) = self.current().map(|i| i.kind.clone()) else { return Flow::Done };
        match kind {
            InputKind::Number => match num(s) {
                Some(v) => self.next(cx, json!(v)),
                None => {
                    cx.msg("Enter a number.");
                    Flow::Continue
                }
            },
            InputKind::Point => match parse_point(s, None) {
                Some(p) => self.next(cx, json!([p.x, p.y])),
                None => {
                    cx.msg("Enter a point as x,y.");
                    Flow::Continue
                }
            },
            _ => self.next(cx, json!(s)),
        }
    }
    fn enter(&mut self, cx: &mut Cx) -> Flow {
        match self.default_value() {
            Some(d) => self.next(cx, d),
            None if self.current().is_some_and(|i| i.kind == InputKind::Text) => self.next(cx, json!("")),
            None => {
                cx.msg("A value is needed here (Esc cancels).");
                Flow::Continue
            }
        }
    }
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;

    #[test]
    fn a_plugin_program_answers_with_operations() {
        let dir = std::env::temp_dir().join(format!("icad-plugin-test-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        // A plugin in shell: echoes a circle at the requested point.
        std::fs::write(
            dir.join("plugin.json"),
            r#"{"name": "Test", "commands": [{"name": "dot", "run": ["sh", "dot.sh"],
                "inputs": [{"name": "at", "kind": "point"}, {"name": "r", "kind": "number", "default": 3}]}]}"#,
        )
        .unwrap();
        std::fs::write(
            dir.join("dot.sh"),
            "req=$(cat)\ncase \"$req\" in *'\"protocol\":1'*) ;; *) exit 3;; esac\n\
             echo '{\"message\": \"dot drawn\", \"actions\": [{\"op\": \"add\", \"shape\": {\"type\": \"circle\", \"c\": [5, 6], \"r\": 3}}]}'\n",
        )
        .unwrap();
        let p = load_plugin(&dir).unwrap();
        assert_eq!(p.commands[0].name, "DOT");
        let doc = icad_core::Document::new();
        let req = request("DOT", json!({"at": [5, 6], "r": 3}), &doc, &[]);
        let ans = call(&dir, &p.commands[0].run, &req, Duration::from_secs(10)).unwrap();
        assert_eq!(ans["message"], "dot drawn");
        let mut doc = doc;
        let rep = icad_vision::assistant::apply(&mut doc, ans["actions"].as_array().unwrap());
        assert_eq!(rep.applied, 1);
        // Failures are reported, not panics.
        let err = call(&dir, &["sh".into(), "-c".into(), "exit 4".into()], &req, Duration::from_secs(10)).unwrap_err();
        assert!(err.contains("failed"), "{err}");
        let slow = call(&dir, &["sh".into(), "-c".into(), "sleep 5".into()], &req, Duration::from_secs(1)).unwrap_err();
        assert!(slow.contains("did not finish"), "{slow}");
        let _ = std::fs::remove_dir_all(&dir);
    }
}
