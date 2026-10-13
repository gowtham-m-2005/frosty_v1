use std::{
    sync::{Arc, Mutex},
    time::Instant,
};

use frosty_core::{
    config::ConfigStore, db::Database, events::EventBus, home::FrostyHome,
    personality::PersonalityEngine, theme::Theme,
};
use serde::Serialize;
use tauri_plugin_global_shortcut::Shortcut;

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
}

impl AppState {
    pub fn now_ms(&self) -> u64 {
        self.started.elapsed().as_millis() as u64
    }
}
