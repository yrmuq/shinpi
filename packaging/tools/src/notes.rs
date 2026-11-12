// Copyright (C) 2026 yrmuq
// SPDX-License-Identifier: GPL-3.0-or-later

use regex::Regex;
use std::collections::{BTreeMap, BTreeSet};

pub fn commits(subjects: &str) -> String {
    let sections = ["Added", "Changed", "Fixed", "Removed", "Security"];
    let mut buckets = vec![Vec::new(); sections.len()];
    for subject in subjects.lines().map(str::trim) {
        let lower = subject.to_lowercase();
        if lower.starts_with("merge ")
            || lower.starts_with("version ")
            || ["chmod ", "./shinpi", "Test build"]
                .iter()
                .any(|part| subject.contains(part))
        {
            continue;
        }
        let word = lower
            .split(|c: char| !c.is_alphanumeric() && c != '_')
            .next()
            .unwrap_or("");
        let section = match word {
            "add" | "implement" => 0,
            "fix" | "resolve" | "prevent" => 2,
            "remove" => 3,
            "redact" | "restrict" => 4,
            _ => 1,
        };
        buckets[section].push(format!(
            "- {subject}{}\n",
            if subject.ends_with('.') { "" } else { "." }
        ));
    }
    sections
        .iter()
        .zip(buckets)
        .filter(|(_, entries)| !entries.is_empty())
        .map(|(name, entries)| format!("### {name}\n{}", entries.concat()))
        .collect::<Vec<_>>()
        .join("\n")
}

pub fn deps(diff: &str) -> String {
    let mut pairs = BTreeSet::new();
    for block in diff.split("diff --git ").skip(1) {
        let Some((header, body)) = block.split_once('\n') else {
            continue;
        };
        let Some((_, path)) = header.split_once(" b/") else {
            continue;
        };
        if path.ends_with("Cargo.lock") {
            pairs.extend(lock_pairs(body));
        } else {
            let minus = changed_lines(body, '-');
            let plus = changed_lines(body, '+');
            if path.ends_with(".yml") || path.ends_with(".yaml") {
                let old = actions(&minus);
                let new = actions(&plus);
                for (name, before) in old {
                    if let Some(after) = new.get(&name).filter(|after| **after != before) {
                        pairs.insert((name, before, after.clone()));
                    }
                }
            } else if path == "packaging/appimage/build.sh" {
                if let (Some(old), Some(new)) = (tool(&minus), tool(&plus)) {
                    if old != new {
                        pairs.insert(("appimagetool".to_owned(), old, new));
                    }
                }
            }
        }
    }
    if pairs.is_empty() {
        return String::new();
    }
    let mut out = "Dependency updates.\n".to_owned();
    for (name, old, new) in pairs {
        out.push_str(&format!("- `{name}` {old} -> {new}\n"));
    }
    out
}

fn changed_lines(body: &str, prefix: char) -> String {
    body.lines()
        .filter_map(|line| line.strip_prefix(prefix))
        .collect::<Vec<_>>()
        .join("\n")
}

fn actions(body: &str) -> BTreeMap<String, String> {
    Regex::new(r"uses:\s*([A-Za-z0-9_.-]+/[A-Za-z0-9_.-]+)@(\S+)")
        .unwrap()
        .captures_iter(body)
        .map(|found| (found[1].to_owned(), found[2].to_owned()))
        .collect()
}

fn tool(body: &str) -> Option<String> {
    Regex::new(r#"download/([^/\s"]+)/appimagetool"#)
        .unwrap()
        .captures(body)
        .map(|found| found[1].to_owned())
}

fn lock_pairs(body: &str) -> Vec<(String, String, String)> {
    let field = Regex::new(r#"^(name|version) = "([^"]+)""#).unwrap();
    let mut pairs = Vec::new();
    let (mut name, mut old, mut new) = (String::new(), String::new(), String::new());
    let flush = |pairs: &mut Vec<_>, name: &mut String, old: &mut String, new: &mut String| {
        if !name.is_empty() && name != "shinpi" && !old.is_empty() && !new.is_empty() && old != new
        {
            pairs.push((name.clone(), old.clone(), new.clone()));
        }
        name.clear();
        old.clear();
        new.clear();
    };
    for line in body.lines() {
        if ["---", "+++", "@@", "index ", "new file", "deleted file"]
            .iter()
            .any(|prefix| line.starts_with(prefix))
        {
            continue;
        }
        let Some(prefix) = line.chars().next().filter(|c| "+- ".contains(*c)) else {
            continue;
        };
        let line = line[1..].trim();
        if line == "[[package]]" {
            flush(&mut pairs, &mut name, &mut old, &mut new);
        } else if let Some(found) = field.captures(line) {
            if &found[1] == "name" {
                if !name.is_empty() && name != found[2] {
                    flush(&mut pairs, &mut name, &mut old, &mut new);
                }
                name = found[2].to_owned();
            } else if !name.is_empty() {
                match prefix {
                    '-' => old = found[2].to_owned(),
                    '+' => new = found[2].to_owned(),
                    _ => {}
                }
            }
        }
    }
    flush(&mut pairs, &mut name, &mut old, &mut new);
    pairs
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn notes_retain_sections_and_dependency_versions() {
        let body =
            commits("Add history\nFix focus\nRemove duplicate\nVersion 0.1.1\nMerge branch 'dev'");
        assert!(body.contains("### Added\n- Add history."));
        assert!(body.contains("### Fixed\n- Fix focus."));
        assert!(body.contains("### Removed\n- Remove duplicate."));
        assert!(!body.contains("Version"));
        let diff = "diff --git a/Cargo.lock b/Cargo.lock\n [[package]]\n name = \"example\"\n-version = \"1.0.0\"\n+version = \"2.0.0\"\ndiff --git a/a.yml b/a.yml\n-uses: actions/checkout@v4\n+uses: actions/checkout@v5\ndiff --git a/packaging/appimage/build.sh b/packaging/appimage/build.sh\n-TOOL_URL=\"https://github.com/AppImage/appimagetool/releases/download/1.0/appimagetool-x86_64.AppImage\"\n+TOOL_URL=\"https://github.com/AppImage/appimagetool/releases/download/2.0/appimagetool-x86_64.AppImage\"\n";
        let body = deps(diff);
        assert!(body.contains("`example` 1.0.0 -> 2.0.0"));
        assert!(body.contains("`actions/checkout` v4 -> v5"));
        assert!(body.contains("`appimagetool` 1.0 -> 2.0"));
    }
}
