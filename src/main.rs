mod config;
mod device;
mod hid;
mod i18n;
mod icons;
mod notices;
mod notify;
mod place;
mod system;
mod theme;
mod tray;
mod updates;

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
    catalog: i18n::Catalog,
    scan: device::Scan,
    opened: Instant,
    blur: Option<Instant>,
    seen_focus: bool,
    popup_focused: bool,
    notices_focused: bool,
    closing: bool,
    busy: bool,
    editing: Option<String>,
    pending_notifications: HashSet<String>,
    notification_error: String,
    update: updates::Status,
}

enum Event {
    Started,
    Scanned(device::Scan),
    UpdateChecked(Result<Option<String>, ()>),
    LinkOpened(Result<(), String>),
    Notified(String, u64, String, Result<(), String>),
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
            warnings: vec![format!("HID scan failed: {e}")],
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

#[derive(Default)]
struct Shell {
    popup: Option<Popup>,
    settings: Option<Settings>,
    notices: Option<Notices>,
}

thread_local! {
    static CREATING_PANEL: Cell<&'static str> = const { Cell::new("") };
    static POPUP_CLASS_HELD: Cell<bool> = const { Cell::new(false) };
}

const POPUP_CLASS: &str = "shinpi-popup";

/// Wayland destroys a hidden popup and maps the next one before its title exists.
/// Hold a dedicated application id until that window exists so KWin skips the taskbar.
fn hold_popup_class() {
    if POPUP_CLASS_HELD.replace(true) {
        return;
    }
    let _ = slint::set_xdg_app_id(POPUP_CLASS);
}

fn release_popup_class(shell: &Shell) {
    if !POPUP_CLASS_HELD.get() {
        return;
    }
    let pending =
        |window: &slint::Window| window.is_visible() && window.with_winit_window(|_| ()).is_none();
    if shell
        .popup
        .as_ref()
        .is_some_and(|popup| pending(popup.window()))
        || shell
            .notices
            .as_ref()
            .is_some_and(|notices| pending(notices.window()))
    {
        return;
    }
    POPUP_CLASS_HELD.set(false);
    let _ = slint::set_xdg_app_id("shinpi");
}

fn window_attributes(
    attributes: winit::window::WindowAttributes,
    panel: &str,
) -> winit::window::WindowAttributes {
    use winit::platform::x11::{WindowAttributesExtX11, WindowType};
    let popup = !panel.is_empty();
    let attributes = attributes
        .with_resizable(false)
        .with_transparent(popup)
        .with_title(if popup { panel } else { "shinpi" });
    if popup {
        attributes.with_x11_window_type(vec![WindowType::PopupMenu])
    } else {
        attributes
    }
}

fn ensure_popup(
    shell: &mut Shell,
    state: &Rc<RefCell<State>>,
    sender: &tokio::sync::mpsc::UnboundedSender<Command>,
) -> Option<Popup> {
    if let Some(popup) = &shell.popup {
        return Some(popup.clone_strong());
    }
    hold_popup_class();
    CREATING_PANEL.set("shinpi-devices");
    let result = Popup::new();
    if let Ok(popup) = &result {
        let _ = popup.window();
    }
    CREATING_PANEL.set("");
    let popup = result.map_err(|e| eprintln!("shinpi: popup: {e}")).ok()?;
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
    popup.on_history_toggled(move || {
        let _ = tx.send(Command::ToggleHistory);
    });
    let tx = sender.clone();
    popup.on_add(move || {
        let _ = tx.send(Command::Add);
    });
    bind_panel_events(popup.window(), &popup, state, sender, false);

    shell.popup = Some(popup.clone_strong());
    Some(popup)
}

fn bind_panel_events(
    window: &slint::Window,
    popup: &Popup,
    state: &Rc<RefCell<State>>,
    sender: &tokio::sync::mpsc::UnboundedSender<Command>,
    notices: bool,
) {
    let weak = popup.as_weak();
    let data = state.clone();
    let tx = sender.clone();
    window.on_close_requested(move || {
        let _ = tx.send(Command::Dismiss);
        slint::CloseRequestResponse::KeepWindowShown
    });
    let tx = sender.clone();
    window.on_winit_window_event(move |_, event| {
        match event {
            winit::event::WindowEvent::Focused(focused) => {
                let mut state = data.borrow_mut();
                if notices {
                    state.notices_focused = *focused;
                } else {
                    state.popup_focused = *focused;
                }
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
}

fn ensure_notices(
    shell: &mut Shell,
    state: &Rc<RefCell<State>>,
    sender: &tokio::sync::mpsc::UnboundedSender<Command>,
) -> Option<Notices> {
    if let Some(notices) = &shell.notices {
        return Some(notices.clone_strong());
    }
    let popup = shell.popup.as_ref()?;
    hold_popup_class();
    CREATING_PANEL.set("shinpi-notices");
    let result = Notices::new();
    if let Ok(notices) = &result {
        let _ = notices.window();
    }
    CREATING_PANEL.set("");
    let notices = result.map_err(|e| eprintln!("shinpi: notices: {e}")).ok()?;
    bind_panel_events(notices.window(), popup, state, sender, true);
    let tx = sender.clone();
    notices.on_dismiss_note(move |index| {
        let _ = tx.send(Command::DismissNotice(index as usize));
    });
    shell.notices = Some(notices.clone_strong());
    Some(notices)
}

fn sync_notices(
    shell: &mut Shell,
    state: &Rc<RefCell<State>>,
    sender: &tokio::sync::mpsc::UnboundedSender<Command>,
) {
    let show = {
        let state = state.borrow();
        shell
            .popup
            .as_ref()
            .is_some_and(|p| p.window().is_visible())
            && !state.closing
            && state.config.history_enabled
            && !state.config.history_hidden
            && !state.config.history.is_empty()
    };
    if !show {
        if let Some(notices) = &shell.notices {
            if notices.get_expanded() {
                notices.set_expanded(false);
                let weak = notices.as_weak();
                slint::Timer::single_shot(Duration::from_millis(180), move || {
                    if let Some(notices) = weak.upgrade() {
                        if !notices.get_expanded() {
                            let _ = notices.hide();
                        }
                    }
                });
            }
        }
        return;
    }
    let fresh = shell.notices.is_none();
    let Some(notices) = ensure_notices(shell, state, sender) else {
        return;
    };
    if fresh {
        paint(shell, &state.borrow());
        fill(shell, &state.borrow());
    }
    if !notices.window().is_visible() {
        hold_popup_class();
        notices.set_expanded(false);
        let _ = notices.get_slide();
        let _ = notices.show();
        if let Some(popup) = &shell.popup {
            let _ = popup.window().with_winit_window(|w| w.focus_window());
        }
    }
    if !notices.get_expanded() {
        notices.set_expanded(true);
    }
    // KWin places Wayland surfaces; other compositors permit positioning on X11.
    if let Some(popup) = &shell.popup {
        let position = popup
            .window()
            .with_winit_window(|w| {
                Some((
                    w.outer_position().ok(),
                    w.inner_size().height,
                    w.current_monitor()?,
                ))
            })
            .flatten();
        if let Some((position, popup_height, monitor)) = position {
            let top = monitor.position().y;
            let scale = notices.window().scale_factor().max(1.0);
            let available =
                (monitor.size().height.saturating_sub(popup_height + 16) as f32 / scale).min(360.0);
            notices.set_available_height(available.max(72.0));
            if let Some(mut position) = position {
                let height = notices.window().size().height as i32;
                if position.y - height < top {
                    position.y = top + height;
                    popup
                        .window()
                        .set_position(slint::PhysicalPosition::new(position.x, position.y));
                }
                let target = slint::PhysicalPosition::new(position.x, position.y - height);
                if notices.window().position() != target {
                    notices.window().set_position(target);
                }
            }
        }
    }
}

fn ensure_settings(
    shell: &mut Shell,
    sender: &tokio::sync::mpsc::UnboundedSender<Command>,
) -> Option<Settings> {
    if let Some(settings) = &shell.settings {
        return Some(settings.clone_strong());
    }
    let settings = Settings::new()
        .map_err(|e| eprintln!("shinpi: settings: {e}"))
        .ok()?;
    let tx = sender.clone();
    settings.on_add(move || {
        let _ = tx.send(Command::Add);
    });
    let tx = sender.clone();
    settings.on_refresh(move || {
        let _ = tx.send(Command::Refresh);
    });
    let tx = sender.clone();
    settings.on_add_device(move |index| {
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
    settings.on_save_device(move |name, kind| {
        let _ = tx.send(Command::SaveEdit(name.to_string(), kind as usize));
    });
    let tx = sender.clone();
    settings.on_language_selected(move |index| {
        let _ = tx.send(Command::Language(index as usize));
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

    let tx = sender.clone();
    settings.on_panel_width_selected(move |value| {
        let _ = tx.send(Command::PanelWidth(value as u32));
    });
    let tx = sender.clone();
    settings.on_history_enabled_selected(move |value| {
        let _ = tx.send(Command::HistoryEnabled(value));
    });
    let tx = sender.clone();
    settings.on_history_count_selected(move |value| {
        let _ = tx.send(Command::HistoryCount(value as u32));
    });

    let tx = sender.clone();
    settings.on_check_update(move || {
        let _ = tx.send(Command::CheckUpdate);
    });
    let tx = sender.clone();
    settings.on_open_link(move |index| {
        let _ = tx.send(Command::OpenLink(index));
    });
    let weak = settings.as_weak();
    settings.on_dismiss(move || {
        if let Some(settings) = weak.upgrade() {
            settings.invoke_close_dialogs();
            let _ = settings.hide();
        }
    });
    let weak = settings.as_weak();
    settings.window().on_close_requested(move || {
        if let Some(settings) = weak.upgrade() {
            settings.invoke_close_dialogs();
        }
        slint::CloseRequestResponse::HideWindow
    });
    shell.settings = Some(settings.clone_strong());
    Some(settings)
}

fn paint(shell: &Shell, state: &State) {
    let cfg = &state.config;
    let catalog = &state.catalog;
    let labels = ModelRc::new(VecModel::from(catalog.labels()));
    let palette = theme::system_palette(&cfg.theme);
    let dark = theme::resolved(&cfg.theme, theme::system_dark()) == "dark";
    let font = theme::system_font();
    macro_rules! window_style {
        ($window:expr) => {{
            let window = $window;
            window.global::<Translations>().set_strings(labels.clone());
            window.set_dark(dark);
            window.set_ui_font_family(font.family.clone().into());
            let scale = window.window().scale_factor().max(1.0);
            window.set_ui_font_size((font.size_px * scale).round() / scale);
            window.set_ui_font_weight(font.weight);
            let breeze = window.global::<Breeze>();
            breeze.set_window(color(palette.window));
            breeze.set_button(color(palette.button));
            breeze.set_button_hover(color(palette.button_hover));
            breeze.set_fg(color(palette.text));
            breeze.set_inactive(color(palette.inactive));
            breeze.set_selection(color(palette.accent));
            breeze.set_selection_fg(color(palette.accent_text));
            breeze.set_frame(color(palette.frame));
            breeze.set_outline(color(palette.outline));
        }};
    }
    if let Some(popup) = &shell.popup {
        window_style!(popup);
        popup.set_panel_width(cfg.panel_width as i32);
        popup.set_history_enabled(cfg.history_enabled);
        popup.set_history_hidden(cfg.history_hidden);
        if let Some(icon) = icons::theme_icon("configure") {
            popup.set_icon_configure(icon.image);
            popup.set_tint_configure(icon.tint);
        }
        if let Some(icon) = icons::theme_icon("window-pin") {
            popup.set_icon_pin(icon.image);
            popup.set_tint_pin(icon.tint);
        }
    }
    if let Some(notices) = &shell.notices {
        window_style!(notices);
        notices.set_panel_width(cfg.panel_width as i32);
    }
    if let Some(settings) = &shell.settings {
        window_style!(settings);
        settings.set_version(env!("CARGO_PKG_VERSION").into());
        settings.set_checking_update(matches!(state.update, updates::Status::Checking));
        settings.set_update_available(matches!(state.update, updates::Status::Available(_)));
        settings.set_update_status(
            match &state.update {
                updates::Status::Idle => String::new(),
                updates::Status::Checking => catalog.tr("Checking for updates…"),
                updates::Status::Current => catalog.tr("You are using the latest version."),
                updates::Status::Available(version) => {
                    catalog.trf("Version {} is available.", &[version])
                }
                updates::Status::Failed => {
                    catalog.tr("Could not check for updates. Try again later.")
                }
            }
            .into(),
        );
        settings.set_panel_width(cfg.panel_width as i32);
        settings.set_history_enabled(cfg.history_enabled);
        settings.set_history_count(cfg.history_count as i32);
        let languages = i18n::languages();
        let mut names = vec![catalog.tr("System language").into()];
        names.extend(
            languages
                .iter()
                .map(|name| slint::SharedString::from(i18n::language_label(name))),
        );
        settings.set_language_names(ModelRc::new(VecModel::from(names)));
        settings.set_language_index(if cfg.language.is_empty() {
            0
        } else {
            languages
                .iter()
                .position(|n| n == &catalog.language)
                .map(|i| i as i32 + 1)
                .unwrap_or(1)
        });
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
        settings
            .set_tray_preview(slint::Image::load_from_svg_data(svg.as_bytes()).unwrap_or_default());
    }
}

fn row(
    name: &str,
    kind: device::Kind,
    reading: &device::Reading,
    source: &str,
    added: bool,
    catalog: &i18n::Catalog,
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
        charge: catalog.reading(reading).into(),
        detail: source.into(),
        icon,
        tint,
        low: reading.low(15),
        added,
    }
}

fn fill(shell: &Shell, state: &State) {
    if shell.popup.is_none() && shell.settings.is_none() {
        return;
    }
    let rows: Vec<_> = state
        .config
        .devices
        .iter()
        .map(|d| row(&d.name, d.kind, &d.reading, &d.source, true, &state.catalog))
        .collect();
    if let Some(popup) = &shell.popup {
        popup.set_rows(ModelRc::new(VecModel::from(rows.clone())));
        popup.set_busy(state.busy);
    }
    if let Some(notices) = &shell.notices {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
        let first = state
            .config
            .history
            .len()
            .saturating_sub(state.config.history_count as usize);
        notices.set_notes(ModelRc::new(VecModel::from(
            state
                .config
                .history
                .iter()
                .enumerate()
                .skip(first)
                .map(|(index, note)| NoticeRow {
                    index: index as i32,
                    message: note.message.clone().into(),
                    when: notices::ago(note.at, now, &state.catalog).into(),
                    fade: note.fade,
                })
                .collect::<Vec<_>>(),
        )));
    }
    let Some(settings) = &shell.settings else {
        return;
    };
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
                &state.catalog,
            )
        })
        .collect();
    settings.set_found_rows(ModelRc::new(VecModel::from(candidates)));
    settings.set_busy(state.busy);
    let mut diagnostics: Vec<_> = state
        .scan
        .warnings
        .iter()
        .map(|s| state.catalog.message(s))
        .collect();
    diagnostics.extend(
        state
            .scan
            .devices
            .iter()
            .filter(|d| !d.status.is_empty())
            .map(|d| {
                format!(
                    "{} · {}: {}",
                    d.name,
                    d.source,
                    state.catalog.message(&d.status)
                )
            }),
    );
    if !state.notification_error.is_empty() {
        diagnostics.push(state.catalog.message(&state.notification_error));
    }
    settings.set_diagnostics(if diagnostics.is_empty() {
        state.catalog.tr("All sources have been scanned.").into()
    } else {
        diagnostics.join("\n\n").into()
    });
}

fn animate(popup: &Popup, state: &Rc<RefCell<State>>, timer: &Rc<slint::Timer>, open: bool) {
    timer.stop();
    if open {
        hold_popup_class();
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
    let catalog = i18n::Catalog::load(&config::load().language);
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.iter().any(|a| a == "--help") {
        println!("shinpi [--window | --once | --print-udev-rules]\n  --window             {}\n  --once               {}\n  --print-udev-rules   {}\n  --help               {}",
            catalog.tr("Open the popup at startup"), catalog.tr("Print current readings as JSON without starting the GUI"),
            catalog.tr("Print access rules for supported HID devices"), catalog.tr("Show help"));
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
        return Err(catalog.tr("Unknown argument. See --help").into());
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
        .with_winit_window_attributes_hook(|a| window_attributes(a, CREATING_PANEL.get()))
        .select()?;
    let _ = slint::set_xdg_app_id("shinpi");
    let shell = Rc::new(RefCell::new(Shell::default()));
    let state = Rc::new(RefCell::new(State {
        config: config::load(),
        catalog,
        scan: device::Scan::default(),
        opened: Instant::now(),
        blur: None,
        seen_focus: false,
        popup_focused: false,
        notices_focused: false,
        closing: false,
        busy: true,
        editing: None,
        pending_notifications: HashSet::new(),
        notification_error: String::new(),
        update: updates::Status::Idle,
    }));
    let (sender, mut commands) = tokio::sync::mpsc::unbounded_channel();
    let (refresh, mut refresh_rx) = tokio::sync::mpsc::unbounded_channel::<()>();
    let (results_tx, results) = std::sync::mpsc::channel();
    let initial_summary = state.borrow().catalog.tr("Scanning for devices…");
    let initial_labels =
        ["Refresh", "Settings", "Quit"].map(|label| state.borrow().catalog.tr(label));
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
                    theme::resolved(&cfg.theme, theme::system_dark()) == "dark",
                )
            },
            summary: initial_summary,
            labels: initial_labels,
        }
        .assume_sni_available(true)
        // Flatpak forbids owning ksni's PID-based well-known bus name.
        .disable_dbus_name(std::path::Path::new("/.flatpak-info").exists())
        .spawn()
        .await
    })?;
    if !runtime.block_on(place::install()) {
        eprintln!("shinpi: KWin positioning unavailable; using compositor placement");
    }
    let notification_tx = results_tx.clone();
    let update_tx = results_tx.clone();
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
    let tick_shell = shell.clone();
    let tx = sender.clone();
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
            release_popup_class(&tick_shell.borrow());
            let mut windows = tick_shell.borrow_mut();
            let mut changed = false;
            let mut redraw = false;
            while let Ok(command) = commands.try_recv() {
                match command {
                    Command::Toggle
                        if last_toggle.get().elapsed() >= Duration::from_millis(300) =>
                    {
                        last_toggle.set(Instant::now());
                        if let Some(popup) = ensure_popup(&mut windows, &data, &tx) {
                            paint(&windows, &data.borrow());
                            fill(&windows, &data.borrow());
                            let open = !popup.window().is_visible() || data.borrow().closing;
                            if open && !data.borrow().busy {
                                data.borrow_mut().busy = true;
                                let _ = refresh.send(());
                            }
                            animate(&popup, &data, &slide, open);
                        }
                    }
                    Command::Refresh => {
                        if !data.borrow().busy {
                            data.borrow_mut().busy = true;
                            let _ = refresh.send(());
                        }
                    }
                    Command::Dismiss => {
                        if let Some(popup) = &windows.popup {
                            if popup.window().is_visible() && !data.borrow().closing {
                                animate(popup, &data, &slide, false);
                            }
                        }
                    }
                    Command::Settings | Command::Add => {
                        let show_add = matches!(command, Command::Add);
                        if let Some(popup) = &windows.popup {
                            if popup.window().is_visible() && !popup.get_pinned() {
                                animate(popup, &data, &slide, false);
                            }
                        }
                        if let Some(settings) = ensure_settings(&mut windows, &tx) {
                            paint(&windows, &data.borrow());
                            fill(&windows, &data.borrow());
                            settings.show().ok();
                            let _ = settings.window().with_winit_window(|w| {
                                w.set_minimized(false);
                                w.focus_window();
                            });
                            if show_add {
                                settings.invoke_open_add();
                                if !data.borrow().busy {
                                    let _ = refresh.send(());
                                }
                            }
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
                        if let (Some(device), Some(settings)) = (device, &windows.settings) {
                            data.borrow_mut().editing = Some(device.id);
                            settings.set_edit_name(device.name.into());
                            settings.set_edit_kind(device.kind as i32);
                            settings.set_edit_error("".into());
                            settings.invoke_open_editor();
                        }
                    }
                    Command::SaveEdit(name, index) => {
                        if let Some(settings) = &windows.settings {
                            let name: String = name.trim().chars().take(80).collect();
                            if name.is_empty() {
                                settings.set_edit_error(
                                    data.borrow().catalog.tr("Enter a device name.").into(),
                                );
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
                                settings.invoke_close_editor();
                            }
                        }
                    }
                    Command::Language(index) => {
                        let language = index
                            .checked_sub(1)
                            .and_then(|i| i18n::languages().get(i).cloned())
                            .unwrap_or_default();
                        let mut state = data.borrow_mut();
                        state.config.language = language;
                        state.catalog = i18n::Catalog::load(&state.config.language);
                        changed = true;
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
                    Command::PanelWidth(value) => {
                        data.borrow_mut().config.panel_width = value.clamp(280, 800);
                        changed = true;
                    }
                    Command::HistoryEnabled(value) => {
                        data.borrow_mut().config.history_enabled = value;
                        changed = true;
                    }
                    Command::HistoryCount(value) => {
                        data.borrow_mut().config.history_count = value.clamp(1, 20);
                        changed = true;
                    }
                    Command::ToggleHistory => {
                        let mut state = data.borrow_mut();
                        state.config.history_hidden = !state.config.history_hidden;
                        changed = true;
                    }
                    Command::DismissNotice(index) => {
                        if let Some(note) = data.borrow_mut().config.history.get_mut(index) {
                            note.closing = true;
                            redraw = true;
                        }
                    }
                    Command::CheckUpdate => {
                        if !matches!(data.borrow().update, updates::Status::Checking) {
                            data.borrow_mut().update = updates::Status::Checking;
                            redraw = true;
                            let tx = update_tx.clone();
                            rt_handle.spawn(async move {
                                let result = tokio::task::spawn_blocking(updates::check)
                                    .await
                                    .unwrap_or(Err(()));
                                let _ = tx.send(Event::UpdateChecked(result));
                            });
                        }
                    }
                    Command::OpenLink(index) => {
                        let tx = update_tx.clone();
                        rt_handle.spawn(async move {
                            let result = updates::open_link(index).await.map_err(|e| e.to_string());
                            let _ = tx.send(Event::LinkOpened(result));
                        });
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
                    Event::UpdateChecked(result) => {
                        state.update = match result {
                            Ok(Some(version)) => updates::Status::Available(version),
                            Ok(None) => updates::Status::Current,
                            Err(()) => updates::Status::Failed,
                        };
                    }
                    Event::LinkOpened(result) => {
                        if let Some(settings) = &windows.settings {
                            settings.set_error(if result.is_err() {
                                state.catalog.tr("Could not open the link.").into()
                            } else {
                                "".into()
                            });
                        }
                    }
                    Event::Started => state.busy = true,
                    Event::Scanned(result) => {
                        state.busy = false;
                        changed |= state.config.observe(&result);
                        let now = SystemTime::now()
                            .duration_since(UNIX_EPOCH)
                            .unwrap_or_default()
                            .as_secs();
                        for (id, message) in
                            notify::due(&state.config, &result, now, &state.catalog)
                        {
                            if state.pending_notifications.insert(id.clone()) {
                                let tx = notification_tx.clone();
                                let title = state.catalog.tr("Low device battery");
                                rt_handle.spawn(async move {
                                    let result = notify::send(&title, &message)
                                        .await
                                        .map_err(|e| e.to_string());
                                    let _ = tx.send(Event::Notified(id, now, message, result));
                                });
                            }
                        }
                        state.scan = result;
                    }
                    Event::Notified(id, at, message, result) => {
                        state.pending_notifications.remove(&id);
                        match result {
                            Ok(()) => {
                                if let Some(device) =
                                    state.config.devices.iter_mut().find(|d| d.id == id)
                                {
                                    device.last_notified = at;
                                }
                                let animate = windows
                                    .notices
                                    .as_ref()
                                    .is_some_and(|n| n.window().is_visible());
                                notices::push(&mut state.config.history, message, at, animate);
                                changed = true;
                                state.notification_error.clear();
                            }
                            Err(error) => {
                                state.notification_error =
                                    format!("Notifications unavailable: {error}")
                            }
                        }
                    }
                }
            }
            {
                let mut state = data.borrow_mut();
                let count = state.config.history.len();
                redraw |= notices::step(&mut state.config.history);
                changed |= count != state.config.history.len();
            }
            if changed {
                let error = config::save(&data.borrow().config)
                    .err()
                    .map(|e| {
                        data.borrow()
                            .catalog
                            .trf("Could not save settings: {}", &[&e.to_string()])
                    })
                    .unwrap_or_default();
                if !error.is_empty() {
                    eprintln!("shinpi: {error}");
                }
                if let Some(settings) = &windows.settings {
                    settings.set_error(error.into());
                }
            }
            if changed || redraw || last_theme.get().elapsed() >= Duration::from_secs(2) {
                last_theme.set(Instant::now());
                let state = data.borrow();
                paint(&windows, &state);
                fill(&windows, &state);
            }
            {
                let state = data.borrow();
                let cfg = &state.config;
                let signature = (
                    cfg.minimum(),
                    cfg.tray_orientation,
                    cfg.colored_icon,
                    theme::resolved(&cfg.theme, theme::system_dark()) == "dark",
                );
                if last_icon.get() != Some(signature) || changed || redraw {
                    last_icon.set(Some(signature));
                    let icons = icons::tray(signature.0, signature.1, signature.2, signature.3);
                    let summary = if cfg.devices.is_empty() {
                        state.catalog.tr("Add devices")
                    } else {
                        cfg.devices
                            .iter()
                            .map(|d| format!("{}: {}", d.name, state.catalog.reading(&d.reading)))
                            .collect::<Vec<_>>()
                            .join("\n")
                    };
                    let labels =
                        ["Refresh", "Settings", "Quit"].map(|label| state.catalog.tr(label));
                    let tray = tray.clone();
                    rt_handle.spawn(async move {
                        tray.update(move |t| {
                            t.icons = icons;
                            t.summary = summary;
                            t.labels = labels;
                        })
                        .await;
                    });
                }
            }
            if let Some(popup) = &windows.popup {
                let should_close = {
                    let state = data.borrow();
                    popup.window().is_visible()
                        && !state.popup_focused
                        && !state.notices_focused
                        && !popup.get_pinned()
                        && !state.closing
                        && state.opened.elapsed() >= Duration::from_millis(350)
                        && state
                            .blur
                            .is_some_and(|at| at.elapsed() >= Duration::from_millis(150))
                };
                if should_close {
                    animate(popup, &data, &slide, false);
                }
            }
            sync_notices(&mut windows, &data, &tx);
        },
    );
    if args.iter().any(|a| a == "--window") {
        let _ = sender.send(Command::Toggle);
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

#[cfg(test)]
mod window_tests {
    use super::*;
    #[test]
    fn native_identity_is_set_before_mapping_and_windows_cannot_resize() {
        let popup = window_attributes(winit::window::WindowAttributes::default(), "shinpi-devices");
        let settings = window_attributes(winit::window::WindowAttributes::default(), "");
        assert_eq!(popup.title, "shinpi-devices");
        assert_eq!(settings.title, "shinpi");
        let notices =
            window_attributes(winit::window::WindowAttributes::default(), "shinpi-notices");
        assert_eq!(notices.title, "shinpi-notices");
        assert!(notices.transparent && !notices.resizable);
        assert!(!popup.resizable && !settings.resizable);
        assert!(popup.transparent && !settings.transparent);
    }
}
