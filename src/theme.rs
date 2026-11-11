// Copyright (C) 2026 yrmuq
// SPDX-License-Identifier: GPL-3.0-or-later

use std::path::Path;

pub fn resolved(choice: &str, system_dark: bool) -> &'static str {
    match choice {
        "light" => "light",
        "dark" => "dark",
        _ => {
            if system_dark {
                "dark"
            } else {
                "light"
            }
        }
    }
}

pub fn kde_is_dark(text: &str) -> Option<bool> {
    let mut section = String::new();
    let mut scheme = String::new();
    let mut background = String::new();
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
        if section == "General" && key == "ColorScheme" {
            scheme = value.to_string();
        } else if section == "Colors:Window" && key == "BackgroundNormal" {
            background = value.to_string();
        }
    }
    if !background.is_empty() {
        let parts: Vec<i32> = background
            .split(',')
            .take(3)
            .map(|p| p.trim().parse().ok())
            .collect::<Option<_>>()?;
        if parts.len() == 3 {
            let luma =
                0.2126 * parts[0] as f32 + 0.7152 * parts[1] as f32 + 0.0722 * parts[2] as f32;
            return Some(luma < 128.0);
        }
    }
    if !scheme.is_empty() {
        return Some(scheme.to_lowercase().contains("dark"));
    }
    None
}

pub fn system_dark() -> bool {
    let path = crate::config::config_dir()
        .parent()
        .unwrap_or(Path::new("/"))
        .join("kdeglobals");
    if let Ok(text) = std::fs::read_to_string(&path) {
        if let Some(dark) = kde_is_dark(&text) {
            return dark;
        }
    }
    let path = crate::config::home().join(".config").join("kdeglobals");
    if let Ok(text) = std::fs::read_to_string(path) {
        if let Some(dark) = kde_is_dark(&text) {
            return dark;
        }
    }
    false
}

pub struct UiFont {
    pub family: String,
    pub size_px: f32,
    pub weight: i32,
}

impl Default for UiFont {
    fn default() -> Self {
        Self {
            family: "Noto Sans".into(),
            size_px: 10.0 * 96.0 / 72.0,
            weight: 400,
        }
    }
}

pub fn font_from_kdeglobals(text: &str) -> Option<UiFont> {
    let mut section = String::new();
    for raw in text.lines() {
        let line = raw.trim();
        if line.starts_with('[') && line.ends_with(']') {
            section = line[1..line.len() - 1].to_string();
            continue;
        }
        if section != "General" {
            continue;
        }
        let Some((key, value)) = line.split_once('=') else {
            continue;
        };
        if key.trim() == "font" {
            return parse_qt_font(value.trim());
        }
    }
    None
}

pub fn parse_qt_font(value: &str) -> Option<UiFont> {
    let parts: Vec<&str> = value.split(',').collect();
    let family = parts.first()?.trim().trim_matches('"');
    if family.is_empty() {
        return None;
    }
    let point = parts
        .get(1)
        .and_then(|s| s.trim().parse::<f32>().ok())
        .unwrap_or(-1.0);
    let pixel = parts
        .get(2)
        .and_then(|s| s.trim().parse::<f32>().ok())
        .unwrap_or(-1.0);
    let qt_weight = parts
        .get(4)
        .and_then(|s| s.trim().parse::<i32>().ok())
        .unwrap_or(50);
    let size_px = if point > 0.0 {
        point * 96.0 / 72.0
    } else if pixel > 0.0 {
        pixel
    } else {
        10.0 * 96.0 / 72.0
    };
    Some(UiFont {
        family: family.to_string(),
        size_px,
        weight: qt_weight_to_css(qt_weight),
    })
}

fn qt_weight_to_css(weight: i32) -> i32 {
    match weight {
        i32::MIN..=10 => 100,
        11..=24 => 200,
        25..=49 => 300,
        50..=56 => 400,
        57..=62 => 500,
        63..=74 => 600,
        75..=80 => 700,
        81..=86 => 800,
        _ => 900,
    }
}

pub fn system_font() -> UiFont {
    let paths = [
        crate::config::config_dir()
            .parent()
            .unwrap_or(Path::new("/"))
            .join("kdeglobals"),
        crate::config::home().join(".config").join("kdeglobals"),
    ];
    for path in paths {
        let Ok(text) = std::fs::read_to_string(&path) else {
            continue;
        };
        if let Some(font) = font_from_kdeglobals(&text) {
            return font;
        }
    }
    UiFont::default()
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Rgb {
    pub r: u8,
    pub g: u8,
    pub b: u8,
}

pub struct UiPalette {
    pub window: Rgb,
    pub button: Rgb,
    pub button_hover: Rgb,
    pub text: Rgb,
    pub inactive: Rgb,
    pub accent: Rgb,
    pub accent_text: Rgb,
    pub frame: Rgb,
    pub outline: Rgb,
}

impl UiPalette {
    fn builtin(dark: bool) -> Self {
        // Breeze Dark / Breeze Light from the current Plasma Breeze color schemes.
        if dark {
            Self::from_parts(
                Rgb::new(32, 35, 38),
                Rgb::new(252, 252, 252),
                Rgb::new(41, 44, 48),
                Rgb::new(161, 169, 177),
                Rgb::new(61, 174, 233),
                Rgb::new(252, 252, 252),
            )
        } else {
            Self::from_parts(
                Rgb::new(239, 240, 241),
                Rgb::new(35, 38, 41),
                Rgb::new(252, 252, 252),
                Rgb::new(112, 125, 138),
                Rgb::new(61, 174, 233),
                Rgb::new(255, 255, 255),
            )
        }
    }

    fn from_parts(
        window: Rgb,
        text: Rgb,
        button: Rgb,
        inactive: Rgb,
        accent: Rgb,
        accent_text: Rgb,
    ) -> Self {
        let frame = mix(window, text, 0.20);
        let outline = mix(accent, text, 0.15);
        let button_hover = mix(button, text, 0.05);
        Self {
            window,
            button,
            button_hover,
            text,
            inactive,
            accent,
            accent_text,
            frame,
            outline,
        }
    }

    fn apply_text(&mut self, text: &str) {
        let map = ini_colors(text);
        if let Some(color) = map.get(&("Colors:Window", "BackgroundNormal")) {
            self.window = *color;
        }
        if let Some(color) = map.get(&("Colors:Window", "ForegroundNormal")) {
            self.text = *color;
        }
        if let Some(color) = map.get(&("Colors:Button", "BackgroundNormal")) {
            self.button = *color;
        }
        if let Some(color) = map
            .get(&("Colors:Button", "ForegroundInactive"))
            .or_else(|| map.get(&("Colors:Window", "ForegroundInactive")))
        {
            self.inactive = *color;
        }
        if let Some(color) = map
            .get(&("Colors:Selection", "BackgroundNormal"))
            .or_else(|| map.get(&("Colors:Button", "DecorationFocus")))
        {
            self.accent = *color;
        }
        if let Some(color) = map.get(&("Colors:Selection", "ForegroundNormal")) {
            self.accent_text = *color;
        }
        if let Some(color) = accent_color(text) {
            self.accent = color;
        }
        *self = Self::from_parts(
            self.window,
            self.text,
            self.button,
            self.inactive,
            self.accent,
            self.accent_text,
        );
    }
}

impl Rgb {
    const fn new(r: u8, g: u8, b: u8) -> Self {
        Self { r, g, b }
    }
}

pub fn mix(base: Rgb, other: Rgb, bias: f32) -> Rgb {
    let bias = bias.clamp(0.0, 1.0);
    let channel = |left: u8, right: u8| {
        (left as f32 * (1.0 - bias) + right as f32 * bias)
            .round()
            .clamp(0.0, 255.0) as u8
    };
    Rgb {
        r: channel(base.r, other.r),
        g: channel(base.g, other.g),
        b: channel(base.b, other.b),
    }
}

pub fn system_palette(choice: &str) -> UiPalette {
    let dark = resolved(choice, system_dark()) == "dark";
    let mut colors = UiPalette::builtin(dark);
    let globals = read_kdeglobals();
    if choice == "light" || choice == "dark" {
        if let Some(text) = read_scheme(if dark { "BreezeDark" } else { "BreezeLight" }) {
            colors.apply_text(&text);
        }
        if let Some(text) = globals.as_deref().and_then(accent_color) {
            colors.accent = text;
            colors = UiPalette::from_parts(
                colors.window,
                colors.text,
                colors.button,
                colors.inactive,
                colors.accent,
                colors.accent_text,
            );
        }
    } else if let Some(text) = globals.as_deref() {
        if let Some(name) = scheme_name(text) {
            if let Some(file) = read_scheme(&name) {
                colors.apply_text(&file);
            }
        }
        colors.apply_text(text);
    }
    colors
}

fn read_kdeglobals() -> Option<String> {
    let paths = [
        crate::config::config_dir()
            .parent()
            .unwrap_or(Path::new("/"))
            .join("kdeglobals"),
        crate::config::home().join(".config").join("kdeglobals"),
    ];
    paths
        .into_iter()
        .find_map(|path| std::fs::read_to_string(path).ok())
}

fn scheme_name(text: &str) -> Option<String> {
    let mut section = String::new();
    for raw in text.lines() {
        let line = raw.trim();
        if line.starts_with('[') && line.ends_with(']') {
            section = line[1..line.len() - 1].to_string();
            continue;
        }
        if section == "General" {
            if let Some(value) = line.strip_prefix("ColorScheme=") {
                let value = value.trim();
                if !value.is_empty() {
                    return Some(value.to_string());
                }
            }
        }
    }
    None
}

fn read_scheme(name: &str) -> Option<String> {
    if name.contains(['/', '\\']) || name.contains("..") {
        return None;
    }
    let file = format!("{name}.colors");
    let mut dirs = Vec::new();
    if let Some(home) = std::env::var_os("XDG_DATA_HOME") {
        dirs.push(std::path::PathBuf::from(home).join("color-schemes"));
    }
    dirs.push(
        crate::config::home()
            .join(".local")
            .join("share")
            .join("color-schemes"),
    );
    if let Some(paths) = std::env::var_os("XDG_DATA_DIRS") {
        for dir in std::env::split_paths(&paths) {
            dirs.push(dir.join("color-schemes"));
        }
    }
    dirs.push(std::path::PathBuf::from("/usr/share/color-schemes"));
    dirs.into_iter()
        .map(|dir| dir.join(&file))
        .find_map(|path| std::fs::read_to_string(path).ok())
}

fn accent_color(text: &str) -> Option<Rgb> {
    let mut section = String::new();
    for raw in text.lines() {
        let line = raw.trim();
        if line.starts_with('[') && line.ends_with(']') {
            section = line[1..line.len() - 1].to_string();
            continue;
        }
        if section == "General" {
            if let Some(value) = line.strip_prefix("AccentColor=") {
                return parse_rgb(value.trim());
            }
        }
    }
    None
}

fn ini_colors(text: &str) -> std::collections::HashMap<(&'static str, &'static str), Rgb> {
    let mut section = String::new();
    let mut out = std::collections::HashMap::new();
    for raw in text.lines() {
        let line = raw.trim();
        if line.starts_with('[') && line.ends_with(']') {
            section = line[1..line.len() - 1].to_string();
            continue;
        }
        let Some((key, value)) = line.split_once('=') else {
            continue;
        };
        let Some(color) = parse_rgb(value.trim()) else {
            continue;
        };
        let known = match (section.as_str(), key.trim()) {
            ("Colors:Window", "BackgroundNormal") => Some(("Colors:Window", "BackgroundNormal")),
            ("Colors:Window", "ForegroundNormal") => Some(("Colors:Window", "ForegroundNormal")),
            ("Colors:Window", "ForegroundInactive") => {
                Some(("Colors:Window", "ForegroundInactive"))
            }
            ("Colors:Button", "BackgroundNormal") => Some(("Colors:Button", "BackgroundNormal")),
            ("Colors:Button", "ForegroundInactive") => {
                Some(("Colors:Button", "ForegroundInactive"))
            }
            ("Colors:Button", "DecorationFocus") => Some(("Colors:Button", "DecorationFocus")),
            ("Colors:Selection", "BackgroundNormal") => {
                Some(("Colors:Selection", "BackgroundNormal"))
            }
            ("Colors:Selection", "ForegroundNormal") => {
                Some(("Colors:Selection", "ForegroundNormal"))
            }
            _ => None,
        };
        if let Some(known) = known {
            out.insert(known, color);
        }
    }
    out
}

fn parse_rgb(value: &str) -> Option<Rgb> {
    let mut parts = value.split(',');
    let r = parts.next()?.trim().parse().ok()?;
    let g = parts.next()?.trim().parse().ok()?;
    let b = parts.next()?.trim().parse().ok()?;
    Some(Rgb { r, g, b })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn kde_background_decides() {
        let text = "[Colors:Window]\nBackgroundNormal=20,20,20\n";
        assert_eq!(kde_is_dark(text), Some(true));
        assert_eq!(resolved("system", true), "dark");
        assert_eq!(resolved("light", true), "light");
    }

    #[test]
    fn qt_font_uses_point_size_and_normal_weight() {
        let text = "[General]\nfont=Noto Sans,10,-1,5,50,0,0,0,0,0,Regular\n";
        let font = font_from_kdeglobals(text).unwrap();
        assert_eq!(font.family, "Noto Sans");
        assert!((font.size_px - (10.0 * 96.0 / 72.0)).abs() < 0.01);
        assert_eq!(font.weight, 400);
        assert_eq!(
            parse_qt_font("Hack,11,-1,5,75,0,0,0,0,0").unwrap().weight,
            700
        );
    }

    #[test]
    fn outline_darkens_the_accent_the_breeze_way() {
        let light = UiPalette::builtin(false);
        assert_eq!(light.frame, Rgb::new(198, 200, 201));
        assert_eq!(light.outline, Rgb::new(57, 154, 204));
        let text = "[Colors:Window]\nBackgroundNormal=239,240,241\nForegroundNormal=35,38,41\n[Colors:Button]\nBackgroundNormal=252,252,252\nDecorationFocus=61,174,233\n[General]\nAccentColor=10,20,30\n";
        let mut colors = UiPalette::builtin(false);
        colors.apply_text(text);
        assert_eq!(colors.accent, Rgb::new(10, 20, 30));
        assert_eq!(
            colors.outline,
            mix(Rgb::new(10, 20, 30), Rgb::new(35, 38, 41), 0.15)
        );
    }
}
