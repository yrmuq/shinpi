use crate::{
    config::{Config, Selected},
    device::Scan,
};
use std::{collections::HashMap, time::Duration};

pub fn due(
    config: &Config,
    fresh: &Scan,
    now: u64,
    catalog: &crate::i18n::Catalog,
) -> Vec<(String, String)> {
    config
        .devices
        .iter()
        .filter(|selected| blocked(config, selected, fresh, now).is_none())
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

fn blocked(config: &Config, selected: &Selected, fresh: &Scan, now: u64) -> Option<&'static str> {
    if !config.notifications {
        return Some("Notifications are disabled.");
    }
    let Some(device) = fresh.devices.iter().find(|d| d.id == selected.id) else {
        return Some("No current reading from the selected source.");
    };
    if device.reading.charging {
        return Some("The device is charging.");
    }
    let Some(percent) = device.reading.percentage() else {
        return Some("Battery level is unavailable.");
    };
    if percent > config.low_threshold {
        return Some("Battery level is above the threshold.");
    }
    if selected.last_notified != 0
        && now >= selected.last_notified
        && now - selected.last_notified < u64::from(config.repeat_minutes) * 60
    {
        return Some("Waiting for the repeat interval.");
    }
    None
}

pub fn diagnostics(
    config: &Config,
    fresh: &Scan,
    now: u64,
    catalog: &crate::i18n::Catalog,
) -> String {
    if config.devices.is_empty() {
        return catalog.tr("No devices are selected.");
    }
    config
        .devices
        .iter()
        .map(|selected| {
            let reading = fresh
                .devices
                .iter()
                .find(|d| d.id == selected.id)
                .map(|d| catalog.reading(&d.reading))
                .unwrap_or_else(|| catalog.tr("No data"));
            catalog.trf(
                "{} ({}): {} — {}",
                &[
                    &selected.name,
                    &selected.source,
                    &reading,
                    &catalog.tr(blocked(config, selected, fresh, now)
                        .unwrap_or("Ready to send a low-battery notification.")),
                ],
            )
        })
        .collect::<Vec<_>>()
        .join("\n\n")
}

pub async fn send(title: &str, message: &str) -> zbus::Result<u32> {
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
    let reply = connection
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
    reply.body().deserialize()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::device::{Device, Kind};
    #[test]
    fn diagnostics_explain_why_a_saved_twenty_percent_reading_cannot_notify() {
        let catalog = crate::i18n::Catalog::load("English");
        let mut headset = Device::new("test".into(), "Headphones".into(), Kind::Headset, "BlueZ");
        headset.reading.percent = Some(20);
        let mut config = Config::default();
        config.add(&headset);
        config.low_threshold = 20;
        let mut fresh = Scan {
            devices: vec![],
            warnings: vec![],
        };
        assert!(diagnostics(&config, &fresh, 100, &catalog).contains("No current reading"));
        assert!(due(&config, &fresh, 100, &catalog).is_empty());
        fresh.devices.push(headset);
        assert!(diagnostics(&config, &fresh, 100, &catalog).contains("Ready to send"));
        assert_eq!(due(&config, &fresh, 100, &catalog).len(), 1);
        config.devices[0].last_notified = 100;
        assert!(diagnostics(&config, &fresh, 101, &catalog).contains("Waiting for the repeat"));
        fresh.devices[0].reading.charging = true;
        assert!(diagnostics(&config, &fresh, 4000, &catalog).contains("device is charging"));
    }

    #[test]
    fn changed_rules_notify_at_twenty_percent_and_repeat_after_one_minute() {
        let catalog = crate::i18n::Catalog::load("English");
        let mut headset = Device::new(
            "headset".into(),
            "Headphones".into(),
            Kind::Headset,
            "BlueZ",
        );
        headset.reading.percent = Some(20);
        let mut scan = Scan {
            devices: vec![headset],
            warnings: vec![],
        };
        let mut config = Config::default();
        config.add(&scan.devices[0]);
        assert!(due(&config, &scan, 10000, &catalog).is_empty());
        config.low_threshold = 20;
        config.repeat_minutes = 1;
        let first = due(&config, &scan, 10000, &catalog);
        assert_eq!(first.len(), 1);
        assert!(first[0].1.contains("20%"));
        config.devices[0].last_notified = 10000;
        assert!(due(&config, &scan, 10059, &catalog).is_empty());
        assert_eq!(due(&config, &scan, 10060, &catalog).len(), 1);
        assert!(due(&config, &Scan::default(), 10060, &catalog).is_empty());
        scan.devices[0].reading.charging = true;
        assert!(due(&config, &scan, 10060, &catalog).is_empty());
    }

    #[test]
    fn common_interval_and_threshold_include_estimates_and_suppress_stale_and_charging() {
        let catalog = crate::i18n::Catalog::load("Russian");
        let mut percent = Device::new("one".into(), "Mouse".into(), Kind::Mouse, "HID");
        percent.reading.percent = Some(15);
        let mut coarse = Device::new("two".into(), "Pad".into(), Kind::Gamepad, "UPower");
        coarse.reading.level = Some("Критический".into());
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
        assert!(due(&config, &scan, 13600, &catalog).is_empty());
        config.low_threshold = 10;
        config.repeat_minutes = 120;
        assert!(due(&config, &scan, 13600, &catalog).is_empty());
        assert!(due(&config, &Scan::default(), 20000, &catalog).is_empty());
        let critical = due(&config, &scan, 20000, &catalog);
        assert_eq!(critical.len(), 1);
        assert!(critical[0].1.contains("10%"));
        scan.devices[1].reading.charging = true;
        assert!(due(&config, &scan, 20000, &catalog).is_empty());
        config.notifications = false;
        assert!(due(&config, &scan, 30000, &catalog).is_empty());
    }
}
