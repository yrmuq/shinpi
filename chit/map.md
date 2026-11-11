# map

- `src/main.rs` — приложение, фоновый опрос, создание окон по запросу, анимации и настройки.
- `src/device.rs` — устройства с отдельным ID на источник, показания и ступени заполнения значка.
- `src/system.rs` — Linux `power_supply`, BlueZ и UPower через `zbus`.
- `src/hid.rs` — ограниченный выбор HID-интерфейсов и запросы заряда через `hidapi`.
- `src/config.rs` — настройки, выбранные источники, последние показания и время уведомлений в `~/.config/shinpi/config.json`.
- `src/notify.rs` — общий порог/интервал и уведомления через `org.freedesktop.Notifications`.
- `src/theme.rs`, `src/icons.rs` — темы, шрифты KDE и значки.
- `src/tray.rs` — StatusNotifier через `ksni`.
- `src/place.rs`, `kwin/placer.js` — размещение окна у панели KWin.
- `ui/app.slint` — окно устройств, настройки и дочерние диалоги добавления/редактирования; `slint` с `FemtoVG`.
- `packaging/` — desktop entry и правила `udev` для поддерживаемых HID.
- `tests/placement.mjs` — размещение у четырёх сторон панели и границ экрана.
- `.github/workflows/check.yml` — форматирование, Clippy, тесты и проверка CLI на `dev`.
- `.github/workflows/dev.yml` — Development build из `dev`, AppImage и контрольная сумма в черновике `shinpi dev`.
- `packaging/appimage/` — сборка AppImage, запуск и обновление черновика через GitHub Actions без создания Git-тега.
- `assets/icon.svg` — горизонтальный зелёный значок приложения для окон, AppImage и README.
