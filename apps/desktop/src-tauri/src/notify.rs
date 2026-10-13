//! Notification center driver (docs/PROJECT_SPEC.md 7.5, docs/DESIGN_SPEC.md 4.5).
//!
//! Capabilities submit notifications. This decides, with the pure rules in
//! `frosty_core::notifications`, whether each one is shown now, held until
//! do-not-disturb or a fullscreen app is over, or dropped. A shown one makes
//! Frosty appear in a minimal state with its actions.

use frosty_core::notifications::{self as center, Conditions, Delivery, Notification};
use serde_json::{Value, json};
use std::sync::atomic::Ordering;
use tauri::{AppHandle, Emitter, Manager, State};

use crate::{
    caps, fullscreen, shell,
    state::AppState,
    windows::{self, OVERLAY},
};

type CmdResult<T> = Result<T, String>;

const MAX_ACTIONS: usize = 2;

fn now_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_millis() as i64)
}

/// Checks the shape of `actions`: up to two `{label, tool, input?}`.
pub fn validate_actions(actions: &Value) -> Result<(), String> {
    let Some(list) = actions.as_array() else {
        return Err("actions must be a list".into());
    };
    if list.len() > MAX_ACTIONS {
        return Err(format!(
            "a notification can have at most {MAX_ACTIONS} actions"
        ));
    }
    for action in list {
        let label = action
            .get("label")
            .and_then(Value::as_str)
            .unwrap_or_default();
        let tool = action
            .get("tool")
            .and_then(Value::as_str)
            .unwrap_or_default();
        if label.is_empty() || label.chars().count() > 24 {
            return Err("each action needs a label of up to 24 characters".into());
        }
        if tool.is_empty() {
            return Err("each action needs a tool".into());
        }
        if action.get("input").is_some_and(|i| !i.is_object()) {
            return Err("an action's input must be an object".into());
        }
    }
    Ok(())
}

fn dnd_active(state: &AppState) -> bool {
    if state.dnd_manual.load(Ordering::SeqCst) {
        return true;
    }
    let cfg = state.config.lock().unwrap();
    let n = &cfg.current().notifications;
    n.dnd_schedule_enabled
        && center::in_dnd_window(&n.dnd_start, &n.dnd_end, fullscreen::local_minutes_of_day())
}

/// Stores the notification, then tries to show it.
pub fn submit(
    app: &AppHandle,
    owner: &str,
    title: &str,
    body: &str,
    actions: &Value,
) -> CmdResult<String> {
    validate_actions(actions)?;
    let id = {
        let state = app.state::<AppState>();
        let db = state.db.lock().unwrap();
        center::insert(&db, owner, title, body, actions, now_ms()).map_err(|e| e.to_string())?
    };
    deliver_pending(app);
    Ok(id)
}

/// Goes through everything waiting. Called after each submit, when a
/// fullscreen app ends, when do-not-disturb ends, and on a slow tick.
pub fn deliver_pending(app: &AppHandle) {
    let state = app.state::<AppState>();
    let now = now_ms();
    let waiting = match center::held(&state.db.lock().unwrap()) {
        Ok(w) => w,
        Err(_) => return,
    };
    if waiting.is_empty() {
        return;
    }

    let dnd = dnd_active(&state);
    let fullscreen_app = state.fullscreen_app.load(Ordering::SeqCst);
    let muted = state
        .config
        .lock()
        .unwrap()
        .current()
        .notifications
        .muted
        .clone();

    let mut to_show = Vec::new();
    {
        let db = state.db.lock().unwrap();
        let mut limiter = state.rate.lock().unwrap();
        for n in waiting {
            let mut c = Conditions {
                dnd_active: dnd,
                fullscreen_app,
                muted: muted.contains(&n.owner),
                rate_limited: false,
            };
            // The allowance is only spent when the notification would show.
            if center::decide(&c) == Delivery::Deliver {
                c.rate_limited = !limiter.allow(&n.owner, now);
            }
            match center::decide(&c) {
                Delivery::Deliver => {
                    let _ = center::mark_delivered(&db, &n.id, now);
                    to_show.push(n);
                }
                Delivery::Hold(reason) => {
                    let _ = center::set_hold_reason(&db, &n.id, reason);
                }
                Delivery::Drop(reason) => {
                    let _ = center::mark_dropped(&db, &n.id, reason, now);
                }
            }
        }
        let _ = center::trim_held(&db, now);
    }

    for n in to_show {
        present(app, &n);
    }
    emit_unread(app);
}

fn emit_unread(app: &AppHandle) {
    let state = app.state::<AppState>();
    let count = center::unread_count(&state.db.lock().unwrap()).unwrap_or(0);
    let _ = app.emit("frosty://notifications-changed", json!({ "unread": count }));
}

/// Shows Frosty with the notification. If the overlay cannot be shown the
/// native notification is the fallback (when allowed in settings).
fn present(app: &AppHandle, n: &Notification) {
    let state = app.state::<AppState>();
    let (seconds, os_fallback) = {
        let cfg = state.config.lock().unwrap();
        let c = cfg.current();
        (c.notifications.chip_seconds, c.notifications.os_fallback)
    };

    let shown = app
        .get_webview_window(OVERLAY)
        .map(|overlay| {
            if overlay.is_visible().unwrap_or(false) {
                return true;
            }
            let _ = windows::place_overlay(app, &overlay);
            overlay.show().is_ok()
        })
        .unwrap_or(false);

    if shown {
        let _ = app.emit_to(
            OVERLAY,
            "frosty://notification",
            json!({
                "id": n.id,
                "owner": n.owner,
                "title": n.title,
                "body": n.body,
                "actions": n.actions,
                "seconds": seconds,
            }),
        );
        shell::publish(
            app,
            "core.notification",
            json!({ "text": n.title, "capability": n.owner, "id": n.id }),
            None,
        );
    } else if os_fallback {
        use tauri_plugin_notification::NotificationExt;
        let _ = app
            .notification()
            .builder()
            .title(&n.title)
            .body(&n.body)
            .show();
    }
}

/// Fullscreen app state, polled while Frosty runs. Reminders wait while a
/// fullscreen app is in front and are released when it ends.
pub fn spawn_fullscreen_watcher(app: AppHandle) {
    std::thread::spawn(move || {
        let mut was = false;
        loop {
            std::thread::sleep(std::time::Duration::from_secs(2));
            let now = fullscreen::foreground_is_fullscreen();
            if now == was {
                continue;
            }
            was = now;
            let state = app.state::<AppState>();
            state.fullscreen_app.store(now, Ordering::SeqCst);
            shell::publish(
                &app,
                if now {
                    "core.fullscreen_app_entered"
                } else {
                    "core.fullscreen_app_exited"
                },
                json!({}),
                None,
            );
            if !now {
                deliver_pending(&app);
            }
        }
    });
}

// --------------------------------------------------------------- commands

#[tauri::command]
pub fn list_notifications(
    state: State<'_, AppState>,
    limit: Option<u32>,
) -> CmdResult<Vec<Notification>> {
    center::list(&state.db.lock().unwrap(), limit.unwrap_or(50).min(200)).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn notifications_mark_read(app: AppHandle, state: State<'_, AppState>) -> CmdResult<()> {
    center::mark_all_read(&state.db.lock().unwrap(), now_ms()).map_err(|e| e.to_string())?;
    emit_unread(&app);
    Ok(())
}

#[tauri::command]
pub fn dismiss_notification(
    app: AppHandle,
    state: State<'_, AppState>,
    id: String,
) -> CmdResult<()> {
    center::mark_dismissed(&state.db.lock().unwrap(), &id, now_ms()).map_err(|e| e.to_string())?;
    emit_unread(&app);
    Ok(())
}

/// Runs one of a notification's actions as its owner, then dismisses it.
#[tauri::command]
pub async fn run_notification_action(app: AppHandle, id: String, index: usize) -> CmdResult<Value> {
    let (owner, action) = {
        let state = app.state::<AppState>();
        let n = center::get(&state.db.lock().unwrap(), &id)
            .map_err(|e| e.to_string())?
            .ok_or("that notification no longer exists")?;
        let action = n
            .actions
            .get(index)
            .cloned()
            .ok_or("that notification has no such action")?;
        (n.owner, action)
    };
    let tool = action["tool"].as_str().unwrap_or_default().to_string();
    let input = action.get("input").cloned().unwrap_or_else(|| json!({}));
    let result = caps::dispatch_tool(&app, Some(&owner), &tool, input).await;
    {
        let state = app.state::<AppState>();
        let _ = center::mark_dismissed(&state.db.lock().unwrap(), &id, now_ms());
    }
    emit_unread(&app);
    result
}

#[derive(serde::Serialize)]
pub struct DndStatus {
    pub manual: bool,
    pub active: bool,
    pub fullscreen_app: bool,
}

#[tauri::command]
pub fn get_dnd(state: State<'_, AppState>) -> DndStatus {
    DndStatus {
        manual: state.dnd_manual.load(Ordering::SeqCst),
        active: dnd_active(&state),
        fullscreen_app: state.fullscreen_app.load(Ordering::SeqCst),
    }
}

pub fn set_dnd(app: &AppHandle, on: bool) {
    let state = app.state::<AppState>();
    state.dnd_manual.store(on, Ordering::SeqCst);
    let _ = state
        .db
        .lock()
        .unwrap()
        .kv_set("dnd.manual", if on { "1" } else { "0" });
    if let Some(item) = state.tray_dnd.lock().unwrap().as_ref() {
        let _ = item.set_text(if on {
            "Do not disturb: on"
        } else {
            "Do not disturb: off"
        });
    }
    if on {
        shell::publish(app, "core.dnd_enabled", json!({}), None);
    } else {
        deliver_pending(app);
    }
    let _ = app.emit("frosty://dnd", json!({ "manual": on }));
}

#[tauri::command]
pub fn set_dnd_manual(app: AppHandle, on: bool) {
    set_dnd(&app, on);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn actions_are_validated() {
        assert!(validate_actions(&json!([])).is_ok());
        assert!(
            validate_actions(
                &json!([{ "label": "Done", "tool": "tasks.complete", "input": { "id": "1" } }])
            )
            .is_ok()
        );
        assert!(validate_actions(&json!("nope")).is_err());
        assert!(validate_actions(&json!([{ "label": "", "tool": "x.y" }])).is_err());
        assert!(validate_actions(&json!([{ "label": "Go" }])).is_err());
        assert!(
            validate_actions(&json!([{ "label": "Go", "tool": "a.b", "input": "no" }])).is_err()
        );
        let three = json!([
            { "label": "a", "tool": "x.a" }, { "label": "b", "tool": "x.b" }, { "label": "c", "tool": "x.c" }
        ]);
        assert!(validate_actions(&three).is_err());
    }
}
