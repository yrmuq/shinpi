<div align="center">
  <img src="assets/icon.svg" alt="shinpi" width="80">
  <h1>shinpi</h1>

  [![License: GPL v3+](https://img.shields.io/badge/License-GPLv3%2B-blue.svg)](https://www.gnu.org/licenses/gpl-3.0)

  <p>A native Linux tray battery monitor using Rust, Slint and FemtoVG. Each battery source is listed separately; you choose which devices to show. Native surfaces, no browser engine.</p>
</div>

---

> [!NOTE]
> Initial implementation for KDE Plasma. Development builds are attached to a draft release and are visible to repository maintainers. HID support requires verification on physical hardware; this repository does not claim tested compatibility with every listed receiver.

---

## Features

- Only added devices are shown, with a centered type icon and a customizable name on the left, battery percentage or reported level on the right. Charging and approximate readings are marked explicitly; missing data is never displayed as `0%`.
- KDE colors, fonts and icon theme, with a rounded popup, slide animation and pin control. KWin positions the popup beside the panel; other compositors choose the placement themselves. Only Settings has a taskbar entry; its device picker and editor are child dialogs. Settings and dialogs have fixed sizes; diagnostics wrap and scroll within the window.
- Background refresh every 30 seconds through Linux `power_supply`, BlueZ `Battery1` and UPower. Sources are never merged or switched automatically: each discovered source is a separate choice. Added devices keep their last readings when disconnected, including across application restarts.
- Read-only HID battery queries for ZAOPIN Z2 Max Compx receivers, Logitech G304/G305 `HID++`, MCHOSE V9 PRO and passive Bluetooth Xbox-compatible reports from supported 8BitDo/Xbox identities. CIY Tester 68 and other peripherals work when the system supplies their battery reading. Unknown receivers are never queried.
- The tray battery shows the lowest reported band among added devices, including saved readings. Color mode is the default: `100%` full green, `51–99%` three-quarter green, `26–50%` half yellow, `0–25%` quarter red. A reported low or critical text level is red. Monochrome mode uses the same fill steps with theme contrast; unknown readings have no colored fill. Four orientations are available: horizontal, horizontal reversed, vertical and vertical reversed.
- Low-battery notifications are enabled by default at `≤15%`, with one shared repeat interval of 60 minutes. Settings allow `0–100%` and `1–10080` minutes. Text-only low/critical levels ignore the percentage threshold. Charging and disconnected sources do not trigger alerts. Successful notification times are saved to avoid immediate repeats after restart.
- Optional notification history above the device popup, with relative times and individual dismissal. Both panels share a configurable width of `280–800` pixels. Show the latest `1–20` records (three by default), collapse them with the triangle or disable the history panel. The last 20 delivered low-battery notifications are saved independently of panel visibility.
- System, light and dark themes, device choices, custom names/icons and last readings are saved in `~/.config/shinpi/config.json` (`0600`, or under `XDG_CONFIG_HOME`). One graphical instance per user.

---

## Controls & Shortcuts

| Input | Action |
| --- | --- |
| Left click on tray | Open or close the device window. |
| Right click on tray | Refresh, Settings or Quit. |
| Refresh icon | Request a fresh scan without closing the window. |
| Gear icon | Open appearance, panels, notifications, added devices and diagnostics. |
| Triangle | Show or hide notification history; absent when the history panel is disabled. |
| `×` on a history record | Dismiss that record. |
| Panels tab in Settings | Set the shared width, history visibility and number of displayed records. |
| `Add` in the empty popup | Open Settings and the device picker. Hidden once a device is added; returns when all devices are removed. |
| `Add` in Settings | Open discovered devices with their source and current charge; add the preferred variant. |
| `+` in the device picker | Add that source. A checkmark indicates an already added source. |
| Pencil in Settings | Edit the device name and type icon. |
| Remove in Settings | Remove that device from the display; it can be added again later. |
| Pin icon | Keep the popup open when focus moves elsewhere. |
| `Escape` | Close the unpinned popup. |
| `Tab`, `Enter`, `Space` | Focus and activate toolbar controls. |
| `--window` | Open the popup at startup. |
| `--once` | Print current readings and source warnings as JSON without starting the GUI. |
| `--print-udev-rules` | Print the HID access rules. |
| `--help` | Show command-line usage. |

---

## Installation & Usage

| Format | `x86_64` | `arm64` |
| --- | :---: | :---: |
| `AppImage` | ✅ | ❌ |
| `Flatpak` | ❌ | ❌ |

### AppImage

The `Development build` workflow builds `dev` and attaches `shinpi-x86_64.AppImage` and its `SHA-256` checksum to the `shinpi dev` draft in [Releases](https://github.com/yrmuq/shinpi/releases). It keeps the package version unchanged. Download the AppImage from that draft and integrate it with `Gear Lever` ([Flathub](https://flathub.org/apps/it.mijorus.gearlever)) or launch it with:

```sh
chmod +x shinpi-x86_64.AppImage
./shinpi-x86_64.AppImage
```

On systems without `FUSE`, use `./shinpi-x86_64.AppImage --appimage-extract-and-run`.

### Flatpak

Planned; no `Flatpak` package is currently provided.

### HID access

The application runs as the regular user. If a supported receiver reports an access error, install `packaging/70-shinpi.rules` for the active local seat, then reconnect the receiver or Bluetooth controller:

```sh
sudo install -m 0644 packaging/70-shinpi.rules /etc/udev/rules.d/70-shinpi.rules
sudo udevadm control --reload-rules
```

The rules grant access with `uaccess`, without making the device world-writable. The system sources need a working system D-Bus and BlueZ or UPower. The tray needs a StatusNotifier host.

---

## License & Attribution

`shinpi` is licensed under `GPL-3.0-or-later`; see `LICENSE` and `NOTICE`. Battery acquisition is implemented independently from the system interfaces and device protocols.

| Component | Upstream | License | Integration Details |
| --- | --- | --- | --- |
| Breeze icons | [KDE/breeze-icons](https://github.com/KDE/breeze-icons) | `LGPL-3.0-or-later` | Bundled gear and pin SVGs; see `ui/icons/COPYING`. Other device icons come from the host theme with original fallbacks. |
| `slint` | [slint-ui/slint](https://github.com/slint-ui/slint) | `GPL-3.0-only OR LicenseRef-Slint-Royalty-free-2.0 OR LicenseRef-Slint-Software-3.0` | Native windows rendered with `FemtoVG`. |
| `ksni` | [iovxw/ksni](https://github.com/iovxw/ksni) | `Unlicense` | StatusNotifier tray. |
| `zbus` | [z-galaxy/zbus](https://github.com/z-galaxy/zbus) | `MIT` | BlueZ, UPower and KWin D-Bus calls. |
| `hidapi` | [ruabmbua/hidapi-rs](https://github.com/ruabmbua/hidapi-rs) | `BSD-3-Clause` | Linux HID transport. |
| `resvg` | [linebender/resvg](https://github.com/linebender/resvg) | `Apache-2.0 OR MIT` | Tray SVG rasterization. |
