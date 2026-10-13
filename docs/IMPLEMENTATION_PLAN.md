# Frosty: Implementation Plan

Status: Draft v0.2 (Windows 11 only, spec fixes applied)
Date: 2026-10-14
Source docs: PROJECT_SPEC.md (v0.5), DESIGN_SPEC.md (v0.5)

This plan turns the two specs into an ordered build sequence. References like `P 7.4` and `D 5.2` point to sections of PROJECT_SPEC.md and DESIGN_SPEC.md. The specs and this plan agree as of v0.5; section 3 records what was changed in the specs to get there.

How to use it: work top to bottom. Each milestone has a task list and an exit check. Tick tasks as they land. Do not start a phase until the previous phase's exit check passes.

---

## Progress

Updated 2026-10-14. "Built" means code exists and automated checks pass. "Verified" means it was run on the dev machine (Windows 11, 175 percent scaling).

| Milestone | State | Notes |
|-----------|-------|-------|
| M0.1 Repo and tooling | Done, except CI never ran | Needs a push to GitHub to prove `ci.yml`. |
| M0.2 Spike A (overlay) | Mostly done | Region click-through verified, 0.0 to 0.3 percent CPU. Fullscreen detection needs a different approach (see docs/spike-findings.md). Real game, other DPI levels, second monitor, focus: not tested. |
| M0.3 Spike B (character) | Not started | Placeholder fox is CSS and SVG. Rive versus sprites is still open (decision 1). |
| M0.4 Spike C (summon latency) | Done (debug build) | 50 of 50 summons, median 49 ms, p95 57 ms, cold 108 ms. Release build not yet measured. |
| M0.5 Spike D (capability loading) | Done | Process model confirmed, CSP blocks network and remote code. 90 s idle only; hidden webview costs about 90 MB. |
| M1.1 Core crate foundations | Built | Config, events, database, home. 42 unit tests. Rotating log files are not written yet. |
| M1.2 Windows, tray, hotkeys | Built, partly verified | App launches and survives a broken config. Hotkeys, tray menu, drag, snap, and multi-monitor placement not yet exercised by hand. Position memory is one global position, not per monitor. |
| M1.3 Character package | Built (placeholder) | All states exist as CSS animation. No Rive, no skin loader, no skin contract validation. |
| M1.4 Personality engine | Done | Cooldowns, levels, tone, packs, overrides, tests. |
| M1.5 Themes | Built | Frost and Cartridge, AA check in tests. Mode fallback is shown. Only two themes; accent override and theme editor are Phase 5. |
| M1.6 Shell UI | Built, not verified by hand | Overlay, dock, chip, fullscreen shell, settings, debug panel. No component tests beyond theme mapping. Keyboard arrow navigation in the dock and the hotkey recorder are missing. |

Phase 1 exit check is not met yet: it needs the hand tests listed in section 6.

## 1. Scope

| Track | Phases | Target |
|-------|--------|--------|
| v1 (this plan, detailed) | 0 to 5 | Windows 11 desktop app with Tasks plus one more capability |
| Later (outlined only) | 6, 7, triggered Python phase | Android, more capabilities, adapters, sync |

Platform: Windows 11 only (P D23). Windows 10 is not built for or tested. Nothing should knowingly rule it out, but no milestone waits on it.

Sizes used below are relative effort for one person, not dates: S (a day or two), M (about a week), L (two weeks or more).

## 2. Decisions needed from the owner

These are open in the specs. Each has a default so work is never blocked, but the first two change real work if they flip later.

| # | Decision | Needed by | Default if undecided | Cost of changing later |
|---|----------|-----------|----------------------|------------------------|
| 1 | Fox art style: pixel or smooth (P 21.1, D 17.1). Picks Rive versus sprite sheets. | Start of M1.3 | Rive placeholder as D 2.3 says, behind a renderer interface that also has a sprite-sheet driver stub | Medium. The placeholder skin is redone; the contract and shell are not. |
| 2 | Process model for capability logic (P 8.10, D29) | End of Phase 0 | Hidden logic host webview, as now written in the spec | High after Phase 2, so Spike D confirms it or switches to the fallback |
| 3 | Second theme for Phase 1 (P 21.5, D 17.2) | M1.5 | Cartridge, because it has seed values and exercises hard shadows and zero radius | Low |
| 4 | Pixel display font (D 17.3) | M1.5 | Pixelify Sans | Low |
| 5 | Default tone (P 21.4) | M1.4 | `casual` | Low |
| 6 | Second capability for Phase 4 | Start of Phase 4 | Guitar log (duration and streak data, very unlike Tasks) | Low |
| 7 | Eye follow, sound, idle sleepy, home default (D 17.4 to 17.7) | Phase 5 | Eye follow off, silent, sleepy on, open on home | Low |
| 8 | Repo name. P 8 says `frosty-companion`; the repo is currently `frosty_v1`. | Phase 0 | Keep the folder, name the packages and app `frosty-companion` | Low |

## 3. Spec fixes applied in v0.5

The first draft of this plan found gaps and contradictions in the specs. They are now fixed in the specs themselves. This table is the record.

| # | Issue | Resolution | Now in |
|---|-------|------------|--------|
| G1 | Capability config path was given two ways | `capability-config/<id>.toml`. Built-ins live in the read-only app bundle, so config cannot sit inside the capability folder. | P 7.1, P 8, P 8.7, D30 |
| G2 | Built-ins ship in the app bundle but discovery only mentioned the Frosty Home folder | Two discovery roots. A user folder with a built-in's id is rejected. | P 7.9, P 8.8, D32 |
| G3 | Enabled flag and permissions were stored in both `frosty.toml` and `capability_state` | `frosty.toml` is the only store. `capability_state` keeps runtime facts. | P 8.6, P 10.2, D31 |
| G4 | Manifest `[[tools]]` had no schema fields | Added `input_schema`, `output_schema`, and `quick_action_control`. | P 8.1, P 8.5 |
| G5 | Manifest reaction intensity was a word; the engine takes 0 to 1 | A number or a named level: `small` 0.3, `medium` 0.5, `large` 0.8. | P 8.1 |
| G6 | Skin contract had no inputs for `bye`, `mode_expand`, `mode_collapse` | Added a `bye` trigger and a `mini` boolean. | D 2.5 |
| G7 | The SDK example assumed logic and views share one JS context | Separate logic and views entries, client hooks for views, badges pushed as values. | P 8.2, P 8.10, D 7.2, D29 |
| G8 | Capability code could bypass Rust permission checks with a direct `fetch` | CSP blocks direct network access. `ctx.host.network` goes through Rust. | P 8.6, D33 |
| G9 | CI was specified for three operating systems | Full pipeline on Windows, tests only on Linux. | P 18, D34 |
| G10 | Phase 1 needs the `kv` table, but storage was scheduled for Phase 2 | Core database lands in Phase 1. | P 19 |
| G11 | SDK v1 exposes scheduler and notify before they were due to exist | Scheduler core and minimal notify in Phase 2, the rest in Phase 3. | P 19 |
| G13 | Windows 10 was a required target | Windows 11 only for now. | P 12.1, P 19, D14, D23, D 17 |

Still to do:

| # | Issue | Resolution | When |
|---|-------|------------|------|
| G12 | Specs sit at the repo root; the layout in P 16 puts them in `docs/` | Move both specs and this plan to `docs/` | M0.1 |

## 4. Architecture notes

These are now in the specs (P 8.2, P 8.6, P 8.10). They are repeated here with the build detail, because they shape Phase 0 and Phase 2 and are verified by Spike D.

### 4.1 Where capability logic runs

Problem: there are two windows (overlay and main), each with its own webview and JS context. Capability logic must also run when both are hidden, because a scheduler job can fire a tool call at any time.

Recommendation: three webviews in one WebView2 browser process.

| Webview | Visible | Runs |
|---------|---------|------|
| Logic host | Never | Each enabled capability's `onActivate`, tool handlers, event subscriptions |
| Overlay | Compact mode | Fox, dock, card, speech chip, quick actions |
| Main | Fullscreen mode | Rail, home, pages, settings, module manager |

Rules that follow:
- A tool call always goes view to Rust to logic host and back. Rust checks permissions on the way through.
- A capability has two entries: `dist/index.js` with `defineCapability({ id, onActivate, onDeactivate })`, loaded only in the logic host, and `dist/views.js` with `defineViews({...})`, loaded in the view windows. View code gets a small client (`useTool`, `useEvent`, `useQuery`) and no `ctx`.
- Dock badges are values: logic calls `ctx.views.setBadge(n)`, Rust stores the value and pushes it to both windows.
- Mode switching carries state for free, because state lives in Rust and the logic host, not in either view window.
- This is exactly the shape a Python backend needs later: the logic host is "runtime adapter 1" behind a Rust trait, and a sidecar becomes "runtime adapter 2" (P 8.9).

Alternative if Spike D shows a hidden webview is unreliable (throttled or suspended timers and message handling): run logic in the main window's webview and never destroy that window. Simpler, but the main window then cannot be closed, only hidden.

### 4.2 Loading capability code

- Capability bundles are ES modules served from a custom Tauri URI scheme (`frosty-cap://<id>/dist/index.js`) that only reads from the two discovery roots.
- `react`, `react-dom`, `@frosty/sdk`, and `@frosty/ui` are externals. The shell supplies them through an import map so every capability shares one React instance and one copy of the design tokens.
- The scaffold tool generates a Vite library-mode config with those externals already set.

### 4.3 Permission enforcement

- Every `ctx` call is a Tauri command carrying the capability id. Rust checks the grant in `frosty.toml` before acting.
- The capability id is asserted by the SDK in v1. This is acceptable under the trusted-code model (P 8.6) and is documented as such.
- CSP for all three webviews: `connect-src` limited to the IPC and capability schemes, no remote scripts. Network access exists only as `ctx.host.network.fetch`, performed by Rust against the manifest's host allowlist.
- Storage: `ctx.db` is a structured API (`insert`, `update`, `softDelete`, `get`, `query` with filters). Rust adds the `cap_<id>_` prefix and the sync columns itself. No raw SQL crosses the bridge in v1.

### 4.4 Click-through overlay

- The overlay window covers the work area of the monitor it is on. The frontend reports the hit rectangles of the fox, dock, card, and chip to Rust whenever they change.
- While the overlay is visible, a Rust task polls the cursor position (about 60 Hz) and flips `set_ignore_cursor_events` when the pointer enters or leaves a hit rectangle. Polling stops when the overlay hides.
- The window is created with no activation so it does not take focus until a card or field is clicked (D 11).

## 5. Phase 0: Foundations and spikes

Goal: prove the four risky things on Windows 11 before any product code. Spike code is throwaway and lives in `spikes/`.

### M0.1 Repo and tooling (S)

- [ ] Move the specs and this plan to `docs/` (G12).
- [ ] Install missing tools: `pnpm` and `just`. (Rust 1.97, Node 24, and WebView2 are already on the dev machine.)
- [ ] Create the layout from P 16: `apps/desktop`, `crates/frosty-core`, `crates/frosty-host`, `packages/sdk`, `packages/ui`, `packages/character`, `capabilities/`, `tools/`, `spikes/`.
- [ ] pnpm workspace, Cargo workspace, shared `tsconfig`, ESLint, Prettier, `rustfmt`, `clippy` with warnings as errors.
- [ ] `justfile`: `dev`, `build`, `test`, `lint`, `fmt`.
- [ ] GitHub Actions per G9: lint, test, and a Tauri build on Windows; tests only on Linux.
- [ ] Tauri v2 app that opens one empty window, built by CI.

### M0.2 Spike A: overlay window (M)

- [ ] Frameless, transparent, always-on-top window with no taskbar entry and no activation on show.
- [ ] Click-through by the method in 4.4. Measure CPU while polling.
- [ ] Mixed-DPI multi-monitor: correct size and position at 100, 125, 150, and 200 percent.
- [ ] Fullscreen detection with `SHQueryUserNotificationState`, checked against a borderless game, an exclusive fullscreen game, a fullscreen browser video, and a presentation.
- [ ] Record what the overlay does over each of those (hidden, visible, steals focus).

### M0.3 Spike B: character rendering (S)

- [ ] Rive state machine with three inputs (`summon`, `celebrate`, `intensity`) playing in the transparent window.
- [ ] One-hour comparison: a sprite sheet drawn to canvas with nearest-neighbour scaling at 125 and 150 percent.
- [ ] Measure idle CPU and GPU for both, and confirm loops stop when the window hides.

### M0.4 Spike C: summon latency (S)

- [ ] Global hotkey shows a pre-created hidden window. Timestamp the hotkey in Rust and the first painted frame in the webview.
- [ ] Target under 150 ms (P 17). Record the median and worst of 50 runs.

### M0.5 Spike D: capability loading and process model (M)

Added by this plan. It covers the risk in 4.1 and 4.2.

- [ ] Load an ES module from a folder outside the app bundle through a custom URI scheme under a strict CSP.
- [ ] That module renders a React component using the shell's React through an import map (hooks must work, proving a single React instance).
- [ ] Hidden logic host webview: register a tool there, call it from the overlay through Rust, get the result back.
- [ ] Leave the logic host hidden for an hour, then fire a call from a Rust timer. Confirm it responds promptly.
- [ ] Measure memory for three webviews against two.

### Phase 0 exit check

- [ ] Spikes A to D pass on Windows 11, or each failure has a written workaround.
- [ ] `docs/spike-findings.md` records measurements, blockers, and the final answers to decisions 1 and 2.
- [ ] CI is green.

## 6. Phase 1: The shell

Goal: a fox you can summon, dismiss, and watch react, with no capabilities at all.

### M1.1 Core crate foundations (M)

- [ ] `frosty-core`: Frosty Home resolution and first-run folder creation (P 9.1).
- [ ] Config service v0: typed `frosty.toml` model, validation with line and column errors, last-known-good fallback, file watcher with debounce, hot-reload event to all webviews.
- [ ] Config writes through `toml_edit`, preserving comments and unknown keys, with a backup before each UI-driven write (keep the last 10).
- [ ] `config_version` and a migration hook, even though there is only version 1.
- [ ] Core database: SQLite open, backup-before-migrate, forward-only migration runner, tables `device`, `kv`, `migrations` (G10).
- [ ] Event bus: typed publish and subscribe, namespaced names, bounded in-memory recent log, bridge to the webviews.
- [ ] Rotating local logs.
- [ ] Tests: config round-trip keeps comments, invalid TOML never panics (start the fuzz test here), migration runner, bus delivery.

### M1.2 Windows, tray, hotkeys (M)

Depends on M1.1 and Spike A.

- [ ] Window manager: overlay and main windows created at startup and hidden, never destroyed.
- [ ] Click-through controller from 4.4, promoted from the spike.
- [ ] Summon on the monitor under the cursor, at the remembered position for that monitor (default bottom right, 24 px margin). Positions stored in `kv`.
- [ ] Drag by the fox, optional snap within 48 px of an edge (D 3.1).
- [ ] Hotkey service: register summon and fullscreen from config, detect conflicts, report them as events, re-register on config change.
- [ ] Tray icon and menu: Summon, Open fullscreen, Do not disturb, Settings, Quit.
- [ ] Autostart from `core.start_on_login`. Single-instance guard.
- [ ] Core events: `core.summoned`, `core.dismissed`, `core.fullscreen_entered`, `core.fullscreen_exited`.

### M1.3 Character package (M)

Depends on decision 1 and Spike B.

- [ ] `packages/character`: a `CharacterRenderer` interface with the skin contract inputs from D 2.5 (plus G6). One driver for the chosen tech, a stub for the other.
- [ ] Skin loader: `skin.toml`, validation of required inputs, fallback to the default skin with a clear error.
- [ ] Placeholder fox with `idle_breathe`, `idle_blink`, `summon_in`, `dismiss_out`, `wave`, `happy_small`, `celebrate`, `curious_tilt`, `nudge`, `oops`, `sleepy`, `thinking`, each honouring `intensity`.
- [ ] Idle scheduler for the random loops (D 2.4). Everything pauses when `visible` is false.
- [ ] Static pose per state for reduced motion.
- [ ] `FoxView` component in three sizes (112, 160, 224) plus the 56 px rail size, crisp at every DPI in D 11.

### M1.4 Personality engine v0 (S)

- [ ] Rust engine: event in, `{ animation, intensity, microcopy }` out.
- [ ] Resolution order from P 11.2 (user override, manifest reaction, core default, none). The manifest step is an empty table until Phase 2.
- [ ] Cooldowns (one per 2 s, one celebrate per 10 s, excess dropped) and reaction level `full`, `subtle`, `off`.
- [ ] Microcopy packs: JSON per locale and tone, weighted variants, `{placeholder}` substitution, no immediate repeats, user overrides from `<frosty-home>/microcopy`.
- [ ] Core pack for `en` casual and neutral with the keys from D 8.4.
- [ ] Tests driven by a fake clock: cooldown, level filtering, resolution order, variant choice.

### M1.5 Design tokens and themes (M)

- [ ] `packages/ui` tokens as CSS custom properties: color (semantic set from D 13.2), type scale, spacing, radius, elevation, motion (D 9).
- [ ] Theme loader in Rust: parse theme TOML, reject a theme missing a required token and keep the previous one, watch the `themes/` folder.
- [ ] Optional tokens wired end to end: `radius-scale`, `shadow-style`, `border-width`, `surface-style`, `font-display`.
- [ ] Built-in themes: Frost (light and dark) and the theme from decision 3.
- [ ] Mode `system`, `light`, `dark`, with fallback when a theme lacks the requested mode.
- [ ] Reduced motion: follows the OS, overridable, switches the character to static poses and motion tokens to short fades.
- [ ] A contrast check script run in CI over every built-in theme (AA).

### M1.6 Shell UI (L)

- [ ] First `@frosty/ui` components: Button, IconButton, Toggle, TextField, Select, Tooltip, Toast, Banner, Badge, Skeleton, Stack, Card, SpeechChip, Dock, DockItem, ModuleCard, Rail, TopBar. Each with keyboard behaviour and labels documented (D 10).
- [ ] Compact mode: fox, empty dock with the "+" button, speech chip, summon and dismiss animation with dock fan-out (D 4.1), dock side chosen by screen edge (D 5.1).
- [ ] Summon rules: summoning while visible focuses the dock; Esc dismisses when nothing is focused.
- [ ] Fullscreen mode: rail with the mini fox, top bar, empty home with the `curious_tilt` state, minimum size 900 x 600 (D 5.5).
- [ ] Mode switch with `mode_expand` and `mode_collapse`.
- [ ] Settings pages backed by config: Frosty, Hotkeys (with KeybindingRecorder and conflict text), Appearance (theme picker with live preview and cancel), Data (paths, open folder, open config file), About. Each control shows its config key on hover (D 6).
- [ ] Debug panel (dev builds and a config flag): fire any event with a payload and significance, see the engine's decision and why a reaction was dropped.
- [ ] Accessibility pass: focus rings, tab order, polite live region for reaction text, fox hidden from screen readers (D 12).

### Phase 1 exit check (P 19)

- [ ] Summon and dismiss work 50 times in a row by hotkey and by tray.
- [ ] Compact and fullscreen switch without losing state.
- [ ] Fake events from the debug panel produce the reactions in D 8.1, and cooldowns hold.
- [ ] Hand-editing `frosty.toml` (tone, theme, hotkey, size) applies live; a broken edit shows an error and keeps the last good config.
- [ ] Summon latency and hidden-state CPU are measured and recorded.

## 7. Phase 2: Plugin architecture

Goal: add a capability without touching core code.

### M2.1 Manifest and host (L)

- [ ] `frosty-host`: `capability.toml` model and validator (schema, `api_version`, id uniqueness, referenced files exist), with G4 and G5 applied.
- [ ] Discovery over both roots (G2), folder watching, state machine `discovered`, `disabled`, `enabled`, `error`.
- [ ] `RuntimeAdapter` trait with one implementation, the TypeScript logic host from 4.1. The `runtime` field is parsed; anything but `ts` gives a clear "not supported yet" error.
- [ ] Lifecycle: enable (grant, migrate, activate), disable (deactivate, unregister tools and views, cancel jobs, keep data), remove (optional data delete). Built-ins cannot be removed.
- [ ] Failure isolation: activation timeout, error captured into `capability_state`, per-capability error counter, shell unaffected.
- [ ] Permission store in `frosty.toml` and the enforcement layer from 4.3. Shell permission needs a second confirmation.
- [ ] Manifest reactions and microcopy packs registered into the personality engine.

### M2.2 Storage, tools, events, scheduler core (L)

- [ ] Per-capability migrations: SQL files applied in a transaction, table names forced under `cap_<id>_`, failure rolls back and disables only that capability.
- [ ] `ctx.db` structured API. Rust fills `id` (UUIDv7), `created_at`, `updated_at`, `deleted_at`, and `device_id`; deletes are soft.
- [ ] Tool registry: register, input and output validation against JSON Schema, routing through Rust, `tools.call:<id>` permission for cross-capability calls.
- [ ] Event emit with payload validation, and subscribe gated by `events_subscribe` namespaces.
- [ ] Scheduler core: `scheduler_jobs` table, `at` and `every`, fires an event or a tool call, survives restart. Missed-job policies come in Phase 3 (G11).
- [ ] Minimal `ctx.notify`: shows a speech chip and writes a `notifications` row.
- [ ] `capability_state`, `scheduler_jobs`, `notifications`, and `event_log` core migrations.

### M2.3 SDK v1 (M)

- [ ] `@frosty/sdk`: `defineCapability`, the `ctx` surface from P 8.2 (`db`, `events`, `tools`, `views`, `scheduler`, `notify`, `config`, `log`, gated `host.*`), every call async.
- [ ] View side: slot declarations for `dock`, `card`, `page`, `widget`, `quick-action`, `settings`, and the client hooks from 4.1.
- [ ] Slot rendering in both windows, each view wrapped in an error boundary that shows the crashed-capability panel from D 14.
- [ ] Shell import map and URI scheme from 4.2.
- [ ] Types generated from the same JSON Schemas the Rust validator uses, so the manifest has one definition.

### M2.4 Settings forms and module manager (L)

- [ ] `SettingsForm` generated from JSON Schema with `x-frosty-widget` hints, writing `capability-config/<id>.toml` (G1), hot-reloading on hand edits.
- [ ] Field components for all v1 types (P 8.7): toggle, text, number, select, multi-select, time, date, color, path picker, list editor, keybinding recorder.
- [ ] Module manager (D 4.3): rows with status chip, runtime badge, toggle, settings, overflow menu (open config file, reveal folder, view permissions, remove), error rows with Copy details, Install from folder.
- [ ] `PermissionSheet` with plain-language lines for each permission.
- [ ] Dock shows enabled capabilities in `dock_order`, badges, overflow after six, card open and close with focus handling (D 5.1, D 5.2, D 12). Card size remembered per capability.

### M2.5 Scaffold, fixture, contract tests (M)

- [ ] `tools/scaffold-capability`: generates manifest, settings schema, a migration, a microcopy pack, logic and views entries, icon, and the build config.
- [ ] `hello` capability built only from the scaffold output, used as the fixture.
- [ ] Contract test suite that drives every SDK surface through the fixture.
- [ ] Broken fixtures: bad manifest, throwing `onActivate`, failing migration, crashing view. Each must land in `error` with a readable message.
- [ ] `docs/capability-authoring.md`, first draft.

### Phase 2 exit check (P 19)

- [ ] Scaffold a capability, drop it in the folder, enable it, see its dock icon, card, and settings form, disable it, with zero core changes.
- [ ] A search of `apps/` and `crates/` finds no capability id other than in tests.
- [ ] Each broken fixture shows an error and the shell keeps running.

## 8. Phase 3: Tasks and reminders

Goal: the first capability you actually live with.

### M3.1 Scheduler and notification center, complete (L)

- [ ] Missed-job policies `fire_once_on_start`, `skip`, `fire_all`. Recurrence rules for daily, weekly, and interval repeats.
- [ ] Notification center rules: do-not-disturb window, per-capability mute, rate limiting, history with read and dismissed state.
- [ ] Fullscreen wait: hold reminders while a fullscreen app is in front, release them on exit (D25), using the detector from Spike A.
- [ ] Nudge presentation (D 4.5): minimal overlay with fox and chip, up to two actions wired to tool calls, 20 second default timeout, then history.
- [ ] OS notification fallback when the overlay cannot show and `os_fallback` is on.
- [ ] Confirmed: the app stays alive in the tray with all windows hidden and reminders still fire.

### M3.2 Tasks capability (L)

Built in `capabilities/tasks` with the SDK only.

- [ ] Data: tasks with title, notes, due date and time, recurrence, completed state.
- [ ] Tools: `tasks.add`, `tasks.complete`, `tasks.uncomplete`, `tasks.update`, `tasks.delete`, `tasks.snooze`, `tasks.list`.
- [ ] Events: `tasks.created`, `tasks.completed`, `tasks.due`, `tasks.streak`.
- [ ] Views: dock icon with open-count badge, card (today, upcoming, add row), quick action (one-line add), settings from schema. Loading, empty, and error states for each (D 14).
- [ ] Reminders through `ctx.scheduler` and `ctx.notify`, with Done and Snooze actions.
- [ ] Streak logic from its own data, emitting significance so the celebrate scales.
- [ ] Destructive actions use an undo toast (D 7.2 rule 8).
- [ ] Missing `@frosty/ui` pieces added as needed: Checkbox, List, ListItem, DatePicker, TimePicker, QuickActionTray, StreakBadge.

### M3.3 End-to-end test and dogfood (M)

- [ ] Scripted end-to-end test with `tauri-driver`: summon, open Tasks, add an item, complete it, reaction fires, dismiss.
- [ ] One week of daily personal use. Log every SDK friction point in `docs/sdk-friction.md`.

### Phase 3 exit check (P 19)

- [ ] A week of real use completed.
- [ ] Reminders fired while Frosty was hidden, across an app restart, and after sleep and wake.
- [ ] Core contains no Tasks special case.

## 9. Phase 4: Second capability and home dashboard

### M4.1 Second capability (L)

- [ ] Build the capability from decision 6 from the scaffold, with a `page` view and at least one `widget`.
- [ ] Every SDK change it forces is recorded as additive or breaking.

### M4.2 Home dashboard (L)

- [ ] Widget grid (D 5.6): 1x1, 2x1, 2x2, row height 160, `WidgetFrame`, title opens the capability page.
- [ ] Drag to reorder and resize, saved to config.
- [ ] Core widgets: today's notifications, recent activity, streaks.
- [ ] Page slot with the optional 360 px right panel; `page` falls back to `card` when absent.
- [ ] Expand from card to page carrying selection and scroll state (P 12.3).
- [ ] Persisted `event_log` with a size bound, used by recent activity and streaks.
- [ ] One cross-capability interaction through events or a permitted tool call, to prove the path.

### M4.3 SDK review and freeze (M)

- [ ] Work through `docs/sdk-friction.md`. Fix what hurt in both capabilities.
- [ ] Freeze `api_version = 1`. Contract tests pinned to it.
- [ ] Update the authoring guide and the scaffold.

### Phase 4 exit check (P 19)

- [ ] Two capabilities with different data shapes, both built without core changes.
- [ ] SDK changes after Tasks were additive only, or the breaking ones are listed with the reason.

## 10. Phase 5: Onboarding and polish

- [ ] **M5.1 Onboarding (M):** the seven steps in D 4.2, written to `frosty.toml`, skippable, re-runnable from settings, including the tray pinning hint.
- [ ] **M5.2 Notification center UI (S):** bell in the top bar, history list, do-not-disturb controls, per-capability mute, chip duration setting.
- [ ] **M5.3 Data (M):** JSON export and import per capability and for the whole device, scheduled database backups, restore from backup, diagnostics copy button.
- [ ] **M5.4 Themes (M):** remaining built-in themes (Frost Pixel, Snowfield, High Contrast), accent override with AA check, font scale, then the theme editor with live preview and contrast warnings.
- [ ] **M5.5 Real character art (L, depends on the artist's timeline):** final skin dropped in through the contract with no code change. If that is not true, fix the contract.
- [ ] **M5.6 Hardening (M):** performance pass against the P 17 targets, full accessibility audit against D 12, text scaling to 200 percent, installer tested on a clean Windows 11 install.

### v1 exit check

- [ ] A clean install on Windows 11 goes from installer to a working reminder through onboarding alone.
- [ ] All P 17 performance numbers are measured and written down.
- [ ] Nothing left in the "Still to do" table in section 3.

## 11. Later phases (outline)

| Phase | First step | Notes |
|-------|-----------|-------|
| 6 Android | Tauri Android hello world on the phone | Shares `packages/ui`. Card becomes a bottom sheet, scheduler becomes OS local notifications. The logic host from 4.1 needs rethinking for mobile background limits. |
| 7 Expansion | Calendar capability, then the first adapter | Keychain API (P 9.3) is built with the first adapter, not before. Sync, ambient awareness, and mirror tone each need their own spec. |
| Triggered: Python backends | A capability that truly needs a Python library | Second `RuntimeAdapter` implementation. The trait from M2.1 is the seam. |

## 12. Standing rules for every milestone

1. Rust services get unit tests in the same change. UI-only changes get component tests for anything in `@frosty/ui`.
2. No capability id in `apps/` or `crates/`. CI greps for the known ids.
3. Every new setting lands in three places at once: the config model, the settings UI, and the example `frosty.toml` in the docs.
4. Every user-visible string goes in a microcopy or locale file.
5. Components use tokens only. A lint rule rejects hex colors and raw pixel font sizes in `packages/ui` and `capabilities/`.
6. When a spec turns out to be wrong, update the spec and its decision log in the same change.

## 13. Immediate next steps

1. Decide the fox art style (decision 1 in section 2). It is not needed until M1.3, so Phase 0 can start without it.
2. Start M0.1.
