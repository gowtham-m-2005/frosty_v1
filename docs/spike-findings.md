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

### Idle CPU of the polling click-through: too high

Measured over about 90 seconds with the overlay visible over a fullscreen video, polling at 60 Hz: 1.9 to 5.5 percent of one core, process only (the WebView2 child processes are not included). Worst single iteration was 20 to 29 ms, which would drop a frame. The target in PROJECT_SPEC.md 17 is "near zero CPU" while idle.

Recommended replacement, to be tested in the next spike iteration: no polling. Set the window region with `SetWindowRgn` to the union of the fox, dock, card, and chip rectangles whenever the page reports them. Windows then sends clicks outside the region to the window behind with zero per-frame work. Caveats to check: soft shadows and anti-aliased edges are clipped at the region edge (fine for the pixel themes, which have hard edges and hard shadows), and the region must update in step with animation (the card scale-in, the fox hop).

### Still open

- **Real borderless game and exclusive fullscreen game:** only a fullscreen video was tried.
- **Region-based click-through** (above): not built yet.
- **Other scale factors** (100, 125, 150, 200 percent) and a second monitor: not tested.
- **Focus behaviour:** not tested. Need to confirm the window does not take keyboard focus on show and that clicking a box does not steal focus from the previous app.

## Spike B: character rendering

Not started.

## Spike C: summon latency

Not started.

## Spike D: capability loading and process model

Not started.
