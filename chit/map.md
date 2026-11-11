# map

- `src/main.rs` — приложение, фоновый опрос, окно, анимации и настройки.
- `src/device.rs` — устройства, показания и объединение источников по физическому устройству.
- `src/system.rs` — Linux `power_supply`, BlueZ и UPower через `zbus`.
- `src/hid.rs` — ограниченный выбор HID-интерфейсов и запросы заряда через `hidapi`.
- `src/config.rs` — `~/.config/shinpi/config.json`.
- `src/theme.rs`, `src/icons.rs` — темы, шрифты KDE и значки, наработки `yoin`.
- `src/tray.rs` — StatusNotifier через `ksni`.
- `src/place.rs`, `kwin/placer.js` — размещение окна у панели KWin.
- `ui/app.slint` — окно устройств и настройки; `slint` с `FemtoVG`.
- `packaging/` — desktop entry и правила `udev` для поддерживаемых HID.
- `tests/placement.mjs` — размещение у четырёх сторон панели и границ экрана.
- `.github/workflows/check.yml` — форматирование, Clippy, тесты и проверка CLI на `dev`.
