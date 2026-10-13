//! Capability runtime: the commands the logic host and the views call, the
//! custom URI scheme that serves capability files, and the scheduler runner.
//! Every call carries a capability id and is checked against the manifest and
//! the grants in `frosty.toml` here, in Rust (docs/PROJECT_SPEC.md 8.6).

use std::{
    fs,
    path::{Component, Path, PathBuf},
    sync::atomic::Ordering,
    time::Duration,
};

use frosty_core::{
    events::Event,
    scheduler::{self, MissedPolicy},
};
use frosty_host::{
    capconfig,
    discovery::discover,
    manifest::{Origin, Permissions},
    permissions as perms,
    registry::{Registry, State as CapState},
    schema,
    store::{CapStore, Query},
};
use serde::Serialize;
use serde_json::{Map, Value, json};
use tauri::{
    AppHandle, Emitter, Manager, State,
    http::{Response, StatusCode},
};
use tauri_plugin_dialog::DialogExt;
use tokio::sync::oneshot;

use crate::{
    shell,
    state::AppState,
    windows::{LOGIC, MAIN, OVERLAY},
};

type CmdResult<T> = Result<T, String>;

const TOOL_TIMEOUT: Duration = Duration::from_secs(10);
/// Only these folders of a capability are ever served to a webview.
const SERVED_DIRS: [&str; 2] = ["dist", "assets"];

// ---------------------------------------------------------------- startup

/// Where built-in capabilities live: an explicit override, the bundled
/// resources, or the repo's `capabilities/` folder when running from source.
pub fn find_builtin_root(app: &AppHandle) -> Option<PathBuf> {
    if let Some(dir) = std::env::var_os("FROSTY_BUILTIN_DIR") {
        return Some(PathBuf::from(dir));
    }
    if let Ok(resources) = app.path().resource_dir() {
        let bundled = resources.join("capabilities");
        if bundled.is_dir() {
            return Some(bundled);
        }
    }
    let exe = std::env::current_exe().ok()?;
    exe.ancestors().map(|a| a.join("capabilities")).find(|c| {
        c.is_dir()
            && c.parent()
                .is_some_and(|p| p.join("pnpm-workspace.yaml").is_file())
    })
}

pub fn initial_registry(builtin: Option<&Path>, user_root: &Path) -> Registry {
    Registry::from_discovery(discover(builtin, user_root))
}

fn granted(state: &AppState, id: &str) -> Permissions {
    let cfg = state.config.lock().unwrap();
    perms::from_table(cfg.current().capabilities.permissions.get(id))
}

/// Applies the enabled list from `frosty.toml`: runs migrations and marks
/// each capability as waiting for the logic host to activate it. A capability
/// that now asks for more than was granted stays off until the user agrees.
pub fn apply_enabled_list(app: &AppHandle) {
    let state = app.state::<AppState>();
    let enabled = state
        .config
        .lock()
        .unwrap()
        .current()
        .capabilities
        .enabled
        .clone();
    for id in enabled {
        let Some((requested, migrations, current)) = ({
            let reg = state.registry.lock().unwrap();
            reg.get(&id).map(|e| {
                (
                    e.loaded.manifest.permissions.clone(),
                    e.loaded.migrations.clone(),
                    e.state,
                )
            })
        }) else {
            continue;
        };
        if matches!(current, CapState::Enabling | CapState::Enabled) {
            continue;
        }
        if !perms::covers(&granted(&state, &id), &requested) {
            let _ = state.registry.lock().unwrap().mark_error(
                &id,
                "This module now asks for permissions you have not granted. Enable it again to review them.",
            );
            continue;
        }
        let migrated = state
            .db
            .lock()
            .unwrap()
            .run_capability_migrations(&id, &migrations);
        let mut reg = state.registry.lock().unwrap();
        match migrated {
            Ok(_) => {
                let _ = reg.begin_enable(&id);
            }
            Err(e) => {
                let _ = reg.mark_error(&id, e.to_string());
            }
        }
    }
}

// ----------------------------------------------------------------- listing

#[derive(Serialize, Clone)]
pub struct ToolInfo {
    pub id: String,
    pub title: String,
    pub quick_action: bool,
    pub quick_action_control: Option<String>,
}

#[derive(Serialize, Clone)]
pub struct CapabilityInfo {
    pub id: String,
    pub name: String,
    pub version: String,
    pub description: String,
    pub runtime: String,
    pub origin: Origin,
    pub state: CapState,
    pub error: Option<String>,
    pub error_count: u32,
    pub tools: Vec<ToolInfo>,
    pub permissions: Vec<perms::PermissionLine>,
    pub has_settings: bool,
    pub entry_url: String,
    pub views_url: Option<String>,
    pub icon_url: Option<String>,
    pub badge: i64,
}

#[derive(Serialize, Clone)]
pub struct RejectedInfo {
    pub dir: String,
    pub errors: Vec<String>,
}

#[derive(Serialize, Clone)]
pub struct CapabilityList {
    pub capabilities: Vec<CapabilityInfo>,
    pub rejected: Vec<RejectedInfo>,
}

pub fn cap_url(id: &str, rel: &str) -> String {
    let rel = rel.trim_start_matches('/');
    if cfg!(windows) {
        format!("http://frosty-cap.localhost/{id}/{rel}")
    } else {
        format!("frosty-cap://localhost/{id}/{rel}")
    }
}

pub fn build_list(state: &AppState) -> CapabilityList {
    let reg = state.registry.lock().unwrap();
    let badges = state.badges.lock().unwrap();
    let capabilities = reg
        .iter()
        .map(|e| {
            let m = &e.loaded.manifest;
            let c = &m.capability;
            CapabilityInfo {
                id: c.id.clone(),
                name: c.name.clone(),
                version: c.version.clone(),
                description: c.description.clone(),
                runtime: c.runtime.clone(),
                origin: e.loaded.origin,
                state: e.state,
                error: e.error.clone(),
                error_count: e.error_count,
                tools: m
                    .tools
                    .iter()
                    .map(|t| ToolInfo {
                        id: t.id.clone(),
                        title: t.title.clone(),
                        quick_action: t.quick_action,
                        quick_action_control: t.quick_action_control.clone(),
                    })
                    .collect(),
                permissions: perms::describe(&m.permissions),
                has_settings: e.loaded.settings_schema.is_some(),
                entry_url: cap_url(&c.id, &c.entry),
                views_url: c.views.as_ref().map(|v| cap_url(&c.id, v)),
                icon_url: c.icon.as_ref().map(|i| cap_url(&c.id, i)),
                badge: badges.get(&c.id).copied().unwrap_or(0),
            }
        })
        .collect();
    let rejected = reg
        .rejected()
        .iter()
        .map(|r| RejectedInfo {
            dir: r.dir.display().to_string(),
            errors: r.errors.clone(),
        })
        .collect();
    CapabilityList {
        capabilities,
        rejected,
    }
}

/// Capability reactions and microcopy packs flow into the personality
/// engine whenever the set of running capabilities changes.
fn sync_personality(state: &AppState) {
    use frosty_core::personality::{Microcopy, ReactionRule};
    let locale = state
        .config
        .lock()
        .unwrap()
        .current()
        .personality
        .locale
        .clone();
    let mut rules = Vec::new();
    let mut packs = Vec::new();
    {
        let reg = state.registry.lock().unwrap();
        for entry in reg
            .iter()
            .filter(|e| matches!(e.state, CapState::Enabling | CapState::Enabled))
        {
            for r in &entry.loaded.manifest.reactions {
                if let Some(intensity) = r.intensity.value() {
                    rules.push(ReactionRule {
                        on: r.on.clone(),
                        animation: r.animation.clone(),
                        intensity,
                        microcopy: r.microcopy.clone(),
                        chip_in_subtle: false,
                    });
                }
            }
            let pack = entry
                .loaded
                .dir
                .join("microcopy")
                .join(format!("{locale}.json"));
            if let Ok(text) = fs::read_to_string(pack)
                && let Ok(pack) = Microcopy::from_json(&text)
            {
                packs.push(pack);
            }
        }
    }
    let mut engine = state.engine.lock().unwrap();
    engine.set_manifest_rules(rules);
    for pack in packs {
        engine.merge_microcopy(pack);
    }
}

pub fn emit_capabilities(app: &AppHandle) {
    sync_personality(&app.state::<AppState>());
    let list = build_list(&app.state::<AppState>());
    let _ = app.emit("frosty://capabilities", list);
}

#[tauri::command]
pub fn list_capabilities(state: State<'_, AppState>) -> CapabilityList {
    build_list(&state)
}

// ------------------------------------------------------------- lifecycle

fn set_enabled_in_config(state: &AppState, id: &str, on: bool) -> CmdResult<()> {
    let mut cfg = state.config.lock().unwrap();
    let mut enabled = cfg.current().capabilities.enabled.clone();
    enabled.retain(|e| e != id);
    if on {
        enabled.push(id.to_string());
    }
    cfg.set_json("capabilities.enabled", &json!(enabled))
        .map_err(|e| e.to_string())?;
    if on {
        let mut order = cfg.current().capabilities.dock_order.clone();
        if !order.iter().any(|o| o == id) {
            order.push(id.to_string());
            cfg.set_json("capabilities.dock_order", &json!(order))
                .map_err(|e| e.to_string())?;
        }
    }
    Ok(())
}

/// The user has seen the permission sheet. Grants everything the manifest
/// asks for, migrates, and asks the logic host to activate it.
#[tauri::command]
pub fn enable_capability(
    app: AppHandle,
    state: State<'_, AppState>,
    id: String,
    confirmed_shell: Option<bool>,
) -> CmdResult<()> {
    let (requested, migrations) = {
        let reg = state.registry.lock().unwrap();
        let entry = reg.get(&id).ok_or_else(|| format!("no module `{id}`"))?;
        (
            entry.loaded.manifest.permissions.clone(),
            entry.loaded.migrations.clone(),
        )
    };
    if requested.shell && confirmed_shell != Some(true) {
        return Err(
            "This module can run programs on your computer. That needs a second confirmation."
                .into(),
        );
    }

    {
        let mut cfg = state.config.lock().unwrap();
        let grant = serde_json::to_value(&requested).map_err(|e| e.to_string())?;
        cfg.set_json(&format!("capabilities.permissions.{id}"), &grant)
            .map_err(|e| e.to_string())?;
    }

    let migrated = state
        .db
        .lock()
        .unwrap()
        .run_capability_migrations(&id, &migrations);
    if let Err(e) = migrated {
        let message = e.to_string();
        let _ = state.registry.lock().unwrap().mark_error(&id, &message);
        emit_capabilities(&app);
        return Err(message);
    }

    set_enabled_in_config(&state, &id, true)?;
    state
        .registry
        .lock()
        .unwrap()
        .begin_enable(&id)
        .map_err(|e| e.to_string())?;
    let _ = app.emit_to(LOGIC, "frosty://capability-enable", json!({ "id": id }));
    emit_capabilities(&app);
    Ok(())
}

#[tauri::command]
pub fn disable_capability(app: AppHandle, state: State<'_, AppState>, id: String) -> CmdResult<()> {
    state
        .registry
        .lock()
        .unwrap()
        .mark_disabled(&id)
        .map_err(|e| e.to_string())?;
    set_enabled_in_config(&state, &id, false)?;
    let _ = scheduler::cancel_owner(&state.db.lock().unwrap(), &id);
    state.badges.lock().unwrap().remove(&id);
    let _ = app.emit_to(LOGIC, "frosty://capability-disable", json!({ "id": id }));
    emit_capabilities(&app);
    Ok(())
}

/// Reported by the logic host after `onActivate` ran (or threw).
#[tauri::command]
pub fn capability_activated(
    app: AppHandle,
    state: State<'_, AppState>,
    id: String,
    ok: bool,
    error: Option<String>,
) {
    {
        let mut reg = state.registry.lock().unwrap();
        if ok {
            let _ = reg.mark_enabled(&id);
        } else {
            let _ = reg.mark_error(&id, error.unwrap_or_else(|| "activation failed".into()));
        }
    }
    if !ok {
        let _ = scheduler::cancel_owner(&state.db.lock().unwrap(), &id);
        shell::publish(&app, "core.error", json!({ "capability": id }), None);
    }
    emit_capabilities(&app);
}

/// A view crashed. The module goes to `error` and stops receiving calls.
#[tauri::command]
pub fn capability_failed(app: AppHandle, state: State<'_, AppState>, id: String, message: String) {
    let _ = state.registry.lock().unwrap().mark_error(&id, message);
    let _ = app.emit_to(LOGIC, "frosty://capability-disable", json!({ "id": id }));
    shell::publish(&app, "core.error", json!({ "capability": id }), None);
    emit_capabilities(&app);
}

#[tauri::command]
pub fn rescan_capabilities(app: AppHandle, state: State<'_, AppState>) {
    let d = discover(
        state.builtin_root.as_deref(),
        &state.home.capabilities_dir(),
    );
    state.registry.lock().unwrap().refresh(d);
    apply_enabled_list(&app);
    emit_capabilities(&app);
}

#[tauri::command]
pub fn remove_capability(
    app: AppHandle,
    state: State<'_, AppState>,
    id: String,
    delete_data: bool,
) -> CmdResult<()> {
    let dir = {
        let reg = state.registry.lock().unwrap();
        let entry = reg.get(&id).ok_or_else(|| format!("no module `{id}`"))?;
        if entry.loaded.origin == Origin::BuiltIn {
            return Err("Built-in modules can be turned off but not removed.".into());
        }
        if matches!(entry.state, CapState::Enabled | CapState::Enabling) {
            return Err("Turn the module off before removing it.".into());
        }
        entry.loaded.dir.clone()
    };
    // Only ever delete inside the user's capabilities folder.
    let root = state.home.capabilities_dir();
    if !dir.starts_with(&root) {
        return Err("That module is not in the capabilities folder.".into());
    }
    fs::remove_dir_all(&dir).map_err(|e| format!("could not delete the folder: {e}"))?;
    if delete_data {
        state
            .db
            .lock()
            .unwrap()
            .drop_capability_data(&id)
            .map_err(|e| e.to_string())?;
    }
    state.registry.lock().unwrap().forget(&id);
    emit_capabilities(&app);
    Ok(())
}

fn copy_dir(from: &Path, to: &Path) -> std::io::Result<()> {
    fs::create_dir_all(to)?;
    for entry in fs::read_dir(from)? {
        let entry = entry?;
        let kind = entry.file_type()?;
        let target = to.join(entry.file_name());
        if kind.is_dir() {
            if entry.file_name() == "node_modules" {
                continue;
            }
            copy_dir(&entry.path(), &target)?;
        } else if kind.is_file() {
            fs::copy(entry.path(), target)?;
        }
    }
    Ok(())
}

/// "Install from folder": native picker, copy, validate, and roll back if the
/// folder is not a valid capability. Returns the new id, or `None` if the
/// picker was cancelled.
#[tauri::command]
pub async fn install_capability_from_folder(
    app: AppHandle,
    state: State<'_, AppState>,
) -> CmdResult<Option<String>> {
    let picked = app.dialog().file().blocking_pick_folder();
    let Some(picked) = picked else {
        return Ok(None);
    };
    let source = picked.into_path().map_err(|e| e.to_string())?;
    let name = source
        .file_name()
        .ok_or("that folder has no name")?
        .to_string_lossy()
        .to_string();
    let target = state.home.capabilities_dir().join(&name);
    if target.exists() {
        return Err(format!(
            "A module folder called `{name}` is already installed."
        ));
    }
    copy_dir(&source, &target).map_err(|e| format!("could not copy the folder: {e}"))?;

    let d = discover(None, &state.home.capabilities_dir());
    let rejection = d
        .rejected
        .iter()
        .find(|r| r.dir == target)
        .map(|r| r.errors.join("\n"));
    if let Some(errors) = rejection {
        let _ = fs::remove_dir_all(&target);
        return Err(errors);
    }
    let id = d
        .loaded
        .iter()
        .find(|c| c.dir == target)
        .map(|c| c.id().to_string());

    let full = discover(
        state.builtin_root.as_deref(),
        &state.home.capabilities_dir(),
    );
    state.registry.lock().unwrap().refresh(full);
    emit_capabilities(&app);
    Ok(id)
}

#[tauri::command]
pub fn capability_reveal(state: State<'_, AppState>, id: String, which: String) -> CmdResult<()> {
    let path = match which.as_str() {
        "folder" => {
            let reg = state.registry.lock().unwrap();
            reg.get(&id)
                .ok_or_else(|| format!("no module `{id}`"))?
                .loaded
                .dir
                .clone()
        }
        "config" => state
            .home
            .capability_config_dir()
            .join(format!("{id}.toml")),
        other => return Err(format!("unknown location `{other}`")),
    };
    #[cfg(windows)]
    {
        let mut cmd = std::process::Command::new("explorer.exe");
        if which == "config" && path.exists() {
            cmd.arg(format!("/select,{}", path.display()));
        } else if which == "config" {
            cmd.arg(state.home.capability_config_dir());
        } else {
            cmd.arg(&path);
        }
        cmd.spawn().map_err(|e| e.to_string())?;
    }
    #[cfg(not(windows))]
    let _ = path;
    Ok(())
}

// --------------------------------------------------------------- settings

fn settings_path(state: &AppState, id: &str) -> PathBuf {
    state
        .home
        .capability_config_dir()
        .join(format!("{id}.toml"))
}

fn schema_of(state: &AppState, id: &str) -> CmdResult<Option<Value>> {
    let reg = state.registry.lock().unwrap();
    Ok(reg
        .get(id)
        .ok_or_else(|| format!("no module `{id}`"))?
        .loaded
        .settings_schema
        .clone())
}

#[tauri::command]
pub fn get_capability_schema(state: State<'_, AppState>, id: String) -> CmdResult<Option<Value>> {
    schema_of(&state, &id)
}

#[tauri::command]
pub fn get_capability_settings(state: State<'_, AppState>, id: String) -> CmdResult<Value> {
    let values = capconfig::read(&settings_path(&state, &id)).map_err(|e| e.to_string())?;
    let values = match schema_of(&state, &id)? {
        Some(schema) => schema::apply_defaults(&schema, &values),
        None => values,
    };
    Ok(Value::Object(values))
}

#[tauri::command]
pub fn set_capability_settings(
    app: AppHandle,
    state: State<'_, AppState>,
    id: String,
    values: Map<String, Value>,
) -> CmdResult<()> {
    if let Some(s) = schema_of(&state, &id)? {
        let merged = {
            let current =
                capconfig::read(&settings_path(&state, &id)).map_err(|e| e.to_string())?;
            let mut all = schema::apply_defaults(&s, &current);
            all.extend(values.clone());
            all
        };
        schema::validate(&s, &Value::Object(merged)).map_err(|errors| errors.join("; "))?;
    }
    capconfig::write(&settings_path(&state, &id), &values).map_err(|e| e.to_string())?;
    notify_settings_changed(&app, &id);
    Ok(())
}

pub fn notify_settings_changed(app: &AppHandle, id: &str) {
    let state = app.state::<AppState>();
    let values =
        get_capability_settings(app.state::<AppState>(), id.to_string()).unwrap_or(Value::Null);
    let _ = state;
    let _ = app.emit_to(
        LOGIC,
        "frosty://capability-config",
        json!({ "id": id, "values": values }),
    );
}

// ------------------------------------------------------------------ tools

/// View or capability -> core -> logic host -> core -> caller.
pub async fn dispatch_tool(
    app: &AppHandle,
    caller: Option<&str>,
    tool: &str,
    input: Value,
) -> Result<Value, String> {
    let state = app.state::<AppState>();
    let (owner, input_schema) = {
        let reg = state.registry.lock().unwrap();
        let entry = reg
            .tool_owner(tool)
            .ok_or_else(|| format!("no enabled module provides `{tool}`"))?;
        (
            entry.loaded.id().to_string(),
            entry
                .loaded
                .tool_schemas
                .get(tool)
                .and_then(|s| s.input.clone()),
        )
    };
    if let Some(caller) = caller
        && caller != owner
    {
        let grants = granted(&state, caller);
        if !perms::may_call_tool(&grants, caller, tool) {
            return Err(format!(
                "`{caller}` is not allowed to use `{tool}` (it needs the tools_call permission)"
            ));
        }
    }
    if let Some(s) = input_schema {
        schema::validate(&s, &input)
            .map_err(|errors| format!("invalid input for `{tool}`: {}", errors.join("; ")))?;
    }

    let id = state.next_call.fetch_add(1, Ordering::SeqCst);
    let (tx, rx) = oneshot::channel();
    state.pending_calls.lock().unwrap().insert(id, tx);
    app.emit_to(
        LOGIC,
        "frosty://tool-call",
        json!({ "id": id, "capability": owner, "tool": tool, "input": input }),
    )
    .map_err(|e| e.to_string())?;

    let reply = match tokio::time::timeout(TOOL_TIMEOUT, rx).await {
        Ok(Ok(v)) => v,
        Ok(Err(_)) => return Err("the module did not answer".into()),
        Err(_) => {
            state.pending_calls.lock().unwrap().remove(&id);
            return Err(format!("`{tool}` timed out"));
        }
    };
    if reply["ok"] == true {
        Ok(reply["value"].clone())
    } else {
        Err(reply["error"]
            .as_str()
            .unwrap_or("the tool failed")
            .to_string())
    }
}

#[tauri::command]
pub async fn call_tool(
    app: AppHandle,
    caller: Option<String>,
    tool: String,
    input: Option<Value>,
) -> CmdResult<Value> {
    dispatch_tool(
        &app,
        caller.as_deref(),
        &tool,
        input.unwrap_or_else(|| json!({})),
    )
    .await
}

#[tauri::command]
pub fn tool_result(state: State<'_, AppState>, id: u64, result: Value) {
    if let Some(tx) = state.pending_calls.lock().unwrap().remove(&id) {
        let _ = tx.send(result);
    }
}

// ------------------------------------------------------------ ctx surface

fn require(
    state: &AppState,
    id: &str,
    check: impl Fn(&Permissions) -> bool,
    what: &str,
) -> CmdResult<()> {
    match state.registry.lock().unwrap().state(id) {
        Some(CapState::Enabling | CapState::Enabled) => {}
        Some(_) => return Err(format!("`{id}` is not enabled")),
        None => return Err(format!("no module `{id}`")),
    }
    if check(&granted(state, id)) {
        Ok(())
    } else {
        Err(format!("`{id}` was not granted the {what} permission"))
    }
}

fn as_map(v: Option<&Value>) -> Map<String, Value> {
    v.and_then(Value::as_object).cloned().unwrap_or_default()
}

/// The structured data API. `op` is insert, get, query, update, or soft_delete.
#[tauri::command]
pub fn cap_db(
    state: State<'_, AppState>,
    capability: String,
    op: String,
    table: String,
    args: Value,
) -> CmdResult<Value> {
    require(&state, &capability, |p| p.storage, "storage")?;
    let db = state.db.lock().unwrap();
    let store = CapStore::new(db.connection(), &capability, &state.device_id);
    let id = args.get("id").and_then(Value::as_str).unwrap_or_default();
    let result = match op.as_str() {
        "insert" => store.insert(&table, &as_map(args.get("record"))).map(Some),
        "get" => store.get(&table, id),
        "update" => store.update(&table, id, &as_map(args.get("patch"))),
        "soft_delete" => store
            .soft_delete(&table, id)
            .map(|done| Some(Value::Bool(done))),
        "query" => store
            .query(
                &table,
                &Query {
                    filter: as_map(args.get("filter")),
                    order_by: args
                        .get("order_by")
                        .and_then(Value::as_str)
                        .map(str::to_string),
                    descending: args
                        .get("descending")
                        .and_then(Value::as_bool)
                        .unwrap_or(false),
                    limit: args.get("limit").and_then(Value::as_u64).map(|n| n as u32),
                },
            )
            .map(|rows| Some(Value::Array(rows))),
        other => return Err(format!("unknown db operation `{other}`")),
    };
    result
        .map(|v| v.unwrap_or(Value::Null))
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub fn cap_emit(
    app: AppHandle,
    state: State<'_, AppState>,
    capability: String,
    name: String,
    payload: Option<Value>,
    significance: Option<f32>,
) -> CmdResult<()> {
    {
        let reg = state.registry.lock().unwrap();
        let entry = reg
            .get(&capability)
            .ok_or_else(|| format!("no module `{capability}`"))?;
        if !matches!(entry.state, CapState::Enabling | CapState::Enabled) {
            return Err(format!("`{capability}` is not enabled"));
        }
        if !entry.loaded.manifest.events.iter().any(|e| e.id == name) {
            return Err(format!(
                "`{capability}` did not declare the event `{name}` in its manifest"
            ));
        }
        if let (Some(schema), Some(payload)) = (entry.loaded.event_schemas.get(&name), &payload) {
            schema::validate(schema, payload)
                .map_err(|errors| format!("invalid payload for `{name}`: {}", errors.join("; ")))?;
        }
    }
    shell::publish(
        &app,
        &name,
        payload.unwrap_or_else(|| json!({})),
        significance,
    );
    Ok(())
}

fn validate_action(state: &AppState, capability: &str, action: &Value) -> CmdResult<()> {
    let reg = state.registry.lock().unwrap();
    let entry = reg
        .get(capability)
        .ok_or_else(|| format!("no module `{capability}`"))?;
    if let Some(event) = action.get("event").and_then(Value::as_str) {
        if !entry.loaded.manifest.events.iter().any(|e| e.id == event) {
            return Err(format!("`{event}` is not an event `{capability}` declared"));
        }
        return Ok(());
    }
    if let Some(tool) = action.get("tool").and_then(Value::as_str) {
        if !perms::may_call_tool(&granted(state, capability), capability, tool) {
            return Err(format!("`{capability}` may not schedule `{tool}`"));
        }
        return Ok(());
    }
    Err("a scheduled action needs an `event` or a `tool`".into())
}

/// `op` is `at`, `every`, or `cancel`.
#[tauri::command]
pub fn cap_schedule(
    state: State<'_, AppState>,
    capability: String,
    op: String,
    args: Value,
) -> CmdResult<Value> {
    require(&state, &capability, |p| p.scheduler, "scheduler")?;
    let policy = match args.get("missed").and_then(Value::as_str) {
        Some("skip") => MissedPolicy::Skip,
        Some("fire_all") => MissedPolicy::FireAll,
        _ => MissedPolicy::FireOnceOnStart,
    };
    let db = state.db.lock().unwrap();
    match op.as_str() {
        "at" | "every" => {
            let action = args.get("action").cloned().unwrap_or(Value::Null);
            drop(db);
            validate_action(&state, &capability, &action)?;
            let db = state.db.lock().unwrap();
            let id = if op == "at" {
                let at = args
                    .get("at_ms")
                    .and_then(Value::as_i64)
                    .ok_or("at_ms is required")?;
                scheduler::schedule_at(&db, &capability, at, &action, policy)
            } else {
                let every = args
                    .get("interval_ms")
                    .and_then(Value::as_i64)
                    .ok_or("interval_ms is required")?;
                scheduler::schedule_every(
                    &db,
                    &capability,
                    every,
                    args.get("first_at_ms").and_then(Value::as_i64),
                    &action,
                    policy,
                )
            }
            .map_err(|e| e.to_string())?;
            Ok(json!(id))
        }
        "cancel" => {
            let job = args
                .get("job_id")
                .and_then(Value::as_str)
                .ok_or("job_id is required")?;
            let removed = scheduler::cancel(&db, &capability, job).map_err(|e| e.to_string())?;
            Ok(json!(removed))
        }
        other => Err(format!("unknown scheduler operation `{other}`")),
    }
}

#[tauri::command]
pub fn cap_notify(
    app: AppHandle,
    state: State<'_, AppState>,
    capability: String,
    title: String,
    body: Option<String>,
) -> CmdResult<()> {
    require(&state, &capability, |p| p.notifications, "notifications")?;
    {
        let db = state.db.lock().unwrap();
        db.connection()
            .execute(
                "INSERT INTO notifications (id, owner, title, body, created_at) VALUES (?1, ?2, ?3, ?4, ?5)",
                rusqlite_params(&capability, &title, body.as_deref().unwrap_or("")),
            )
            .map_err(|e| e.to_string())?;
    }
    shell::publish(
        &app,
        "core.notification",
        json!({ "text": title, "capability": capability }),
        None,
    );
    Ok(())
}

fn rusqlite_params(owner: &str, title: &str, body: &str) -> impl rusqlite::Params {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_millis() as i64);
    (
        uuid::Uuid::now_v7().to_string(),
        owner.to_string(),
        title.to_string(),
        body.to_string(),
        now,
    )
}

#[tauri::command]
pub fn cap_badge(app: AppHandle, state: State<'_, AppState>, capability: String, value: i64) {
    state.badges.lock().unwrap().insert(capability, value);
    emit_capabilities(&app);
}

#[tauri::command]
pub fn cap_log(state: State<'_, AppState>, capability: String, level: String, message: String) {
    use std::io::Write;
    let _ = fs::create_dir_all(state.home.logs_dir());
    if let Ok(mut file) = fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(state.home.logs_dir().join("capabilities.log"))
    {
        let _ = writeln!(file, "[{level}] {capability}: {message}");
    }
}

// ---------------------------------------------------------------- scheduler

/// Checks the job table once a second and turns due jobs into events or
/// tool calls. Jobs of a capability that is not running are left in place.
pub fn spawn_scheduler(app: AppHandle) {
    tauri::async_runtime::spawn(async move {
        loop {
            tokio::time::sleep(Duration::from_secs(1)).await;
            let now = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map_or(0, |d| d.as_millis() as i64);
            let state = app.state::<AppState>();
            let jobs = match scheduler::due(&state.db.lock().unwrap(), now) {
                Ok(j) => j,
                Err(_) => continue,
            };
            for job in jobs {
                if state.registry.lock().unwrap().state(&job.owner) != Some(CapState::Enabled) {
                    continue;
                }
                let _ = scheduler::after_fire(&state.db.lock().unwrap(), &job, now);
                if let Some(event) = job.action.get("event").and_then(Value::as_str) {
                    let payload = job
                        .action
                        .get("payload")
                        .cloned()
                        .unwrap_or_else(|| json!({}));
                    shell::publish(&app, event, payload, None);
                } else if let Some(tool) = job.action.get("tool").and_then(Value::as_str) {
                    let input = job
                        .action
                        .get("input")
                        .cloned()
                        .unwrap_or_else(|| json!({}));
                    let app = app.clone();
                    let (owner, tool) = (job.owner.clone(), tool.to_string());
                    tauri::async_runtime::spawn(async move {
                        if let Err(e) = dispatch_tool(&app, Some(&owner), &tool, input).await {
                            eprintln!("scheduled call to {tool} failed: {e}");
                        }
                    });
                }
            }
        }
    });
}

// ---------------------------------------------------------- event forwarding

/// Called for every bus event: views get all of them, and each enabled
/// capability's logic gets only the namespaces it may listen to.
pub fn forward_event(app: &AppHandle, event: &Event) {
    let _ = app.emit_to(OVERLAY, "frosty://event", event);
    let _ = app.emit_to(MAIN, "frosty://event", event);

    let state = app.state::<AppState>();
    let targets: Vec<String> = {
        let reg = state.registry.lock().unwrap();
        reg.iter()
            .filter(|e| e.state == CapState::Enabled)
            .map(|e| e.loaded.id().to_string())
            .collect()
    };
    for id in targets {
        let grants = granted(&state, &id);
        if perms::may_listen_to(&grants, &id, &event.name) {
            let _ = app.emit_to(
                LOGIC,
                "frosty://cap-event",
                json!({ "capability": id, "event": event }),
            );
        }
    }
}

// ------------------------------------------------------------ file serving

fn content_type(path: &Path) -> &'static str {
    match path.extension().and_then(|e| e.to_str()) {
        Some("js" | "mjs") => "text/javascript",
        Some("css") => "text/css",
        Some("json") => "application/json",
        Some("svg") => "image/svg+xml",
        Some("png") => "image/png",
        Some("woff2") => "font/woff2",
        _ => "application/octet-stream",
    }
}

/// Maps `/<id>/<dist|assets>/...` to a file inside that capability's folder.
pub fn resolve_request(dir: &Path, rest: &str) -> Option<PathBuf> {
    let rel = Path::new(rest);
    let mut parts = rel.components();
    let first = match parts.next()? {
        Component::Normal(p) => p.to_str()?,
        _ => return None,
    };
    if !SERVED_DIRS.contains(&first) {
        return None;
    }
    if !rel.components().all(|c| matches!(c, Component::Normal(_))) {
        return None;
    }
    Some(dir.join(rel))
}

pub fn serve(app: &AppHandle, uri_path: &str) -> Response<std::borrow::Cow<'static, [u8]>> {
    let respond = |status: StatusCode, body: Vec<u8>, ctype: &str| {
        Response::builder()
            .status(status)
            .header("Content-Type", ctype)
            .header("Access-Control-Allow-Origin", "*")
            .body(std::borrow::Cow::Owned(body))
            .unwrap_or_else(|_| Response::new(std::borrow::Cow::Owned(Vec::new())))
    };
    let trimmed = uri_path.trim_start_matches('/');
    let Some((id, rest)) = trimmed.split_once('/') else {
        return respond(StatusCode::NOT_FOUND, Vec::new(), "text/plain");
    };
    let dir = {
        let state = app.state::<AppState>();
        let reg = state.registry.lock().unwrap();
        reg.get(id).map(|e| e.loaded.dir.clone())
    };
    let Some(dir) = dir else {
        return respond(StatusCode::NOT_FOUND, Vec::new(), "text/plain");
    };
    let Some(path) = resolve_request(&dir, rest) else {
        return respond(StatusCode::FORBIDDEN, Vec::new(), "text/plain");
    };
    match fs::read(&path) {
        Ok(bytes) => respond(StatusCode::OK, bytes, content_type(&path)),
        Err(_) => respond(StatusCode::NOT_FOUND, Vec::new(), "text/plain"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_dist_and_assets_are_served() {
        let dir = Path::new("C:/caps/notes");
        assert!(resolve_request(dir, "dist/index.js").is_some());
        assert!(resolve_request(dir, "assets/icon.svg").is_some());
        assert!(resolve_request(dir, "capability.toml").is_none());
        assert!(resolve_request(dir, "migrations/0001.sql").is_none());
        assert!(resolve_request(dir, "dist/../capability.toml").is_none());
        assert!(resolve_request(dir, "../other/dist/index.js").is_none());
        assert!(resolve_request(dir, "").is_none());
    }

    #[test]
    fn urls_use_the_capability_id_prefix() {
        let url = cap_url("notes", "/dist/views.js");
        assert!(url.ends_with("/notes/dist/views.js"), "{url}");
    }
}
