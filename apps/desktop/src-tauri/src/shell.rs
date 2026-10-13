//! App shell: startup, tray, hotkeys, summon and dismiss, config watching,
//! and the bridge from the event bus to the personality engine.

use std::{
    fs,
    sync::{Arc, Mutex},
    thread,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

use frosty_core::{
    config::{ConfigStore, DEFAULT_CONFIG_FILE, FrostyConfig},
    db::Database,
    events::{Event, EventBus},
    home::FrostyHome,
    personality::{Decision, Microcopy, PersonalityEngine},
    theme::{Theme, built_in_themes},
};
use serde::Serialize;
use serde_json::json;
use tauri::{
    AppHandle, Emitter, Manager,
    menu::{Menu, MenuItem},
    tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent},
};
use tauri_plugin_autostart::ManagerExt as _;
use tauri_plugin_global_shortcut::{GlobalShortcutExt, Shortcut, ShortcutEvent, ShortcutState};

use crate::{
    caps, notify,
    state::{AppState, Hotkeys, Notice},
    windows::{self, MAIN, OVERLAY},
};

#[derive(Serialize, Clone)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum DecisionDto {
    Play {
        animation: Option<String>,
        intensity: f32,
        text: Option<String>,
        show_chip: bool,
        key: Option<String>,
    },
    Dropped {
        reason: String,
    },
}

#[derive(Serialize, Clone)]
pub struct DecisionPayload {
    pub event: String,
    pub decision: DecisionDto,
}

fn unix_millis() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_millis() as u64)
}

/// Loads the config. A file that does not parse is moved aside and replaced
/// with defaults so Frosty still starts (principle 5, fail soft).
fn load_config(home: &FrostyHome) -> (ConfigStore, Option<String>) {
    match ConfigStore::load(home.config_file(), home.backups_dir()) {
        Ok(store) => (store, None),
        Err(error) => {
            let _ = fs::create_dir_all(home.backups_dir());
            let saved = home
                .backups_dir()
                .join(format!("frosty-broken-{}.toml", unix_millis()));
            let _ = fs::copy(home.config_file(), &saved);
            let _ = fs::write(home.config_file(), DEFAULT_CONFIG_FILE);
            let store = ConfigStore::load(home.config_file(), home.backups_dir())
                .expect("the default config must load");
            let notice = format!(
                "frosty.toml could not be read ({error}). It was replaced with defaults, and the broken file was kept in the backups folder."
            );
            (store, Some(notice))
        }
    }
}

fn load_themes(home: &FrostyHome) -> Vec<Theme> {
    let mut themes = built_in_themes();
    if let Ok(entries) = fs::read_dir(home.themes_dir()) {
        for path in entries.filter_map(Result::ok).map(|e| e.path()) {
            if path.extension().and_then(|e| e.to_str()) != Some("toml") {
                continue;
            }
            match fs::read_to_string(&path)
                .map_err(|e| e.to_string())
                .and_then(|t| Theme::parse(&t).map_err(|e| e.to_string()))
            {
                // A user theme with a built-in id replaces the built-in.
                Ok(theme) => {
                    themes.retain(|t| t.id != theme.id);
                    themes.push(theme);
                }
                Err(e) => eprintln!("skipping theme {}: {e}", path.display()),
            }
        }
    }
    themes
}

fn build_engine(cfg: &FrostyConfig, home: &FrostyHome) -> PersonalityEngine {
    let mut microcopy = Microcopy::core_en();
    let user_pack = home
        .microcopy_dir()
        .join(format!("{}.json", cfg.personality.locale));
    if let Ok(text) = fs::read_to_string(user_pack) {
        match Microcopy::from_json(&text) {
            Ok(pack) => microcopy.merge(pack),
            Err(e) => eprintln!("ignoring user microcopy: {e}"),
        }
    }
    let mut engine =
        PersonalityEngine::new(cfg.personality.tone, cfg.character.reactions, microcopy);
    engine.set_never_react(cfg.personality.never_react.clone());
    engine
}

pub fn publish(app: &AppHandle, name: &str, payload: serde_json::Value, significance: Option<f32>) {
    let mut event = Event::new(name, payload);
    if let Some(s) = significance {
        event = event.with_significance(s);
    }
    app.state::<AppState>().bus.publish(event);
}

/// Replaces every notice from `source` and tells the windows.
pub fn set_notices(app: &AppHandle, source: &str, messages: Vec<String>) {
    let state = app.state::<AppState>();
    let all: Vec<Notice> = {
        let mut notices = state.notices.lock().unwrap();
        notices.retain(|n| n.source != source);
        notices.extend(messages.into_iter().map(|message| Notice {
            source: source.to_string(),
            level: "error".into(),
            message,
        }));
        notices.clone()
    };
    let _ = app.emit("frosty://notices", all);
}

pub fn setup(app: &mut tauri::App) -> Result<(), Box<dyn std::error::Error>> {
    let home = FrostyHome::resolve();
    home.ensure()?;
    let (config, startup_notice) = load_config(&home);
    let db = Database::open(&home.database_file(), &home.backups_dir())?;
    let device_id = db.device_id(&config.current().core.device_name)?;
    let builtin_root = caps::find_builtin_root(app.handle());
    let registry = caps::initial_registry(builtin_root.as_deref(), &home.capabilities_dir());
    let dnd_manual = db.kv_get("dnd.manual").ok().flatten().as_deref() == Some("1");
    let engine = build_engine(config.current(), &home);
    let themes = load_themes(&home);

    app.manage(AppState {
        home,
        config: Mutex::new(config),
        db: Mutex::new(db),
        engine: Mutex::new(engine),
        bus: Arc::new(EventBus::default()),
        themes: Mutex::new(themes),
        hotkeys: Mutex::new(Hotkeys::default()),
        notices: Mutex::new(Vec::new()),
        started: Instant::now(),
        summon_started: Mutex::new(None),
        registry: Mutex::new(registry),
        pending_calls: Mutex::new(std::collections::HashMap::new()),
        next_call: std::sync::atomic::AtomicU64::new(1),
        badges: Mutex::new(std::collections::HashMap::new()),
        builtin_root,
        device_id,
        fullscreen_app: std::sync::atomic::AtomicBool::new(false),
        prev_foreground: std::sync::atomic::AtomicIsize::new(0),
        dnd_manual: std::sync::atomic::AtomicBool::new(dnd_manual),
        rate: Mutex::new(frosty_core::notifications::RateLimiter::default()),
        tray_dnd: Mutex::new(None),
    });

    let handle = app.handle().clone();
    windows::create_overlay(&handle)?;
    windows::create_logic(&handle)?;
    install_reactions(&handle);
    build_tray(&handle)?;
    apply_config(&handle);
    caps::apply_enabled_list(&handle);
    caps::emit_capabilities(&handle);
    caps::spawn_scheduler(handle.clone());
    notify::spawn_fullscreen_watcher(handle.clone());
    spawn_smoke_calls(&handle);
    if let Some(message) = startup_notice {
        set_notices(&handle, "startup", vec![message]);
    }
    spawn_config_watcher(handle.clone());

    // Say hello once so a first launch is not silent. After that Frosty stays
    // in the tray until summoned (principle 7, quiet by default).
    let first_run = {
        let state = handle.state::<AppState>();
        let db = state.db.lock().unwrap();
        let first = db.kv_get("first_run_done").ok().flatten().is_none();
        if first {
            let _ = db.kv_set("first_run_done", "1");
        }
        first
    };
    if first_run {
        summon(&handle);
    }
    Ok(())
}

/// Every event on the bus goes through the personality engine, and the
/// decision (played or dropped, with the reason) goes to both windows.
fn install_reactions(app: &AppHandle) {
    let handle = app.clone();
    let bus = app.state::<AppState>().bus.clone();
    bus.subscribe("*", move |event| {
        caps::forward_event(&handle, event);
        let state = handle.state::<AppState>();
        let now = state.now_ms();
        let decision = state.engine.lock().unwrap().handle(event, now);
        let dto = match decision {
            Decision::Play(r) => DecisionDto::Play {
                animation: r.animation,
                intensity: r.intensity,
                text: r.text,
                show_chip: r.show_chip,
                key: r.microcopy_key,
            },
            Decision::Dropped(reason) => DecisionDto::Dropped {
                reason: format!("{reason:?}"),
            },
        };
        let _ = handle.emit(
            "frosty://decision",
            DecisionPayload {
                event: event.name.clone(),
                decision: dto,
            },
        );
    });
}

/// Pushes the current config into the engine, hotkeys, and autostart, then
/// tells the windows. Called at startup and after every change.
pub fn apply_config(app: &AppHandle) {
    let state = app.state::<AppState>();
    let cfg = state.config.lock().unwrap().current().clone();
    {
        let mut engine = state.engine.lock().unwrap();
        engine.set_tone(cfg.personality.tone);
        engine.set_level(cfg.character.reactions);
        engine.set_never_react(cfg.personality.never_react.clone());
    }
    register_hotkeys(app, &cfg);

    let autostart = app.autolaunch();
    let enabled = autostart.is_enabled().unwrap_or(false);
    if cfg.core.start_on_login && !enabled {
        let _ = autostart.enable();
    } else if !cfg.core.start_on_login && enabled {
        let _ = autostart.disable();
    }

    let _ = app.emit("frosty://config", &cfg);
}

fn register_hotkeys(app: &AppHandle, cfg: &FrostyConfig) {
    let shortcuts = app.global_shortcut();
    let _ = shortcuts.unregister_all();
    let mut problems = Vec::new();
    let mut registered = Hotkeys::default();

    for (name, text) in [
        ("summon", &cfg.hotkeys.summon),
        ("fullscreen", &cfg.hotkeys.fullscreen),
    ] {
        match text.parse::<Shortcut>() {
            Ok(shortcut) => match shortcuts.register(shortcut) {
                Ok(()) => {
                    if name == "summon" {
                        registered.summon = Some(shortcut);
                    } else {
                        registered.fullscreen = Some(shortcut);
                    }
                }
                Err(e) => problems.push(format!(
                    "The {name} hotkey {text} could not be registered ({e}). Another app may already use it."
                )),
            },
            Err(e) => problems.push(format!("`{text}` is not a valid {name} hotkey ({e}).")),
        }
    }
    *app.state::<AppState>().hotkeys.lock().unwrap() = registered;
    set_notices(app, "hotkeys", problems);
}

pub fn on_shortcut(app: &AppHandle, shortcut: &Shortcut, event: ShortcutEvent) {
    if event.state() != ShortcutState::Pressed {
        return;
    }
    let state = app.state::<AppState>();
    let (is_summon, is_fullscreen) = {
        let hotkeys = state.hotkeys.lock().unwrap();
        (
            hotkeys.summon.as_ref() == Some(shortcut),
            hotkeys.fullscreen.as_ref() == Some(shortcut),
        )
    };
    if is_summon {
        toggle_summon(app);
    } else if is_fullscreen {
        toggle_fullscreen(app);
    }
}

fn spawn_config_watcher(app: AppHandle) {
    thread::spawn(move || {
        let path = app.state::<AppState>().home.config_file();
        let modified = |p: &std::path::Path| fs::metadata(p).and_then(|m| m.modified()).ok();
        let mut last = modified(&path);
        let cap_dir = app.state::<AppState>().home.capability_config_dir();
        let mut cap_files: std::collections::HashMap<std::path::PathBuf, std::time::SystemTime> =
            std::collections::HashMap::new();
        loop {
            thread::sleep(Duration::from_millis(500));

            // Hand edits to a capability's settings file reach its logic.
            if let Ok(entries) = fs::read_dir(&cap_dir) {
                for path in entries.filter_map(Result::ok).map(|e| e.path()) {
                    let Some(mtime) = modified(&path) else {
                        continue;
                    };
                    let known = cap_files.insert(path.clone(), mtime);
                    if known.is_some_and(|k| k != mtime)
                        && let Some(id) = path.file_stem().and_then(|s| s.to_str())
                    {
                        caps::notify_settings_changed(&app, id);
                    }
                }
            }

            let now = modified(&path);
            if now == last {
                continue;
            }
            last = now;
            // Editors write in several steps; let the file settle.
            thread::sleep(Duration::from_millis(100));
            let result = app.state::<AppState>().config.lock().unwrap().reload();
            match result {
                Ok(changed) => {
                    set_notices(&app, "config", Vec::new());
                    if changed {
                        apply_config(&app);
                    }
                }
                Err(e) => set_notices(
                    &app,
                    "config",
                    vec![format!(
                        "frosty.toml has an error, so the last good settings are still in use: {e}"
                    )],
                ),
            }
        }
    });
}

pub fn summon(app: &AppHandle) {
    if windows::is_visible(app, MAIN) {
        if let Some(main) = app.get_webview_window(MAIN) {
            let _ = main.set_focus();
        }
        return;
    }
    let Some(overlay) = app.get_webview_window(OVERLAY) else {
        return;
    };
    if overlay.is_visible().unwrap_or(false) {
        // Already up: focus the dock instead of replaying the animation.
        let _ = app.emit_to(OVERLAY, "frosty://focus-dock", ());
        return;
    }
    *app.state::<AppState>().summon_started.lock().unwrap() = Some(Instant::now());
    crate::fullscreen::remember_foreground(&app.state::<AppState>());
    let _ = windows::place_overlay(app, &overlay);
    let _ = overlay.show();
    let _ = app.emit_to(OVERLAY, "frosty://summon", ());
    publish(app, "core.summoned", json!({}), None);

    let today = (unix_millis() / 86_400_000).to_string();
    let first_today = {
        let state = app.state::<AppState>();
        let db = state.db.lock().unwrap();
        let seen = db.kv_get("last_summon_day").ok().flatten();
        let first = seen.as_deref() != Some(today.as_str());
        if first {
            let _ = db.kv_set("last_summon_day", &today);
        }
        first
    };
    if first_today {
        publish(app, "core.first_summon_today", json!({}), None);
    }
}

/// Hotkey, tray click, and Esc all end up here. The overlay plays its exit
/// animation and then calls the `overlay_hidden` command.
pub fn toggle_summon(app: &AppHandle) {
    if windows::is_visible(app, OVERLAY) {
        let _ = app.emit_to(OVERLAY, "frosty://dismiss-request", ());
    } else {
        summon(app);
    }
}

pub fn overlay_hidden(app: &AppHandle) {
    if let Some(overlay) = app.get_webview_window(OVERLAY) {
        let _ = overlay.hide();
    }
    publish(app, "core.dismissed", json!({}), None);
}

pub fn toggle_fullscreen(app: &AppHandle) {
    if windows::is_visible(app, MAIN) {
        if let Some(main) = app.get_webview_window(MAIN) {
            let _ = main.hide();
        }
        publish(app, "core.fullscreen_exited", json!({}), None);
    } else {
        open_main(app, None);
    }
}

pub fn open_main(app: &AppHandle, page: Option<&str>) {
    if let Some(overlay) = app.get_webview_window(OVERLAY) {
        let _ = overlay.hide();
    }
    let Some(main) = app.get_webview_window(MAIN) else {
        return;
    };
    let was_visible = main.is_visible().unwrap_or(false);
    let _ = main.show();
    let _ = main.unminimize();
    if !was_visible {
        let _ = main.maximize();
        publish(app, "core.fullscreen_entered", json!({}), None);
    }
    let _ = main.set_focus();
    if let Some(page) = page {
        let _ = app.emit_to(MAIN, "frosty://navigate", page);
    }
}

fn build_tray(app: &AppHandle) -> tauri::Result<()> {
    let summon_item = MenuItem::with_id(app, "summon", "Summon Frosty", true, None::<&str>)?;
    let fullscreen_item =
        MenuItem::with_id(app, "fullscreen", "Open fullscreen", true, None::<&str>)?;
    let settings_item = MenuItem::with_id(app, "settings", "Settings", true, None::<&str>)?;
    let quit_item = MenuItem::with_id(app, "quit", "Quit", true, None::<&str>)?;
    let dnd_on = app
        .state::<AppState>()
        .dnd_manual
        .load(std::sync::atomic::Ordering::SeqCst);
    let dnd_item = MenuItem::with_id(
        app,
        "dnd",
        if dnd_on {
            "Do not disturb: on"
        } else {
            "Do not disturb: off"
        },
        true,
        None::<&str>,
    )?;
    *app.state::<AppState>().tray_dnd.lock().unwrap() = Some(dnd_item.clone());
    let menu = Menu::with_items(
        app,
        &[
            &summon_item,
            &fullscreen_item,
            &dnd_item,
            &settings_item,
            &quit_item,
        ],
    )?;

    let mut builder = TrayIconBuilder::with_id("frosty")
        .tooltip("Frosty")
        .menu(&menu)
        .show_menu_on_left_click(false)
        .on_menu_event(|app, event| match event.id.as_ref() {
            "summon" => summon(app),
            "fullscreen" => open_main(app, None),
            "settings" => open_main(app, Some("settings")),
            "dnd" => {
                let on = !app
                    .state::<AppState>()
                    .dnd_manual
                    .load(std::sync::atomic::Ordering::SeqCst);
                notify::set_dnd(app, on);
            }
            "quit" => {
                publish(app, "core.quit", json!({}), None);
                app.exit(0);
            }
            _ => {}
        })
        .on_tray_icon_event(|tray, event| {
            if let TrayIconEvent::Click {
                button: MouseButton::Left,
                button_state: MouseButtonState::Up,
                ..
            } = event
            {
                toggle_summon(tray.app_handle());
            }
        });
    if let Some(icon) = app.default_window_icon() {
        builder = builder.icon(icon.clone());
    }
    builder.build(app)?;
    Ok(())
}

/// Debug builds only: `FROSTY_SMOKE_CALLS` names a JSON file of tool calls to
/// run once the capabilities are up, with results in `logs/smoke.log`. It lets
/// a script drive the real app end to end without clicking.
///
///   [{"tool": "tasks.add", "input": {"title": "x"}}, {"wait_ms": 2000}]
fn spawn_smoke_calls(app: &AppHandle) {
    if !cfg!(debug_assertions) {
        return;
    }
    let Some(path) = std::env::var_os("FROSTY_SMOKE_CALLS") else {
        return;
    };
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        use std::io::Write;
        let state = app.state::<AppState>();
        let log_path = state.home.logs_dir().join("smoke.log");
        let log = |line: String| {
            let _ = fs::create_dir_all(state.home.logs_dir());
            if let Ok(mut f) = fs::OpenOptions::new()
                .create(true)
                .append(true)
                .open(&log_path)
            {
                let _ = writeln!(f, "{line}");
            }
        };
        let Ok(text) = fs::read_to_string(&path) else {
            log("cannot read the smoke calls file".into());
            return;
        };
        let Ok(calls) = serde_json::from_str::<Vec<serde_json::Value>>(&text) else {
            log("the smoke calls file is not a JSON list".into());
            return;
        };

        // Wait for every enabled capability to finish starting.
        for _ in 0..60 {
            let starting = state
                .registry
                .lock()
                .unwrap()
                .iter()
                .any(|e| e.state == frosty_host::registry::State::Enabling);
            if !starting {
                break;
            }
            tokio::time::sleep(Duration::from_millis(250)).await;
        }
        log("ready".into());

        for call in calls {
            if let Some(ms) = call.get("wait_ms").and_then(|v| v.as_u64()) {
                tokio::time::sleep(Duration::from_millis(ms)).await;
                continue;
            }
            let Some(tool) = call.get("tool").and_then(|v| v.as_str()) else {
                continue;
            };
            let input = call.get("input").cloned().unwrap_or_else(|| json!({}));
            match caps::dispatch_tool(&app, None, tool, input).await {
                Ok(v) => log(format!("ok {tool} {v}")),
                Err(e) => log(format!("err {tool} {e}")),
            }
        }
        log("done".into());
    });
}
