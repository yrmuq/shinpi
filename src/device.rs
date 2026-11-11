use serde::Serialize;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
pub enum Kind {
    Mouse,
    Keyboard,
    Headset,
    Gamepad,
    Other,
}

impl Kind {
    pub fn icon(self) -> &'static str {
        match self {
            Self::Mouse => "input-mouse",
            Self::Keyboard => "input-keyboard",
            Self::Headset => "audio-headphones",
            Self::Gamepad => "input-gaming",
            Self::Other => "battery",
        }
    }

    pub fn from_name(name: &str) -> Self {
        let name = name.to_lowercase();
        if ["mouse", "zaopin", "logitech g30"]
            .iter()
            .any(|s| name.contains(s))
        {
            Self::Mouse
        } else if ["keyboard", "tester", "ciy"]
            .iter()
            .any(|s| name.contains(s))
        {
            Self::Keyboard
        } else if ["head", "ear", "mchose v9"]
            .iter()
            .any(|s| name.contains(s))
        {
            Self::Headset
        } else if ["game", "controller", "8bitdo", "xbox"]
            .iter()
            .any(|s| name.contains(s))
        {
            Self::Gamepad
        } else {
            Self::Other
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize)]
pub struct Reading {
    pub percent: Option<u8>,
    pub level: Option<String>,
    pub charging: bool,
    pub approximate: bool,
}

impl Reading {
    pub fn text(&self) -> String {
        let value = self
            .percent
            .map(|p| format!("{}{p}%", if self.approximate { "≈" } else { "" }))
            .or_else(|| self.level.clone())
            .unwrap_or_else(|| "Нет данных".into());
        if self.charging {
            if self.percent.is_some() {
                format!("ϟ {value}")
            } else {
                "Заряжается".into()
            }
        } else {
            value
        }
    }

    pub fn low(&self) -> bool {
        !self.charging
            && (self.percent.is_some_and(|p| p <= 15)
                || self
                    .level
                    .as_deref()
                    .is_some_and(|s| matches!(s, "Низкий" | "Критический")))
    }
}

#[derive(Clone, Debug, Serialize)]
pub struct Device {
    #[serde(skip)]
    pub keys: Vec<String>,
    pub name: String,
    pub kind: Kind,
    pub reading: Reading,
    pub source: String,
    pub status: String,
}

impl Device {
    pub fn new(key: String, name: String, kind: Kind, source: &str) -> Self {
        Self {
            keys: vec![key],
            name,
            kind,
            reading: Reading::default(),
            source: source.into(),
            status: String::new(),
        }
    }

    pub fn has_reading(&self) -> bool {
        self.reading.percent.is_some() || self.reading.level.is_some() || self.reading.charging
    }
}

#[derive(Default, Serialize)]
pub struct Scan {
    pub devices: Vec<Device>,
    pub warnings: Vec<String>,
}

impl Scan {
    pub fn insert(&mut self, next: Device) {
        if let Some(old) = self
            .devices
            .iter_mut()
            .find(|d| d.keys.iter().any(|key| next.keys.contains(key)))
        {
            if !old.has_reading()
                || (next.has_reading()
                    && next.reading.percent.is_some()
                    && old.reading.percent.is_none())
            {
                let keys = std::mem::take(&mut old.keys);
                *old = next;
                old.keys.extend(keys);
            } else {
                old.keys.extend(next.keys);
                old.reading.charging |= next.reading.charging;
                if old.kind == Kind::Other {
                    old.kind = next.kind;
                }
            }
            old.keys.sort();
            old.keys.dedup();
        } else {
            self.devices.push(next);
        }
    }
}

pub fn address(text: &str) -> Option<String> {
    let value = text.trim().replace('_', ":").to_ascii_lowercase();
    let bytes = value.as_bytes();
    (bytes.len() == 17
        && bytes.iter().enumerate().all(|(i, b)| {
            if i % 3 == 2 {
                *b == b':'
            } else {
                b.is_ascii_hexdigit()
            }
        }))
    .then(|| format!("bt:{value}"))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn merges_sources_by_identity_without_merging_same_named_devices() {
        let mut scan = Scan::default();
        let mut a = Device::new("bt:one".into(), "Mouse".into(), Kind::Mouse, "BlueZ");
        a.reading.percent = Some(0);
        scan.insert(a);
        let mut b = Device::new("bt:one".into(), "Mouse".into(), Kind::Mouse, "UPower");
        b.reading.level = Some("Низкий".into());
        scan.insert(b);
        scan.insert(Device::new(
            "bt:two".into(),
            "Mouse".into(),
            Kind::Mouse,
            "BlueZ",
        ));
        assert_eq!(scan.devices.len(), 2);
        assert_eq!(scan.devices[0].reading.text(), "0%");
        assert!(scan.devices[0].reading.low());
        assert_eq!(scan.devices[1].reading.text(), "Нет данных");
        assert_eq!(address("AA_BB_CC_DD_EE_FF"), address("aa:bb:cc:dd:ee:ff"));
        assert!(address("aa:bb:cc:dd:ee:ff:00").is_none());
    }
}
