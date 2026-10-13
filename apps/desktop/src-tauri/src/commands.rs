//! Commands the frontend calls. Thin: they validate input and delegate.

use std::collections::BTreeMap;

use frosty_core::{config::FrostyConfig, theme::Mode};
use serde::Serialize;
use tauri::{AppHandle, State, WebviewWindow};

use crate::{
    shell,
    state::{AppState, Notice},
    windows::{self, HitRect},
};

type CmdResult<T> = Result<T, String>;

#[tauri::command]
pub fn get_config(state: State<'_, AppState>) -> FrostyConfig {
    state.config.lock().unwrap().current().clone()
}

#[tauri::command]
pub fn set_config_value(
    app: AppHandle,
    state: State<'_, AppState>,
    key: String,
    value: serde_json::Value,
) -> CmdResult<()> {
    state
        .config
        .lock()
        .unwrap()
        .set_json(&key, &value)
        .map_err(|e| e.to_string())?;
    shell::apply_config(&app);
    shell::publish(
        &app,
        "core.setting_saved",
        serde_json::json!({ "key": key }),
        None,
    );
    Ok(())
}

#[tauri::command]
pub fn fire_event(
    app: AppHandle,
    name: String,
    payload: Option<serde_json::Value>,
    significance: Option<f32>,
) -> CmdResult<()> {
    if !frosty_core::events::is_valid_event_name(&name) {
        return Err(format!(
            "`{name}` is not a valid event name (use `namespace.event`)"
        ));
    }
    shell::publish(
        &app,
        &name,
        payload.unwrap_or(serde_json::json!({})),
        significance,
    );
    Ok(())
}

#[tauri::command]
pub fn report_hit_rects(window: WebviewWindow, rects: Vec<HitRect>) {
    windows::set_hit_region(&window, &rects);
}

#[tauri::command]
pub fn overlay_hidden(app: AppHandle) {
    shell::overlay_hidden(&app);
}

/// Esc and the dismiss affordance ask for a dismissal; the overlay plays its
/// exit animation and then reports back with `overlay_hidden`.
#[tauri::command]
pub fn toggle_summon(app: AppHandle) {
    shell::toggle_summon(&app);
}

#[tauri::command]
pub fn toggle_fullscreen(app: AppHandle) {
    shell::toggle_fullscreen(&app);
}

#[tauri::command]
pub fn open_main(app: AppHandle, page: Option<String>) {
    shell::open_main(&app, page.as_deref());
}

#[derive(Serialize)]
pub struct ThemeInfo {
    pub id: String,
    pub name: String,
    pub modes: Vec<&'static str>,
}

#[tauri::command]
pub fn list_themes(state: State<'_, AppState>) -> Vec<ThemeInfo> {
    state
        .themes
        .lock()
        .unwrap()
        .iter()
        .map(|t| {
            let mut modes = Vec::new();
            if t.light.is_some() {
                modes.push("light");
            }
            if t.dark.is_some() {
                modes.push("dark");
            }
            ThemeInfo {
                id: t.id.clone(),
                name: t.name.clone(),
                modes,
            }
        })
        .collect()
}

#[derive(Serialize)]
pub struct ResolvedTheme {
    pub id: String,
    pub name: String,
    pub mode: &'static str,
    /// True when the theme does not support the requested mode.
    pub fallback: bool,
    pub tokens: BTreeMap<String, String>,
    pub options: BTreeMap<String, String>,
}

#[tauri::command]
pub fn resolve_theme(state: State<'_, AppState>, system_dark: bool) -> CmdResult<ResolvedTheme> {
    let cfg = state.config.lock().unwrap().current().clone();
    let themes = state.themes.lock().unwrap();
    // An unknown theme id falls back to Frost rather than failing.
    let theme = themes
        .iter()
        .find(|t| t.id == cfg.appearance.theme)
        .or_else(|| themes.iter().find(|t| t.id == "frost"))
        .ok_or("no themes available")?;
    let (mode, fallback) = theme.resolve_mode(cfg.appearance.mode, system_dark);
    let tokens = theme.tokens(mode).cloned().ok_or("theme has no tokens")?;
    Ok(ResolvedTheme {
        id: theme.id.clone(),
        name: theme.name.clone(),
        mode: if mode == Mode::Dark { "dark" } else { "light" },
        fallback,
        tokens,
        options: theme.options.clone(),
    })
}

#[tauri::command]
pub fn get_notices(state: State<'_, AppState>) -> Vec<Notice> {
    state.notices.lock().unwrap().clone()
}

#[tauri::command]
pub fn get_fox_position(state: State<'_, AppState>) -> Option<String> {
    state
        .db
        .lock()
        .unwrap()
        .kv_get("fox.position")
        .ok()
        .flatten()
}

#[tauri::command]
pub fn save_fox_position(state: State<'_, AppState>, position: String) -> CmdResult<()> {
    state
        .db
        .lock()
        .unwrap()
        .kv_set("fox.position", &position)
        .map_err(|e| e.to_string())
}

#[derive(Serialize)]
pub struct Paths {
    pub home: String,
    pub config_file: String,
    pub themes: String,
}

#[tauri::command]
pub fn get_paths(state: State<'_, AppState>) -> Paths {
    Paths {
        home: state.home.root().display().to_string(),
        config_file: state.home.config_file().display().to_string(),
        themes: state.home.themes_dir().display().to_string(),
    }
}

/// `which` is `home`, `config`, or `themes`.
#[tauri::command]
pub fn reveal(state: State<'_, AppState>, which: String) -> CmdResult<()> {
    let path = match which.as_str() {
        "home" => state.home.root().to_path_buf(),
        "config" => state.home.config_file(),
        "themes" => state.home.themes_dir(),
        other => return Err(format!("unknown location `{other}`")),
    };
    #[cfg(windows)]
    {
        let mut cmd = std::process::Command::new("explorer.exe");
        if which == "config" {
            // Select the file in its folder.
            cmd.arg(format!("/select,{}", path.display()));
        } else {
            cmd.arg(&path);
        }
        cmd.spawn().map_err(|e| e.to_string())?;
    }
    #[cfg(not(windows))]
    let _ = path;
    Ok(())
}
