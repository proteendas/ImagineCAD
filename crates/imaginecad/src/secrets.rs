// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Proteen Das

//! Secure storage for the AI API key.
//!
//! * macOS: Keychain · Windows: Credential Manager (via the `keyring` crate)
//! * Linux / fallback: `<config>/ImagineCAD/credentials`, readable by the owner only
//! * `IMAGINECAD_API_KEY` overrides everything (CI, scripting).

use std::path::PathBuf;

const SERVICE: &str = "ImagineCAD";
const ACCOUNT: &str = "ai-api-key";

fn file() -> Option<PathBuf> {
    Some(dirs::config_dir()?.join("ImagineCAD").join("credentials"))
}

#[cfg(any(target_os = "macos", windows))]
fn entry() -> Option<keyring::Entry> {
    keyring::Entry::new(SERVICE, ACCOUNT).ok()
}

/// Load the stored API key.
pub fn load() -> Option<String> {
    if let Ok(k) = std::env::var("IMAGINECAD_API_KEY") {
        if !k.trim().is_empty() {
            return Some(k.trim().to_string());
        }
    }
    #[cfg(any(target_os = "macos", windows))]
    if let Some(k) = entry().and_then(|e| e.get_password().ok()) {
        return Some(k);
    }
    std::fs::read_to_string(file()?).ok().map(|s| s.trim().to_string()).filter(|s| !s.is_empty())
}

/// Store the API key. Returns a description of where it was stored.
pub fn save(key: &str) -> Result<&'static str, String> {
    #[cfg(any(target_os = "macos", windows))]
    if let Some(e) = entry() {
        if e.set_password(key).is_ok() {
            if let Some(f) = file() {
                let _ = std::fs::remove_file(f);
            }
            return Ok(if cfg!(windows) { "Windows Credential Manager" } else { "macOS Keychain" });
        }
    }
    let f = file().ok_or("no configuration directory")?;
    if let Some(d) = f.parent() {
        std::fs::create_dir_all(d).map_err(|e| e.to_string())?;
    }
    std::fs::write(&f, key).map_err(|e| e.to_string())?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = std::fs::set_permissions(&f, std::fs::Permissions::from_mode(0o600));
    }
    Ok("a private settings file")
}

pub fn delete() {
    #[cfg(any(target_os = "macos", windows))]
    if let Some(e) = entry() {
        let _ = e.delete_credential();
    }
    if let Some(f) = file() {
        let _ = std::fs::remove_file(f);
    }
    let _ = SERVICE;
}

/// Mask a key for display: `sk-ant-…a1b2`.
pub fn mask(key: &str) -> String {
    let k = key.trim();
    if k.len() <= 10 {
        return "••••".into();
    }
    let head: String = k.chars().take(6).collect();
    let tail: String = k.chars().rev().take(4).collect::<Vec<_>>().into_iter().rev().collect();
    format!("{head}…{tail}")
}
