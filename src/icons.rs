// Adapted from yoin, Copyright (C) 2026 yrmuq.
// SPDX-License-Identifier: GPL-3.0-or-later
use std::path::{Path, PathBuf};
pub struct ThemeIcon {
    pub image: slint::Image,
    pub tint: bool,
}
pub fn theme_icon(name: &str) -> Option<ThemeIcon> {
    let path = locate_icon(name)?;
    let tint = should_tint(&path);
    let image = load_slint_image(&path)?;
    Some(ThemeIcon { image, tint })
}

fn locate_icon(name: &str) -> Option<PathBuf> {
    let mut seen = std::collections::HashSet::new();
    let theme = icon_theme_name();
    find_icon(&theme, name, &mut seen)
        .or_else(|| find_icon("breeze", name, &mut seen))
        .or_else(|| find_icon("hicolor", name, &mut seen))
}

fn icon_theme_name() -> String {
    for path in kdeglobals_paths() {
        if let Some(name) = ini_value(&path, "Icons", "Theme") {
            if !name.is_empty() && name != "hicolor" {
                return name;
            }
        }
    }
    let gtk = crate::config::home()
        .join(".config")
        .join("gtk-3.0")
        .join("settings.ini");
    if let Some(name) = ini_value(&gtk, "Settings", "gtk-icon-theme-name") {
        if !name.is_empty() {
            return name;
        }
    }
    "breeze".into()
}

fn kdeglobals_paths() -> [PathBuf; 2] {
    [
        crate::config::config_dir()
            .parent()
            .unwrap_or(Path::new("/"))
            .join("kdeglobals"),
        crate::config::home().join(".config").join("kdeglobals"),
    ]
}

fn icon_roots() -> Vec<PathBuf> {
    let mut roots = Vec::new();
    if let Some(home) = std::env::var_os("XDG_DATA_HOME") {
        roots.push(PathBuf::from(home).join("icons"));
    }
    roots.push(
        crate::config::home()
            .join(".local")
            .join("share")
            .join("icons"),
    );
    roots.push(crate::config::home().join(".icons"));
    if let Some(paths) = std::env::var_os("XDG_DATA_DIRS") {
        for dir in std::env::split_paths(&paths) {
            roots.push(dir.join("icons"));
        }
    }
    roots.push(PathBuf::from("/usr/share/icons"));
    roots.push(PathBuf::from("/usr/local/share/icons"));
    roots
}

fn find_icon(
    theme: &str,
    name: &str,
    seen: &mut std::collections::HashSet<String>,
) -> Option<PathBuf> {
    if theme.is_empty() || !seen.insert(theme.to_string()) {
        return None;
    }
    let root = icon_roots()
        .into_iter()
        .map(|dir| dir.join(theme))
        .find(|dir| dir.join("index.theme").is_file())?;
    let index = parse_index(&root.join("index.theme"));
    let mut dirs = index.dirs;
    dirs.sort_by_key(|dir| {
        if dir.size == 22 && !dir.scalable {
            0
        } else if dir.scalable {
            80
        } else {
            (dir.size - 22).abs().min(70)
        }
    });
    for dir in &dirs {
        for ext in ["svg", "png"] {
            let path = root.join(&dir.rel).join(format!("{name}.{ext}"));
            if path.is_file() {
                return Some(path);
            }
        }
    }
    for parent in index.inherits {
        if let Some(found) = find_icon(&parent, name, seen) {
            return Some(found);
        }
    }
    None
}

struct ThemeIndex {
    inherits: Vec<String>,
    dirs: Vec<IconDir>,
}

struct IconDir {
    rel: String,
    size: i32,
    scalable: bool,
}

fn parse_index(path: &Path) -> ThemeIndex {
    let Ok(text) = std::fs::read_to_string(path) else {
        return ThemeIndex {
            inherits: vec!["hicolor".into()],
            dirs: Vec::new(),
        };
    };
    let mut section = String::new();
    let mut inherits = Vec::new();
    let mut directories = Vec::new();
    let mut sizes = std::collections::HashMap::<String, (i32, bool)>::new();
    for raw in text.lines() {
        let line = raw.trim();
        if line.starts_with('[') && line.ends_with(']') {
            section = line[1..line.len() - 1].to_string();
            continue;
        }
        let Some((key, value)) = line.split_once('=') else {
            continue;
        };
        let key = key.trim();
        let value = value.trim();
        if section == "Icon Theme" && key == "Inherits" {
            inherits = value
                .split(',')
                .map(|item| item.trim().to_string())
                .filter(|item| !item.is_empty())
                .collect();
        } else if section == "Icon Theme" && key == "Directories" {
            directories = value
                .split(',')
                .map(|item| item.trim().to_string())
                .filter(|item| !item.is_empty())
                .collect();
        } else if key == "Size" {
            if let Ok(size) = value.parse() {
                sizes.entry(section.clone()).or_insert((size, false)).0 = size;
            }
        } else if key == "Type" && value.eq_ignore_ascii_case("Scalable") {
            sizes.entry(section.clone()).or_insert((128, true)).1 = true;
        }
    }
    let dirs = directories
        .into_iter()
        .map(|rel| {
            let (size, scalable) = sizes.get(&rel).copied().unwrap_or((22, false));
            IconDir {
                rel,
                size,
                scalable,
            }
        })
        .collect();
    if inherits.is_empty() {
        inherits.push("hicolor".into());
    }
    ThemeIndex { inherits, dirs }
}

fn ini_value(path: &Path, section_name: &str, key_name: &str) -> Option<String> {
    let text = std::fs::read_to_string(path).ok()?;
    let mut section = String::new();
    for raw in text.lines() {
        let line = raw.trim();
        if line.starts_with('[') && line.ends_with(']') {
            section = line[1..line.len() - 1].to_string();
            continue;
        }
        if section == section_name {
            if let Some(value) = line.strip_prefix(&format!("{key_name}=")) {
                return Some(value.trim().to_string());
            }
        }
    }
    None
}

fn should_tint(path: &Path) -> bool {
    let Ok(bytes) = std::fs::read(path) else {
        return false;
    };
    if bytes.starts_with(b"\x89PNG") {
        return false;
    }
    let text = String::from_utf8_lossy(&bytes).to_ascii_lowercase();
    if text.contains("currentcolor") || text.contains("colorscheme-") {
        return true;
    }
    let mut colors = std::collections::HashSet::new();
    for key in ["fill=\"", "fill:", "stroke=\"", "stroke:"] {
        let mut rest = text.as_str();
        while let Some(index) = rest.find(key) {
            rest = &rest[index + key.len()..];
            let color: String = rest
                .chars()
                .take_while(|c| !matches!(c, '"' | ';' | ' ' | '\n' | ','))
                .collect();
            if color.is_empty() || color == "none" || color == "transparent" {
                continue;
            }
            colors.insert(color);
        }
    }
    colors.len() <= 1
}

fn load_slint_image(path: &Path) -> Option<slint::Image> {
    if path.extension()?.to_str()? == "svg" {
        slint::Image::load_from_svg_data(&std::fs::read(path).ok()?).ok()
    } else {
        slint::Image::load_from_path(path).ok()
    }
}

pub fn tray() -> Vec<ksni::Icon> {
    let bytes = include_bytes!("../ui/icons/battery.svg");
    let Ok(tree) = resvg::usvg::Tree::from_data(bytes, &resvg::usvg::Options::default()) else {
        return vec![];
    };
    [22, 32, 48]
        .into_iter()
        .filter_map(|size| {
            let mut pixmap = resvg::tiny_skia::Pixmap::new(size, size)?;
            resvg::render(
                &tree,
                resvg::tiny_skia::Transform::from_scale(size as f32 / 24.0, size as f32 / 24.0),
                &mut pixmap.as_mut(),
            );
            let data = pixmap
                .pixels()
                .iter()
                .flat_map(|p| {
                    let p = p.demultiply();
                    [p.alpha(), p.red(), p.green(), p.blue()]
                })
                .collect();
            Some(ksni::Icon {
                width: size as i32,
                height: size as i32,
                data,
            })
        })
        .collect()
}
