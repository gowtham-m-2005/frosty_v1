//! Window manager: the overlay (compact mode) and the main window
//! (fullscreen mode). Both are created once and only ever hidden, so summon
//! stays fast (docs/PROJECT_SPEC.md 7.8).

use serde::Deserialize;
use tauri::{
    AppHandle, Manager, PhysicalPosition, PhysicalSize, WebviewUrl, WebviewWindow,
    WebviewWindowBuilder,
};

pub const OVERLAY: &str = "overlay";
pub const MAIN: &str = "main";
/// The hidden webview that runs every capability's logic (docs/PROJECT_SPEC.md 8.10).
pub const LOGIC: &str = "logic";

#[derive(Debug, Clone, Copy, Deserialize)]
pub struct HitRect {
    pub x: f64,
    pub y: f64,
    pub w: f64,
    pub h: f64,
}

pub fn create_overlay(app: &AppHandle) -> tauri::Result<WebviewWindow> {
    WebviewWindowBuilder::new(
        app,
        OVERLAY,
        WebviewUrl::App("index.html?window=overlay".into()),
    )
    .title("Frosty")
    .transparent(true)
    .decorations(false)
    .shadow(false)
    .always_on_top(true)
    .skip_taskbar(true)
    .resizable(false)
    .visible(false)
    .focused(false)
    .build()
}

pub fn create_logic(app: &AppHandle) -> tauri::Result<WebviewWindow> {
    WebviewWindowBuilder::new(
        app,
        LOGIC,
        WebviewUrl::App("index.html?window=logic".into()),
    )
    .title("Frosty logic host")
    .visible(false)
    .skip_taskbar(true)
    .focused(false)
    .build()
}

/// Covers the work area of the monitor that holds the cursor, falling back to
/// the primary monitor.
pub fn place_overlay(app: &AppHandle, overlay: &WebviewWindow) -> tauri::Result<()> {
    let monitor = app
        .cursor_position()
        .ok()
        .and_then(|p| app.monitor_from_point(p.x, p.y).ok().flatten())
        .or_else(|| app.primary_monitor().ok().flatten());
    if let Some(monitor) = monitor {
        let area = monitor.work_area();
        overlay.set_position(PhysicalPosition::new(area.position.x, area.position.y))?;
        overlay.set_size(PhysicalSize::new(area.size.width, area.size.height))?;
    }
    Ok(())
}

pub fn is_visible(app: &AppHandle, label: &str) -> bool {
    app.get_webview_window(label)
        .and_then(|w| w.is_visible().ok())
        .unwrap_or(false)
}

/// Click-through without polling: clip the window to the union of the hit
/// rectangles so clicks elsewhere reach the window behind (Spike A).
#[cfg(windows)]
pub fn set_hit_region(window: &WebviewWindow, rects: &[HitRect]) {
    use windows::Win32::{
        Foundation::HWND,
        Graphics::Gdi::{CombineRgn, CreateRectRgn, RGN_OR, SetWindowRgn},
    };

    let Ok(hwnd) = window.hwnd() else { return };
    let hwnd = hwnd.0 as isize;
    let scale = window.scale_factor().unwrap_or(1.0);
    let rects: Vec<HitRect> = rects.to_vec();
    let _ = window.run_on_main_thread(move || unsafe {
        let region = CreateRectRgn(0, 0, 0, 0);
        for r in &rects {
            let piece = CreateRectRgn(
                (r.x * scale).floor() as i32,
                (r.y * scale).floor() as i32,
                ((r.x + r.w) * scale).ceil() as i32,
                ((r.y + r.h) * scale).ceil() as i32,
            );
            CombineRgn(Some(region), Some(region), Some(piece), RGN_OR);
        }
        // The window owns the region after this call.
        SetWindowRgn(HWND(hwnd as _), Some(region), true);
    });
}

#[cfg(not(windows))]
pub fn set_hit_region(_window: &WebviewWindow, _rects: &[HitRect]) {}
