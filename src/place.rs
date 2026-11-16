// Copyright (C) 2026 yrmuq
// SPDX-License-Identifier: GPL-3.0-or-later

use std::fs;
use std::io::{self, Write};
use std::os::unix::fs::OpenOptionsExt;
use std::path::{Path, PathBuf};
use std::time::Duration;

const SCRIPT: &str = include_str!("../kwin/placer.js");

struct RuleReload(tokio::sync::mpsc::UnboundedSender<()>);

#[zbus::interface(name = "io.github.yrmuq.shinpi.Placer")]
impl RuleReload {
    fn ready(&self) {
        let _ = self.0.send(());
    }
}

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
    let (ready, mut reloaded) = tokio::sync::mpsc::unbounded_channel();
    if conn
        .object_server()
        .at("/shinpi/Placer", RuleReload(ready))
        .await
        .is_err()
    {
        return false;
    }
    let Some(destination) = conn.unique_name() else {
        return false;
    };
    let destination = serde_json::to_string(destination.as_str()).unwrap();
    let script = format!("const shinpiReadyBus = {destination};\n{SCRIPT}");
    if fs::write(&path, script).is_err() {
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
    let name_path = folder.join("placer.name");
    if let Ok(previous) = fs::read_to_string(&name_path) {
        let previous = previous.trim();
        if previous.strip_prefix("shinpiplace").is_some_and(|suffix| {
            !suffix.is_empty() && suffix.chars().all(|c| c.is_ascii_digit() || c == '_')
        }) {
            let _ = conn
                .call_method(
                    Some("org.kde.KWin"),
                    "/Scripting",
                    Some("org.kde.kwin.Scripting"),
                    "unloadScript",
                    &(previous,),
                )
                .await;
        }
    }
    // A new name avoids reusing a script that KWin is still deleting.
    let stamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    let name = format!("shinpiplace{}_{stamp}", std::process::id());
    if !write_taskbar_rule() {
        return false;
    }
    let reply = conn
        .call_method(
            Some("org.kde.KWin"),
            "/Scripting",
            Some("org.kde.kwin.Scripting"),
            "loadScript",
            &(path.display().to_string(), name.as_str()),
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
    if ran {
        let _ = fs::write(name_path, name);
    }
    if !ran
        || conn
            .call_method(
                Some("org.kde.KWin"),
                "/KWin",
                Some("org.kde.KWin"),
                "reconfigure",
                &(),
            )
            .await
            .is_err()
    {
        return false;
    }
    if tokio::time::timeout(Duration::from_secs(5), reloaded.recv())
        .await
        .ok()
        .flatten()
        .is_none()
    {
        return false;
    }
    // configChanged precedes rule loading; this round trip waits for it to finish.
    conn.call_method(
        Some("org.kde.KWin"),
        "/KWin",
        Some("org.kde.KWin"),
        "supportInformation",
        &(),
    )
    .await
    .is_ok()
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
wmclass=^shinpi-popup(-devices|-notices)?$
wmclassmatch=3
wmclasscomplete=false
noborder=true
noborderrule=2
";

/// Rules must be ready before tray surfaces can be created, even on a warm start.
fn write_taskbar_rule() -> bool {
    let path = crate::config::config_dir()
        .parent()
        .unwrap()
        .join("kwinrulesrc");
    update_taskbar_rule(&path).is_ok()
}

fn update_taskbar_rule(path: &Path) -> io::Result<()> {
    let previous = match fs::read_to_string(path) {
        Ok(text) => Some(text),
        Err(e) if e.kind() == io::ErrorKind::NotFound => None,
        Err(e) => return Err(e),
    };
    let Some(next) = merge_kwin_rules(previous.as_deref().unwrap_or_default()) else {
        return Ok(());
    };
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    if let Some(previous) = &previous {
        let backup = path.with_extension("shinpi.bak");
        match fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o600)
            .open(&backup)
        {
            Ok(mut file) => {
                if let Err(error) = file
                    .write_all(previous.as_bytes())
                    .and_then(|()| file.sync_all())
                {
                    let _ = fs::remove_file(backup);
                    return Err(error);
                }
            }
            Err(e) if e.kind() == io::ErrorKind::AlreadyExists => {
                if !fs::symlink_metadata(&backup)?.file_type().is_file() {
                    return Err(io::Error::other("KWin rules backup is not a regular file"));
                }
            }
            Err(e) => return Err(e),
        }
    }
    let temporary = path.with_extension("shinpi.tmp");
    fs::write(&temporary, next)?;
    if let Ok(metadata) = fs::metadata(path) {
        fs::set_permissions(&temporary, metadata.permissions())?;
    }
    let current = match fs::read_to_string(path) {
        Ok(text) => Some(text),
        Err(e) if e.kind() == io::ErrorKind::NotFound => None,
        Err(e) => return Err(e),
    };
    if current != previous {
        let _ = fs::remove_file(temporary);
        return Err(io::Error::other(
            "KWin rules changed while preparing the update",
        ));
    }
    fs::rename(temporary, path)
}

fn section_range(text: &str, name: &str) -> Option<std::ops::Range<usize>> {
    let header = format!("[{name}]");
    let mut offset = 0;
    let mut start = None;
    for line in text.split_inclusive('\n') {
        let trimmed = line.trim();
        if trimmed.starts_with('[') && trimmed.ends_with(']') {
            if let Some(start) = start {
                return Some(start..offset);
            }
            if trimmed == header {
                start = Some(offset);
            }
        }
        offset += line.len();
    }
    start.map(|start| start..text.len())
}

fn setting<'a>(section: &'a str, key: &str) -> Option<&'a str> {
    section.lines().find_map(|line| {
        let (name, value) = line.split_once('=')?;
        (name.trim() == key).then_some(value.trim())
    })
}

fn contains_rule(list: &str, rule: &str) -> bool {
    let mut start = 0;
    let mut escaped = false;
    for (index, ch) in list
        .char_indices()
        .chain(std::iter::once((list.len(), ',')))
    {
        if ch == ',' && !escaped {
            if &list[start..index] == rule {
                return true;
            }
            start = index + 1;
        }
        escaped = ch == '\\' && !escaped;
    }
    false
}

fn merge_kwin_rules(text: &str) -> Option<String> {
    let general_range = section_range(text, "General");
    let general = general_range
        .as_ref()
        .map(|range| &text[range.clone()])
        .unwrap_or_default();
    // Plasma 6.8 migrates a non-empty legacy list by deleting every unlisted group.
    let modern = setting(general, "Order").is_some();
    let key = if modern { "Order" } else { "rules" };
    let mut ids = setting(general, key).unwrap_or_default().to_string();
    if !modern {
        // Every group must survive KWin's later migration of the legacy list.
        let mut groups = text
            .lines()
            .filter_map(|line| {
                let group = line.trim().strip_prefix('[')?.strip_suffix(']')?;
                (group != "General" && !group.starts_with('$') && !group.contains("]["))
                    .then(|| group.replace('\\', "\\\\").replace(',', "\\,"))
            })
            .collect::<Vec<_>>();
        if ids.is_empty()
            && setting(general, "count").is_some_and(|count| count.parse::<u32>().unwrap_or(0) > 0)
        {
            groups.sort_by_key(|group| group.parse::<u32>().unwrap_or(u32::MAX));
        }
        for group in groups {
            if !contains_rule(&ids, &group) {
                if !ids.is_empty() {
                    ids.push(',');
                }
                ids.push_str(&group);
            }
        }
    }
    if !contains_rule(&ids, "shinpi-tray") {
        if !ids.is_empty() {
            ids.push(',');
        }
        ids.push_str("shinpi-tray");
    }
    let mut next_general = String::new();
    let mut wrote_key = false;
    for line in general.split_inclusive('\n') {
        let name = line.split_once('=').map(|(name, _)| name.trim());
        if name == Some(key) {
            if !wrote_key {
                if setting(general, key) == Some(ids.as_str()) {
                    next_general.push_str(line);
                } else {
                    next_general.push_str(&format!("{key}={ids}\n"));
                }
                wrote_key = true;
            }
            continue;
        }
        if modern && matches!(name, Some("rules" | "count")) {
            continue;
        }
        next_general.push_str(line);
    }
    if general_range.is_none() {
        next_general.push_str("[General]\n");
    }
    if !wrote_key {
        if !next_general.ends_with('\n') {
            next_general.push('\n');
        }
        next_general.push_str(&format!("{key}={ids}\n"));
    }
    let mut out = text.to_string();
    if let Some(range) = general_range {
        out.replace_range(range, &next_general);
    } else {
        if !out.is_empty() && !out.ends_with('\n') {
            out.push('\n');
        }
        out.push_str(&next_general);
    }
    if let Some(range) = section_range(&out, "shinpi-tray") {
        let current = &out[range.clone()];
        let expected = TASKBAR_RULE.lines().skip(1).all(|line| {
            let (key, value) = line.split_once('=').unwrap();
            setting(current, key) == Some(value)
        }) && setting(current, "title").is_none();
        if !expected {
            out.replace_range(range, TASKBAR_RULE);
        }
    } else {
        if !out.ends_with('\n') {
            out.push('\n');
        }
        out.push_str(TASKBAR_RULE);
    }
    (out != text).then_some(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn modern_order_preserves_foreign_rules_and_removes_destructive_legacy_list() {
        let foreign = "[editor]\nDescription=Editor\nwmclass=kate\ntitle=My document\n\n[yoin-tray]\nDescription=yoin tray popup\nwmclass=yoin-popup\n";
        for legacy in ["", "rules=shinpi-tray\ncount=1\n"] {
            for tray in ["", TASKBAR_RULE] {
                let text = format!("[$Version]\nupdate_info=kept\n\n[General]\nOrder=yoin-tray,editor\n{legacy}\n{foreign}\n{tray}");
                let next = merge_kwin_rules(&text).unwrap();
                let general = &next[section_range(&next, "General").unwrap()];
                assert_eq!(
                    setting(general, "Order"),
                    Some("yoin-tray,editor,shinpi-tray")
                );
                // This is the migration trigger in KWin 6.8's usrRead().
                assert_eq!(setting(general, "rules"), None);
                assert_eq!(setting(general, "count"), None);
                assert!(next.contains(foreign));
                assert!(next.starts_with("[$Version]\nupdate_info=kept\n"));
                assert!(merge_kwin_rules(&next).is_none());
            }
        }
        let current = format!("[General]\nOrder=editor,shinpi-tray\n\n{foreign}\n{TASKBAR_RULE}");
        assert!(merge_kwin_rules(&current).is_none());
        let unregistered = format!("[General]\nOrder=editor\n\n{foreign}\n{TASKBAR_RULE}");
        assert!(merge_kwin_rules(&unregistered)
            .unwrap()
            .contains("Order=editor,shinpi-tray\n"));
    }

    #[test]
    fn legacy_count_and_missing_general_preserve_all_rule_groups() {
        let groups = "[3]\nwmclass=third\n\n[1]\nwmclass=first\n\n[2]\nwmclass=second\n";
        let next = merge_kwin_rules(&format!("[General]\ncount=3\n\n{groups}")).unwrap();
        assert!(next.contains("rules=1,2,3,shinpi-tray\n"));
        assert!(next.contains(groups));
        assert!(merge_kwin_rules(&next).is_none());
        let next = merge_kwin_rules(groups).unwrap();
        assert!(next.contains("rules=3,1,2,shinpi-tray\n"));
        assert!(next.starts_with(groups));
        let next = merge_kwin_rules(&format!("[General]\nrules=2\n\n{groups}")).unwrap();
        assert!(next.contains("rules=2,3,1,shinpi-tray\n"));
        assert!(next.contains(groups));
        assert!(merge_kwin_rules(&next).is_none());
        assert!(!contains_rule("other\\,shinpi-tray", "shinpi-tray"));
        assert!(contains_rule("other\\,rule,shinpi-tray", "shinpi-tray"));
    }

    #[test]
    fn disk_update_keeps_original_backup_and_aborts_on_read_or_backup_errors() {
        let stamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let dir = std::env::temp_dir().join(format!("shinpi-kwin-{}-{stamp}", std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        let path = dir.join("kwinrulesrc");
        let backup = path.with_extension("shinpi.bak");
        let original = "[General]\nOrder=editor\n\n[editor]\ntitle=Keep me\nwmclass=kate\n";
        fs::write(&path, original).unwrap();
        update_taskbar_rule(&path).unwrap();
        assert_eq!(fs::read_to_string(&backup).unwrap(), original);
        let updated = fs::read_to_string(&path).unwrap();
        update_taskbar_rule(&path).unwrap();
        assert_eq!(fs::read_to_string(&path).unwrap(), updated);
        fs::write(
            &path,
            original.replace("Order=editor", "Order=editor,another"),
        )
        .unwrap();
        update_taskbar_rule(&path).unwrap();
        assert_eq!(fs::read_to_string(&backup).unwrap(), original);
        fs::remove_file(&backup).unwrap();
        fs::create_dir(&backup).unwrap();
        fs::write(&path, original).unwrap();
        assert!(update_taskbar_rule(&path).is_err());
        assert_eq!(fs::read_to_string(&path).unwrap(), original);
        fs::remove_dir(&backup).unwrap();
        fs::write(&path, [0xff, 0xfe]).unwrap();
        assert!(update_taskbar_rule(&path).is_err());
        assert_eq!(fs::read(&path).unwrap(), [0xff, 0xfe]);
        assert!(!backup.exists());
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn kwin_rule_is_inserted_once() {
        let first = merge_kwin_rules("[General]\nrules=1\ncount=1\n\n[1]\nwmclass=kate\n").unwrap();
        assert!(first.contains("rules=1,shinpi-tray\n"));
        assert!(first.contains("count=1\n"));
        assert!(first.contains("skiptaskbar=true\n"));
        assert!(first.contains("wmclass=^shinpi-popup(-devices|-notices)?$\n"));
        assert!(first.contains("wmclassmatch=3\n"));
        assert!(first.contains("noborder=true\n"));
        assert!(!first.contains("title="));
        assert!(merge_kwin_rules(&first).is_none());
        let stale = "[General]\nrules=shinpi-tray\ncount=1\n\n[shinpi-tray]\nwmclass=shinpi\nskiptaskbar=true\n";
        let updated = merge_kwin_rules(stale).unwrap();
        assert!(updated.contains("wmclass=^shinpi-popup(-devices|-notices)?$\n"));
        assert!(!updated.contains("wmclass=shinpi\n"));
        assert!(merge_kwin_rules(&updated).is_none());
        let old = "[General]\nrules=shinpi-tray\ncount=1\n\n[shinpi-tray]\nDescription=shinpi tray popup\ntitle=shinpi-(devices|notices)\ntitlematch=3\nwmclass=shinpi\nwmclassmatch=1\n";
        let fixed = merge_kwin_rules(old).unwrap();
        assert!(fixed.contains("wmclass=^shinpi-popup(-devices|-notices)?$\n"));
        assert!(!fixed.contains("title="));
        assert!(merge_kwin_rules(&fixed).is_none());
        let previous = "[General]\nrules=shinpi-tray\ncount=1\n\n[shinpi-tray]\nwmclass=shinpi-popup\nwmclassmatch=1\n";
        let migrated = merge_kwin_rules(previous).unwrap();
        assert!(migrated.contains("wmclass=^shinpi-popup(-devices|-notices)?$\n"));
        assert!(merge_kwin_rules(&migrated).is_none());
        let empty = merge_kwin_rules("").unwrap();
        assert!(empty.contains("[shinpi-tray]\n"));
        assert!(empty.contains("rules=shinpi-tray\n"));
    }
}
