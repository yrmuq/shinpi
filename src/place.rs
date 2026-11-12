// Copyright (C) 2026 yrmuq
// SPDX-License-Identifier: GPL-3.0-or-later

use std::fs;
use std::path::PathBuf;

const SCRIPT: &str = include_str!("../kwin/placer.js");

pub async fn install() -> bool {
    let Ok(conn) = zbus::Connection::session().await else {
        return false;
    };
    let Ok(bus) = zbus::fdo::DBusProxy::new(&conn).await else {
        return false;
    };
    if !bus
        .name_has_owner("org.kde.KWin".try_into().unwrap())
        .await
        .unwrap_or(false)
    {
        return false;
    }
    let cache = std::env::var_os("XDG_CACHE_HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| crate::config::home().join(".cache"));
    let folder = cache.join("shinpi");
    if fs::create_dir_all(&folder).is_err() {
        return false;
    }
    let path = folder.join("placer.js");
    if fs::write(&path, SCRIPT).is_err() {
        return false;
    }
    let _ = conn
        .call_method(
            Some("org.kde.KWin"),
            "/Scripting",
            Some("org.kde.kwin.Scripting"),
            "unloadScript",
            &("shinpi-place",),
        )
        .await;
    let rules_changed = write_taskbar_rule();
    if rules_changed {
        let _ = conn
            .call_method(
                Some("org.kde.KWin"),
                "/KWin",
                Some("org.kde.KWin"),
                "reconfigure",
                &(),
            )
            .await;
    }
    let reply = conn
        .call_method(
            Some("org.kde.KWin"),
            "/Scripting",
            Some("org.kde.kwin.Scripting"),
            "loadScript",
            &(path.display().to_string(), "shinpi-place"),
        )
        .await;
    let Ok(reply) = reply else { return false };
    let Ok(id) = reply.body().deserialize::<i32>() else {
        return false;
    };
    let object = format!("/Scripting/Script{id}");
    let ran = conn
        .call_method(
            Some("org.kde.KWin"),
            object.as_str(),
            Some("org.kde.kwin.Script"),
            "run",
            &(),
        )
        .await
        .is_ok();
    ran
}

const TASKBAR_RULE: &str = "\
[shinpi-tray]
Description=shinpi tray popup
skiptaskbar=true
skiptaskbarrule=2
skippager=true
skippagerrule=2
skipswitcher=true
skipswitcherrule=2
wmclass=shinpi-popup
wmclassmatch=1
wmclasscomplete=false
";

/// KWin applies window rules while the window is created. The popup class is
/// set before mapping, before a title exists, so the task manager never sees it.
fn write_taskbar_rule() -> bool {
    let path = crate::config::config_dir()
        .parent()
        .unwrap()
        .join("kwinrulesrc");
    let previous = match fs::read_to_string(&path) {
        Ok(text) => text,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => String::new(),
        Err(_) => return false,
    };
    let Some(next) = merge_kwin_rules(&previous) else {
        return false;
    };
    if let Some(parent) = path.parent() {
        let _ = fs::create_dir_all(parent);
    }
    let temporary = path.with_extension("shinpi.tmp");
    if fs::write(&temporary, next).is_err() {
        return false;
    }
    if let Ok(metadata) = fs::metadata(&path) {
        if fs::set_permissions(&temporary, metadata.permissions()).is_err() {
            return false;
        }
    }
    fs::rename(temporary, path).is_ok()
}

fn rewrite_tray_rule(text: &str) -> String {
    let Some(start) = text.find("[shinpi-tray]") else {
        return text.to_string();
    };
    let after = &text[start + "[shinpi-tray]".len()..];
    let end = after
        .find("\n[")
        .map(|index| start + "[shinpi-tray]".len() + index + 1)
        .unwrap_or(text.len());
    let mut out = String::new();
    out.push_str(&text[..start]);
    out.push_str(TASKBAR_RULE);
    if !out.ends_with('\n') {
        out.push('\n');
    }
    out.push_str(text[end..].trim_start_matches('\n'));
    if !out.ends_with('\n') {
        out.push('\n');
    }
    out
}
fn merge_kwin_rules(text: &str) -> Option<String> {
    if text.contains("[shinpi-tray]") {
        if text.contains("wmclass=shinpi-popup\n")
            && text.contains("wmclassmatch=1\n")
            && !text.contains("title=")
        {
            return None;
        }
        return Some(rewrite_tray_rule(text));
    }
    let mut out = String::new();
    let mut in_general = false;
    let mut saw_general = false;
    let mut saw_rules = false;
    for line in text.split_inclusive('\n') {
        let trimmed = line.trim().trim_end_matches('\r');
        if trimmed.starts_with('[') && trimmed.ends_with(']') {
            if in_general && !saw_rules {
                out.push_str("rules=shinpi-tray\n");
                saw_rules = true;
            }
            in_general = trimmed == "[General]";
            saw_general |= in_general;
            out.push_str(line);
            continue;
        }
        if in_general {
            if let Some(rest) = trimmed.strip_prefix("rules=") {
                let rest = rest.trim();
                let value = if rest.is_empty() {
                    "shinpi-tray".to_string()
                } else {
                    format!("{rest},shinpi-tray")
                };
                out.push_str("rules=");
                out.push_str(&value);
                out.push('\n');
                saw_rules = true;
                continue;
            }
            if let Some(rest) = trimmed.strip_prefix("count=") {
                if let Ok(count) = rest.trim().parse::<u32>() {
                    out.push_str(&format!("count={}\n", count + 1));
                    continue;
                }
            }
        }
        out.push_str(line);
    }
    if in_general && !saw_rules {
        if !out.ends_with('\n') {
            out.push('\n');
        }
        out.push_str("rules=shinpi-tray\n");
    }
    if !saw_general {
        if !out.is_empty() && !out.ends_with('\n') {
            out.push('\n');
        }
        out.push_str("[General]\nrules=shinpi-tray\ncount=1\n");
    }
    if !out.ends_with('\n') {
        out.push('\n');
    }
    out.push('\n');
    out.push_str(TASKBAR_RULE);
    Some(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn kwin_rule_is_inserted_once() {
        let first = merge_kwin_rules("[General]\nrules=1\ncount=1\n\n[1]\nwmclass=kate\n").unwrap();
        assert!(first.contains("rules=1,shinpi-tray\n"));
        assert!(first.contains("count=2\n"));
        assert!(first.contains("skiptaskbar=true\n"));
        assert!(first.contains("wmclass=shinpi-popup\n"));
        assert!(first.contains("wmclassmatch=1\n"));
        assert!(!first.contains("title="));
        assert!(merge_kwin_rules(&first).is_none());
        let stale = "[General]\nrules=shinpi-tray\ncount=1\n\n[shinpi-tray]\nwmclass=shinpi\nskiptaskbar=true\n";
        let updated = merge_kwin_rules(stale).unwrap();
        assert!(updated.contains("wmclass=shinpi-popup\n"));
        assert!(!updated.contains("wmclass=shinpi\n"));
        assert!(merge_kwin_rules(&updated).is_none());
        let old = "[General]\nrules=shinpi-tray\ncount=1\n\n[shinpi-tray]\nDescription=shinpi tray popup\ntitle=shinpi-(devices|notices)\ntitlematch=3\nwmclass=shinpi\nwmclassmatch=1\n";
        let fixed = merge_kwin_rules(old).unwrap();
        assert!(fixed.contains("wmclass=shinpi-popup\n"));
        assert!(!fixed.contains("title="));
        assert!(merge_kwin_rules(&fixed).is_none());
        let empty = merge_kwin_rules("").unwrap();
        assert!(empty.contains("[shinpi-tray]\n"));
        assert!(empty.contains("rules=shinpi-tray\n"));
    }
}
