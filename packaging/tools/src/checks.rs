// Copyright (C) 2026 yrmuq
// SPDX-License-Identifier: GPL-3.0-or-later

use crate::{Context, Result};
use serde_json::Value;
use std::collections::HashMap;
use std::fs;
use std::path::Path;

pub fn readings(input: &str) -> Result<()> {
    let value: Value = serde_json::from_str(input)?;
    if !value["devices"].is_array() || !value["warnings"].is_array() {
        return Err("Expected devices and warnings arrays".into());
    }
    Ok(())
}

pub fn zsync(context: &Context, path: &Path) -> Result<()> {
    let bytes = fs::read(context.root.join(format!("{}.zsync", path.display())))?;
    let end = bytes
        .windows(2)
        .position(|part| part == b"\n\n")
        .ok_or("Missing zsync header delimiter")?;
    let fields: HashMap<_, _> = std::str::from_utf8(&bytes[..end])?
        .lines()
        .filter_map(|line| line.split_once(": "))
        .collect();
    let name = path
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or("Invalid AppImage name")?;
    if fields.get("Filename") != Some(&name)
        || fields
            .get("Length")
            .ok_or("Missing zsync length")?
            .parse::<u64>()?
            != fs::metadata(context.root.join(path))?.len()
    {
        return Err("zsync filename or size mismatch".into());
    }
    let digest = context.run("sha1sum", &[path.to_str().ok_or("Invalid AppImage path")?])?;
    if fields.get("SHA-1").copied() != digest.split_whitespace().next() {
        return Err("zsync checksum mismatch".into());
    }
    Ok(())
}

pub fn flatpakref(context: &Context) -> Result<()> {
    let key = context.run("base64", &["-w0", "flatpak-repo/shinpi.gpg"])?;
    fs::write(context.root.join("dist/shinpi.flatpakref"), format!(
        "[Flatpak Ref]\nName=io.github.yrmuq.shinpi\nBranch=stable\nTitle=shinpi\nUrl=https://yrmuq.github.io/shinpi/\nSuggestRemoteName=shinpi\nGPGKey={key}\nRuntimeRepo=https://dl.flathub.org/repo/flathub.flatpakrepo\nIsRuntime=false\n"
    ))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn readings_require_valid_json_and_both_arrays() {
        assert!(readings(r#"{"devices":[],"warnings":[]}"#).is_ok());
        for input in [
            "invalid",
            "{}",
            r#"{"devices":{},"warnings":[]}"#,
            r#"{"devices":[],"warnings":null}"#,
        ] {
            assert!(readings(input).is_err(), "{input}");
        }
    }
}
