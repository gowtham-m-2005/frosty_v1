use std::{
    collections::HashMap,
    path::PathBuf,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, AtomicU64},
    },
    time::Instant,
};

use frosty_core::{
    config::ConfigStore, db::Database, events::EventBus, home::FrostyHome,
    notifications::RateLimiter, personality::PersonalityEngine, theme::Theme,
};
use frosty_host::registry::Registry;
use serde::Serialize;
use tauri_plugin_global_shortcut::Shortcut;
use tokio::sync::oneshot;

#[derive(Default)]
pub struct Hotkeys {
    pub summon: Option<Shortcut>,
    pub fullscreen: Option<Shortcut>,
}

/// A message the user should see: a rejected hotkey, a broken config file.
#[derive(Debug, Clone, Serialize)]
pub struct Notice {
    /// Which part of the app owns this notice, so it can be replaced later.
    pub source: String,
    /// `error` or `info`.
    pub level: String,
    pub message: String,
}

pub struct AppState {
    pub home: FrostyHome,
    pub config: Mutex<ConfigStore>,
    pub db: Mutex<Database>,
    pub engine: Mutex<PersonalityEngine>,
    pub bus: Arc<EventBus>,
    pub themes: Mutex<Vec<Theme>>,
    pub hotkeys: Mutex<Hotkeys>,
    pub notices: Mutex<Vec<Notice>>,
    pub started: Instant,
    /// Set when a summon begins; read by `summon_painted` to measure latency.
    pub summon_started: Mutex<Option<Instant>>,
    pub registry: Mutex<Registry>,
    /// Tool calls waiting for the logic host's answer, by call id.
    pub pending_calls: Mutex<HashMap<u64, oneshot::Sender<serde_json::Value>>>,
    pub next_call: AtomicU64,
    pub badges: Mutex<HashMap<String, i64>>,
    pub builtin_root: Option<PathBuf>,
    pub device_id: String,
    /// A fullscreen app is in front (reminders wait).
    pub fullscreen_app: AtomicBool,
    /// Do not disturb switched on by hand, from the tray or settings.
    pub dnd_manual: AtomicBool,
    pub rate: Mutex<RateLimiter>,
    /// The tray's do-not-disturb entry, kept so its text can follow the state.
    pub tray_dnd: Mutex<Option<tauri::menu::MenuItem<tauri::Wry>>>,
}

impl AppState {
    pub fn now_ms(&self) -> u64 {
        self.started.elapsed().as_millis() as u64
    }
}
