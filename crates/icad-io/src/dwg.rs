// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Proteen Das

//! DWG support through a free external converter.
//!
//! DWG is a closed, undocumented format; there is no complete open-source Rust
//! writer. ImagineCAD writes DXF itself and, when one of these free tools is
//! installed, converts DXF ⇄ DWG:
//!
//! * **ODA File Converter** (free download from opendesign.com) – best fidelity.
//! * **LibreDWG** `dxf2dwg` / `dwg2dxf` (GPL, free) – experimental writer.

use std::path::{Path, PathBuf};
use std::process::Command;

#[derive(Clone, Debug, PartialEq)]
pub enum Converter {
    Oda(PathBuf),
    LibreDwg { dxf2dwg: Option<PathBuf>, dwg2dxf: Option<PathBuf> },
}

impl Converter {
    pub fn name(&self) -> &'static str {
        match self {
            Converter::Oda(_) => "ODA File Converter",
            Converter::LibreDwg { .. } => "LibreDWG",
        }
    }
}

fn which(name: &str) -> Option<PathBuf> {
    let exts: &[&str] = if cfg!(windows) { &[".exe", ""] } else { &[""] };
    std::env::var_os("PATH").and_then(|paths| {
        std::env::split_paths(&paths).find_map(|dir| exts.iter().map(|e| dir.join(format!("{name}{e}"))).find(|p| p.is_file()))
    })
}

fn oda_candidates() -> Vec<PathBuf> {
    let mut v = vec![];
    if cfg!(target_os = "macos") {
        v.push(PathBuf::from("/Applications/ODAFileConverter.app/Contents/MacOS/ODAFileConverter"));
    } else if cfg!(windows) {
        for root in ["C:\\Program Files\\ODA", "C:\\Program Files (x86)\\ODA"] {
            if let Ok(rd) = std::fs::read_dir(root) {
                for e in rd.flatten() {
                    v.push(e.path().join("ODAFileConverter.exe"));
                }
            }
        }
    } else {
        v.push(PathBuf::from("/usr/bin/ODAFileConverter"));
        v.push(PathBuf::from("/usr/local/bin/ODAFileConverter"));
    }
    v
}

/// Find an installed DXF⇄DWG converter.
/// Directories where an ImagineCAD installer places its bundled LibreDWG tools.
pub fn bundled_dirs() -> Vec<PathBuf> {
    let mut v = vec![];
    if let Some(d) = std::env::var_os("IMAGINECAD_LIBREDWG") {
        v.push(PathBuf::from(d));
    }
    if let Some(exe_dir) = std::env::current_exe().ok().and_then(|p| p.parent().map(|d| d.to_path_buf())) {
        v.push(exe_dir.join("libredwg"));
        v.push(exe_dir.join("../Resources/libredwg")); // macOS .app bundle
        v.push(exe_dir.join("../lib/imaginecad")); // Linux .deb / tarball
    }
    v
}

fn exe(name: &str) -> String {
    if cfg!(windows) {
        format!("{name}.exe")
    } else {
        name.to_string()
    }
}

/// Find a DXF⇄DWG converter: the free ODA File Converter if installed
/// (highest fidelity), otherwise the LibreDWG tools bundled with ImagineCAD,
/// otherwise LibreDWG on the PATH.
pub fn find_converter() -> Option<Converter> {
    if let Some(p) = which("ODAFileConverter").or_else(|| oda_candidates().into_iter().find(|p| p.is_file())) {
        return Some(Converter::Oda(p));
    }
    for d in bundled_dirs() {
        let (a, b) = (d.join(exe("dxf2dwg")), d.join(exe("dwg2dxf")));
        if a.is_file() && b.is_file() {
            return Some(Converter::LibreDwg { dxf2dwg: Some(a), dwg2dxf: Some(b) });
        }
    }
    let dxf2dwg = which("dxf2dwg");
    let dwg2dxf = which("dwg2dxf");
    if dxf2dwg.is_some() || dwg2dxf.is_some() {
        return Some(Converter::LibreDwg { dxf2dwg, dwg2dxf });
    }
    None
}

/// A fresh working folder; the counter keeps folders apart when conversions
/// run at the same time in one process.
fn tmp_dir(tag: &str) -> std::io::Result<PathBuf> {
    static SEQ: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    let n = SEQ.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let nanos = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_nanos()).unwrap_or(0);
    let d = std::env::temp_dir().join(format!("imaginecad-{tag}-{}-{nanos}-{n}", std::process::id()));
    std::fs::create_dir_all(&d)?;
    Ok(d)
}

fn run(cmd: &mut Command) -> Result<(), String> {
    let out = cmd.output().map_err(|e| format!("failed to start converter: {e}"))?;
    if out.status.success() {
        Ok(())
    } else {
        Err(format!("converter failed: {}", String::from_utf8_lossy(&out.stderr)))
    }
}

/// Convert DXF text into a DWG file at `out`.
pub fn dxf_to_dwg(dxf: &str, out: &Path, conv: &Converter) -> Result<(), String> {
    let work = tmp_dir("dwg").map_err(|e| e.to_string())?;
    let stem = out.file_stem().and_then(|s| s.to_str()).unwrap_or("drawing").to_string();
    let in_dir = work.join("in");
    let out_dir = work.join("out");
    std::fs::create_dir_all(&in_dir).map_err(|e| e.to_string())?;
    std::fs::create_dir_all(&out_dir).map_err(|e| e.to_string())?;
    let src = in_dir.join(format!("{stem}.dxf"));
    std::fs::write(&src, dxf).map_err(|e| e.to_string())?;
    let produced = out_dir.join(format!("{stem}.dwg"));
    match conv {
        Converter::Oda(exe) => run(Command::new(exe).args([
            in_dir.as_os_str(),
            out_dir.as_os_str(),
            "ACAD2018".as_ref(),
            "DWG".as_ref(),
            "0".as_ref(),
            "1".as_ref(),
            "*.DXF".as_ref(),
        ]))?,
        Converter::LibreDwg { dxf2dwg: Some(exe), .. } => run(Command::new(exe).arg("-y").arg("-o").arg(&produced).arg(&src))?,
        Converter::LibreDwg { .. } => return Err("LibreDWG dxf2dwg not found".into()),
    }
    if !produced.is_file() {
        return Err("converter did not produce a DWG file".into());
    }
    std::fs::copy(&produced, out).map_err(|e| e.to_string())?;
    let _ = std::fs::remove_dir_all(&work);
    Ok(())
}

/// Convert a DWG file to DXF text.
pub fn dwg_to_dxf(dwg: &Path, conv: &Converter) -> Result<String, String> {
    let work = tmp_dir("dxf").map_err(|e| e.to_string())?;
    let in_dir = work.join("in");
    let out_dir = work.join("out");
    std::fs::create_dir_all(&in_dir).map_err(|e| e.to_string())?;
    std::fs::create_dir_all(&out_dir).map_err(|e| e.to_string())?;
    let stem = dwg.file_stem().and_then(|s| s.to_str()).unwrap_or("drawing").to_string();
    let src = in_dir.join(format!("{stem}.dwg"));
    std::fs::copy(dwg, &src).map_err(|e| e.to_string())?;
    let produced = out_dir.join(format!("{stem}.dxf"));
    match conv {
        Converter::Oda(exe) => run(Command::new(exe).args([
            in_dir.as_os_str(),
            out_dir.as_os_str(),
            "ACAD2018".as_ref(),
            "DXF".as_ref(),
            "0".as_ref(),
            "1".as_ref(),
            "*.DWG".as_ref(),
        ]))?,
        Converter::LibreDwg { dwg2dxf: Some(exe), .. } => run(Command::new(exe).arg("-y").arg("-o").arg(&produced).arg(&src))?,
        Converter::LibreDwg { .. } => return Err("LibreDWG dwg2dxf not found".into()),
    }
    let s = std::fs::read(&produced).map_err(|e| format!("converter did not produce DXF: {e}"))?;
    let _ = std::fs::remove_dir_all(&work);
    Ok(String::from_utf8_lossy(&s).into_owned())
}
