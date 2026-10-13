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

### Open items

- **Fullscreen detection is not verified.** `SHQueryUserNotificationState` returned `BUSY (fullscreen app)` while no game or video was running. The likely cause is the overlay itself: it covers the whole 2560x1440 monitor, and Windows treats a full-monitor window as a fullscreen app. If so, the call cannot be used as is, because Frosty would see itself as a fullscreen app and hold its own reminders forever. To settle it: run the spike with the overlay hidden, then with it visible, and compare. Fallbacks if the overlay does trigger it: make the overlay one pixel smaller than the monitor, or detect fullscreen by comparing the foreground window rectangle to its monitor and excluding Frosty's own windows.
- **Behaviour over real fullscreen apps** (borderless game, exclusive fullscreen, fullscreen video): not tested. Needs a manual run with a game or video.
- **Idle CPU of the polling loop:** not measured. The self-test exits before the first 10 second report. Needs a longer run without `--selftest`.
- **Other scale factors** (100, 125, 150, 200 percent) and a second monitor: not tested.
- **Focus behaviour:** not tested. Need to confirm the window does not take keyboard focus on show and that clicking a box does not break the previous app's focus.

## Spike B: character rendering

Not started.

## Spike C: summon latency

Not started.

## Spike D: capability loading and process model

Not started.
