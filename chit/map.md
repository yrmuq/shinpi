# map

- `src/main.rs` — приложение, фоновый опрос, создание окон по запросу, анимации и настройки.
- `src/device.rs` — устройства с отдельным ID на источник, показания и ступени заполнения значка.
- `src/system.rs` — Linux `power_supply`, BlueZ и UPower через `zbus`.
- `src/hid.rs` — ограниченный выбор HID-интерфейсов и запросы заряда через `hidapi`.
- `src/config.rs` — настройки, выбранные источники, последние показания, история и время уведомлений в `~/.config/shinpi/config.json`.
- `src/notices.rs` — последние 20 уведомлений, время записей и анимация удаления.
- `src/notify.rs` — общий порог/интервал и уведомления через `org.freedesktop.Notifications`.
- `src/i18n.rs`, `languages/*.ini` — English/русский, системная локаль, ручной выбор и внешние переводы.
- `src/theme.rs`, `src/icons.rs` — темы, шрифты KDE и значки.
- `src/tray.rs` — StatusNotifier через `ksni`.
- `src/place.rs`, `kwin/placer.js` — размещение связанных панелей устройств и уведомлений у панели KWin.
- `ui/app.slint` — панели устройств/уведомлений, настройки и дочерние диалоги добавления/редактирования; `slint` с `FemtoVG`.
- `packaging/` — desktop entry и правила `udev` для поддерживаемых HID.
- `tests/placement.mjs` — размещение у четырёх сторон панели и границ экрана.
- `.github/workflows/check.yml` — форматирование, Clippy, тесты и проверка CLI на `dev`.
- `.github/workflows/dev.yml` — автоматическая проверка каждого push в `dev`, x86_64 AppImage, zsync и контрольные суммы, разовый x86_64 Flatpak в черновике `shinpi dev`.
- `.github/workflows/release.yml` — обычный релиз после merge `dev` в `main` и возврат версии в `dev`.
- `.github/dependabot.yml`, `.github/workflows/deps.yml`, `.github/workflows/pins.yml` — еженедельные обновления Cargo/Actions/appimagetool и сборка/публикация prerelease на `deps`.
- `.github/actions/setup/`, `packaging/check.sh` — общие библиотеки, Rust, кеш и проверки для CI.
- `.github/actions/tools/` — отдельная сборка служебной утилиты до подготовки версии; кеш приложения выбирается после подготовки итогового lockfile.
- `packaging/tools/src/flow.rs`, `notes.rs`, `pins.rs` — служебная утилита Rust: подготовка версий и описаний, публикация CI-релизов, синхронизация `dev`/`deps`, блокировка релиза с непроверенными зависимостями и обновление `appimagetool`.
- `packaging/tools/src/draft.rs`, `checks.rs` — обновление черновика, проверка JSON показаний и zsync, создание `flatpakref`; не включаются в приложение.
- `packaging/appimage/` — сборка x86_64 AppImage, проверка zsync, запуск и обновление черновика через GitHub Actions без создания Git-тега.
- `packaging/tools/tests/` — релиз и синхронизация веток на временных Git-репозиториях, защита публикации и черновика, контрольные суммы и закреплённые инструменты.
- `assets/icon.svg`, `assets/screenshot.png` — значок приложения и настоящий скриншот настроек для README.

- `packaging/flatpak/` — сборка x86_64 в Freedesktop SDK, подпись, сохранение истории и проверка репозитория обновлений GitHub Pages.
- `packaging/release/build.sh` — полная сборка x86_64 AppImage и Flatpak.
