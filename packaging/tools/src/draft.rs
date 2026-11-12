// Copyright (C) 2026 yrmuq
// SPDX-License-Identifier: GPL-3.0-or-later

use crate::{string, Context, Result};
use serde_json::{json, Value};

pub fn needed(context: &Context) -> Result<()> {
    let repo = std::env::var("GITHUB_REPOSITORY")?;
    let sha = std::env::var("GITHUB_SHA")?;
    let target = if std::env::var("INCLUDE_FLATPAK").as_deref() == Ok("true") {
        "appimage-flatpak"
    } else {
        "appimage"
    };
    let mut built = false;
    for page in 1.. {
        let runs = context.api(&format!("repos/{repo}/actions/workflows/dev.yml/runs?head_sha={sha}&status=success&per_page=100&page={page}"), "GET", None)?;
        let runs = runs["workflow_runs"]
            .as_array()
            .ok_or("Expected workflow runs")?;
        built |= runs.iter().any(|run| {
            run["head_sha"] == sha
                && run["conclusion"] == "success"
                && run["display_title"]
                    .as_str()
                    .is_some_and(|title| title.ends_with(&format!(" / {target}")))
        });
        if built || runs.len() < 100 {
            break;
        }
    }
    if let Some(path) = std::env::var_os("GITHUB_OUTPUT") {
        use std::io::Write;
        writeln!(
            std::fs::OpenOptions::new()
                .create(true)
                .append(true)
                .open(path)?,
            "needed={}",
            !built
        )?;
    }
    Ok(())
}

pub fn update(context: &Context) -> Result<()> {
    let repo = std::env::var("GITHUB_REPOSITORY")?;
    let commit = std::env::var("GITHUB_SHA")?;
    let current = context.api(&format!("repos/{repo}/git/ref/heads/dev"), "GET", None)?;
    if current["object"]["sha"] != commit {
        println!("Skipping a superseded development build");
        return Ok(());
    }
    let mut assets = vec![
        "dist/shinpi-x86_64.AppImage",
        "dist/shinpi-x86_64.AppImage.sha256",
        "dist/shinpi-x86_64.AppImage.zsync",
    ];
    if std::env::var("INCLUDE_FLATPAK").as_deref() == Ok("true") {
        assets.extend([
            "dist/shinpi-x86_64.flatpak",
            "dist/shinpi-x86_64.flatpak.sha256",
        ]);
    }
    for path in &assets {
        if !context.root.join(path).is_file() {
            return Err(format!("Missing build asset: {path}").into());
        }
    }
    let dist = Context {
        root: context.root.join("dist"),
    };
    for asset in &assets {
        if asset.ends_with(".sha256") {
            dist.run("sha256sum", &["-c", asset.strip_prefix("dist/").unwrap()])?;
        }
    }
    let mut releases = Vec::new();
    for page in 1.. {
        let batch = context.api(
            &format!("repos/{repo}/releases?per_page=100&page={page}"),
            "GET",
            None,
        )?;
        let batch = batch.as_array().ok_or("Expected a release list")?;
        releases.extend(batch.iter().cloned());
        if batch.len() < 100 {
            break;
        }
    }
    let existing = releases.iter().find(|release| release["tag_name"] == "dev");
    if existing.is_some_and(|release| release["draft"] != true) {
        return Err("The dev release is published; refusing to change it".into());
    }
    let payload = json!({"tag_name":"dev", "target_commitish":commit, "name":"shinpi dev", "draft":true, "prerelease":true, "body":""});
    let release = if let Some(existing) = existing {
        context.api(
            &format!("repos/{repo}/releases/{}", id(&existing["id"])?),
            "PATCH",
            Some(&payload),
        )?
    } else {
        context.api(&format!("repos/{repo}/releases"), "POST", Some(&payload))?
    };
    let mut args = vec!["release", "upload", "dev"];
    args.extend(assets.iter().copied());
    args.extend(["--repo", &repo, "--clobber"]);
    context.run("gh", &args)?;
    // Ordinary builds remove optional packages left by a previous test build.
    let previous = context.api(
        &format!("repos/{repo}/releases/{}/assets", id(&release["id"])?),
        "GET",
        None,
    )?;
    for asset in previous.as_array().ok_or("Expected a release asset list")? {
        let name = string(&asset["name"])?;
        if !assets
            .iter()
            .any(|path| path.strip_prefix("dist/") == Some(name))
        {
            context.api(
                &format!("repos/{repo}/releases/assets/{}", id(&asset["id"])?),
                "DELETE",
                None,
            )?;
        }
    }
    println!("{}", string(&release["html_url"])?);
    Ok(())
}

fn id(value: &Value) -> Result<u64> {
    value
        .as_u64()
        .ok_or_else(|| "Expected a release or asset ID".into())
}
