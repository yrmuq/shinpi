<div align="center">
  <img src="assets/icon.svg" alt="shinpi logo" width="80">
  <h1>shinpi</h1>

  <a href="LICENSE"><img src="https://img.shields.io/badge/License-GPLv3%2B-blue.svg" alt="License: GPL-3.0-or-later"></a>
</div>

---

> [!NOTE]
> **Active Development:** `shinpi` changes frequently and may contain bugs. Saved settings may become outdated after updates. Support for individual HID devices still requires verification on physical hardware.

---

`shinpi` is a Linux tray application for checking device battery levels in one place. Built with Rust, Slint and FemtoVG, it provides native windows styled for KDE Plasma without a browser engine.

---

## Features

- Battery readings through Linux `power_supply`, BlueZ, UPower and selected HID receivers.
- Manual device selection, custom names and icons, and saved readings for disconnected devices.
- A tray battery icon showing the lowest device level, with color, monochrome and orientation options.
- Low-battery notifications with a configurable threshold and shared repeat interval.
- Optional notification history and adjustable popup width.
- English and Russian interfaces, automatic language selection, and system, light and dark themes.

---

## Controls & Shortcuts

<details>
<summary>Show controls and shortcuts</summary>

| Input | Action |
| --- | --- |
| Left-click on tray | Toggle the device panel and refresh readings when opening. |
| Right-click on tray | Open the Refresh, Settings and Quit menu. |
| Gear icon | Open settings. |
| `Add` | Choose a discovered device to display. |
| Pencil in Settings | Change a device's name and icon. |
| Remove in Settings | Remove a device from the panel. |
| Triangle | Toggle notification history. |
| `×` on a notification | Dismiss the entry. |
| Pin icon | Keep the panel open when it loses focus. |
| `Escape` | Close the unpinned panel. |
| `Tab`, `Enter`, `Space` | Navigate and activate toolbar controls. |
| `--window` | Open the panel at startup. |
| `--once` | Print readings and source warnings as JSON. |
| `--print-udev-rules` | Print access rules for supported HID devices. |
| `--help` | Show command-line help. |

The empty panel's `Add` button disappears after selecting a device; further devices are added through Settings. Sources are listed separately, so choose the source that works best for each device.

</details>

---

## Installation & Usage

### Architecture & Format Support

| Format | `x86_64` | `arm64` |
| --- | :---: | :---: |
| `AppImage` | ✅ | ❌ |
| `Flatpak` | ❌ | ❌ |

Stable releases use `X.Y.0`. Versions with a non-zero final component, such as `X.Y.1`, are automatic dependency-update prereleases.

Currently, builds are available only in the development draft on the [Releases page](https://github.com/yrmuq/shinpi/releases), visible to repository maintainers. No public stable release is available yet.

### AppImage

Requires `x86_64` Linux, working graphics drivers, a system D-Bus and a StatusNotifier-compatible tray. KDE Plasma is the primary desktop environment.

Download `shinpi-x86_64.AppImage` from the development draft. Use [Gear Lever](https://flathub.org/apps/it.mijorus.gearlever) to install and integrate it with the desktop. Updates currently require downloading and importing the new build; an embedded update source is not yet provided.

### Flatpak

A Flatpak version is planned for a future release.

### HID Access

System battery sources work through the available kernel interfaces, BlueZ or UPower. Some supported HID receivers also need local device-access rules. If Diagnostics reports a permission error, export and install the rules, then reconnect the device:

```sh
./shinpi-x86_64.AppImage --print-udev-rules > 70-shinpi.rules
sudo install -m 0644 70-shinpi.rules /etc/udev/rules.d/70-shinpi.rules
sudo udevadm control --reload-rules
```

The application runs as a regular user. The rules use `uaccess` for the active local session.

---

## License & Attribution

`shinpi` is licensed under [GPL-3.0-or-later](LICENSE). See [NOTICE](NOTICE), the [Breeze icon license](ui/icons/COPYING) and [bundled component notices](packaging/appimage/BUNDLED.txt) for attribution details.

| Component | Upstream | License | Integration Details |
| --- | --- | --- | --- |
| Breeze icons | [KDE/breeze-icons](https://github.com/KDE/breeze-icons) | `LGPL-3.0-or-later` | Bundled gear and pin icons. |
| `slint` | [slint-ui/slint](https://github.com/slint-ui/slint) | `GPL-3.0-only` or [alternative terms](https://github.com/slint-ui/slint/blob/master/LICENSE.md) | Native interface with FemtoVG rendering. |
| `ksni` | [iovxw/ksni](https://github.com/iovxw/ksni) | `Unlicense` | StatusNotifier tray integration. |
| `zbus` | [z-galaxy/zbus](https://github.com/z-galaxy/zbus) | `MIT` | Desktop and device D-Bus interfaces. |
| `hidapi` | [ruabmbua/hidapi-rs](https://github.com/ruabmbua/hidapi-rs) | `BSD-3-Clause` | HID device access. |
| `resvg` | [linebender/resvg](https://github.com/linebender/resvg) | `Apache-2.0 OR MIT` | Tray icon rasterization. |
