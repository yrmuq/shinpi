use serde::{Deserialize, Serialize};
use std::{fs, io::Write, os::unix::fs::OpenOptionsExt, path::PathBuf};

#[derive(Default, Deserialize, Serialize)]
#[serde(default)]
pub struct Config {
    pub theme: String,
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
    fs::read(config_dir().join("config.json"))
        .ok()
        .and_then(|s| serde_json::from_slice(&s).ok())
        .unwrap_or_default()
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
