use crate::device::{Band, Device, Kind, Reading, Scan};
use serde::{Deserialize, Serialize};
use std::{fs, io::Write, os::unix::fs::OpenOptionsExt, path::PathBuf};

#[derive(Clone, Deserialize, Serialize)]
#[serde(default)]
pub struct Config {
    pub theme: String,
    pub tray_orientation: u8,
    pub colored_icon: bool,
    pub notifications: bool,
    pub low_threshold: u8,
    pub repeat_minutes: u32,
    pub devices: Vec<Selected>,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            theme: String::new(),
            tray_orientation: 2,
            colored_icon: true,
            notifications: true,
            low_threshold: 15,
            repeat_minutes: 60,
            devices: Vec::new(),
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
    fn selection_is_per_source_and_keeps_last_reading_and_customization() {
        let mut config: Config = serde_json::from_str("{\"theme\":\"dark\"}").unwrap();
        assert!(config.devices.is_empty());
        assert!(config.colored_icon);
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
