//! Is a fullscreen app (a game, a video) in front? Reminders wait while one
//! is (docs/PROJECT_SPEC.md 7.5, decision D25).
//!
//! Spike A showed `SHQueryUserNotificationState` reports Frosty's own
//! full-monitor overlay as a fullscreen app, so this compares the foreground
//! window with its monitor instead, and ignores Frosty's own windows, the
//! desktop, and the taskbar.

pub type Rect = (i32, i32, i32, i32);

/// A window that reaches every edge of its monitor.
pub fn rect_covers(window: Rect, monitor: Rect) -> bool {
    window.0 <= monitor.0 && window.1 <= monitor.1 && window.2 >= monitor.2 && window.3 >= monitor.3
}

/// Maximized normal windows can also reach every edge when the taskbar hides
/// itself, so a title bar means "not fullscreen". Games and video players go
/// borderless.
pub const WS_CAPTION: u32 = 0x00C0_0000;

pub fn looks_fullscreen(window: Rect, monitor: Rect, style: u32) -> bool {
    rect_covers(window, monitor) && style & WS_CAPTION != WS_CAPTION
}

#[cfg(windows)]
pub fn foreground_is_fullscreen() -> bool {
    use windows::Win32::{
        Foundation::RECT,
        Graphics::Gdi::{
            GetMonitorInfoW, MONITOR_DEFAULTTONEAREST, MONITORINFO, MonitorFromWindow,
        },
        System::Threading::GetCurrentProcessId,
        UI::WindowsAndMessaging::{
            GWL_STYLE, GetClassNameW, GetForegroundWindow, GetWindowLongPtrW, GetWindowRect,
            GetWindowThreadProcessId, IsIconic,
        },
    };

    unsafe {
        let hwnd = GetForegroundWindow();
        if hwnd.0.is_null() || IsIconic(hwnd).as_bool() {
            return false;
        }
        let mut pid = 0u32;
        GetWindowThreadProcessId(hwnd, Some(&mut pid));
        if pid == GetCurrentProcessId() {
            return false;
        }

        let mut class = [0u16; 64];
        let len = GetClassNameW(hwnd, &mut class) as usize;
        let class = String::from_utf16_lossy(&class[..len.min(class.len())]);
        if matches!(
            class.as_str(),
            "Progman" | "WorkerW" | "Shell_TrayWnd" | "Shell_SecondaryTrayWnd"
        ) {
            return false;
        }

        let mut rect = RECT::default();
        if GetWindowRect(hwnd, &mut rect).is_err() {
            return false;
        }
        let monitor = MonitorFromWindow(hwnd, MONITOR_DEFAULTTONEAREST);
        let mut info = MONITORINFO {
            cbSize: std::mem::size_of::<MONITORINFO>() as u32,
            ..Default::default()
        };
        if !GetMonitorInfoW(monitor, &mut info).as_bool() {
            return false;
        }
        let m = info.rcMonitor;
        let style = GetWindowLongPtrW(hwnd, GWL_STYLE) as u32;
        looks_fullscreen(
            (rect.left, rect.top, rect.right, rect.bottom),
            (m.left, m.top, m.right, m.bottom),
            style,
        )
    }
}

#[cfg(not(windows))]
pub fn foreground_is_fullscreen() -> bool {
    false
}

/// Records the foreground window unless it is one of ours.
#[cfg(windows)]
pub fn remember_foreground(state: &crate::state::AppState) {
    use std::sync::atomic::Ordering;
    use windows::Win32::{
        System::Threading::GetCurrentProcessId,
        UI::WindowsAndMessaging::{GetForegroundWindow, GetWindowThreadProcessId},
    };
    unsafe {
        let hwnd = GetForegroundWindow();
        if hwnd.0.is_null() {
            return;
        }
        let mut pid = 0u32;
        GetWindowThreadProcessId(hwnd, Some(&mut pid));
        if pid != GetCurrentProcessId() {
            state
                .prev_foreground
                .store(hwnd.0 as isize, Ordering::SeqCst);
        }
    }
}

#[cfg(windows)]
pub fn restore_foreground(state: &crate::state::AppState) {
    use std::sync::atomic::Ordering;
    use windows::Win32::{
        Foundation::HWND,
        UI::WindowsAndMessaging::{IsWindow, SetForegroundWindow},
    };
    let raw = state.prev_foreground.load(Ordering::SeqCst);
    if raw == 0 {
        return;
    }
    unsafe {
        let hwnd = HWND(raw as *mut _);
        if IsWindow(Some(hwnd)).as_bool() {
            let _ = SetForegroundWindow(hwnd);
        }
    }
}

#[cfg(not(windows))]
pub fn remember_foreground(_state: &crate::state::AppState) {}

#[cfg(not(windows))]
pub fn restore_foreground(_state: &crate::state::AppState) {}

/// Minutes after midnight in the user's local time.
#[cfg(windows)]
pub fn local_minutes_of_day() -> u32 {
    use windows::Win32::System::SystemInformation::GetLocalTime;
    let t = unsafe { GetLocalTime() };
    u32::from(t.wHour) * 60 + u32::from(t.wMinute)
}

#[cfg(not(windows))]
pub fn local_minutes_of_day() -> u32 {
    let secs = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_secs());
    ((secs / 60) % (24 * 60)) as u32
}

#[cfg(test)]
mod tests {
    use super::*;

    const MONITOR: Rect = (0, 0, 2560, 1440);

    #[test]
    fn a_borderless_window_over_the_whole_monitor_is_fullscreen() {
        assert!(looks_fullscreen((0, 0, 2560, 1440), MONITOR, 0x9000_0000));
        // Exclusive fullscreen windows are sometimes slightly oversized.
        assert!(looks_fullscreen((-1, -1, 2561, 1441), MONITOR, 0));
    }

    #[test]
    fn a_maximized_window_with_a_title_bar_is_not() {
        assert!(!looks_fullscreen((0, 0, 2560, 1440), MONITOR, 0x16CF_0000));
    }

    #[test]
    fn a_window_that_does_not_reach_every_edge_is_not() {
        assert!(!looks_fullscreen((0, 0, 2560, 1400), MONITOR, 0));
        assert!(!looks_fullscreen((100, 100, 1000, 800), MONITOR, 0));
    }

    #[test]
    fn a_window_on_a_second_monitor() {
        let second: Rect = (2560, 0, 4480, 1080);
        assert!(looks_fullscreen((2560, 0, 4480, 1080), second, 0));
        assert!(!looks_fullscreen((0, 0, 2560, 1440), second, 0));
    }
}
