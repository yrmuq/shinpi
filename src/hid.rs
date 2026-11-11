use crate::device::{address, Device, Kind, Reading, Scan};
use hidapi::{BusType, DeviceInfo, HidApi, HidDevice};
use std::{
    fs,
    path::Path,
    time::{Duration, Instant},
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Protocol {
    Compx,
    Hidpp,
    V9,
    Xbox,
}

fn protocol(info: &DeviceInfo) -> Option<Protocol> {
    match (
        info.vendor_id(),
        info.product_id(),
        info.usage_page(),
        info.usage(),
    ) {
        (0x3554, 0xf517 | 0xf524 | 0xf50d | 0xf527 | 0xf526 | 0xf50f, 0xff02, 2) => {
            Some(Protocol::Compx)
        }
        (0x291d, 0x385d, 0xff90, 1) => Some(Protocol::V9),
        (0x046d, 0xc53f, 0xff00, 1) => Some(Protocol::Hidpp),
        (0x2dc8, 0x6003 | 0x6006 | 0x3106, 1, 5) | (0x045e, 0x02e0 | 0x02fd | 0x0b13, 1, 5)
            if matches!(info.bus_type(), BusType::Bluetooth) =>
        {
            Some(Protocol::Xbox)
        }
        _ => None,
    }
}

fn send(device: &HidDevice, bytes: &[u8]) -> Result<(), String> {
    match device.write(bytes) {
        Ok(n) if n == bytes.len() => Ok(()),
        Ok(_) => Err("Неполная отправка запроса".into()),
        Err(error) => Err(error.to_string()),
    }
}

fn receive<T>(device: &HidDevice, parse: impl Fn(&[u8]) -> Option<T>) -> Result<T, String> {
    let deadline = Instant::now() + Duration::from_millis(800);
    let mut buffer = [0u8; 128];
    while Instant::now() < deadline {
        let timeout = deadline
            .saturating_duration_since(Instant::now())
            .as_millis()
            .clamp(1, 80) as i32;
        let length = device
            .read_timeout(&mut buffer, timeout)
            .map_err(|e| e.to_string())?;
        if let Some(value) = parse(&buffer[..length]) {
            return Ok(value);
        }
    }
    Err("Нет ответа: устройство спит или недоступно".into())
}

fn compx_request(command: u8, payload: &[u8]) -> [u8; 17] {
    let mut frame = [0u8; 17];
    frame[0] = 8;
    frame[1] = command;
    frame[5] = payload.len() as u8;
    frame[6..6 + payload.len()].copy_from_slice(payload);
    frame[16] = 0x55u8.wrapping_sub(frame[..16].iter().copied().fold(0u8, u8::wrapping_add));
    frame
}

fn compx_data(frame: &[u8], command: u8) -> Option<&[u8]> {
    if frame.len() != 17
        || frame[..3] != [8, command, 0]
        || frame[5] > 10
        || frame.iter().copied().fold(0u8, u8::wrapping_add) != 0x55
    {
        return None;
    }
    Some(&frame[6..6 + usize::from(frame[5])])
}

fn compx(device: &HidDevice, pid: u16) -> Result<Reading, String> {
    let mut challenge = [0u8; 8];
    use std::io::Read;
    fs::File::open("/dev/urandom")
        .and_then(|mut f| f.read_exact(&mut challenge[..4]))
        .map_err(|e| e.to_string())?;
    send(device, &compx_request(1, &challenge))?;
    let model = receive(device, |p| {
        compx_data(p, 1)
            .filter(|d| d.len() >= 7)
            .map(|d| (d[4], d[5]))
    })?;
    if model != (7, 5) && pid != 0xf517 {
        return Err("Модель ZAOPIN Z2 Max не подтверждена".into());
    }
    std::thread::sleep(Duration::from_millis(10));
    send(device, &compx_request(3, &[]))?;
    if !receive(device, |p| {
        compx_data(p, 3).and_then(|d| d.first()).map(|v| *v != 0)
    })? {
        return Err("Устройство отключено или спит".into());
    }
    std::thread::sleep(Duration::from_millis(10));
    send(device, &compx_request(4, &[]))?;
    receive(device, |p| {
        let data = compx_data(p, 4)?;
        if data.len() < 2 || data[0] > 100 || data[1] > 1 {
            return None;
        }
        Some(Reading {
            percent: Some(data[0]),
            charging: data[1] == 1,
            ..Default::default()
        })
    })
}

fn v9_reading(p: &[u8]) -> Option<Result<Reading, String>> {
    if p.len() < 4 || p[..2] != [0x55, 0x65] {
        return None;
    }
    match p[3] {
        2 if p[2] <= 100 => Some(Ok(Reading {
            percent: Some(p[2]),
            approximate: true,
            ..Default::default()
        })),
        // Cable reports can contain a frozen percentage; retain only the power state.
        0 | 3 if !(p[2] == 1 && p[3..].iter().all(|v| *v == 0)) => Some(Ok(Reading {
            charging: true,
            ..Default::default()
        })),
        0x1a => Some(Err("Устройство отключено".into())),
        _ => None,
    }
}

fn xbox_reading(p: &[u8]) -> Option<Reading> {
    if p.len() != 2 || p[0] != 4 || p[1] > 15 || matches!(p[1] >> 2, 0 | 3) {
        return None;
    }
    let level = ["Критический", "Низкий", "Средний", "Высокий"][usize::from(p[1] & 3)];
    Some(Reading {
        level: Some(level.into()),
        approximate: true,
        ..Default::default()
    })
}

fn hidpp_reply(p: &[u8], feature: u8) -> Option<u8> {
    (p.len() >= 7 && matches!(p[0], 0x10 | 0x11) && p[1..4] == [1, feature, 0x0d]).then(|| p[4])
}

fn query(device: &HidDevice, protocol: Protocol, pid: u16) -> Result<Reading, String> {
    match protocol {
        Protocol::Compx => compx(device, pid),
        Protocol::V9 => {
            let mut packet = [0u8; 64];
            packet[..3].copy_from_slice(&[0x55, 0x65, 1]);
            send(device, &packet)?;
            receive(device, v9_reading)?
        }
        Protocol::Hidpp => {
            send(device, &[0x10, 1, 0, 0x0d, 0x10, 0, 0])?;
            let feature = receive(device, |p| hidpp_reply(p, 0))?;
            if feature == 0 {
                return Err("HID++ Battery Status недоступен".into());
            }
            send(device, &[0x10, 1, feature, 0x0d, 0, 0, 0])?;
            let percent = receive(device, |p| hidpp_reply(p, feature).filter(|p| *p <= 100))?;
            Ok(Reading {
                percent: Some(percent),
                approximate: true,
                ..Default::default()
            })
        }
        Protocol::Xbox => receive(device, xbox_reading),
    }
}

pub fn scan(scan: &mut Scan) {
    let api = match HidApi::new() {
        Ok(api) => api,
        Err(e) => {
            scan.warnings.push(format!("HID: {e}"));
            return;
        }
    };
    let mut paths = std::collections::HashSet::new();
    for info in api.device_list() {
        let Some(protocol) = protocol(info) else {
            continue;
        };
        if !paths.insert(info.path().to_bytes()) {
            continue;
        }
        let (name, kind, source) = match protocol {
            Protocol::Compx => ("ZAOPIN Z2 Max", Kind::Mouse, "USB HID · Compx"),
            Protocol::Hidpp => ("Logitech G304 / G305", Kind::Mouse, "USB HID · HID++"),
            Protocol::V9 => ("MCHOSE V9 PRO", Kind::Headset, "USB HID · V9"),
            Protocol::Xbox => ("Геймпад", Kind::Gamepad, "Bluetooth HID"),
        };
        let raw_path = info.path().to_string_lossy();
        let node = Path::new(raw_path.as_ref()).file_name().unwrap_or_default();
        let sysfs = Path::new("/sys/class/hidraw").join(node).join("device");
        let unique = fs::read_to_string(sysfs.join("uniq")).unwrap_or_default();
        let key = address(&unique)
            .or_else(|| info.serial_number().and_then(address))
            .unwrap_or_else(|| format!("sys:{}", sysfs.canonicalize().unwrap_or(sysfs).display()));
        if scan
            .devices
            .iter()
            .any(|d| d.keys.contains(&key) && d.has_reading())
        {
            continue;
        }
        let mut row = Device::new(key, name.into(), kind, source);
        let result = info
            .open_device(&api)
            .map_err(|e| e.to_string())
            .and_then(|device| query(&device, protocol, info.product_id()));
        match result {
            Ok(reading) => row.reading = reading,
            Err(error) => {
                row.status = if error.to_ascii_lowercase().contains("permission denied")
                    || error.contains("os error 13")
                {
                    "Нужен доступ к HID: установите правила udev и переподключите приёмник".into()
                } else {
                    error
                };
            }
        }
        scan.insert(row);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn wire_replies_reject_echoes_corruption_and_other_requests() {
        let p = compx_request(4, &[76, 1, 0, 0]);
        assert_eq!(compx_data(&p, 4), Some([76, 1, 0, 0].as_slice()));
        for index in 0..17 {
            let mut bad = p;
            bad[index] ^= 1;
            assert!(compx_data(&bad, 4).is_none());
        }
        assert!(compx_data(&p[..8], 4).is_none());
        assert!(compx_data(&p, 3).is_none());
        assert!(v9_reading(&[0x55, 0x65, 1, 0, 0]).is_none());
        assert!(v9_reading(&[0x55, 0x65, 101, 2]).is_none());
        assert_eq!(
            v9_reading(&[0x55, 0x65, 90, 2]).unwrap().unwrap().text(),
            "≈90%"
        );
        assert_eq!(
            v9_reading(&[0x55, 0x65, 50, 3]).unwrap().unwrap().text(),
            "Заряжается"
        );
        assert!(v9_reading(&[0x55, 0x65, 0, 0x1a]).unwrap().is_err());
        assert_eq!(xbox_reading(&[4, 6]).unwrap().percent, None);
        assert!(xbox_reading(&[4, 0xff]).is_none());
        assert_eq!(hidpp_reply(&[0x10, 1, 5, 0x0d, 90, 0, 0], 5), Some(90));
        assert!(hidpp_reply(&[0x10, 1, 5, 0x0b, 90, 0, 0], 5).is_none());
    }
}
