mod config;
mod device;
mod hid;
mod icons;
mod notify;
mod place;
mod system;
mod theme;
mod tray;

use slint::{
    winit_030::{winit, EventResult, WinitWindowAccessor},
    ComponentHandle, ModelRc, VecModel,
};
use std::{
    cell::{Cell, RefCell},
    collections::HashSet,
    fs::File,
    os::fd::AsRawFd,
    rc::Rc,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};
use tray::Command;

slint::include_modules!();

struct State {
    config: config::Config,
    scan: device::Scan,
    opened: Instant,
    blur: Option<Instant>,
    seen_focus: bool,
    closing: bool,
    busy: bool,
    editing: Option<String>,
    pending_notifications: HashSet<String>,
    notification_error: String,
}

enum Event {
    Started,
    Scanned(device::Scan),
    Notified(String, u64, Result<(), String>),
}

async fn scan() -> device::Scan {
    let data = system::scan().await;
    match tokio::task::spawn_blocking(move || {
        let mut data = data;
        hid::scan(&mut data);
        data.devices
            .sort_by_key(|d| (d.name.to_lowercase(), d.source.clone(), d.id.clone()));
        data
    })
    .await
    {
        Ok(data) => data,
        Err(e) => device::Scan {
            devices: vec![],
            warnings: vec![format!("Ошибка опроса HID: {e}")],
        },
    }
}

fn single_instance() -> std::io::Result<Option<File>> {
    std::fs::create_dir_all(config::config_dir())?;
    let file = File::create(config::config_dir().join("shinpi.lock"))?;
    let result = unsafe { libc::flock(file.as_raw_fd(), libc::LOCK_EX | libc::LOCK_NB) };
    if result == 0 {
        Ok(Some(file))
    } else {
        let error = std::io::Error::last_os_error();
        if error.kind() == std::io::ErrorKind::WouldBlock {
            Ok(None)
        } else {
            Err(error)
        }
    }
}

fn color(rgb: theme::Rgb) -> slint::Color {
    slint::Color::from_rgb_u8(rgb.r, rgb.g, rgb.b)
}

fn paint(
    popup: &Popup,
    settings: &Settings,
    add: &AddDevices,
    editor: &DeviceEditor,
    cfg: &config::Config,
) {
    let palette = theme::system_palette(&cfg.theme);
    let dark = theme::resolved(&cfg.theme, theme::system_dark()) == "dark";
    let font = theme::system_font();
    let scale = popup.window().scale_factor().max(1.0);
    let size = (font.size_px * scale).round() / scale;
    macro_rules! window_style {
        ($window:expr) => {{
            $window.set_dark(dark);
            $window.set_ui_font_family(font.family.clone().into());
            $window.set_ui_font_size(size);
            $window.set_ui_font_weight(font.weight);
        }};
    }
    window_style!(popup);
    window_style!(settings);
    window_style!(add);
    window_style!(editor);
    for breeze in [
        popup.global::<Breeze>(),
        settings.global::<Breeze>(),
        add.global::<Breeze>(),
        editor.global::<Breeze>(),
    ] {
        breeze.set_window(color(palette.window));
        breeze.set_button(color(palette.button));
        breeze.set_button_hover(color(palette.button_hover));
        breeze.set_fg(color(palette.text));
        breeze.set_inactive(color(palette.inactive));
        breeze.set_selection(color(palette.accent));
        breeze.set_selection_fg(color(palette.accent_text));
        breeze.set_frame(color(palette.frame));
        breeze.set_outline(color(palette.outline));
    }
    settings.set_theme_index(match cfg.theme.as_str() {
        "light" => 1,
        "dark" => 2,
        _ => 0,
    });
    settings.set_orientation(i32::from(cfg.tray_orientation));
    settings.set_colored(cfg.colored_icon);
    settings.set_notifications(cfg.notifications);
    settings.set_threshold(i32::from(cfg.low_threshold));
    settings.set_repeat_minutes(cfg.repeat_minutes as i32);
    let svg = icons::tray_svg(cfg.minimum(), cfg.tray_orientation, cfg.colored_icon, dark);
    settings.set_tray_preview(slint::Image::load_from_svg_data(svg.as_bytes()).unwrap_or_default());
    if let Some(icon) = icons::theme_icon("configure") {
        popup.set_icon_configure(icon.image);
        popup.set_tint_configure(icon.tint);
    }
    if let Some(icon) = icons::theme_icon("window-pin") {
        popup.set_icon_pin(icon.image);
        popup.set_tint_pin(icon.tint);
    }
    if let Some(icon) = icons::theme_icon("view-refresh") {
        popup.set_icon_refresh(icon.image);
        popup.set_tint_refresh(icon.tint);
    }
}

fn row(
    name: &str,
    kind: device::Kind,
    reading: &device::Reading,
    source: &str,
    added: bool,
) -> DeviceRow {
    let themed = icons::theme_icon(kind.icon());
    let (icon, tint) = match themed {
        Some(icon) => (icon.image, icon.tint),
        None => {
            let data: &[u8] = match kind {
                device::Kind::Mouse => include_bytes!("../ui/icons/input-mouse.svg"),
                device::Kind::Keyboard => include_bytes!("../ui/icons/input-keyboard.svg"),
                device::Kind::Headset => include_bytes!("../ui/icons/audio-headphones.svg"),
                device::Kind::Gamepad => include_bytes!("../ui/icons/input-gaming.svg"),
                device::Kind::Other => include_bytes!("../ui/icons/battery.svg"),
            };
            (
                slint::Image::load_from_svg_data(data).unwrap_or_default(),
                true,
            )
        }
    };
    DeviceRow {
        name: name.into(),
        charge: reading.text().into(),
        detail: source.into(),
        icon,
        tint,
        low: reading.low(15),
        added,
    }
}

fn fill(popup: &Popup, settings: &Settings, add: &AddDevices, state: &State) {
    let rows: Vec<_> = state
        .config
        .devices
        .iter()
        .map(|d| row(&d.name, d.kind, &d.reading, &d.source, true))
        .collect();
    popup.set_rows(ModelRc::new(VecModel::from(rows.clone())));
    settings.set_rows(ModelRc::new(VecModel::from(rows)));
    let candidates: Vec<_> = state
        .scan
        .devices
        .iter()
        .map(|d| {
            row(
                &d.name,
                d.kind,
                &d.reading,
                &d.source,
                state.config.devices.iter().any(|s| s.id == d.id),
            )
        })
        .collect();
    add.set_rows(ModelRc::new(VecModel::from(candidates)));
    add.set_busy(state.busy);
    popup.set_busy(state.busy);
    let mut diagnostics = state.scan.warnings.clone();
    diagnostics.extend(
        state
            .scan
            .devices
            .iter()
            .filter(|d| !d.status.is_empty())
            .map(|d| format!("{} · {}: {}", d.name, d.source, d.status)),
    );
    if !state.notification_error.is_empty() {
        diagnostics.push(state.notification_error.clone());
    }
    settings.set_diagnostics(if diagnostics.is_empty() {
        "Все источники опрошены.".into()
    } else {
        diagnostics.join("\n\n").into()
    });
}

fn animate(popup: &Popup, state: &Rc<RefCell<State>>, timer: &Rc<slint::Timer>, open: bool) {
    timer.stop();
    if open {
        popup.set_slide(popup.get_layout_height());
        if let Err(e) = popup.show() {
            eprintln!("shinpi: {e}");
            return;
        }
        let _ = popup.window().with_winit_window(|w| {
            w.set_resizable(false);
            w.focus_window();
        });
        let mut state = state.borrow_mut();
        state.opened = Instant::now();
        state.blur = None;
        state.seen_focus = false;
        state.closing = false;
    } else {
        state.borrow_mut().closing = true;
        state.borrow_mut().blur = None;
    }
    let weak = popup.as_weak();
    let state = state.clone();
    let start = Instant::now();
    let timer_weak = Rc::downgrade(timer);
    let from = popup.get_slide();
    let to = if open { 0.0 } else { popup.get_layout_height() };
    timer.start(
        slint::TimerMode::Repeated,
        Duration::from_millis(16),
        move || {
            if let Some(popup) = weak.upgrade() {
                let t = (start.elapsed().as_secs_f32() / 0.18).min(1.0);
                let easing = if open {
                    1.0 - (1.0 - t).powi(3)
                } else {
                    t.powi(3)
                };
                popup.set_slide(from + (to - from) * easing);
                if t >= 1.0 && !open && state.borrow().closing {
                    let _ = popup.hide();
                    state.borrow_mut().closing = false;
                }
                if t >= 1.0 {
                    if let Some(timer) = timer_weak.upgrade() {
                        timer.stop();
                    }
                }
            }
        },
    );
}

fn run() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.iter().any(|a| a == "--help") {
        println!("shinpi [--window | --once | --print-udev-rules]\n  --window             Открыть окно при запуске\n  --once               Напечатать текущие показания в JSON без GUI\n  --print-udev-rules   Напечатать правила доступа к поддерживаемым HID");
        return Ok(());
    }
    if args.iter().any(|a| a == "--print-udev-rules") {
        print!("{}", include_str!("../packaging/70-shinpi.rules"));
        return Ok(());
    }
    if args
        .iter()
        .any(|a| !matches!(a.as_str(), "--once" | "--window"))
    {
        return Err("Неизвестный параметр. См. --help".into());
    }
    let runtime = tokio::runtime::Runtime::new()?;
    if args.iter().any(|a| a == "--once") {
        println!(
            "{}",
            serde_json::to_string_pretty(&runtime.block_on(scan()))?
        );
        return Ok(());
    }
    let Some(_lock) = single_instance()? else {
        return Ok(());
    };
    slint::BackendSelector::new()
        .backend_name("winit".into())
        .renderer_name("femtovg".into())
        .with_winit_window_attributes_hook(|a| a.with_transparent(true))
        .select()?;
    let _ = slint::set_xdg_app_id("shinpi");
    let popup = Popup::new()?;
    let settings = Settings::new()?;
    let add = AddDevices::new()?;
    let editor = DeviceEditor::new()?;
    let state = Rc::new(RefCell::new(State {
        config: config::load(),
        scan: device::Scan::default(),
        opened: Instant::now(),
        blur: None,
        seen_focus: false,
        closing: false,
        busy: true,
        editing: None,
        pending_notifications: HashSet::new(),
        notification_error: String::new(),
    }));
    paint(&popup, &settings, &add, &editor, &state.borrow().config);
    fill(&popup, &settings, &add, &state.borrow());
    let (sender, mut commands) = tokio::sync::mpsc::unbounded_channel();
    let (refresh, mut refresh_rx) = tokio::sync::mpsc::unbounded_channel::<()>();
    let (results_tx, results) = std::sync::mpsc::channel();
    let tray = runtime.block_on(async {
        use ksni::TrayMethods;
        tray::Tray {
            sender: sender.clone(),
            icons: {
                let cfg = &state.borrow().config;
                icons::tray(
                    cfg.minimum(),
                    cfg.tray_orientation,
                    cfg.colored_icon,
                    popup.get_dark(),
                )
            },
            summary: "Поиск устройств…".into(),
        }
        .assume_sni_available(true)
        .spawn()
        .await
    })?;
    if !runtime.block_on(place::install()) {
        eprintln!("shinpi: KWin positioning unavailable; using compositor placement");
    }
    let notification_tx = results_tx.clone();
    runtime.spawn(async move {
        loop {
            if results_tx.send(Event::Started).is_err() {
                break;
            }
            let result = scan().await;
            if results_tx.send(Event::Scanned(result)).is_err() {
                break;
            }
            if matches!(
                tokio::time::timeout(Duration::from_secs(30), refresh_rx.recv()).await,
                Ok(None)
            ) {
                break;
            }
            while refresh_rx.try_recv().is_ok() {}
        }
    });
    let animation = Rc::new(slint::Timer::default());
    let tx = sender.clone();
    popup.on_refresh(move || {
        let _ = tx.send(Command::Refresh);
    });
    let tx = sender.clone();
    popup.on_configure(move || {
        let _ = tx.send(Command::Settings);
    });
    let weak = popup.as_weak();
    popup.on_pin_toggled(move || {
        if let Some(popup) = weak.upgrade() {
            popup.set_pinned(!popup.get_pinned());
        }
    });
    let tx = sender.clone();
    popup.on_add(move || {
        let _ = tx.send(Command::Add);
    });
    let tx = sender.clone();
    settings.on_add(move || {
        let _ = tx.send(Command::Add);
    });
    let tx = sender.clone();
    add.on_refresh(move || {
        let _ = tx.send(Command::Refresh);
    });
    let tx = sender.clone();
    add.on_add_device(move |index| {
        let _ = tx.send(Command::Select(index as usize));
    });
    let tx = sender.clone();
    settings.on_edit_device(move |index| {
        let _ = tx.send(Command::Edit(index as usize));
    });
    let tx = sender.clone();
    settings.on_remove_device(move |index| {
        let _ = tx.send(Command::Remove(index as usize));
    });
    let tx = sender.clone();
    editor.on_save_device(move |name, kind| {
        let _ = tx.send(Command::SaveEdit(name.to_string(), kind as usize));
    });
    let tx = sender.clone();
    settings.on_theme_selected(move |index| {
        let _ = tx.send(Command::Theme(index));
    });
    let tx = sender.clone();
    settings.on_orientation_selected(move |index| {
        let _ = tx.send(Command::Orientation(index as u8));
    });
    let tx = sender.clone();
    settings.on_colored_selected(move |value| {
        let _ = tx.send(Command::Colored(value));
    });
    let tx = sender.clone();
    settings.on_notifications_selected(move |value| {
        let _ = tx.send(Command::Notifications(value));
    });
    let tx = sender.clone();
    settings.on_threshold_selected(move |value| {
        let _ = tx.send(Command::Threshold(value as u8));
    });
    let tx = sender.clone();
    settings.on_repeat_selected(move |value| {
        let _ = tx.send(Command::Repeat(value as u32));
    });
    macro_rules! dismiss_window {
        ($window:expr) => {{
            let weak = $window.as_weak();
            $window.on_dismiss(move || {
                if let Some(window) = weak.upgrade() {
                    let _ = window.hide();
                }
            });
            $window
                .window()
                .on_close_requested(|| slint::CloseRequestResponse::HideWindow);
        }};
    }
    dismiss_window!(settings);
    dismiss_window!(add);
    dismiss_window!(editor);
    let weak = popup.as_weak();
    let data = state.clone();
    let tx = sender.clone();
    popup.window().on_close_requested(move || {
        let _ = tx.send(Command::Dismiss);
        slint::CloseRequestResponse::KeepWindowShown
    });
    let tx = sender.clone();
    popup.window().on_winit_window_event(move |_, event| {
        match event {
            winit::event::WindowEvent::Focused(focused) => {
                let mut state = data.borrow_mut();
                if *focused {
                    state.seen_focus = true;
                    state.blur = None;
                } else if state.seen_focus {
                    state.blur = Some(Instant::now());
                }
            }
            winit::event::WindowEvent::KeyboardInput { event, .. }
                if event.state == winit::event::ElementState::Pressed
                    && event.logical_key
                        == winit::keyboard::Key::Named(winit::keyboard::NamedKey::Escape) =>
            {
                if let Some(popup) = weak.upgrade() {
                    if !popup.get_pinned() {
                        let _ = tx.send(Command::Dismiss);
                    }
                }
            }
            _ => {}
        }
        EventResult::Propagate
    });
    let tick_popup = popup.as_weak();
    let tick_settings = settings.as_weak();
    let tick_add = add.as_weak();
    let tick_editor = editor.as_weak();
    let data = state.clone();
    let slide = animation.clone();
    let last_toggle = Cell::new(Instant::now() - Duration::from_secs(1));
    let last_theme = Cell::new(Instant::now());
    let rt_handle = runtime.handle().clone();
    let last_icon = Cell::new(None);
    let poll = slint::Timer::default();
    poll.start(
        slint::TimerMode::Repeated,
        Duration::from_millis(60),
        move || {
            let (Some(popup), Some(settings), Some(add), Some(editor)) = (
                tick_popup.upgrade(),
                tick_settings.upgrade(),
                tick_add.upgrade(),
                tick_editor.upgrade(),
            ) else {
                return;
            };
            let mut changed = false;
            let mut redraw = false;
            while let Ok(command) = commands.try_recv() {
                match command {
                    Command::Toggle
                        if last_toggle.get().elapsed() >= Duration::from_millis(300) =>
                    {
                        last_toggle.set(Instant::now());
                        let open = !popup.window().is_visible() || data.borrow().closing;
                        animate(&popup, &data, &slide, open);
                    }
                    Command::Refresh => {
                        if !data.borrow().busy {
                            data.borrow_mut().busy = true;
                            let _ = refresh.send(());
                        }
                    }
                    Command::Dismiss => {
                        if popup.window().is_visible() && !data.borrow().closing {
                            animate(&popup, &data, &slide, false);
                        }
                    }
                    Command::Settings | Command::Add => {
                        let show_add = matches!(command, Command::Add);
                        if popup.window().is_visible() && !popup.get_pinned() {
                            animate(&popup, &data, &slide, false);
                        }
                        settings.show().ok();
                        if show_add {
                            add.show().ok();
                            let _ = add.window().with_winit_window(|w| {
                                w.set_minimized(false);
                                w.focus_window();
                            });
                            if !data.borrow().busy {
                                let _ = refresh.send(());
                            }
                        }
                        if !show_add {
                            let _ = settings.window().with_winit_window(|w| {
                                w.set_minimized(false);
                                w.focus_window();
                            });
                        }
                    }
                    Command::Select(index) => {
                        let device = data.borrow().scan.devices.get(index).cloned();
                        if let Some(device) = device {
                            data.borrow_mut().config.add(&device);
                            changed = true;
                        }
                    }
                    Command::Remove(index) => {
                        if index < data.borrow().config.devices.len() {
                            data.borrow_mut().config.devices.remove(index);
                            changed = true;
                        }
                    }
                    Command::Edit(index) => {
                        let device = data.borrow().config.devices.get(index).cloned();
                        if let Some(device) = device {
                            data.borrow_mut().editing = Some(device.id);
                            editor.set_device_name(device.name.into());
                            editor.set_kind_index(device.kind as i32);
                            editor.set_error("".into());
                            editor.show().ok();
                            let _ = editor.window().with_winit_window(|w| {
                                w.set_minimized(false);
                                w.focus_window();
                            });
                        }
                    }
                    Command::SaveEdit(name, index) => {
                        let name: String = name.trim().chars().take(80).collect();
                        if name.is_empty() {
                            editor.set_error("Введите название устройства.".into());
                        } else {
                            let mut state = data.borrow_mut();
                            let id = state.editing.clone();
                            if let Some(device) = state
                                .config
                                .devices
                                .iter_mut()
                                .find(|d| Some(&d.id) == id.as_ref())
                            {
                                device.name = name;
                                device.kind = match index {
                                    0 => device::Kind::Mouse,
                                    1 => device::Kind::Keyboard,
                                    2 => device::Kind::Headset,
                                    3 => device::Kind::Gamepad,
                                    _ => device::Kind::Other,
                                };
                                changed = true;
                            }
                            editor.hide().ok();
                        }
                    }
                    Command::Theme(index) => {
                        data.borrow_mut().config.theme = match index {
                            1 => "light",
                            2 => "dark",
                            _ => "system",
                        }
                        .into();
                        changed = true;
                    }
                    Command::Orientation(index) => {
                        data.borrow_mut().config.tray_orientation = index.min(3);
                        changed = true;
                    }
                    Command::Colored(value) => {
                        data.borrow_mut().config.colored_icon = value;
                        changed = true;
                    }
                    Command::Notifications(value) => {
                        data.borrow_mut().config.notifications = value;
                        changed = true;
                    }
                    Command::Threshold(value) => {
                        data.borrow_mut().config.low_threshold = value.min(100);
                        changed = true;
                    }
                    Command::Repeat(value) => {
                        data.borrow_mut().config.repeat_minutes = value.clamp(1, 10080);
                        changed = true;
                    }
                    Command::Quit => {
                        let _ = slint::quit_event_loop();
                    }
                    _ => {}
                }
            }
            while let Ok(event) = results.try_recv() {
                redraw = true;
                let mut state = data.borrow_mut();
                match event {
                    Event::Started => state.busy = true,
                    Event::Scanned(result) => {
                        state.busy = false;
                        changed |= state.config.observe(&result);
                        let now = SystemTime::now()
                            .duration_since(UNIX_EPOCH)
                            .unwrap_or_default()
                            .as_secs();
                        for (id, message) in notify::due(&state.config, &result, now) {
                            if state.pending_notifications.insert(id.clone()) {
                                let tx = notification_tx.clone();
                                rt_handle.spawn(async move {
                                    let result =
                                        notify::send(&message).await.map_err(|e| e.to_string());
                                    let _ = tx.send(Event::Notified(id, now, result));
                                });
                            }
                        }
                        state.scan = result;
                    }
                    Event::Notified(id, at, result) => {
                        state.pending_notifications.remove(&id);
                        match result {
                            Ok(()) => {
                                if let Some(device) =
                                    state.config.devices.iter_mut().find(|d| d.id == id)
                                {
                                    device.last_notified = at;
                                    changed = true;
                                }
                                state.notification_error.clear();
                            }
                            Err(error) => {
                                state.notification_error =
                                    format!("Уведомления недоступны: {error}")
                            }
                        }
                    }
                }
            }
            if changed {
                if let Err(e) = config::save(&data.borrow().config) {
                    settings.set_error(format!("Не удалось сохранить настройки: {e}").into());
                } else {
                    settings.set_error("".into());
                }
            }
            if changed || redraw || last_theme.get().elapsed() >= Duration::from_secs(2) {
                last_theme.set(Instant::now());
                let state = data.borrow();
                paint(&popup, &settings, &add, &editor, &state.config);
                fill(&popup, &settings, &add, &state);
            }
            {
                let state = data.borrow();
                let cfg = &state.config;
                let signature = (
                    cfg.minimum(),
                    cfg.tray_orientation,
                    cfg.colored_icon,
                    popup.get_dark(),
                );
                if last_icon.get() != Some(signature) || changed || redraw {
                    last_icon.set(Some(signature));
                    let icons = icons::tray(signature.0, signature.1, signature.2, signature.3);
                    let summary = if cfg.devices.is_empty() {
                        "Добавьте устройства".into()
                    } else {
                        cfg.devices
                            .iter()
                            .map(|d| format!("{}: {}", d.name, d.reading.text()))
                            .collect::<Vec<_>>()
                            .join("\n")
                    };
                    let tray = tray.clone();
                    rt_handle.spawn(async move {
                        tray.update(move |t| {
                            t.icons = icons;
                            t.summary = summary;
                        })
                        .await;
                    });
                }
            }
            let should_close = {
                let state = data.borrow();
                popup.window().is_visible()
                    && !popup.get_pinned()
                    && !state.closing
                    && state.opened.elapsed() >= Duration::from_millis(350)
                    && state
                        .blur
                        .is_some_and(|at| at.elapsed() >= Duration::from_millis(150))
            };
            if should_close {
                animate(&popup, &data, &slide, false);
            }
        },
    );
    if args.iter().any(|a| a == "--window") {
        animate(&popup, &state, &animation, true);
    }
    slint::run_event_loop_until_quit()?;
    Ok(())
}

fn main() {
    if let Err(error) = run() {
        eprintln!("shinpi: {error}");
        std::process::exit(1);
    }
}
