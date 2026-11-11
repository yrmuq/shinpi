mod config;
mod device;
mod hid;
mod icons;
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
    fs::File,
    os::fd::AsRawFd,
    rc::Rc,
    time::{Duration, Instant},
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
}

async fn scan() -> device::Scan {
    let data = system::scan().await;
    match tokio::task::spawn_blocking(move || {
        let mut data = data;
        hid::scan(&mut data);
        data.devices.sort_by_key(|d| d.name.to_lowercase());
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

fn paint(popup: &Popup, settings: &Settings, choice: &str) {
    let palette = theme::system_palette(choice);
    let dark = theme::resolved(choice, theme::system_dark()) == "dark";
    popup.set_dark(dark);
    settings.set_dark(dark);
    for breeze in [popup.global::<Breeze>(), settings.global::<Breeze>()] {
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
    let font = theme::system_font();
    popup.set_ui_font_family(font.family.clone().into());
    settings.set_ui_font_family(font.family.into());
    let scale = popup.window().scale_factor().max(1.0);
    let size = (font.size_px * scale).round() / scale;
    popup.set_ui_font_size(size);
    settings.set_ui_font_size(size);
    popup.set_ui_font_weight(font.weight);
    settings.set_ui_font_weight(font.weight);
    settings.set_theme_index(match choice {
        "light" => 1,
        "dark" => 2,
        _ => 0,
    });
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

fn fill(popup: &Popup, settings: &Settings, scan: &device::Scan) {
    let rows: Vec<DeviceRow> = scan
        .devices
        .iter()
        .map(|device| {
            let themed = icons::theme_icon(device.kind.icon());
            let (icon, tint) = match themed {
                Some(icon) => (icon.image, icon.tint),
                None => {
                    let data: &[u8] = match device.kind {
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
                name: device.name.clone().into(),
                charge: device.reading.text().into(),
                detail: if device.status.is_empty() {
                    device.source.clone()
                } else {
                    device.status.clone()
                }
                .into(),
                icon,
                tint,
                low: device.reading.low(),
            }
        })
        .collect();
    popup.set_rows(ModelRc::new(VecModel::from(rows)));
    popup.set_empty_message(if scan.warnings.is_empty() {
        "Устройств с батареей не найдено.\nПодключите устройство и обновите список."
    } else { "Устройств не найдено.\nНекоторые источники заряда недоступны.\nПодробности в настройках." }.into());
    let mut diagnostics = scan.warnings.clone();
    diagnostics.extend(
        scan.devices
            .iter()
            .filter(|d| !d.status.is_empty())
            .map(|d| format!("{}: {}", d.name, d.status)),
    );
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
    let state = Rc::new(RefCell::new(State {
        config: config::load(),
        scan: device::Scan::default(),
        opened: Instant::now(),
        blur: None,
        seen_focus: false,
        closing: false,
        busy: true,
    }));
    paint(&popup, &settings, &state.borrow().config.theme);
    let (sender, mut commands) = tokio::sync::mpsc::unbounded_channel();
    let (refresh, mut refresh_rx) = tokio::sync::mpsc::unbounded_channel::<()>();
    let (results_tx, results) = std::sync::mpsc::channel();
    let tray = runtime.block_on(async {
        use ksni::TrayMethods;
        tray::Tray {
            sender: sender.clone(),
            icons: icons::tray(),
            summary: "Поиск устройств…".into(),
        }
        .assume_sni_available(true)
        .spawn()
        .await
    })?;
    if !runtime.block_on(place::install()) {
        eprintln!("shinpi: KWin positioning unavailable; using compositor placement");
    }
    runtime.spawn(async move {
        loop {
            if results_tx.send(None).is_err() {
                break;
            }
            let result = scan().await;
            if results_tx.send(Some(result)).is_err() {
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
    let weak = settings.as_weak();
    settings.on_dismiss(move || {
        if let Some(settings) = weak.upgrade() {
            let _ = settings.hide();
        }
    });
    settings
        .window()
        .on_close_requested(|| slint::CloseRequestResponse::HideWindow);
    let weak_popup = popup.as_weak();
    let weak_settings = settings.as_weak();
    let data = state.clone();
    settings.on_theme_selected(move |index| {
        let (Some(popup), Some(settings)) = (weak_popup.upgrade(), weak_settings.upgrade()) else {
            return;
        };
        let mut state = data.borrow_mut();
        state.config.theme = match index {
            1 => "light",
            2 => "dark",
            _ => "system",
        }
        .into();
        if let Err(e) = config::save(&state.config) {
            settings.set_error(format!("Не удалось сохранить настройки: {e}").into());
        } else {
            settings.set_error("".into());
        }
        paint(&popup, &settings, &state.config.theme);
    });
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
    let data = state.clone();
    let slide = animation.clone();
    let last_toggle = Cell::new(Instant::now() - Duration::from_secs(1));
    let last_theme = Cell::new(Instant::now());
    let rt_handle = runtime.handle().clone();
    let poll = slint::Timer::default();
    poll.start(
        slint::TimerMode::Repeated,
        Duration::from_millis(60),
        move || {
            let (Some(popup), Some(settings)) = (tick_popup.upgrade(), tick_settings.upgrade())
            else {
                return;
            };
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
                    Command::Settings => {
                        if popup.window().is_visible() && !popup.get_pinned() {
                            animate(&popup, &data, &slide, false);
                        }
                        settings.show().ok();
                        let _ = settings.window().with_winit_window(|w| {
                            w.set_minimized(false);
                            w.focus_window();
                        });
                    }
                    Command::Quit => {
                        let _ = slint::quit_event_loop();
                    }
                    _ => {}
                }
            }
            while let Ok(result) = results.try_recv() {
                let mut state = data.borrow_mut();
                state.busy = result.is_none();
                if let Some(result) = result {
                    fill(&popup, &settings, &result);
                    let summary = if result.devices.is_empty() {
                        "Устройств не найдено".into()
                    } else {
                        result
                            .devices
                            .iter()
                            .map(|d| format!("{}: {}", d.name, d.reading.text()))
                            .collect::<Vec<_>>()
                            .join("\n")
                    };
                    let tray = tray.clone();
                    rt_handle.spawn(async move {
                        tray.update(move |t| t.summary = summary).await;
                    });
                    state.scan = result;
                }
            }
            popup.set_busy(data.borrow().busy);
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
            if last_theme.get().elapsed() >= Duration::from_secs(2) {
                last_theme.set(Instant::now());
                paint(&popup, &settings, &data.borrow().config.theme);
                fill(&popup, &settings, &data.borrow().scan);
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
