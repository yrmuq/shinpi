<div align="center">
  <img src="ui/icons/battery.svg" alt="shinpi" width="80">
  <h1>shinpi</h1>

  [![License: GPL v3+](https://img.shields.io/badge/License-GPLv3%2B-blue.svg)](https://www.gnu.org/licenses/gpl-3.0)

  <p>A native Linux tray battery monitor using Rust, Slint and FemtoVG. System battery services come first; supported USB receivers provide a fallback. One window, no browser engine.</p>
</div>

---

> [!NOTE]
> Initial implementation for KDE Plasma. Development builds are attached to a draft release and are visible to repository maintainers. HID support requires verification on physical hardware; this repository does not claim tested compatibility with every listed receiver.

---

## Features

- Device type and name on the left, battery percentage or reported level on the right. Charging and approximate readings are marked explicitly; missing data is never displayed as `0%`.
- KDE colors, fonts, icon theme, rounded popup, slide animation and pin behavior reuse `yoin` conventions. KWin positions the popup beside the panel; other compositors choose the placement themselves.
- Background refresh every 30 seconds through Linux `power_supply`, BlueZ `Battery1` and UPower. Multiple sources for the same physical device are merged; identical names alone are not merged. Disconnected devices disappear on refresh; readings are not persisted between runs.
- Read-only HID battery queries for ZAOPIN Z2 Max Compx receivers, Logitech G304/G305 `HID++`, MCHOSE V9 PRO and passive Bluetooth Xbox-compatible reports from supported 8BitDo/Xbox identities. CIY Tester 68 and other peripherals work when the system supplies their battery reading. Unknown receivers are never queried.
- System, light and dark themes; the selection is saved in `~/.config/shinpi/config.json` (or under `XDG_CONFIG_HOME`). One graphical instance per user.

---

## Controls & Shortcuts

| Input | Action |
| --- | --- |
| Left click on tray | Open or close the device window. |
| Right click on tray | Refresh, Settings or Quit. |
| Refresh icon | Request a fresh scan without closing the window. |
| Gear icon | Open theme settings and source diagnostics. |
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
| `yoin` | [yrmuq/yoin](https://github.com/yrmuq/yoin) | `GPL-3.0-or-later` | KDE palette, fonts, icon lookup, popup conventions and KWin placement. |
| Breeze icons | [KDE/breeze-icons](https://github.com/KDE/breeze-icons) | `LGPL-3.0-or-later` | Bundled gear and pin SVGs; see `ui/icons/COPYING`. Other device icons come from the host theme with original fallbacks. |
| `slint` | [slint-ui/slint](https://github.com/slint-ui/slint) | `GPL-3.0-only OR LicenseRef-Slint-Royalty-free-2.0 OR LicenseRef-Slint-Software-3.0` | Native windows rendered with `FemtoVG`. |
| `ksni` | [iovxw/ksni](https://github.com/iovxw/ksni) | `Unlicense` | StatusNotifier tray. |
| `zbus` | [z-galaxy/zbus](https://github.com/z-galaxy/zbus) | `MIT` | BlueZ, UPower and KWin D-Bus calls. |
| `hidapi` | [ruabmbua/hidapi-rs](https://github.com/ruabmbua/hidapi-rs) | `BSD-3-Clause` | Linux HID transport. |
| `resvg` | [linebender/resvg](https://github.com/linebender/resvg) | `Apache-2.0 OR MIT` | Tray SVG rasterization. |
