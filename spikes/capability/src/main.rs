//! Spike D: can capability code live outside the app bundle, share the
//! shell's React, run its logic in a hidden webview, and be reached through
//! the Rust core? Throwaway code. See docs/spike-findings.md.
//!
//! Usage (from this folder, after `node shell/build.mjs`):
//!   spike-capability.exe [--idle SECONDS] [--no-logic]

use std::{
    collections::HashMap,
    fs,
    io::Write,
    path::{Component, Path, PathBuf},
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, AtomicU64, Ordering},
    },
    time::{Duration, Instant},
};

use serde_json::{Value, json};
use tauri::{
    AppHandle, Emitter, Manager, State, WebviewUrl, WebviewWindowBuilder,
    http::{Response, StatusCode},
};
use tokio::sync::oneshot;

#[derive(Default)]
struct Shared {
    pending: Mutex<HashMap<u64, oneshot::Sender<Value>>>,
    next_id: AtomicU64,
    logic_ready: AtomicBool,
}

fn log(line: &str) {
    println!("{line}");
    if let Ok(mut f) = fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open("spike.log")
    {
        let _ = writeln!(f, "{line}");
    }
}

/// Maps a request path to a file under `root`, refusing anything that could
/// leave it.
fn resolve(root: &Path, request_path: &str) -> Option<PathBuf> {
    let rel = Path::new(request_path.trim_start_matches('/'));
    if rel.as_os_str().is_empty() || !rel.components().all(|c| matches!(c, Component::Normal(_))) {
        return None;
    }
    Some(root.join(rel))
}

#[tauri::command]
fn report(line: String) {
    log(&format!("  {line}"));
}

#[tauri::command]
fn logic_ready(state: State<'_, Arc<Shared>>) {
    state.logic_ready.store(true, Ordering::SeqCst);
}

#[tauri::command]
fn tool_result(id: u64, result: Value, state: State<'_, Arc<Shared>>) {
    if let Some(tx) = state.pending.lock().unwrap().remove(&id) {
        let _ = tx.send(result);
    }
}

/// View -> core -> logic host -> core -> view.
async fn dispatch(app: &AppHandle, shared: &Shared, tool: &str, input: Value) -> Result<Value, String> {
    let id = shared.next_id.fetch_add(1, Ordering::SeqCst);
    let (tx, rx) = oneshot::channel();
    shared.pending.lock().unwrap().insert(id, tx);
    app.emit_to("logic", "tool-call", json!({ "id": id, "tool": tool, "input": input }))
        .map_err(|e| e.to_string())?;
    match tokio::time::timeout(Duration::from_secs(5), rx).await {
        Ok(Ok(value)) => Ok(value),
        Ok(Err(_)) => Err("logic host dropped the call".into()),
        Err(_) => {
            shared.pending.lock().unwrap().remove(&id);
            Err("timed out waiting for the logic host".into())
        }
    }
}

#[tauri::command]
async fn call_tool(
    app: AppHandle,
    state: State<'_, Arc<Shared>>,
    tool: String,
    input: Value,
) -> Result<Value, String> {
    dispatch(&app, &state, &tool, input).await
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let idle_seconds: u64 = args
        .iter()
        .position(|a| a == "--idle")
        .and_then(|i| args.get(i + 1))
        .and_then(|v| v.parse().ok())
        .unwrap_or(0);
    let no_logic = args.iter().any(|a| a == "--no-logic");

    let shared = Arc::new(Shared::default());
    let root = std::env::current_dir().unwrap().join("fixtures");

    tauri::Builder::default()
        .manage(shared.clone())
        .register_uri_scheme_protocol("frosty-cap", move |_ctx, request| {
            let respond = |status: StatusCode, body: Vec<u8>| {
                Response::builder()
                    .status(status)
                    .header("Content-Type", "text/javascript")
                    .header("Access-Control-Allow-Origin", "*")
                    .body(std::borrow::Cow::Owned(body))
                    .unwrap()
            };
            match resolve(&root, request.uri().path()) {
                None => respond(StatusCode::FORBIDDEN, Vec::new()),
                Some(path) => match fs::read(&path) {
                    Ok(bytes) => respond(StatusCode::OK, bytes),
                    Err(_) => respond(StatusCode::NOT_FOUND, Vec::new()),
                },
            }
        })
        .invoke_handler(tauri::generate_handler![report, logic_ready, tool_result, call_tool])
        .setup(move |app| {
            if !no_logic {
                WebviewWindowBuilder::new(app, "logic", WebviewUrl::App("logic.html".into()))
                    .title("logic host")
                    .visible(false)
                    .build()?;
            }
            WebviewWindowBuilder::new(app, "view", WebviewUrl::App("view.html".into()))
                .title("view")
                .inner_size(420.0, 240.0)
                .build()?;

            let handle = app.handle().clone();
            std::thread::spawn(move || {
                tauri::async_runtime::block_on(orchestrate(handle, shared, idle_seconds, no_logic));
            });
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running spike");
}

async fn orchestrate(app: AppHandle, shared: Arc<Shared>, idle_seconds: u64, no_logic: bool) {
    log(&format!(
        "spike D start (idle {idle_seconds}s, logic host {})",
        if no_logic { "absent" } else { "present" }
    ));

    if no_logic {
        tokio::time::sleep(Duration::from_secs(idle_seconds.max(15))).await;
        app.exit(0);
        return;
    }

    let deadline = Instant::now() + Duration::from_secs(20);
    while !shared.logic_ready.load(Ordering::SeqCst) {
        if Instant::now() > deadline {
            log("FAIL: logic host never became ready");
            app.exit(1);
            return;
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    // Let the view finish its own checks.
    tokio::time::sleep(Duration::from_secs(4)).await;

    // Round trip through the core into the hidden webview and back.
    let mut samples = Vec::new();
    for _ in 0..30 {
        let t = Instant::now();
        match dispatch(&app, &shared, "hello.ping", json!({})).await {
            Ok(v) if v["ok"] == true => samples.push(t.elapsed().as_secs_f64() * 1000.0),
            other => log(&format!("  ping failed: {other:?}")),
        }
    }
    samples.sort_by(|a, b| a.partial_cmp(b).unwrap());
    if let (Some(first), Some(median), Some(worst)) =
        (samples.first(), samples.get(samples.len() / 2), samples.last())
    {
        log(&format!(
            "tool round trip view->core->logic host->core: {} calls, best {first:.2} ms, median {median:.2} ms, worst {worst:.2} ms",
            samples.len()
        ));
    }

    match dispatch(&app, &shared, "hello.nope", json!({})).await {
        Ok(v) if v["ok"] == false => log("  unknown tool is reported as an error, not a hang: PASS"),
        other => log(&format!("  unknown tool handling: FAIL {other:?}")),
    }

    if idle_seconds > 0 {
        // Hide the only visible window so the whole app is in the background,
        // then check the hidden logic host still answers on time.
        if let Some(view) = app.get_webview_window("view") {
            let _ = view.hide();
        }
        log(&format!("idle phase: all windows hidden for {idle_seconds}s, ping every 5s"));
        let start = Instant::now();
        let mut slow = 0;
        let mut last_ticks = 0u64;
        while start.elapsed() < Duration::from_secs(idle_seconds) {
            tokio::time::sleep(Duration::from_secs(5)).await;
            let t = Instant::now();
            match dispatch(&app, &shared, "hello.ping", json!({})).await {
                Ok(v) if v["ok"] == true => {
                    let ms = t.elapsed().as_secs_f64() * 1000.0;
                    let ticks = v["ticks"].as_u64().unwrap_or(0);
                    log(&format!(
                        "  t+{:>3}s ping {ms:.1} ms, 1 s timer ticks so far {ticks} (+{})",
                        start.elapsed().as_secs(),
                        ticks.saturating_sub(last_ticks)
                    ));
                    last_ticks = ticks;
                    if ms > 100.0 {
                        slow += 1;
                    }
                }
                other => {
                    slow += 1;
                    log(&format!("  ping failed while hidden: {other:?}"));
                }
            }
        }
        log(&format!(
            "idle phase done: {slow} slow or failed pings. Timer ticks should be about {} if timers are not throttled.",
            start.elapsed().as_secs()
        ));
    }

    log("spike D done");
    app.exit(0);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolve_stays_inside_root() {
        let root = Path::new("C:/caps");
        assert!(resolve(root, "/hello/dist/index.js").is_some());
        assert!(resolve(root, "/../secret.txt").is_none());
        assert!(resolve(root, "/hello/../../secret.txt").is_none());
        assert!(resolve(root, "/").is_none());
        assert!(resolve(root, "/C:/Windows/win.ini").is_none());
    }
}
