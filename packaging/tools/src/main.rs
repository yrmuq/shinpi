// Copyright (C) 2026 yrmuq
// SPDX-License-Identifier: GPL-3.0-or-later

use shinpi_tools::{checks, draft, flow, notes, pins, Context, Result};
use std::io::{self, Read};
use std::path::Path;

fn run() -> Result<()> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let args: Vec<&str> = args.iter().map(String::as_str).collect();
    let context = Context {
        root: std::env::current_dir()?,
    };
    match args.as_slice() {
        ["prepare", mode] => flow::prepare(&context, mode),
        ["publish", mode, version] => flow::publish(&context, mode, version, ""),
        ["publish", mode, version, old_deps] => flow::publish(&context, mode, version, old_deps),
        ["sync-dev"] => flow::sync_dev(&context),
        ["sync-deps"] => flow::sync_deps(&context),
        ["dev-needed"] => draft::needed(&context),
        ["draft"] => draft::update(&context),
        ["pin-tool"] => { pins::update(&context)?; Ok(()) },
        ["check-zsync", path] => checks::zsync(&context, Path::new(path)),
        ["flatpakref"] => checks::flatpakref(&context),
        ["check-readings"] => checks::readings(&stdin()?),
        ["notes", mode @ ("deps" | "commits")] => {
            let input = stdin()?;
            let body = if *mode == "deps" { notes::deps(&input) } else { notes::commits(&input) };
            if body.is_empty() { return Err("Release notes are empty".into()); }
            print!("{body}");
            Ok(())
        }
        _ => Err("Expected prepare, publish, sync-dev, draft, pin-tool, check-zsync, flatpakref, check-readings or notes".into()),
    }
}

fn stdin() -> Result<String> {
    let mut input = String::new();
    io::stdin().read_to_string(&mut input)?;
    Ok(input)
}

fn main() {
    if let Err(error) = run() {
        eprintln!("{error}");
        std::process::exit(1);
    }
}
