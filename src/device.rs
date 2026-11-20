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
            50..=99 => Band::High,
            26..=49 => Band::Medium,
            _ => Band::Low,
        })
    }
}

#[derive(Clone, Debug, Serialize)]
pub struct Device {
    #[serde(skip)]
    pub id: String,
    #[serde(skip)]
    pub native_id: Option<String>,
    #[serde(skip)]
    pub updated_at: u64,
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
            native_id: None,
            updated_at: 0,
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

    pub fn matches_id(&self, id: &str) -> bool {
        same_id(&self.id, id)
            || self
                .native_id
                .as_deref()
                .is_some_and(|native| same_id(native, id))
    }

    fn reading_priority(&self) -> (bool, u64, u32) {
        let path = self.native_id.as_deref().unwrap_or(&self.id);
        let instance = usb_hid_key(path).map_or(0, |(_, instance)| instance);
        (self.has_reading(), self.updated_at, instance)
    }
}

pub fn same_id(left: &str, right: &str) -> bool {
    left == right
        || usb_hid_key(left)
            .is_some_and(|(key, _)| usb_hid_key(right).is_some_and(|(other, _)| key == other))
}

fn usb_hid_key(id: &str) -> Option<(String, u32)> {
    let (source, path) = id.split_once(":sys:/sys/devices/")?;
    if path.starts_with("virtual/") {
        return None;
    }
    let mut usb = false;
    let mut instance = None;
    let mut key = format!("{source}:sys:/sys/devices");
    for segment in path.split('/') {
        usb |= segment
            .strip_prefix("usb")
            .is_some_and(|number| !number.is_empty() && number.bytes().all(|b| b.is_ascii_digit()));
        let stable = segment.split_once('.').and_then(|(name, suffix)| {
            let model = name.strip_prefix("0003:")?;
            let (vendor, product) = model.split_once(':')?;
            let hex = |value: &str| value.bytes().all(|b| b.is_ascii_hexdigit());
            if usb
                && vendor.len() == 4
                && product.len() == 4
                && hex(vendor)
                && hex(product)
                && suffix.len() >= 4
                && hex(suffix)
            {
                instance = u32::from_str_radix(suffix, 16).ok();
                Some(name)
            } else {
                None
            }
        });
        // Linux appends an enumeration counter to each USB HID device name.
        key.push('/');
        key.push_str(stable.unwrap_or(segment));
    }
    Some((key, instance?))
}

#[derive(Default, Serialize)]
pub struct Scan {
    pub devices: Vec<Device>,
    pub warnings: Vec<String>,
}

impl Scan {
    pub fn insert(&mut self, mut device: Device) {
        if let Some(existing) = self
            .devices
            .iter_mut()
            .find(|row| row.matches_id(&device.id) || device.matches_id(&row.id))
        {
            if device.reading_priority() > existing.reading_priority() {
                std::mem::swap(existing, &mut device);
            }
            // Keep the stronger ID and the path of the record supplying the reading.
            if existing.native_id.is_none() {
                if existing.id == device.id {
                    existing.native_id = device.native_id;
                } else if device.native_id.is_some() {
                    existing.native_id = Some(std::mem::replace(&mut existing.id, device.id));
                }
            }
        } else {
            self.devices.push(device);
        }
    }

    pub fn find(&self, id: &str) -> Option<&Device> {
        if let Some(exact) = self.devices.iter().find(|row| row.id == id) {
            return Some(exact);
        }
        let mut matches = self.devices.iter().filter(|row| row.matches_id(id));
        let first = matches.next()?;
        matches.next().is_none().then_some(first)
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
    fn same_source_keeps_the_newest_reading_for_reenumerated_usb_hid() {
        let port = "sys:/sys/devices/pci0000:00/usb1/1-2/1-2:1.0/0003:046D:C53F";
        let mut old = Device::new(
            format!("{port}.000A"),
            "Mouse".into(),
            Kind::Mouse,
            "UPower",
        );
        old.reading.percent = Some(20);
        old.updated_at = 100;
        let mut current = Device::new(
            "serial:12345678:Mouse".into(),
            "Mouse".into(),
            Kind::Mouse,
            "UPower",
        );
        current.native_id = Some(format!("UPower:{port}.000B"));
        current.reading.percent = Some(100);
        current.updated_at = 101;

        let mut scan = Scan::default();
        scan.insert(current.clone());
        scan.insert(old.clone());
        assert_eq!(scan.devices.len(), 1);
        let found = scan.find(&old.id).unwrap();
        assert_eq!(found.id, current.id);
        assert_eq!(found.reading.percent, Some(100));
        assert!(!current.matches_id(&old.id.replace("1-2", "1-3")));
        assert!(!current.matches_id(&old.id.replace("1.0", "1.1")));
        assert!(!current.matches_id(&old.id.replace("C53F", "C540")));
        assert!(!current.matches_id(&old.id.replace("UPower:", "Linux power_supply:")));

        // The scan also keeps the serial-based ID if only the path record has fresh data.
        old.updated_at = 102;
        scan.insert(old.clone());
        assert_eq!(scan.devices.len(), 1);
        let found = scan.find(&old.id).unwrap();
        assert_eq!(found.id, current.id);
        assert_eq!(found.reading.percent, Some(20));

        let mut current = old.clone();
        current.id = format!("UPower:{port}.000C");
        current.reading.percent = Some(100);
        let mut scan = Scan::default();
        scan.insert(old.clone());
        scan.insert(current);
        assert_eq!(scan.devices.len(), 1);
        assert_eq!(scan.find(&old.id).unwrap().reading.percent, Some(100));
    }
    #[test]
    fn charge_bands_preserve_boundaries_and_text_levels() {
        for (percent, expected) in [
            (100, Band::Full),
            (99, Band::High),
            (75, Band::High),
            (51, Band::High),
            (50, Band::High),
            (49, Band::Medium),
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
            ("Средний", 50, Band::High),
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
