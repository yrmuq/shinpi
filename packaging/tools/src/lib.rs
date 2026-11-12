// Copyright (C) 2026 yrmuq
// SPDX-License-Identifier: GPL-3.0-or-later

pub mod checks;
pub mod draft;
pub mod flow;
pub mod notes;
pub mod pins;

use serde_json::Value;
use std::io::Write;
use std::path::PathBuf;
use std::process::{Command, Output, Stdio};

pub type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;

pub struct Context {
    pub root: PathBuf,
}

impl Context {
    pub fn command(&self, program: &str, args: &[&str]) -> Command {
        let mut command = Command::new(program);
        command.args(args).current_dir(&self.root);
        command
    }

    pub fn run(&self, program: &str, args: &[&str]) -> Result<String> {
        checked(self.command(program, args).output()?)
    }

    pub fn git(&self, args: &[&str]) -> Result<String> {
        Ok(self.run("git", args)?.trim().to_owned())
    }

    pub fn succeeds(&self, program: &str, args: &[&str]) -> Result<bool> {
        Ok(self.command(program, args).output()?.status.success())
    }

    pub fn api(&self, path: &str, method: &str, payload: Option<&Value>) -> Result<Value> {
        let mut command = self.command("gh", &["api", path, "--method", method]);
        command.stdout(Stdio::piped()).stderr(Stdio::piped());
        let output = if let Some(payload) = payload {
            let mut child = command
                .args(["--input", "-"])
                .stdin(Stdio::piped())
                .spawn()?;
            child
                .stdin
                .take()
                .ok_or("Missing API input pipe")?
                .write_all(serde_json::to_string(payload)?.as_bytes())?;
            child.wait_with_output()?
        } else {
            command.output()?
        };
        let body = checked(output)?;
        Ok(if body.trim().is_empty() {
            Value::Null
        } else {
            serde_json::from_str(&body)?
        })
    }
}

fn checked(output: Output) -> Result<String> {
    if !output.status.success() {
        return Err(format!(
            "Command failed ({}): {}",
            output.status,
            String::from_utf8_lossy(&output.stderr)
        )
        .into());
    }
    Ok(String::from_utf8(output.stdout)?)
}

pub fn string(value: &Value) -> Result<&str> {
    value
        .as_str()
        .ok_or_else(|| "Expected a JSON string".into())
}
