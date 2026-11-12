use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
};

const ENGLISH: &str = include_str!("../languages/English.ini");
const RUSSIAN: &str = include_str!("../languages/Russian.ini");

#[derive(Clone)]
struct Row {
    en: String,
    text: String,
}

#[derive(Clone)]
pub struct Catalog {
    pub language: String,
    rows: BTreeMap<u32, Row>,
}

impl Catalog {
    pub fn load(choice: &str) -> Self {
        Self::from_folder(&language_dir(), choice, &system_languages())
    }

    fn from_folder(folder: &Path, choice: &str, locales: &[String]) -> Self {
        let names = languages_in(folder);
        let language = if choice.is_empty() {
            locales
                .iter()
                .find_map(|locale| {
                    let code = locale
                        .split(['_', '-', '.'])
                        .next()
                        .unwrap_or("")
                        .to_lowercase();
                    names
                        .iter()
                        .find(|name| metadata(&source(folder, name), "code") == code)
                        .cloned()
                })
                .unwrap_or_else(|| "English".into())
        } else if names.iter().any(|name| name == choice) {
            choice.to_string()
        } else {
            "English".into()
        };
        let mut rows = parse(ENGLISH);
        for (id, translated) in parse(&source(folder, &language)) {
            if let Some(row) = rows.get_mut(&id) {
                if row.en == translated.en && compatible(&row.en, &translated.text) {
                    row.text = translated.text;
                }
            }
        }
        Self { language, rows }
    }

    pub fn tr(&self, original: &str) -> String {
        self.rows
            .values()
            .find(|row| row.en == original)
            .map(|row| row.text.clone())
            .unwrap_or_else(|| original.into())
    }

    pub fn trf(&self, original: &str, args: &[&str]) -> String {
        render(&self.tr(original), args)
            .or_else(|| render(original, args))
            .unwrap_or_else(|| original.into())
    }

    pub fn labels(&self) -> Vec<slint::SharedString> {
        let mut labels = vec![
            slint::SharedString::default();
            self.rows.keys().next_back().copied().unwrap_or(0) as usize + 1
        ];
        for (id, row) in &self.rows {
            labels[*id as usize] = row.text.clone().into();
        }
        labels
    }

    pub fn reading(&self, reading: &crate::device::Reading) -> String {
        if reading.charging && reading.percent.is_none() {
            return self.tr("Charging");
        }
        let text = reading.text();
        self.tr(match text.as_str() {
            "Нет данных" => "No data",
            "Заряжается" => "Charging",
            "Критический" => "Critical",
            "Низкий" => "Low",
            "Средний" => "Normal",
            "Высокий" => "High",
            "Полный" => "Full",
            _ => &text,
        })
    }

    pub fn message(&self, text: &str) -> String {
        self.message_at(text, 0)
    }

    fn message_at(&self, text: &str, depth: u8) -> String {
        if depth >= 8 {
            return text.into();
        }
        if self.rows.values().any(|row| row.en == text) {
            return self.tr(text);
        }
        for row in self.rows.values() {
            if row.en.matches("{}").count() != 1 {
                continue;
            }
            let (before, after) = row.en.split_once("{}").unwrap();
            if before.is_empty() && after.is_empty() {
                continue;
            }
            if let Some(arg) = text
                .strip_prefix(before)
                .and_then(|s| s.strip_suffix(after))
            {
                return self.trf(&row.en, &[&self.message_at(arg, depth + 1)]);
            }
        }
        if let Some((prefix, tail)) = text.split_once(": ") {
            return format!("{prefix}: {}", self.message_at(tail, depth + 1));
        }
        text.into()
    }
}

fn placeholders(pattern: &str) -> Option<Vec<std::ops::Range<usize>>> {
    let mut fields = Vec::new();
    let mut chars = pattern.char_indices();
    while let Some((start, ch)) = chars.next() {
        match ch {
            '{' => match chars.next()? {
                (end, '}') => fields.push(start..end + 1),
                (_, '{') => {}
                _ => return None,
            },
            '}' if chars.next()?.1 != '}' => return None,
            _ => {}
        }
    }
    Some(fields)
}

fn compatible(original: &str, translated: &str) -> bool {
    match (placeholders(original), placeholders(translated)) {
        (Some(original), Some(translated)) => original.len() == translated.len(),
        _ => false,
    }
}

fn render(pattern: &str, args: &[&str]) -> Option<String> {
    let fields = placeholders(pattern)?;
    if fields.len() != args.len() {
        return None;
    }
    let literal = |text: &str| text.replace("{{", "{").replace("}}", "}");
    let mut out = String::new();
    let mut previous = 0;
    for (field, arg) in fields.iter().zip(args) {
        out.push_str(&literal(&pattern[previous..field.start]));
        out.push_str(arg);
        previous = field.end;
    }
    out.push_str(&literal(&pattern[previous..]));
    Some(out)
}

fn parse(text: &str) -> BTreeMap<u32, Row> {
    text.trim_start_matches('\u{feff}')
        .lines()
        .filter_map(|line| {
            let line = line.trim();
            if line.starts_with([';', '#', '[']) {
                return None;
            }
            let (id, rest) = line.split_once('=')?;
            let (en, shown) = rest.rsplit_once('=')?;
            let en = en.trim().replace("\\n", "\n");
            let shown = shown.trim().replace("\\n", "\n");
            Some((
                id.trim().parse().ok()?,
                Row {
                    text: if shown.is_empty() { en.clone() } else { shown },
                    en,
                },
            ))
        })
        .collect()
}

fn metadata(text: &str, key: &str) -> String {
    text.trim_start_matches('\u{feff}')
        .lines()
        .find_map(|line| {
            let (name, value) = line.trim().trim_start_matches(';').trim().split_once('=')?;
            (name.trim() == key).then(|| value.trim().to_string())
        })
        .unwrap_or_default()
}

fn safe_file(folder: &Path, name: &str) -> Option<PathBuf> {
    if name.is_empty() || name == "." || name == ".." || name.contains(['/', '\\']) {
        return None;
    }
    let root = folder.canonicalize().ok()?;
    let path = root.join(format!("{name}.ini")).canonicalize().ok()?;
    (path.parent()? == root && path.is_file()).then_some(path)
}

fn source(folder: &Path, name: &str) -> String {
    safe_file(folder, name)
        .and_then(|p| std::fs::read_to_string(p).ok())
        .unwrap_or_else(|| match name {
            "English" => ENGLISH.into(),
            "Russian" => RUSSIAN.into(),
            _ => String::new(),
        })
}

fn languages_in(folder: &Path) -> Vec<String> {
    let mut names = vec!["English".to_string(), "Russian".to_string()];
    if let Ok(entries) = std::fs::read_dir(folder) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().and_then(|p| p.to_str()) == Some("ini") {
                if let Some(name) = path.file_stem().and_then(|p| p.to_str()) {
                    if safe_file(folder, name).is_some() && !names.iter().any(|n| n == name) {
                        names.push(name.into());
                    }
                }
            }
        }
    }
    names.sort();
    names.retain(|n| n != "English");
    names.insert(0, "English".into());
    names
}

pub fn languages() -> Vec<String> {
    languages_in(&language_dir())
}

pub fn language_label(name: &str) -> String {
    let label = metadata(&source(&language_dir(), name), "name");
    if label.is_empty() {
        name.into()
    } else {
        label
    }
}

fn system_languages() -> Vec<String> {
    ["LANGUAGE", "LC_ALL", "LC_MESSAGES", "LANG"]
        .iter()
        .find_map(|key| std::env::var(key).ok().filter(|s| !s.is_empty()))
        .unwrap_or_default()
        .split(':')
        .map(str::to_string)
        .collect()
}

fn language_dir() -> PathBuf {
    if let Some(path) = std::env::var_os("SHINPI_LANGUAGES").filter(|p| !p.is_empty()) {
        return path.into();
    }
    if let Ok(exe) = std::env::current_exe() {
        if let Some(dir) = exe.parent() {
            let shared = dir.join("../share/shinpi/languages");
            if shared.is_dir() {
                return shared;
            }
        }
    }
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("languages")
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn translations_reject_invalid_fields_and_keep_literal_braces() {
        for text in ["{} {name}", "{} {0}", "{} {:.2}", "{} {", "{} }"] {
            assert!(!compatible("Value: {}", text), "{text}");
        }
        assert!(compatible("Value: {}", "Значение: {{{}}}"));
        assert_eq!(
            render("Значение: {{{}}}", &["Mouse {}"]),
            Some("Значение: {Mouse {}}".into())
        );
    }
    #[test]
    fn locale_fallback_placeholders_and_canonical_readings_stay_safe() {
        let folder = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("languages");
        let ru = Catalog::from_folder(&folder, "", &["de_DE".into(), "ru-RU.UTF-8".into()]);
        assert_eq!(ru.language, "Russian");
        assert_eq!(ru.tr("Devices"), "Устройства");
        assert_eq!(
            ru.trf("{}: low battery — {}", &["Mouse {}", "12%"]),
            "Mouse {}: низкий заряд — 12%"
        );
        let en = Catalog::from_folder(&folder, "../Russian", &["ru".into()]);
        assert_eq!(en.language, "English");
        assert_eq!(
            en.reading(&crate::device::Reading {
                level: Some("Низкий".into()),
                ..Default::default()
            }),
            "Low"
        );
        assert_eq!(
            ru.message("System D-Bus unavailable: timeout"),
            "Системный D-Bus недоступен: timeout"
        );
        let base = parse(ENGLISH);
        let overlay = parse(RUSSIAN);
        assert_eq!(base.len(), overlay.len());
        for (id, row) in base {
            assert_eq!(
                row.en.matches("{}").count(),
                overlay[&id].text.matches("{}").count()
            );
        }
        assert!(safe_file(&folder, "../../secret").is_none());
    }
    #[test]
    fn external_translations_cannot_escape_folder_or_drop_arguments() {
        use std::os::unix::fs::symlink;
        let base = std::env::temp_dir().join(format!(
            "shinpi-i18n-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let folder = base.join("languages");
        std::fs::create_dir_all(&folder).unwrap();
        let id = parse(ENGLISH)
            .into_iter()
            .find(|(_, row)| row.en == "Notifications unavailable: {}")
            .unwrap()
            .0;
        std::fs::write(folder.join("Russian.ini"), format!("\u{feff}; code = ru\n1 = Devices = Приборы\n{id} = Notifications unavailable: {{}} = потерян аргумент\n")).unwrap();
        std::fs::write(base.join("outside.ini"), "; code = xx").unwrap();
        symlink(base.join("outside.ini"), folder.join("Outside.ini")).unwrap();
        let catalog = Catalog::from_folder(&folder, "", &["ru_RU.UTF-8".into()]);
        assert_eq!(catalog.tr("Devices"), "Приборы");
        assert_eq!(
            catalog.trf("Notifications unavailable: {}", &["timeout"]),
            "Notifications unavailable: timeout"
        );
        assert!(!languages_in(&folder).iter().any(|n| n == "Outside"));
        std::fs::remove_dir_all(base).unwrap();
    }
}
