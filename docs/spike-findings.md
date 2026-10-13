# Phase 0 spike findings

Test machine: Windows 11 Home, 2560x1440 display at 175 percent scaling.

## Spike A: overlay window (spikes/overlay)

Status: partly done. Click-through is proven. Fullscreen detection, other DPI levels, and CPU cost are not yet measured.

### What was built

A frameless, transparent, always-on-top window sized to the monitor work area, with no taskbar entry and created unfocused. A Rust thread polls the cursor every 16 ms and toggles `set_ignore_cursor_events` when the pointer enters or leaves the fox, dock, or card rectangles reported by the page. This is the method in IMPLEMENTATION_PLAN.md 4.4.

### Results

| Check | Result |
|-------|--------|
| Transparent, frameless, always-on-top window | Works |
| Pointer over fox, dock, and card reaches the overlay | Pass (`WindowFromPoint` from a scripted self-test) |
| Pointer over a transparent area goes through to the window behind | Pass |
| Correct at a non-integer scale (175 percent) | Pass. Hit rectangles are in logical pixels and scaled by the window scale factor. |

Run it with `spikes/overlay/target/debug/spike-overlay.exe --selftest` (moves the cursor for about 5 seconds, then exits and restores it).

### Fullscreen detection: the overlay triggers it itself (confirmed)

| Run | `SHQueryUserNotificationState` |
|-----|--------------------------------|
| Overlay running, no fullscreen app (self-test) | `BUSY` |
| No overlay window, probe only (`--probe`), nothing fullscreen | `ACCEPTS_NOTIFICATIONS` |
| Overlay running over a fullscreen video | `BUSY`, with one short `APP` blip |

The overlay covers the whole monitor, so Windows reports a fullscreen app because of Frosty's own window. As written, the call cannot tell a real fullscreen app from the overlay. The overlay drawing on top of a fullscreen video also works, so hiding is entirely Frosty's job.

Decision needed in Phase 3 (not blocking Phase 1). Options, in order of preference:
1. Do not ask the OS. Compare the foreground window's rectangle to its monitor, excluding Frosty's own windows. Needs a short test over a borderless game and a fullscreen video.
2. Query `SHQueryUserNotificationState` only while the overlay is hidden, and remember the answer for as long as it stays visible. Cheap, but it cannot notice a game launching while Frosty is up.

### Click-through method: use the window region, not polling (confirmed)

| Method | Idle process CPU (overlay visible, over a fullscreen video) |
|--------|--------------------------------------------------------------|
| Poll the cursor at 60 Hz and toggle ignore-cursor-events | 1.9 to 5.5 percent of one core, with 20 to 29 ms hiccups |
| `SetWindowRgn` to the union of the hit rectangles, no polling | 0.0 to 0.3 percent of one core |

Both methods pass the same self-test (pointer reaches the fox, dock, and card, and passes through transparent areas at 175 percent scaling). Numbers cover the main process only, not the WebView2 child processes, so the real total is higher for both, but the polling cost is gone.

Decision: the window manager uses `SetWindowRgn`. Caveats to handle in Phase 1: soft shadows and anti-aliased edges are clipped at the region edge (fine for the pixel themes, which have hard edges), and the region has to follow animated elements (the card scale-in, the fox hop), so the page must report rectangles during animation or the region must be padded for the animation's extent.

### Still open

- **Real borderless game and exclusive fullscreen game:** only a fullscreen video was tried.
- **Region following animation:** the self-test used static boxes. Needs a check with the fox hop and card scale-in.
- **Other scale factors** (100, 125, 150, 200 percent) and a second monitor: not tested.
- **Focus behaviour:** not tested. Need to confirm the window does not take keyboard focus on show and that clicking a box does not steal focus from the previous app.

## Spike B: character rendering

Not started.

## Spike C: summon latency

Status: done for the debug build. A release build measurement is still worth doing before the Phase 1 sign-off.

Method: `spikes/latency/measure.ps1` starts the app, sends the real global hotkey (Ctrl+Alt+F) through `keybd_event` 50 times with a dismiss in between, and reads `%APPDATA%/frosty/logs/summon-latency.log`. Each sample runs from the start of the summon handler in Rust to the second animation frame painted by the overlay after it was shown. The app logs this itself, so any build can be measured the same way.

| Metric | Result (debug build, 2560x1440 at 175 percent) |
|--------|-----------------------------------------------|
| Summons logged | 50 of 50 |
| First (cold) | 108.1 ms |
| Median | 49.1 ms |
| 95th percentile | 56.6 ms |
| Best / worst | 43.0 ms / 108.1 ms |

Target in PROJECT_SPEC.md 17: under 150 ms. Met, including the cold first summon.

Caveats: the clock starts when the hotkey handler runs, not at the key press, so the keyboard and the global-hotkey hook add a few milliseconds that are not in these numbers. The timing is to the second painted frame, which includes the compositor delay for a transparent window.

## Spike D: capability loading and process model (spikes/capability)

Status: passes every check below. The idle test ran 90 seconds, not the hour in the plan, so a long soak is still open.

What was built: a Tauri app with a hidden "logic host" webview and a visible "view" webview. A custom URI scheme (`frosty-cap`) serves a capability folder that lives outside the app bundle. `react` and `react-dom/client` come from one import map shared by both windows. Run it with `spikes/capability/run.ps1`.

| Check | Result |
|-------|--------|
| Capability logic module loaded from a folder outside the bundle, over the custom scheme | Pass |
| Capability view module loaded the same way, rendered by the shell's React, `useState` from the capability works | Pass. One React instance, no invalid-hook error. |
| Tool call view to Rust core to hidden logic host and back | Pass. 30 calls: best 1.3 ms, median 1.7 ms, worst 3.7 ms |
| Unknown tool returns an error instead of hanging | Pass |
| Direct network `fetch` from capability code | Blocked by the CSP (`connect-src ipc:`) |
| Dynamic `import()` from an arbitrary origin | Blocked by the CSP |
| Request path traversal on the capability scheme (`..`, absolute paths) | Rejected (unit test) |
| Hidden logic host still answers with every window hidden, 90 s, ping every 5 s | 18 of 18 answered, 2.6 to 5.4 ms |
| Timers in the hidden host are not throttled | A 1 s timer ticked 94 times in about 94 s |

Memory (working set summed over the process tree, shared pages counted per process, debug build): 423 MB with the hidden logic host (9 processes) versus 335 MB without it (8 processes). The hidden webview costs roughly 90 MB. This is the figure to beat later; if it matters, the fallback in PROJECT_SPEC.md 8.10 (logic in the main window's webview) avoids it.

Conclusions:
- The three-webview model in PROJECT_SPEC.md 8.10 works as written. Decision D29 can move from proposed to agreed.
- G8 holds: with a strict CSP, a capability cannot reach the network or load remote code by itself.
- On Windows the scheme's origin is `http://frosty-cap.localhost`. The capability folder is served with `Access-Control-Allow-Origin: *`.

Not covered:
- The test CSP used `script-src 'unsafe-inline'` so the inline import map would run. The real app needs a hash or nonce for that inline script. Untested.
- Only a one-view, one-capability case. Multiple capabilities, capability errors thrown during activation, and teardown on disable are Phase 2 work.
- A real one-hour or overnight soak, and behaviour across sleep and wake.
- The logic host window was created hidden; it was not first shown and then hidden.
