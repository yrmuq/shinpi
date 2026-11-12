use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
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

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Reading {
    pub percent: Option<u8>,
    pub level: Option<String>,
    pub charging: bool,
    pub approximate: bool,
}

impl Reading {
    pub fn percentage(&self) -> Option<u8> {
        self.percent.or_else(|| match self.level.as_deref()? {
            "Критический" => Some(10),
            "Низкий" => Some(25),
            "Средний" => Some(50),
            "Высокий" => Some(75),
            "Полный" => Some(100),
            _ => None,
        })
    }

    pub fn text(&self) -> String {
        let value = self
            .percentage()
            .map(|p| format!("{p}%"))
            .or_else(|| self.level.clone())
            .unwrap_or_else(|| "Нет данных".into());
        if self.charging {
            if self.percentage().is_some() {
                format!("ϟ {value}")
            } else {
                "Заряжается".into()
            }
        } else {
            value
        }
    }

    pub fn low(&self, threshold: u8) -> bool {
        !self.charging && self.percentage().is_some_and(|p| p <= threshold)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Band {
    Low = 1,
    Medium = 2,
    High = 3,
    Full = 4,
}

impl Reading {
    pub fn band(&self) -> Option<Band> {
        self.percentage().map(|percent| match percent {
            100 => Band::Full,
            51..=99 => Band::High,
            26..=50 => Band::Medium,
            _ => Band::Low,
        })
    }
}

#[derive(Clone, Debug, Serialize)]
pub struct Device {
    #[serde(skip)]
    pub id: String,
    pub name: String,
    pub kind: Kind,
    pub reading: Reading,
    pub source: String,
    pub status: String,
}

impl Device {
    pub fn new(key: String, name: String, kind: Kind, source: &str) -> Self {
        Self {
            id: format!("{source}:{key}"),
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
    pub fn insert(&mut self, device: Device) {
        self.devices.push(device);
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
    fn charge_bands_preserve_boundaries_and_text_levels() {
        for (percent, expected) in [
            (100, Band::Full),
            (99, Band::High),
            (75, Band::High),
            (51, Band::High),
            (50, Band::Medium),
            (26, Band::Medium),
            (25, Band::Low),
            (0, Band::Low),
        ] {
            assert_eq!(
                Reading {
                    percent: Some(percent),
                    ..Default::default()
                }
                .band(),
                Some(expected)
            );
        }
        let mut coarse = Reading {
            level: Some("Низкий".into()),
            ..Default::default()
        };
        assert!(!coarse.low(24));
        assert!(coarse.low(25));
        assert_eq!(coarse.band(), Some(Band::Low));
        coarse.charging = true;
        assert!(!coarse.low(100));
        assert_eq!(Reading::default().band(), None);
        assert_eq!(address("AA_BB_CC_DD_EE_FF"), address("aa:bb:cc:dd:ee:ff"));
        assert!(address("aa:bb:cc:dd:ee:ff:00").is_none());
    }

    #[test]
    fn estimated_levels_share_percentage_bands_and_thresholds() {
        for (level, percent, band) in [
            ("Критический", 10, Band::Low),
            ("Низкий", 25, Band::Low),
            ("Средний", 50, Band::Medium),
            ("Высокий", 75, Band::High),
            ("Полный", 100, Band::Full),
        ] {
            let mut reading = Reading {
                level: Some(level.into()),
                approximate: true,
                ..Default::default()
            };
            assert_eq!(reading.percentage(), Some(percent));
            assert_eq!(reading.text(), format!("{percent}%"));
            assert_eq!(reading.band(), Some(band));
            assert!(reading.low(percent));
            assert!(!reading.low(percent - 1));
            assert_eq!(reading.percent, None);
            reading.charging = true;
            assert_eq!(reading.text(), format!("ϟ {percent}%"));
            assert!(!reading.low(100));
            reading.percent = Some(0);
            assert_eq!(reading.percentage(), Some(0));
        }
        let unknown = Reading {
            level: Some("Unknown".into()),
            ..Default::default()
        };
        assert_eq!(unknown.percentage(), None);
        assert_eq!(unknown.band(), None);
        assert!(!unknown.low(100));
        assert_eq!(Reading::default().text(), "Нет данных");
        assert_eq!(
            Reading {
                charging: true,
                ..Default::default()
            }
            .text(),
            "Заряжается"
        );
    }
}
