//! Spike A: transparent, frameless, always-on-top overlay with click-through
//! on transparent pixels, plus fullscreen-app detection. Throwaway code.
//!
//! Run `spike-overlay.exe` to look at it, or `spike-overlay.exe --selftest` to
//! have it move the cursor over and off the boxes and report whether clicks
//! would reach the window behind.

use std::{
    fs::OpenOptions,
    io::Write,
    sync::{Arc, Mutex},
    thread,
    time::{Duration, Instant},
};

use serde::Deserialize;
use tauri::{Emitter, State, WebviewUrl, WebviewWindowBuilder};
use windows::Win32::{
    Foundation::{FILETIME, HWND, POINT},
    System::Threading::{GetCurrentProcess, GetProcessTimes},
    UI::{
        Shell::SHQueryUserNotificationState,
        WindowsAndMessaging::{GA_ROOT, GetAncestor, GetCursorPos, SetCursorPos, WindowFromPoint},
    },
};

#[derive(Clone, Copy, Deserialize)]
struct Rect {
    x: f64,
    y: f64,
    w: f64,
    h: f64,
}

#[derive(Deserialize)]
struct NamedRect {
    id: String,
    x: f64,
    y: f64,
    w: f64,
    h: f64,
}

#[derive(Default)]
struct HitRects(Mutex<Vec<(String, Rect)>>);

#[tauri::command]
fn set_hit_rects(rects: Vec<NamedRect>, state: State<'_, Arc<HitRects>>) {
    *state.0.lock().unwrap() = rects
        .into_iter()
        .map(|r| {
            let rect = Rect { x: r.x, y: r.y, w: r.w, h: r.h };
            (r.id, rect)
        })
        .collect();
}

fn log(line: &str) {
    println!("{line}");
    if let Ok(mut f) = OpenOptions::new().create(true).append(true).open("spike.log") {
        let _ = writeln!(f, "{line}");
    }
}

fn filetime_100ns(ft: FILETIME) -> u64 {
    (u64::from(ft.dwHighDateTime) << 32) | u64::from(ft.dwLowDateTime)
}

fn process_cpu_100ns() -> u64 {
    let mut creation = FILETIME::default();
    let mut exit = FILETIME::default();
    let mut kernel = FILETIME::default();
    let mut user = FILETIME::default();
    unsafe {
        let _ = GetProcessTimes(
            GetCurrentProcess(),
            &mut creation,
            &mut exit,
            &mut kernel,
            &mut user,
        );
    }
    filetime_100ns(kernel) + filetime_100ns(user)
}

fn cursor() -> POINT {
    let mut p = POINT::default();
    unsafe {
        let _ = GetCursorPos(&mut p);
    }
    p
}

fn notification_state() -> String {
    match unsafe { SHQueryUserNotificationState() } {
        Ok(s) => match s.0 {
            1 => "NOT_PRESENT".into(),
            2 => "BUSY (fullscreen app, not Direct3D exclusive)".into(),
            3 => "RUNNING_D3D_FULL_SCREEN (exclusive fullscreen game)".into(),
            4 => "PRESENTATION_MODE".into(),
            5 => "ACCEPTS_NOTIFICATIONS (normal)".into(),
            6 => "QUIET_TIME".into(),
            7 => "APP (fullscreen store app)".into(),
            n => format!("unknown({n})"),
        },
        Err(e) => format!("error: {e}"),
    }
}

fn main() {
    let selftest = std::env::args().any(|a| a == "--selftest");
    let rects = Arc::new(HitRects::default());

    tauri::Builder::default()
        .manage(rects.clone())
        .invoke_handler(tauri::generate_handler![set_hit_rects])
        .setup(move |app| {
            let monitor = app.primary_monitor()?.expect("no primary monitor");
            let work = monitor.work_area();
            let scale = monitor.scale_factor();
            log(&format!(
                "monitor: {}x{} at ({},{}), work area {}x{} at ({},{}), scale {scale}",
                monitor.size().width,
                monitor.size().height,
                monitor.position().x,
                monitor.position().y,
                work.size.width,
                work.size.height,
                work.position.x,
                work.position.y,
            ));

            let window =
                WebviewWindowBuilder::new(app, "overlay", WebviewUrl::App("index.html".into()))
                    .title("Frosty overlay spike")
                    .transparent(true)
                    .decorations(false)
                    .shadow(false)
                    .always_on_top(true)
                    .skip_taskbar(true)
                    .resizable(false)
                    .focused(false)
                    .position(
                        f64::from(work.position.x) / scale,
                        f64::from(work.position.y) / scale,
                    )
                    .inner_size(
                        f64::from(work.size.width) / scale,
                        f64::from(work.size.height) / scale,
                    )
                    .build()?;

            // Click-through controller: poll the cursor and flip
            // ignore-cursor-events when it enters or leaves a hit rectangle.
            let w = window.clone();
            let rects_poll = rects.clone();
            thread::spawn(move || {
                let mut ignoring: Option<bool> = None;
                let mut polls = 0u64;
                let mut toggles = 0u64;
                let mut worst = Duration::ZERO;
                let mut last_report = Instant::now();
                let mut cpu_at_report = process_cpu_100ns();
                let mut last_state = String::new();
                let mut last_state_check = Instant::now() - Duration::from_secs(1);
                loop {
                    thread::sleep(Duration::from_millis(16));
                    let t = Instant::now();
                    let p = cursor();
                    let origin = w.outer_position().unwrap_or_default();
                    let scale = w.scale_factor().unwrap_or(1.0);
                    let inside = rects_poll.0.lock().unwrap().iter().any(|(_, r)| {
                        let x = f64::from(origin.x) + r.x * scale;
                        let y = f64::from(origin.y) + r.y * scale;
                        let (px, py) = (f64::from(p.x), f64::from(p.y));
                        px >= x && px < x + r.w * scale && py >= y && py < y + r.h * scale
                    });
                    let want_ignore = !inside;
                    if ignoring != Some(want_ignore) {
                        let _ = w.set_ignore_cursor_events(want_ignore);
                        ignoring = Some(want_ignore);
                        toggles += 1;
                    }
                    polls += 1;
                    worst = worst.max(t.elapsed());

                    if last_state_check.elapsed() >= Duration::from_secs(1) {
                        last_state_check = Instant::now();
                        let s = notification_state();
                        if s != last_state {
                            log(&format!("fullscreen state: {s}"));
                            let _ = w.emit("spike-status", format!("state: {s}"));
                            last_state = s;
                        }
                    }

                    if last_report.elapsed() >= Duration::from_secs(10) {
                        let cpu_now = process_cpu_100ns();
                        let cpu_pct = (cpu_now - cpu_at_report) as f64
                            / (last_report.elapsed().as_secs_f64() * 1e7)
                            * 100.0;
                        log(&format!(
                            "poll: {polls} polls, {toggles} toggles, worst iteration {worst:?}, process CPU {cpu_pct:.2}% of one core"
                        ));
                        polls = 0;
                        worst = Duration::ZERO;
                        cpu_at_report = cpu_now;
                        last_report = Instant::now();
                    }
                }
            });

            if selftest {
                let w = window.clone();
                let rects_test = rects.clone();
                let handle = app.handle().clone();
                thread::spawn(move || {
                    thread::sleep(Duration::from_secs(3));
                    let origin = w.outer_position().unwrap();
                    let scale = w.scale_factor().unwrap();
                    let overlay = HWND(w.hwnd().unwrap().0 as _);
                    let probe = |name: &str, x: i32, y: i32, expect_overlay: bool| -> bool {
                        unsafe {
                            let _ = SetCursorPos(x, y);
                        }
                        // Give the poller a few frames to flip the flag.
                        thread::sleep(Duration::from_millis(150));
                        let hit = unsafe { WindowFromPoint(POINT { x, y }) };
                        let root = unsafe { GetAncestor(hit, GA_ROOT) };
                        let is_overlay = root == overlay;
                        let ok = is_overlay == expect_overlay;
                        log(&format!(
                            "selftest {name}: ({x},{y}) hit overlay={is_overlay} expected={expect_overlay} -> {}",
                            if ok { "PASS" } else { "FAIL" }
                        ));
                        ok
                    };
                    let saved = cursor();
                    let mut all = true;
                    let list = rects_test.0.lock().unwrap().clone();
                    log(&format!("selftest: {} hit rects reported by the page", list.len()));
                    all &= !list.is_empty();
                    for (id, r) in &list {
                        let cx = f64::from(origin.x) + (r.x + r.w / 2.0) * scale;
                        let cy = f64::from(origin.y) + (r.y + r.h / 2.0) * scale;
                        all &= probe(&format!("center of {id}"), cx as i32, cy as i32, true);
                    }
                    // Near the top left of the work area nothing is drawn.
                    let tx = origin.x + (40.0 * scale) as i32;
                    let ty = origin.y + (120.0 * scale) as i32;
                    all &= probe("transparent area", tx, ty, false);
                    unsafe {
                        let _ = SetCursorPos(saved.x, saved.y);
                    }
                    log(&format!("selftest RESULT: {}", if all { "PASS" } else { "FAIL" }));
                    thread::sleep(Duration::from_secs(1));
                    handle.exit(0);
                });
            }
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running spike");
}
