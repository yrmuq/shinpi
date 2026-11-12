// Copyright (C) 2026 yrmuq
// SPDX-License-Identifier: GPL-3.0-or-later

use crate::{string, Context, Result};
use regex::Regex;
use serde_json::Value;
use std::fs;

pub fn update(context: &Context) -> Result<bool> {
    let release: Value = serde_json::from_str(&context.run(
        "curl",
        &[
            "--fail",
            "--location",
            "--max-time",
            "30",
            "--header",
            "Accept: application/vnd.github+json",
            "--user-agent",
            "shinpi-pins",
            "https://api.github.com/repos/AppImage/appimagetool/releases/latest",
        ],
    )?)?;
    let asset = release["assets"]
        .as_array()
        .ok_or("Missing release assets")?
        .iter()
        .find(|asset| asset["name"] == "appimagetool-x86_64.AppImage")
        .ok_or("Missing appimagetool asset")?;
    let url = string(&asset["browser_download_url"])?;
    if !Regex::new(r"^https://github\.com/AppImage/appimagetool/releases/download/[^/\s]+/appimagetool-x86_64\.AppImage$")?.is_match(url) {
        return Err("Unexpected appimagetool download URL".into());
    }
    let path = context.root.join("packaging/appimage/build.sh");
    let previous = fs::read_to_string(&path)?;
    let temporary = context
        .root
        .join("packaging/appimage/appimagetool.pin-download");
    // Remove the download even when validation fails.
    let result = (|| {
        context.run(
            "curl",
            &[
                "--fail",
                "--location",
                "--max-time",
                "60",
                "--output",
                temporary.to_str().ok_or("Invalid download path")?,
                url,
            ],
        )?;
        let digest = context.run(
            "sha256sum",
            &[temporary.to_str().ok_or("Invalid download path")?],
        )?;
        let digest = digest
            .split_whitespace()
            .next()
            .ok_or("Missing tool checksum")?;
        if asset
            .get("digest")
            .is_some_and(|value| !value.is_null() && value != "")
            && asset["digest"] != format!("sha256:{digest}")
        {
            return Err("appimagetool checksum does not match GitHub's digest".into());
        }
        replace_pin(&previous, url, digest)
    })();
    let _ = fs::remove_file(temporary);
    let next = result?;
    if next == previous {
        return Ok(false);
    }
    fs::write(path, next)?;
    Ok(true)
}

fn replace_pin(text: &str, url: &str, digest: &str) -> Result<String> {
    let urls = Regex::new(r#"(?m)^TOOL_URL="[^"]+"$"#)?;
    let hashes = Regex::new(r#"(?m)^TOOL_SHA="[a-f0-9]+"$"#)?;
    if urls.captures_iter(text).count() != 1 || hashes.captures_iter(text).count() != 1 {
        return Err("Expected exactly one appimagetool pin".into());
    }
    let text = urls.replace(text, regex::NoExpand(&format!("TOOL_URL=\"{url}\"")));
    Ok(hashes
        .replace(&text, regex::NoExpand(&format!("TOOL_SHA=\"{digest}\"")))
        .into_owned())
}
