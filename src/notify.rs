use crate::{config::Config, device::Scan};
use std::{collections::HashMap, time::Duration};

pub fn due(
    config: &Config,
    fresh: &Scan,
    now: u64,
    catalog: &crate::i18n::Catalog,
) -> Vec<(String, String)> {
    if !config.notifications {
        return vec![];
    }
    config
        .devices
        .iter()
        .filter(|selected| {
            fresh
                .devices
                .iter()
                .any(|d| d.id == selected.id && d.reading.low(config.low_threshold))
                && (selected.last_notified == 0
                    || now < selected.last_notified
                    || now.saturating_sub(selected.last_notified)
                        >= u64::from(config.repeat_minutes) * 60)
        })
        .map(|d| {
            (
                d.id.clone(),
                catalog.trf(
                    "{}: low battery — {}",
                    &[&d.name, &catalog.reading(&d.reading)],
                ),
            )
        })
        .collect()
}

pub async fn send(title: &str, message: &str) -> zbus::Result<()> {
    let connection = zbus::connection::Builder::session()
        .method_timeout(Duration::from_secs(3))
        .build()
        .await?;
    // Device names are user-controlled; notification bodies support markup.
    let message = message
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;");
    let hints = HashMap::from([
        (
            "desktop-entry",
            zbus::Variant::from("io.github.yrmuq.shinpi"),
        ),
        ("urgency", zbus::Variant::from(1u8)),
    ]);
    let actions: Vec<&str> = vec![];
    connection
        .call_method(
            Some("org.freedesktop.Notifications"),
            "/org/freedesktop/Notifications",
            Some("org.freedesktop.Notifications"),
            "Notify",
            &(
                "shinpi",
                0u32,
                "battery-low",
                title,
                message.as_str(),
                actions,
                hints,
                -1i32,
            ),
        )
        .await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::device::{Device, Kind};
    #[test]
    fn common_interval_and_threshold_preserve_coarse_levels_and_suppress_stale_and_charging() {
        let catalog = crate::i18n::Catalog::load("Russian");
        let mut percent = Device::new("one".into(), "Mouse".into(), Kind::Mouse, "HID");
        percent.reading.percent = Some(15);
        let mut coarse = Device::new("two".into(), "Pad".into(), Kind::Gamepad, "UPower");
        coarse.reading.level = Some("Низкий".into());
        let mut scan = Scan {
            devices: vec![percent, coarse],
            warnings: vec![],
        };
        let mut config = Config::default();
        for device in &scan.devices {
            config.add(device);
        }
        assert_eq!(due(&config, &scan, 10000, &catalog).len(), 2);
        for selected in &mut config.devices {
            selected.last_notified = 10000;
        }
        assert!(due(&config, &scan, 13599, &catalog).is_empty());
        let restored: Config =
            serde_json::from_str(&serde_json::to_string(&config).unwrap()).unwrap();
        assert_eq!(due(&restored, &scan, 13600, &catalog).len(), 2);
        config.low_threshold = 5;
        assert_eq!(due(&config, &scan, 13600, &catalog).len(), 1);
        config.repeat_minutes = 120;
        assert!(due(&config, &scan, 13600, &catalog).is_empty());
        assert!(due(&config, &Scan::default(), 20000, &catalog).is_empty());
        scan.devices[1].reading.charging = true;
        assert!(due(&config, &scan, 20000, &catalog).is_empty());
        config.notifications = false;
        assert!(due(&config, &scan, 30000, &catalog).is_empty());
    }
}
