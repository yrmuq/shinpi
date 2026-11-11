use crate::device::{address, Device, Kind, Reading, Scan};
use std::{collections::HashMap, fs, path::Path};
use zbus::{OwnedObjectPath, OwnedVariant};

type Props = HashMap<String, OwnedVariant>;
type Objects = HashMap<OwnedObjectPath, HashMap<String, Props>>;

fn string(p: &Props, name: &str) -> String {
    p.get(name)
        .and_then(|v| <&str>::try_from(v).ok())
        .unwrap_or_default()
        .into()
}
fn number(p: &Props, name: &str) -> u32 {
    p.get(name).and_then(|v| u32::try_from(v).ok()).unwrap_or(0)
}
fn flag(p: &Props, name: &str) -> Option<bool> {
    p.get(name).and_then(|v| bool::try_from(v).ok())
}
fn read(path: &Path, name: &str) -> String {
    fs::read_to_string(path.join(name))
        .unwrap_or_default()
        .trim()
        .into()
}

pub fn power_key(path: &Path) -> String {
    let unique = read(&path.join("device"), "uniq");
    if let Some(key) = address(&unique) {
        return key;
    }
    let name = path.file_name().unwrap_or_default().to_string_lossy();
    if let Some(key) = name
        .strip_prefix("hid-")
        .and_then(|s| s.strip_suffix("-battery"))
        .and_then(address)
    {
        return key;
    }
    let device = path
        .join("device")
        .canonicalize()
        .or_else(|_| path.canonicalize());
    format!(
        "sys:{}",
        device.unwrap_or_else(|_| path.to_owned()).display()
    )
}

fn sysfs(scan: &mut Scan, folder: &Path) {
    let Ok(entries) = fs::read_dir(folder) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if read(&path, "scope") != "Device" || read(&path, "present") == "0" {
            continue;
        }
        let model = read(&path, "model_name");
        let name = if model.is_empty() {
            "Беспроводное устройство".into()
        } else {
            model
        };
        let mut device = Device::new(
            power_key(&path),
            name.clone(),
            Kind::from_name(&name),
            "Linux power_supply",
        );
        device.reading.charging = read(&path, "status") == "Charging";
        device.reading.level = match read(&path, "capacity_level").as_str() {
            "Critical" => Some("Критический"),
            "Low" => Some("Низкий"),
            "Normal" => Some("Средний"),
            "High" => Some("Высокий"),
            "Full" => Some("Полный"),
            _ => None,
        }
        .map(str::to_owned);
        if device.reading.level.is_none() {
            device.reading.percent = read(&path, "capacity")
                .parse::<u8>()
                .ok()
                .filter(|p| *p <= 100);
        }
        device.reading.approximate = device.reading.level.is_some();
        scan.insert(device);
    }
}

async fn bluez(connection: &zbus::Connection, scan: &mut Scan) -> zbus::Result<()> {
    let proxy = zbus::Proxy::new(
        connection,
        "org.bluez",
        "/",
        "org.freedesktop.DBus.ObjectManager",
    )
    .await?;
    let objects: Objects = proxy.call("GetManagedObjects", &()).await?;
    for interfaces in objects.values() {
        let Some(info) = interfaces.get("org.bluez.Device1") else {
            continue;
        };
        if flag(info, "Connected") != Some(true) {
            continue;
        }
        let Some(key) = address(&string(info, "Address")) else {
            continue;
        };
        let alias = string(info, "Alias");
        let name = if alias.is_empty() {
            string(info, "Name")
        } else {
            alias
        };
        let name = if name.is_empty() || address(&name).is_some() {
            "Устройство Bluetooth".into()
        } else {
            name
        };
        let kind = match string(info, "Icon").as_str() {
            "input-mouse" => Kind::Mouse,
            "input-keyboard" => Kind::Keyboard,
            "audio-headset" | "audio-headphones" => Kind::Headset,
            "input-gaming" => Kind::Gamepad,
            _ => Kind::from_name(&name),
        };
        let mut device = Device::new(key, name, kind, "BlueZ");
        device.reading.percent = interfaces
            .get("org.bluez.Battery1")
            .and_then(|p| p.get("Percentage"))
            .and_then(|v| u8::try_from(v).ok())
            .filter(|p| *p <= 100);
        scan.insert(device);
    }
    Ok(())
}

fn upower_device(path: &str, props: &Props) -> Option<Device> {
    if flag(props, "IsPresent") == Some(false) || flag(props, "PowerSupply") != Some(false) {
        return None;
    }
    let kind = match number(props, "Type") {
        5 => Kind::Mouse,
        6 => Kind::Keyboard,
        17 | 18 => Kind::Headset,
        28 => Kind::Gamepad,
        12..=16 | 19..=22 => Kind::Other,
        _ => return None,
    };
    let model = string(props, "Model");
    let name = if model.trim().is_empty() {
        "Беспроводное устройство".into()
    } else {
        model
    };
    let serial = string(props, "Serial");
    let native = string(props, "NativePath");
    let native_name = Path::new(&native).file_name().unwrap_or_default();
    let battery_path = Path::new("/sys/class/power_supply").join(native_name);
    // UPower's object path may contain a renumbered hidpp_battery_N node.
    let key = address(&serial).unwrap_or_else(|| {
        if !serial.is_empty() {
            format!("serial:{serial}:{name}")
        } else if !native_name.is_empty() && battery_path.exists() {
            power_key(&battery_path)
        } else {
            path.to_owned()
        }
    });
    let mut device = Device::new(key, name, kind, "UPower");
    device.reading = Reading {
        percent: if number(props, "BatteryLevel") == 1 {
            props
                .get("Percentage")
                .and_then(|v| f64::try_from(v).ok())
                .filter(|v| v.is_finite() && (0.0..=100.0).contains(v))
                .map(|v| v.round() as u8)
        } else {
            None
        },
        level: match number(props, "BatteryLevel") {
            3 => Some("Низкий"),
            4 => Some("Критический"),
            6 => Some("Средний"),
            7 => Some("Высокий"),
            8 => Some("Полный"),
            _ => None,
        }
        .map(str::to_owned),
        charging: number(props, "State") == 1,
        approximate: matches!(number(props, "BatteryLevel"), 3..=8),
    };
    Some(device)
}

async fn upower(connection: &zbus::Connection, scan: &mut Scan) -> zbus::Result<()> {
    let root = zbus::Proxy::new(
        connection,
        "org.freedesktop.UPower",
        "/org/freedesktop/UPower",
        "org.freedesktop.UPower",
    )
    .await?;
    let paths: Vec<OwnedObjectPath> = root.call("EnumerateDevices", &()).await?;
    for path in paths {
        let properties = zbus::Proxy::new(
            connection,
            "org.freedesktop.UPower",
            path.as_str(),
            "org.freedesktop.DBus.Properties",
        )
        .await?;
        // Hot unplug can remove an object after enumeration.
        if let Ok(props) = properties
            .call::<_, _, Props>("GetAll", &("org.freedesktop.UPower.Device",))
            .await
        {
            if let Some(device) = upower_device(path.as_str(), &props) {
                scan.insert(device);
            }
        }
    }
    Ok(())
}

pub async fn scan() -> Scan {
    let mut scan = Scan::default();
    sysfs(&mut scan, Path::new("/sys/class/power_supply"));
    match zbus::connection::Builder::system()
        .method_timeout(std::time::Duration::from_secs(2))
        .build()
        .await
    {
        Ok(connection) => {
            if let Err(e) = upower(&connection, &mut scan).await {
                scan.warnings.push(format!("UPower: {e}"));
            }
            if let Err(e) = bluez(&connection, &mut scan).await {
                scan.warnings.push(format!("BlueZ: {e}"));
            }
        }
        Err(e) => scan
            .warnings
            .push(format!("Системный D-Bus недоступен: {e}")),
    }
    scan
}

#[cfg(test)]
mod tests {
    use super::*;
    struct BluetoothDevice;
    #[zbus::interface(name = "org.bluez.Device1")]
    impl BluetoothDevice {
        #[zbus(property)]
        fn connected(&self) -> bool {
            true
        }
        #[zbus(property)]
        fn address(&self) -> &str {
            "aa:bb:cc:dd:ee:ff"
        }
        #[zbus(property)]
        fn alias(&self) -> &str {
            "Test mouse"
        }
        #[zbus(property)]
        fn icon(&self) -> &str {
            "input-mouse"
        }
    }
    struct BluetoothBattery;
    #[zbus::interface(name = "org.bluez.Battery1")]
    impl BluetoothBattery {
        #[zbus(property)]
        fn percentage(&self) -> u8 {
            0
        }
    }
    struct PowerService;
    #[zbus::interface(name = "org.freedesktop.UPower")]
    impl PowerService {
        fn enumerate_devices(&self) -> Vec<OwnedObjectPath> {
            vec![OwnedObjectPath::try_from("/org/freedesktop/UPower/devices/mouse").unwrap()]
        }
    }
    struct PowerDevice;
    #[zbus::interface(name = "org.freedesktop.UPower.Device")]
    impl PowerDevice {
        #[zbus(property)]
        fn is_present(&self) -> bool {
            true
        }
        #[zbus(property)]
        fn power_supply(&self) -> bool {
            false
        }
        #[zbus(property, name = "Type")]
        fn device_type(&self) -> u32 {
            5
        }
        #[zbus(property)]
        fn battery_level(&self) -> u32 {
            3
        }
        #[zbus(property)]
        fn serial(&self) -> &str {
            "AA:BB:CC:DD:EE:FF"
        }
        #[zbus(property)]
        fn model(&self) -> &str {
            "Test mouse"
        }
    }

    #[test]
    fn dbus_sources_remain_separate_and_keep_real_zero() {
        use std::io::{BufRead, BufReader};
        struct Bus(std::process::Child);
        impl Drop for Bus {
            fn drop(&mut self) {
                let _ = self.0.kill();
                let _ = self.0.wait();
            }
        }
        let mut bus = Bus(std::process::Command::new("dbus-daemon")
            .args(["--session", "--nofork", "--print-address=1"])
            .stdout(std::process::Stdio::piped())
            .spawn()
            .expect("dbus-daemon"));
        let mut address = String::new();
        BufReader::new(bus.0.stdout.take().unwrap())
            .read_line(&mut address)
            .unwrap();
        tokio::runtime::Runtime::new().unwrap().block_on(async {
            let _server = zbus::connection::Builder::address(address.trim())
                .name("org.bluez")
                .name("org.freedesktop.UPower")
                .serve_at("/", zbus::fdo::ObjectManager)
                .serve_at("/org/bluez/hci0/dev_AA_BB_CC_DD_EE_FF", BluetoothDevice)
                .serve_at("/org/bluez/hci0/dev_AA_BB_CC_DD_EE_FF", BluetoothBattery)
                .serve_at("/org/freedesktop/UPower", PowerService)
                .serve_at("/org/freedesktop/UPower/devices/mouse", PowerDevice)
                .build()
                .await
                .unwrap();
            let client = zbus::connection::Builder::address(address.trim())
                .build()
                .await
                .unwrap();
            let mut scan = Scan::default();
            upower(&client, &mut scan).await.unwrap();
            bluez(&client, &mut scan).await.unwrap();
            assert_eq!(scan.devices.len(), 2);
            assert_ne!(scan.devices[0].id, scan.devices[1].id);
            assert!(scan.devices[0].id.contains("aa:bb:cc:dd:ee:ff"));
            assert_eq!(scan.devices[0].reading.text(), "Низкий");
            assert_eq!(scan.devices[1].reading.text(), "0%");
            assert_eq!(scan.devices[1].kind, Kind::Mouse);
        });
    }
    #[test]
    fn upower_preserves_coarse_unknown_and_zero_without_inventing_percentages() {
        let mut p = Props::from([
            ("Type".into(), OwnedVariant::from(5u32)),
            ("PowerSupply".into(), OwnedVariant::from(false)),
            ("BatteryLevel".into(), OwnedVariant::from(3u32)),
            ("Percentage".into(), OwnedVariant::from(10.0f64)),
        ]);
        let d = upower_device("/mouse", &p).unwrap();
        assert_ne!(d.id, upower_device("/another_mouse", &p).unwrap().id);
        assert_eq!(d.reading.percent, None);
        assert_eq!(d.reading.text(), "Низкий");
        p.insert("BatteryLevel".into(), OwnedVariant::from(0u32));
        assert!(!upower_device("/mouse", &p).unwrap().has_reading());
        p.insert("BatteryLevel".into(), OwnedVariant::from(1u32));
        p.insert("Percentage".into(), OwnedVariant::from(0.0f64));
        assert_eq!(
            upower_device("/mouse", &p).unwrap().reading.percent,
            Some(0)
        );
        p.insert("Percentage".into(), OwnedVariant::from(f64::NAN));
        assert_eq!(upower_device("/mouse", &p).unwrap().reading.percent, None);
        p.insert("PowerSupply".into(), OwnedVariant::from(true));
        assert!(upower_device("/mouse", &p).is_none());
    }
}
