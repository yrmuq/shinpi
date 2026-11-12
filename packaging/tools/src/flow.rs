// Copyright (C) 2026 yrmuq
// SPDX-License-Identifier: GPL-3.0-or-later

use crate::{notes, pins, Context, Result};
use regex::Regex;
use std::fs::{self, OpenOptions};
use std::io::Write;

pub fn parse_version(value: &str) -> Result<[u64; 3]> {
    if !Regex::new(r"^[0-9]+\.[0-9]+\.[0-9]+$")
        .unwrap()
        .is_match(value)
    {
        return Err("Package version must be X.Y.Z".into());
    }
    let parts = value
        .split('.')
        .map(str::parse)
        .collect::<std::result::Result<Vec<_>, _>>()?;
    Ok(parts
        .try_into()
        .map_err(|_| "Package version must be X.Y.Z")?)
}

pub fn version(context: &Context) -> Result<[u64; 3]> {
    let manifest: toml::Value =
        toml::from_str(&fs::read_to_string(context.root.join("Cargo.toml"))?)?;
    parse_version(
        manifest["package"]["version"]
            .as_str()
            .ok_or("Missing package version")?,
    )
}

pub fn bump(context: &Context, value: &str) -> Result<()> {
    parse_version(value)?;
    let cargo = context.root.join("Cargo.toml");
    let lock = context.root.join("Cargo.lock");
    let manifest = fs::read_to_string(&cargo)?;
    let lockfile = fs::read_to_string(&lock)?;
    let pattern = Regex::new(r#"(?m)^(version = ")[^"\n]+(")"#)?;
    let locked = Regex::new(r#"(\[\[package\]\]\nname = "shinpi"\nversion = ")[^"\n]+(")"#)?;
    if !pattern.is_match(&manifest) || locked.captures_iter(&lockfile).count() != 1 {
        return Err("Expected one shinpi package in Cargo.toml and Cargo.lock".into());
    }
    let replacement = format!("${{1}}{value}${{2}}");
    let cargo_new = pattern.replacen(&manifest, 1, replacement.as_str());
    let lock_new = locked.replace(&lockfile, replacement.as_str());
    fs::write(cargo, cargo_new.as_bytes())?;
    fs::write(lock, lock_new.as_bytes())?;
    Ok(())
}

fn output(values: &[(&str, &str)]) -> Result<()> {
    if let Some(path) = std::env::var_os("GITHUB_OUTPUT") {
        let mut file = OpenOptions::new().create(true).append(true).open(path)?;
        for (key, value) in values {
            writeln!(file, "{key}={value}")?;
        }
    }
    Ok(())
}

fn ancestor(context: &Context, first: &str, second: &str) -> Result<bool> {
    context.succeeds("git", &["merge-base", "--is-ancestor", first, second])
}

fn fetch_deps(context: &Context) -> Result<String> {
    let remote = context.git(&["ls-remote", "origin", "refs/heads/deps"])?;
    let sha = remote.split_whitespace().next().unwrap_or("").to_owned();
    if !sha.is_empty() {
        context.git(&[
            "fetch",
            "origin",
            "refs/heads/deps:refs/remotes/origin/deps",
        ])?;
    }
    Ok(sha)
}

fn merge_main(context: &Context) -> Result<()> {
    if ancestor(context, "origin/main", "HEAD")? {
        return Ok(());
    }
    if ancestor(context, "HEAD", "origin/main")? {
        context.git(&["merge", "--ff-only", "origin/main"])?;
        return Ok(());
    }
    let manifest: toml::Value = toml::from_str(&context.git(&["show", "origin/main:Cargo.toml"])?)?;
    let value = manifest["package"]["version"]
        .as_str()
        .ok_or("Missing main version")?;
    // Normalize only the application version; preserve dependency edits in both files.
    bump(context, value)?;
    context.git(&["add", "Cargo.toml", "Cargo.lock"])?;
    if !context.succeeds("git", &["diff", "--cached", "--quiet"])? {
        context.git(&["commit", "-m", "Keep the main application version on deps"])?;
    }
    if let Err(error) = context.git(&[
        "merge",
        "--no-ff",
        "origin/main",
        "-m",
        "Merge main into deps",
    ]) {
        context.git(&["merge", "--abort"])?;
        return Err(format!("Dependency synchronization stopped on a conflict: {error}").into());
    }
    Ok(())
}

pub fn sync_deps(context: &Context) -> Result<()> {
    context.git(&[
        "fetch",
        "origin",
        "refs/heads/main:refs/remotes/origin/main",
    ])?;
    let previous = fetch_deps(context)?;
    context.git(&[
        "checkout",
        "--detach",
        if previous.is_empty() {
            "origin/main"
        } else {
            &previous
        },
    ])?;
    merge_main(context)?;
    context.git(&["push", "origin", "HEAD:refs/heads/deps"])?;
    Ok(())
}

pub fn prepare(context: &Context, mode: &str) -> Result<()> {
    if !["release", "deps", "pins"].contains(&mode) {
        return Err("Unknown release mode".into());
    }
    context.git(&[
        "fetch",
        "origin",
        "refs/heads/main:refs/remotes/origin/main",
        "refs/heads/dev:refs/remotes/origin/dev",
        "--tags",
    ])?;
    context.git(&["config", "user.name", "github-actions[bot]"])?;
    context.git(&[
        "config",
        "user.email",
        "41898282+github-actions[bot]@users.noreply.github.com",
    ])?;
    let mut old_deps = String::new();
    let (value, body) = if mode == "release" {
        let parents = context.git(&["rev-list", "--parents", "-n", "1", "HEAD"])?;
        let parents: Vec<_> = parents.split_whitespace().collect();
        if parents.len() != 3
            || context.git(&["log", "-1", "--format=%s"])? != "Merge branch 'dev'"
            || !ancestor(context, parents[2], "origin/dev")?
        {
            return output(&[("ready", "false")]);
        }
        if !fetch_deps(context)?.is_empty() && !ancestor(context, "origin/deps", "origin/dev")? {
            return Err("Release blocked: merge deps into dev and test those updates first".into());
        }
        let [major, minor, _] = version(context)?;
        let stable = context.git(&[
            "tag",
            "--merged",
            "HEAD",
            "--list",
            "v*.*.0",
            "--sort=-version:refname",
        ])?;
        let range = stable
            .lines()
            .next()
            .map(|base| format!("{base}..HEAD"))
            .unwrap_or("HEAD".to_owned());
        let subjects = context.git(&["log", "--format=%s", &range])?;
        (
            format!(
                "{major}.{}.0",
                minor.checked_add(1).ok_or("Version overflow")?
            ),
            notes::commits(&subjects),
        )
    } else {
        let candidate = context.git(&["rev-parse", "HEAD"])?;
        old_deps = fetch_deps(context)?;
        let base = if old_deps.is_empty() {
            "origin/main"
        } else {
            old_deps.as_str()
        };
        context.git(&["checkout", "--detach", base])?;
        merge_main(context)?;
        if mode == "deps" {
            context.git(&[
                "merge",
                "--no-ff",
                &candidate,
                "-m",
                "Merge dependency updates",
            ])?;
        } else {
            if !pins::update(context)? {
                return output(&[("ready", "false")]);
            }
            context.git(&["add", "packaging/appimage/build.sh"])?;
            context.git(&["commit", "-m", "Update the pinned appimagetool"])?;
        }
        let [major, minor, mut patch] = version(context)?;
        for tag in context
            .git(&["tag", "--list", &format!("v{major}.{minor}.*")])?
            .lines()
        {
            if let Ok([tag_major, tag_minor, tag_patch]) =
                parse_version(tag.trim_start_matches('v'))
            {
                if tag_major == major && tag_minor == minor {
                    patch = patch.max(tag_patch);
                }
            }
        }
        let diff = context.git(&[
            "diff",
            "origin/main",
            "--",
            "Cargo.toml",
            "Cargo.lock",
            ".github",
            "packaging/appimage/build.sh",
        ])?;
        (
            format!(
                "{major}.{minor}.{}",
                patch.checked_add(1).ok_or("Version overflow")?
            ),
            notes::deps(&diff),
        )
    };
    if body.trim().is_empty() {
        return Err("Release notes are empty".into());
    }
    let tag = format!("v{value}");
    if context.succeeds(
        "git",
        &["rev-parse", "--verify", &format!("refs/tags/{tag}")],
    )? {
        return Err(format!("Tag {tag} already exists").into());
    }
    fs::write(context.root.join("notes.md"), body)?;
    bump(context, &value)?;
    context.git(&["add", "Cargo.toml", "Cargo.lock"])?;
    context.git(&["commit", "-m", &format!("Version {value}")])?;
    context.git(&["tag", "-a", &tag, "-F", "notes.md"])?;
    output(&[
        ("ready", "true"),
        ("version", &value),
        ("old_deps", &old_deps),
    ])
}

pub fn publish(context: &Context, mode: &str, value: &str, old_deps: &str) -> Result<()> {
    if !["release", "deps", "pins"].contains(&mode) {
        return Err("Unknown release mode".into());
    }
    if version(context)? != parse_version(value)? {
        return Err("Build version does not match the release".into());
    }
    let tag = format!("v{value}");
    if context.git(&["rev-parse", &format!("{tag}^{{commit}}")])?
        != context.git(&["rev-parse", "HEAD"])?
    {
        return Err("Tag does not match the build commit".into());
    }
    let tag_ref = format!("refs/tags/{tag}");
    if mode == "release" {
        context.git(&[
            "push",
            "--atomic",
            "origin",
            "HEAD:refs/heads/main",
            &tag_ref,
        ])?;
    } else {
        // The lease rejects concurrent writers; the prepared history retains old deps.
        context.git(&[
            "push",
            "--atomic",
            &format!("--force-with-lease=refs/heads/deps:{old_deps}"),
            "origin",
            "HEAD:refs/heads/deps",
            &tag_ref,
        ])?;
    }
    let mut assets = Vec::new();
    for entry in fs::read_dir(context.root.join("dist"))? {
        let entry = entry?;
        if entry.file_type()?.is_file() {
            assets.push(format!(
                "dist/{}",
                entry.file_name().to_str().ok_or("Invalid asset name")?
            ));
        }
    }
    assets.sort();
    let repo = std::env::var("GITHUB_REPOSITORY")?;
    let title = format!("shinpi {value}");
    let mut args = vec!["release", "create", tag.as_str()];
    args.extend(assets.iter().map(String::as_str));
    args.extend([
        "--repo",
        &repo,
        "--verify-tag",
        "--title",
        &title,
        "--notes-file",
        "notes.md",
    ]);
    if mode == "release" {
        args.push("--latest");
    } else {
        args.extend(["--prerelease", "--latest=false"]);
    }
    context.run("gh", &args)?;
    Ok(())
}

pub fn sync_dev(context: &Context) -> Result<()> {
    let released = context.git(&["rev-parse", "HEAD"])?;
    context.git(&["fetch", "origin", "refs/heads/dev:refs/remotes/origin/dev"])?;
    if ancestor(context, &released, "origin/dev")? {
        return Ok(());
    }
    context.git(&["checkout", "--detach", "origin/dev"])?;
    context.git(&[
        "merge",
        "--no-ff",
        &released,
        "-m",
        "Merge released main into dev",
    ])?;
    context.git(&["push", "origin", "HEAD:refs/heads/dev"])?;
    // GITHUB_TOKEN pushes do not trigger push workflows.
    context.run(
        "gh",
        &[
            "workflow",
            "run",
            "dev.yml",
            "--repo",
            &std::env::var("GITHUB_REPOSITORY")?,
            "--ref",
            "dev",
        ],
    )?;
    Ok(())
}
