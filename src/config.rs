use crate::device::{Band, Device, Kind, Reading, Scan};
use serde::{Deserialize, Serialize};
use std::{fs, io::Write, os::unix::fs::OpenOptionsExt, path::PathBuf};

pub const DEFAULT_SETTINGS_WIDTH: u32 = 600;
pub const DEFAULT_SETTINGS_HEIGHT: u32 = 620;
pub const DEFAULT_LOGO_SIZE: u32 = 96;

#[derive(Clone, Deserialize, Serialize)]
#[serde(default)]
pub struct Config {
    pub theme: String,
    pub language: String,
    pub tray_orientation: u8,
    pub colored_icon: bool,
    pub notifications: bool,
    pub low_threshold: u8,
    pub repeat_minutes: u32,
    pub panel_width: u32,
    pub history_enabled: bool,
    pub history_hidden: bool,
    pub history_count: u32,
    pub history: Vec<crate::notices::Note>,
    pub devices: Vec<Selected>,
    #[serde(skip_serializing_if = "default_width")]
    pub settings_width: u32,
    #[serde(skip_serializing_if = "default_height")]
    pub settings_height: u32,
    #[serde(skip_serializing_if = "default_logo")]
    pub logo_size: u32,
    #[serde(skip_serializing_if = "is_false")]
    pub dev_tab: bool,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            theme: String::new(),
            language: String::new(),
            tray_orientation: 2,
            colored_icon: true,
            notifications: true,
            low_threshold: 15,
            repeat_minutes: 60,
            panel_width: 300,
            history_enabled: true,
            history_hidden: false,
            history_count: 3,
            history: Vec::new(),
            devices: Vec::new(),
            settings_width: DEFAULT_SETTINGS_WIDTH,
            settings_height: DEFAULT_SETTINGS_HEIGHT,
            logo_size: DEFAULT_LOGO_SIZE,
            dev_tab: false,
        }
    }
}

#[derive(Clone, Deserialize, Serialize)]
pub struct Selected {
    pub id: String,
    pub name: String,
    pub kind: Kind,
    pub source: String,
    #[serde(default)]
    pub reading: Reading,
    #[serde(default)]
    pub last_notified: u64,
}

impl Config {
    pub fn normalize_metrics(&mut self) {
        self.settings_width = self.settings_width.clamp(360, 2200);
        self.settings_height = self.settings_height.clamp(240, 1800);
        self.logo_size = self.logo_size.clamp(32, 256);
    }

    pub fn reset_dev(&mut self) {
        self.settings_width = DEFAULT_SETTINGS_WIDTH;
        self.settings_height = DEFAULT_SETTINGS_HEIGHT;
        self.logo_size = DEFAULT_LOGO_SIZE;
    }

    pub fn reset_appearance(&mut self) {
        let defaults = Self::default();
        self.language = defaults.language;
        self.theme = defaults.theme;
        self.tray_orientation = defaults.tray_orientation;
        self.colored_icon = defaults.colored_icon;
        self.panel_width = defaults.panel_width;
        self.history_enabled = defaults.history_enabled;
        self.history_hidden = defaults.history_hidden;
        self.history_count = defaults.history_count;
        self.reset_dev();
        self.dev_tab = false;
    }

    pub fn add(&mut self, device: &Device) {
        if self.devices.iter().any(|d| d.id == device.id) {
            return;
        }
        self.devices.push(Selected {
            id: device.id.clone(),
            name: device.name.clone(),
            kind: device.kind,
            source: device.source.clone(),
            reading: device.reading.clone(),
            last_notified: 0,
        });
    }

    pub fn observe(&mut self, scan: &Scan) -> bool {
        let mut changed = false;
        for selected in &mut self.devices {
            if let Some(fresh) = scan
                .devices
                .iter()
                .find(|d| d.id == selected.id && d.has_reading())
            {
                if selected.reading != fresh.reading {
                    selected.reading = fresh.reading.clone();
                    changed = true;
                }
                if !fresh.reading.low(self.low_threshold) && selected.last_notified != 0 {
                    selected.last_notified = 0;
                    changed = true;
                }
            }
        }
        changed
    }

    pub fn minimum(&self) -> Option<Band> {
        self.devices.iter().filter_map(|d| d.reading.band()).min()
    }
}

fn default_width(value: &u32) -> bool {
    *value == DEFAULT_SETTINGS_WIDTH
}
fn default_height(value: &u32) -> bool {
    *value == DEFAULT_SETTINGS_HEIGHT
}
fn default_logo(value: &u32) -> bool {
    *value == DEFAULT_LOGO_SIZE
}
fn is_false(value: &bool) -> bool {
    !*value
}

pub fn home() -> PathBuf {
    std::env::var_os("HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("/tmp"))
}

pub fn config_dir() -> PathBuf {
    std::env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| home().join(".config"))
        .join("shinpi")
}

pub fn load() -> Config {
    let path = config_dir().join("config.json");
    let mut config: Config = match fs::read(&path) {
        Ok(bytes) => match serde_json::from_slice(&bytes) {
            Ok(config) => config,
            Err(_) => {
                let _ = fs::copy(&path, path.with_extension("json.bak"));
                Config::default()
            }
        },
        Err(_) => Config::default(),
    };
    config.panel_width = config.panel_width.clamp(280, 800);
    config.normalize_metrics();
    config.history_count = config.history_count.clamp(1, 20);
    let excess = config.history.len().saturating_sub(crate::notices::LIMIT);
    config.history.drain(..excess);
    config.tray_orientation = config.tray_orientation.min(3);
    config.low_threshold = config.low_threshold.min(100);
    config.repeat_minutes = config.repeat_minutes.clamp(1, 10080);
    for device in &mut config.devices {
        device.reading.percent = device.reading.percent.filter(|p| *p <= 100);
    }
    config
}

pub fn save(config: &Config) -> std::io::Result<()> {
    fs::create_dir_all(config_dir())?;
    let temporary = config_dir().join("config.json.tmp");
    let mut file = fs::OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .mode(0o600)
        .open(&temporary)?;
    file.write_all(&serde_json::to_vec_pretty(config)?)?;
    file.sync_all()?;
    fs::rename(temporary, config_dir().join("config.json"))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn dev_settings_round_trip_and_resets_preserve_device_and_notification_data() {
        let mut config: Config = serde_json::from_str("{}").unwrap();
        let json = serde_json::to_string(&config).unwrap();
        for key in ["settings_width", "settings_height", "logo_size", "dev_tab"] {
            assert!(!json.contains(key));
        }
        config.settings_width = 10;
        config.settings_height = 5000;
        config.logo_size = 1;
        config.dev_tab = true;
        config.theme = "dark".into();
        config.language = "Russian".into();
        config.panel_width = 450;
        config.notifications = false;
        config.low_threshold = 42;
        config.repeat_minutes = 120;
        let mut device = Device::new("mouse".into(), "My mouse".into(), Kind::Mouse, "UPower");
        device.reading.percent = Some(10);
        config.add(&device);
        config.devices[0].last_notified = 123;
        crate::notices::push(&mut config.history, "Kept notice".into(), 123, false);
        config.normalize_metrics();
        let mut restored: Config =
            serde_json::from_str(&serde_json::to_string(&config).unwrap()).unwrap();
        assert_eq!(
            (
                restored.settings_width,
                restored.settings_height,
                restored.logo_size
            ),
            (360, 1800, 32)
        );
        assert!(restored.dev_tab);
        restored.reset_dev();
        assert!(restored.dev_tab);
        assert_eq!(restored.theme, "dark");
        assert_eq!(restored.panel_width, 450);
        assert_eq!(
            (
                restored.settings_width,
                restored.settings_height,
                restored.logo_size
            ),
            (600, 620, 96)
        );
        restored.reset_appearance();
        assert!(!restored.dev_tab);
        assert!(restored.theme.is_empty() && restored.language.is_empty());
        assert_eq!(restored.panel_width, 300);
        assert!(!restored.notifications);
        assert_eq!((restored.low_threshold, restored.repeat_minutes), (42, 120));
        assert_eq!(restored.devices[0].name, "My mouse");
        assert_eq!(restored.devices[0].reading.percent, Some(10));
        assert_eq!(restored.devices[0].last_notified, 123);
        assert_eq!(restored.history[0].message, "Kept notice");
        assert_eq!(restored.history[0].at, 123);
    }

    #[test]
    fn selection_is_per_source_and_keeps_last_reading_and_customization() {
        let mut config: Config = serde_json::from_str("{\"theme\":\"dark\"}").unwrap();
        assert!(config.devices.is_empty());
        assert!(config.colored_icon);
        assert_eq!(config.panel_width, 300);
        assert_eq!(config.history_count, 3);
        assert!(config.history_enabled && !config.history_hidden);
        assert!(config.history.is_empty());
        let mut a = Device::new("mouse".into(), "Mouse".into(), Kind::Mouse, "UPower");
        a.reading.percent = Some(12);
        let mut b = a.clone();
        b.id = "HID:mouse".into();
        b.reading.percent = Some(85);
        config.add(&a);
        config.add(&a);
        config.add(&b);
        config.devices[0].name = "My mouse".into();
        config.devices[0].kind = Kind::Other;
        config.observe(&Scan {
            devices: vec![b.clone()],
            warnings: vec![],
        });
        assert_eq!(config.devices.len(), 2);
        assert_eq!(config.minimum(), Some(Band::Low));
        a.reading.percent = Some(60);
        config.observe(&Scan {
            devices: vec![a],
            warnings: vec![],
        });
        let restored: Config =
            serde_json::from_str(&serde_json::to_string(&config).unwrap()).unwrap();
        assert_eq!(restored.devices[0].name, "My mouse");
        assert_eq!(restored.devices[0].kind, Kind::Other);
        assert_eq!(restored.devices[0].reading.percent, Some(60));
        assert_eq!(restored.minimum(), Some(Band::High));
    }
}
