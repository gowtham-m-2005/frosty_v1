# Frosty: Project Spec (Architecture)

Status: Draft v0.5 (Windows 11 only for now, plus fixes from the implementation plan review)
Date: 2026-10-14
Owner: Gowtham
Companion doc: DESIGN_SPEC.md (frontend and visual design)

Working title is "Frosty". There are two other projects already called Frosty (the air-gapped Tamil document app and the dev workflow CLI). To avoid confusion, the repo for this project should be named `frosty-companion`.

---

## 1. Summary

Frosty is a personal digital companion. It is an arctic fox character that lives on your devices, appears when you summon it, and gains abilities through pluggable capabilities (calendar, tasks, reading tracker, guitar log, and so on).

Frosty is not a chatbot and not an AI assistant. Its personality and reactions are designed and rule-based. Its capabilities are modular, and you decide which ones exist.

One line version: one companion, multiple devices, pluggable capabilities, your rules.

## 2. Goals and non-goals

### Goals

1. A character shell that feels alive: summon, react, dismiss, with polished animation.
2. A capability system where adding a new ability never requires touching the core codebase.
3. Everything configurable through a human-editable config file AND through the UI. Anything you can do in one, you can do in the other.
4. Native-first modules. Calendar, tasks, reading, and the rest are built as part of Frosty. External services (Google Calendar, Steam, Spotify) are optional adapters.
5. Local-first and private. Data lives on the device. No account required, no telemetry.
6. Windows desktop first, Android later. Each device is independent for now, but the data model must not block sync later.
7. Built for the owner first, open-sourceable later.

### Non-goals (for now)

- No conversational AI, no chat window, no LLM dependency in the core.
- No cross-device sync in v1 (designed for, not built).
- No plugin marketplace or remote plugin installation in v1.
- No ambient awareness (Frosty watching what you do) in v1. Reactions are triggered only by your interaction and by events from capabilities.
- No web-hosted version. Native only.
- No iOS target. Android is the only planned mobile platform, and it comes after the desktop app is solid.
- No tone mirroring in v1. Tone is a manual setting (casual or neutral). Mirror mode is parked for a later phase (see 7.6.2).

## 3. Principles

1. **Character first, capabilities second.** Frosty is still Frosty with zero capabilities installed.
2. **The shell never knows about specific capabilities.** Core code must not contain "if tasks" or "if guitar". Capabilities plug in through the same contract, including the built-in ones.
3. **Dogfood the plugin API.** First-party capabilities use exactly the SDK that third-party ones would. If the SDK is awkward, we fix the SDK.
4. **Config file is the source of truth.** The UI is a friendly editor over the same files.
5. **Fail soft.** A broken capability or invalid config line must never crash the shell. Frosty falls back to the last known good state and shows a clear error.
6. **Local-first.** Network access is opt-in per capability.
7. **Quiet by default.** Frosty reacts to what you do. It does not interrupt unless you set a reminder.

## 4. Glossary

| Term | Meaning |
|------|---------|
| Frosty Core | The always-present shell: character, windows, config, storage, events, scheduler, notifications, personality. Not a plugin. |
| Capability | A pluggable module that gives Frosty an ability (Tasks, Calendar, Guitar...). Also called a module in conversation. |
| Tool | A discrete action a capability exposes, such as `tasks.add` or `guitar.log_session`. Tools can be triggered from quick actions, hotkeys, other capabilities (with permission), and future automation. |
| View | A UI surface a capability provides: dock icon, compact card, fullscreen page, home widget, settings panel. |
| Slot | A named place where a view can be mounted (`dock`, `card`, `page`, `widget`, `quick-action`, `settings`). |
| Event | A typed message on the event bus, for example `tasks.completed`. |
| Reaction | A mapping from an event to a character animation plus optional microcopy. |
| Adapter | A piece of a capability that talks to an external service (Google Calendar adapter inside the Calendar capability). |
| Frosty Home | The per-device directory that holds config, data, and capabilities. |

## 5. System overview

```
+----------------------------------------------------------------+
| Frontend (WebView: React + TypeScript)                         |
|  Shell UI | Character renderer | Capability views and widgets  |
|            ^ Capability SDK (typed bridge) ^                    |
+------------|-----------------------------------|---------------+
             | Tauri commands + events           |
+------------v-----------------------------------v---------------+
| Frosty Core (Rust)                                             |
|  Config service | Storage service | Event bus | Scheduler      |
|  Notification center | Personality engine | Hotkey service     |
|  Window manager | Capability host (load, permissions, lifecycle)|
+----------------------------------------------------------------+
| OS: tray, global shortcuts, notifications, filesystem, clipboard|
+----------------------------------------------------------------+
```

Data flow in one sentence: a capability does something, emits an event, the event bus delivers it to the personality engine, which picks an animation and microcopy, and the character renderer plays it.

## 6. Technology choices

These are recommendations with reasons. Each is also recorded in the decision log (section 22) so they can be revisited.

| Area | Choice | Why |
|------|--------|-----|
| App framework | Tauri v2 | Native-feeling, small footprint, Rust core, supports Windows desktop now and Android later from one project, already familiar from the Tamil document app. |
| Core language | Rust | Tauri backend, good for scheduler, storage, and file watching. |
| Frontend | React + TypeScript + Vite | Familiar stack, large ecosystem, easy to type the capability SDK. Confirmed in review. |
| State management | Zustand for UI state, core is source of truth for data | Small, no boilerplate. |
| Character animation | Rive (state machine runtime) for a smooth vector fox, or sprite sheets if the fox becomes pixel art | Rive state machines map directly onto "play this reaction with this intensity". Sprite sheets suit pixel art. Decided after the art style question in DESIGN_SPEC.md section 17. Lottie is the fallback if Rive turns out to be a problem. |
| Storage | SQLite (via rusqlite or sqlx) | Local, reliable, single file per device, easy to export. |
| Config | TOML (`serde` + `toml_edit` to preserve comments when the UI writes) | Human-friendly, comments allowed, matches the Python/Rust ecosystem. |
| Capability runtime | TypeScript first, Python backends later on a real need | One language and one SDK for v1. Seams are kept so a Python process can plug in later (see 8.9). |
| Settings UI | Generated from JSON Schema | Capabilities declare settings once and get a UI for free. |
| IDs | UUIDv7 | Sortable by time and safe for future sync. |
| Packaging | Tauri bundler, per-OS installers | Standard. |

Platform plugins to use: global-shortcut, notification, autostart, fs, store or sql (if useful), updater (later), window-state.

## 7. Frosty Core services

Every service lives in Rust, is unit-testable without the UI, and exposes a narrow typed API to the frontend.

### 7.1 Config service

- Loads and validates `frosty.toml` and each `capability-config/<id>.toml`.
- Watches files. When a file changes on disk, validates it and hot-reloads. If invalid, keeps the last good config and surfaces an error with line and column.
- UI edits go through the same service and write back using `toml_edit` so user comments and formatting survive.
- Every config has a `config_version` and a migration path.

### 7.2 Storage service

- One SQLite database per device: `frosty.db`.
- Core tables are owned by the core. Each capability gets a namespaced set of tables (`cap_<id>_*`) created from its own migrations. A capability cannot touch another capability's tables.
- Capabilities access storage only through the SDK (`ctx.db`), never raw file access.
- Every syncable table carries `id` (UUIDv7), `created_at`, `updated_at`, `deleted_at` (soft delete), and `device_id`. See section 15.
- Export and import: full JSON export per capability and for the whole device.

### 7.3 Event bus

- In-process, typed, publish and subscribe.
- Event names are namespaced `<capability_id>.<event>` (for example `tasks.completed`, `guitar.session_logged`). Core events use the `core.` prefix (`core.summoned`, `core.dismissed`, `core.fullscreen_entered`).
- Payloads are JSON validated against the schema declared in the capability manifest.
- Capabilities can emit their own events and subscribe to others if they hold the `events.subscribe` permission for that namespace.
- A bounded recent-event log is kept in memory (and optionally persisted) for streaks and debugging.

### 7.3.1 Why events and tools instead of direct calls

Capabilities never import each other. They talk through events (loose, one to many) and tools (explicit, one to one, permission checked). This keeps every capability removable.

### 7.4 Scheduler

- Persistent job scheduler in Rust, backed by SQLite.
- Capabilities register one-shot and recurring jobs (`ctx.scheduler.at(...)`, `ctx.scheduler.every(...)`) that fire an event or tool call.
- Survives app restarts. Missed jobs while the app was closed are handled by a per-job policy: `fire_once_on_start`, `skip`, or `fire_all`.
- Needs a tray or background process on desktop so reminders can fire when Frosty is hidden. On mobile this becomes OS-scheduled local notifications.

### 7.5 Notification center

- Frosty delivers all reminders and alerts. A reminder shows up as Frosty appearing and nudging, not as a generic OS popup.
- Fallbacks: if the overlay cannot be shown (mobile background, full-screen app on some OS, window manager limits), the center falls back to a native OS notification.
- Keeps a history list (viewable in the fullscreen home) with read and dismissed state.
- Rules: do-not-disturb schedule, per-capability mute, rate limiting.
- Fullscreen apps: while a fullscreen app (a game or video) is in front, reminders do not appear. They wait in the history list and are shown or listed when the user leaves fullscreen. The summon hotkey still works. Detection method is settled in the Phase 0 spike (candidate: the Windows SHQueryUserNotificationState call).

### 7.6 Personality engine

Rule-based, not AI.

Inputs:
- Events from the bus.
- The tone setting (`casual` or `neutral` in v1).
- Later: a rolling register score for mirror mode (see 7.6.2).

Outputs:
- An animation id plus intensity (0 to 1) sent to the character renderer.
- Optional microcopy chosen from a tone pack.

Rules:
- Reaction cooldowns so Frosty never spams (default: at most one reaction per 2 seconds, one celebratory reaction per 10 seconds).
- Intensity scales with significance (a streak milestone beats a routine task).
- User-controlled reaction level: `full`, `subtle`, `off`.

#### 7.6.1 Microcopy packs

- JSON files per locale and tone, each key holding several weighted variants so Frosty does not repeat itself.
- Packs are data, so tone and language (including Tamil) can be added without code changes.
- Capabilities ship their own microcopy keys in the same format.

#### 7.6.2 Tone, and mirror mode (deferred)

v1 has two tones, `casual` and `neutral`, chosen in settings. Microcopy packs carry both. The proposed default is `casual`.

Mirror mode ("casual back when you are casual") is parked. It is not needed now, and it needs a design decision first, because there is no chat to read your tone from. The idea to revisit later: capabilities opt in to report user-authored text (a task title, a note), the engine keeps only a rolling casualness score from cheap heuristics, it falls back to `neutral` when there is too little signal, and a manual setting always overrides it. Nothing in v1 should block this, so the tone value stays an enum that can gain a `mirror` member later.

### 7.7 Hotkey service

- Registers global shortcuts (summon, toggle fullscreen, open dock item N).
- Detects conflicts and shows a warning in the UI rather than failing silently.
- All bindings are configurable in the config file and the UI.

### 7.8 Window manager

- Owns the overlay window (compact mode) and the main window (fullscreen mode). See section 12.
- Remembers fox position per monitor, handles multi-monitor, and shows Frosty on the monitor that holds the cursor at summon time.
- Hidden state means the window is hidden, not destroyed, so summon is fast. Animation loops are paused while hidden.

### 7.9 Capability host

- Discovers capabilities in two roots (the app bundle for built-ins and `<frosty-home>/capabilities` for user capabilities, see 8.8), validates manifests, checks `api_version` compatibility, and tracks state (`discovered`, `disabled`, `enabled`, `error`).
- Handles activate and deactivate lifecycle, permission grants, and isolation of failures.
- Talks to a capability's logic through a runtime adapter. v1 has exactly one adapter (TypeScript running in the app). A Python adapter can be added later without changing the host's public behavior (see 8.9).
- Details in section 8.

## 8. Capability system

This is the heart of the project. A capability is a folder.

```
capabilities/
  tasks/
    capability.toml        manifest
    settings.schema.json   settings declared as JSON Schema
    migrations/            SQL migrations for this capability
    microcopy/en.json      tone packs for this capability
    schemas/               JSON Schemas for tool input and output and event payloads
    dist/index.js          compiled logic entry (exports defineCapability)
    dist/views.js          compiled views entry (exports defineViews)
    assets/icon.svg
```

The capability's settings values are not stored in this folder. They live in `<frosty-home>/capability-config/<id>.toml` (see 9.1), because built-in capability folders sit in the read-only app bundle.

### 8.1 Manifest (capability.toml)

```toml
[capability]
id = "tasks"
name = "Tasks"
version = "0.1.0"
api_version = "1"
runtime = "ts"          # "ts" in v1, "python" later (see 8.9)
description = "Tasks and reminders"
entry = "dist/index.js"   # logic, loaded in the logic host
views = "dist/views.js"   # views, loaded in the overlay and main windows
icon = "assets/icon.svg"

[permissions]
storage = true
scheduler = true
notifications = true
clipboard = false
network = []          # allowed hosts, empty means none
fs = []               # allowed paths, empty means none
shell = false
events_subscribe = [] # namespaces this capability may listen to

[storage]
migrations = "migrations/"

[settings]
schema = "settings.schema.json"

[[tools]]
id = "tasks.add"
title = "Add task"
input_schema = "schemas/tasks.add.input.json"
output_schema = "schemas/tasks.add.output.json"
quick_action = true
quick_action_control = "text"   # stepper | text | toggle | button

[[events]]
id = "tasks.completed"
payload_schema = "schemas/tasks.completed.json"

[[reactions]]
on = "tasks.completed"
animation = "celebrate"
intensity = "small"             # a number from 0 to 1, or a named level
microcopy = "tasks.completed"
```

Reaction intensity accepts a number from 0 to 1 or a named level: `small` is 0.3, `medium` is 0.5, `large` is 0.8.

### 8.2 SDK shape

A capability has two entries. Logic and views run in different JavaScript contexts (see 8.10), so they are separate modules and never share in-memory state.

Logic entry (`dist/index.js`), loaded once in the logic host:

```ts
import { defineCapability } from "@frosty/sdk";

export default defineCapability({
  id: "tasks",

  async onActivate(ctx) {
    ctx.tools.register("tasks.add", async ({ title, due }) => {
      const id = await ctx.db.insert("tasks", { title, due });
      await ctx.events.emit("tasks.created", { id });
      await ctx.views.setBadge(await openCount(ctx));
      return { id };
    });
  },

  async onDeactivate(ctx) {
    // release timers, subscriptions
  },
});
```

Views entry (`dist/views.js`), loaded in the overlay and main windows:

```ts
import { defineViews } from "@frosty/sdk/views";

export default defineViews({
  dock: { icon: "assets/icon.svg" },
  card: TasksCard,
  page: TasksPage,
  widget: TodayWidget,
});
```

View components get no `ctx`. They reach their logic through client hooks (`useTool`, `useEvent`, `useQuery`), which call tools and listen to events through the core. A dock badge is a value the logic pushes with `ctx.views.setBadge(n)`, not a function the shell calls.

The `ctx` object exposes only what the manifest permissions allow: `ctx.db`, `ctx.events`, `ctx.tools`, `ctx.views`, `ctx.scheduler`, `ctx.notify`, `ctx.config`, `ctx.log`, and gated `ctx.host.*` (clipboard, fs, shell, network).

`ctx.db` is a structured API (`insert`, `update`, `softDelete`, `get`, `query` with filters). The core adds the `cap_<id>_` table prefix and fills the sync columns itself. No raw SQL crosses the bridge in v1.

Design rule: every `ctx` call is async (returns a promise), even though the v1 implementation runs in the same process. This keeps the API valid if a capability's logic later runs in another process (see 8.9).

### 8.3 Lifecycle

1. Discover: scan the capabilities folder at startup and on folder change.
2. Validate: manifest schema, `api_version`, id uniqueness, migration integrity.
3. Enable (user toggles on): show requested permissions, user grants, run migrations, call `onActivate`.
4. Run: views mount in slots, tools registered, events flow.
5. Disable: call `onDeactivate`, unregister views and tools, cancel its scheduler jobs. Data is kept.
6. Remove: separate explicit step that can optionally delete the capability's data.

Failures at any step move the capability to `error` with a readable message. The shell keeps running.

### 8.4 Views and slots

| Slot | Where it appears | Required |
|------|------------------|----------|
| `dock` | Icon in the compact dock and fullscreen rail, optional badge | yes |
| `card` | Compact panel opened next to the fox | yes |
| `page` | Fullscreen module page | no (falls back to card) |
| `widget` | Tile on the fullscreen home dashboard | no |
| `quick-action` | One-tap or small inline action in the dock | no |
| `settings` | Custom settings panel for complex cases | no (generated form by default) |

Views are React components rendered by the shell using shared UI components and design tokens (see DESIGN_SPEC.md section 7).

### 8.5 Tools

- A tool has an id, a title, a JSON Schema for input and output (`input_schema` and `output_schema` in the manifest), and an optional `quick_action` flag with a `quick_action_control` that says which inline control the dock shows.
- Every tool call is routed through the Rust core, which validates the input and checks permissions before the logic host runs the handler.
- Callers: dock quick actions, hotkeys, other capabilities (needs a `tools.call:<id>` permission), later an automation layer.
- Tools are the unit that makes a capability scriptable without giving others direct access to its internals.

### 8.6 Permissions

- Declared in the manifest, granted by the user at enable time, stored in `frosty.toml`. `frosty.toml` is the only place grants and the enabled list are stored.
- Enforced in the Rust host, not just in the SDK. Every `ctx` call is a core command that carries the capability id, and the core checks the grant before acting. A capability cannot reach the filesystem, shell, clipboard, or network without a grant.
- Network: the webviews run under a content security policy that blocks direct network requests. The only network path is `ctx.host.network.fetch`, which the Rust core performs against the manifest's host allowlist.
- In v1 the capability id on a call is asserted by the SDK, which is acceptable under the trust model below.
- v1 trust model: capabilities are trusted local code (you wrote them or you chose to install them). Permissions are a safety net and a clarity tool. Real sandboxing of untrusted third-party code (running in an isolated iframe or WASM) is a later phase and the SDK is designed so that move does not break the API.

### 8.7 Settings

- Capabilities declare settings as JSON Schema with UI hints (`x-frosty-widget`, labels, descriptions, grouping).
- The shell renders the form, validates input, and writes `capability-config/<id>.toml`.
- Editing the TOML by hand has the same effect and hot-reloads.
- Supported field types in v1: toggle, text, number, select, multi-select, time, date, color, path picker, list editor, keybinding recorder.

### 8.8 Built-in vs installed

- Built-in capabilities ship in the app bundle but live in the same format and load through the same host.
- User capabilities are installed by dropping a folder into `<frosty-home>/capabilities` or using "Install from folder" in the module manager.
- The host scans both roots. A user capability whose id matches a built-in is rejected with a clear error.
- A built-in can be disabled but not deleted.

### 8.9 Capability languages and runtime plan

Decision: **start with TypeScript only, keep the seams in place, and add Python backends later when a real capability needs them.** Rust capabilities are not planned.

#### What a capability is made of

- **UI half:** always TypeScript and React. It renders in the WebView whatever else is chosen.
- **Logic half:** the code behind the tools, events, storage use, and scheduling. In v1 this is TypeScript running in the app. Later it may be a Python process.

The two halves talk only through tools and events, with JSON in and JSON out.

#### Options considered

| Option | What it is | Gain | Cost | Status |
|--------|-----------|------|------|--------|
| TypeScript only | UI and logic in one TypeScript module, running in the app | One language, one SDK, nothing extra to ship | No heavy compute and no Python libraries. The code is trusted and shares the WebView with the shell. | v1 |
| TypeScript plus Python | Python logic as a separate process (a Tauri sidecar) talking JSON-RPC over stdio | Reuse Python scripts and libraries, for example local AI or text processing | Process lifecycle, bundling Python on Windows, a larger attack surface | Later, when triggered |
| TypeScript plus Rust | Rust logic compiled into the app, or run as its own executable | Speed and direct OS access | Rust has no stable plugin format, so loading at runtime is brittle. Compiling in means a rebuild per capability. A second SDK. | Not planned. The Rust core already covers system-level needs. |
| WASM | Capabilities compiled to WebAssembly and run in a strict sandbox | Strongest isolation, any source language | Heavy tooling | Revisit only if an open plugin ecosystem needs real sandboxing |

#### Seams to keep from day one

These cost almost nothing and keep the Python path open:

1. Logic is reachable only through tools and events with JSON input and output (schemas in the manifest). No capability imports another capability's code, and a capability's UI never shares in-memory state with its logic.
2. Every `ctx` call is async, even when the v1 implementation is in-process.
3. The SDK interface is the seam. No wire protocol is built before it is needed.
4. The manifest keeps the `runtime` field (`"ts"` in v1).
5. Storage goes through the host (`ctx.db`). A backend never opens `frosty.db` directly.

#### Deliberately not built yet

The JSON-RPC protocol, the sidecar launcher, and Python bundling. A protocol designed for a runtime that does not exist yet tends to be the wrong protocol. The first real Python capability will show what it needs.

#### Trigger for adding Python

A capability you actually want needs a Python library with no decent TypeScript equivalent. When that happens, run the triggered phase described in section 19 ("Triggered phase: Python backends").

#### What changes when Python arrives

- **Manifest:** `runtime = "python"` plus a `backend` entry. The UI stays TypeScript.
- **Host:** a second runtime adapter next to the in-process TypeScript one. It starts the process, restarts it after a crash, stops it with Frosty, and proxies `ctx` calls (storage, scheduler, notifications) over JSON-RPC.
- **Mixed capabilities:** not everything moves. Tasks, calendar, and notes can stay pure TypeScript indefinitely, and both kinds coexist.
- **Trust:** a Python process can do whatever the OS allows, so permissions gate what it may ask Frosty for, not what the process itself can do. The trust model stays "install only code you trust", and the permission sheet says so plainly for Python capabilities (DESIGN_SPEC.md section 4.3).
- **Packaging:** bundle a Python runtime in the Windows installer, or require one on the machine. For personal use, relying on your own Python install is fine at first.

### 8.10 Where capability code runs

The app has two visible windows, each with its own webview and JavaScript context, and capability logic must also run when both are hidden (a scheduler job can fire a tool call at any time). So the app has three webviews in one WebView2 browser process:

| Webview | Visible | Runs |
|---------|---------|------|
| Logic host | Never | Each enabled capability's logic entry: `onActivate`, tool handlers, event subscriptions |
| Overlay | Compact mode | Fox, dock, card, speech chip, quick actions, and capability views |
| Main | Fullscreen mode | Rail, home, pages, settings, module manager, and capability views |

- A tool call goes from a view to the Rust core to the logic host and back.
- Switching modes never reloads capability state, because state lives in the core and the logic host, not in either visible window.
- The logic host is the v1 runtime adapter (see 7.9). A Python backend later is a second adapter behind the same interface.
- Capability bundles are ES modules served through a custom URI scheme that reads only from the two discovery roots. `react`, `react-dom`, `@frosty/sdk`, and `@frosty/ui` are supplied by the shell, so every capability shares one React instance and one set of design tokens.

This model is verified by Phase 0 Spike D. If a hidden webview turns out to be unreliable, the fallback is to run logic in the main window's webview and only ever hide that window, never close it.

## 9. Configuration

### 9.1 Locations

Frosty Home is resolved via the Tauri path API per OS (for example `%APPDATA%/frosty` on Windows, `~/Library/Application Support/frosty` on macOS, `~/.config/frosty` and `~/.local/share/frosty` on Linux). The settings UI shows the exact path with "Open folder" and "Open config file" buttons. Decision: v1 uses the standard app-data folder only. A user-chosen data folder is not planned for v1.

```
<frosty-home>/
  frosty.toml
  frosty.db
  capabilities/
    tasks/ ...
  capability-config/
    tasks.toml
  microcopy/            user overrides for tone packs
  skins/                character skins
  themes/               UI themes (color token files)
  logs/
  backups/
```

### 9.2 Example frosty.toml

```toml
[core]
config_version = 1
device_name = "laptop"
start_on_login = true

[appearance]
theme = "frost"              # a theme name from the themes folder
mode = "system"              # system | light | dark

[hotkeys]
summon = "Ctrl+Alt+F"
fullscreen = "Ctrl+Alt+Shift+F"

[character]
skin = "arctic-fox-default"
size = "medium"             # small | medium | large
reactions = "full"          # full | subtle | off
reduced_motion = "system"   # system | on | off

[personality]
tone = "casual"             # casual | neutral (mirror reserved for later)
locale = "en"

[notifications]
dnd_start = "23:00"
dnd_end = "07:00"
os_fallback = true
fullscreen_behavior = "wait"   # only "wait" in v1

[capabilities]
enabled = ["tasks"]
dock_order = ["tasks"]

[capabilities.permissions.tasks]
storage = true
scheduler = true
notifications = true
```

### 9.3 Rules

- UI and file must stay equivalent. If a setting exists in the UI, it exists in the file, and the reverse.
- Unknown keys are preserved, not deleted, so newer config survives older builds.
- Secrets (API keys for adapters) are not stored in TOML. They go in the OS keychain through a core API.
- Automatic backup of config before any UI-driven rewrite (keep the last N).

## 10. Data and storage

### 10.1 Principles

- Local SQLite. One file per device.
- Capability data is namespaced and isolated.
- Every row that may sync later is syncable by construction (section 15).
- Export is a first-class feature: you can always take your data out as JSON.

### 10.2 Core tables (initial)

| Table | Purpose |
|-------|---------|
| `device` | This device's id and name |
| `capability_state` | Runtime facts only: installed version, current state, last error, error count. The enabled list and granted permissions live in `frosty.toml`. |
| `kv` | Core key-value (window positions, onboarding state) |
| `scheduler_jobs` | Persistent jobs |
| `notifications` | Notification history |
| `event_log` | Optional bounded log of recent events (streaks, debugging) |
| `migrations` | Applied migrations per owner (core or capability) |

### 10.3 Migrations

- Forward-only SQL files, applied in order, recorded in `migrations`.
- A failed migration disables only that capability and rolls back its changes.
- Core takes an automatic DB backup before applying any migration.

## 11. Events and reactions

### 11.1 Event contract

```json
{
  "id": "01J...",              
  "name": "tasks.completed",
  "source": "tasks",
  "at": "2026-10-13T10:00:00Z",
  "payload": { "id": "...", "title": "..." },
  "significance": 0.3
}
```

`significance` is optional (0 to 1) and lets the personality engine scale reactions.

### 11.2 Reaction resolution order

1. User override in config (for example "never react to `notes.*`").
2. Capability-declared reaction in its manifest.
3. Core default for the event class.
4. No reaction.

The reaction then passes through cooldown, reaction level, and tone selection before reaching the renderer. The animation catalog and microcopy examples are in DESIGN_SPEC.md section 8 and 9.

## 12. Windowing and platform behavior

### 12.1 Desktop (Windows first)

Windows 11 is the only v1 target, and it is what everything in this spec is built and tested on. Windows 10 is not a target for now. Nothing should be done that knowingly rules it out, but it is not tested. macOS and Linux come later and are listed in the caveats below so they stay in mind. Tauri uses the system WebView2 runtime, which is preinstalled on Windows 11.

Two visible windows (a third, hidden webview runs capability logic, see 8.10):

1. **Overlay window (compact mode):** frameless, transparent, always on top, no taskbar entry, not focus-stealing until clicked. Contains the fox, dock, and open card. Click-through on transparent regions so it never blocks what is behind it.
2. **Main window (fullscreen mode):** a normal resizable window that can be maximized or fullscreen. Contains the home dashboard, module pages, module manager, and settings.

Tray icon: always present while the app runs. On Windows the icon may land in the hidden overflow area of the taskbar, so onboarding should show how to pin it. Menu: Summon, Open fullscreen, Do not disturb, Settings, Quit.

Known platform caveats (verify in the Phase 0 spike):
- macOS (later): transparent windows need the `macOSPrivateApi` option in Tauri, which rules out Mac App Store distribution. Fine for an open-source direct download.
- Linux and Wayland (later): always-on-top and arbitrary window positioning are restricted on some compositors. X11 is the safe target first, and Wayland behavior needs testing.
- Windows (primary target): generally the smoothest for transparent always-on-top windows. Still verify click-through hit testing, multi-monitor DPI differences, and behavior over fullscreen games and video, where always-on-top overlays can be hidden or can steal focus.
- Click-through hit testing on transparent pixels needs a custom approach (toggle ignore-cursor-events based on pointer position over fox and dock bounds).

### 12.2 Mobile (Android, later)

Android is the only planned mobile platform, and it comes after the Windows app is solid. iOS is not planned: it does not allow a floating overlay over other apps, so the character model would not fit.

Constraints differ from desktop:

- Android can support an overlay with the draw-over-apps permission, but that is optional and later. The first Android version is a normal app where summon means opening the app or tapping a home screen widget.
- No global hotkeys and no tray on mobile. Equivalent entry points: app icon, home screen widget, notification tap.
- Fullscreen is the default mobile mode. The compact "card" becomes a bottom sheet.
- Reminders use OS-scheduled local notifications because the scheduler cannot rely on a background process.

### 12.3 Mode switching

Compact and fullscreen share one data layer. Switching modes never reloads capability state. Opening a module in fullscreen from compact carries over the module and its scroll or selection state where possible.

## 13. Security and privacy

- No telemetry, no analytics, no account.
- Network is off by default per capability, enabled by explicit host allowlist.
- Adapter secrets live in the OS keychain.
- Filesystem access is limited to granted paths.
- The shell command permission is the most sensitive grant. It requires an extra confirmation and is never granted to built-ins by default.
- Capability code is treated as trusted in v1 (see 8.6). The docs and manifest permission display make that explicit.
- Logs never include raw user content from capabilities unless debug logging is turned on by the user.
- Updates (later) must be signed.

## 14. Integrations: build, integrate, or both

Each capability chooses one of three modes, and the user can pick where more than one exists:

1. **Native only:** Frosty owns the data (calendar stored in `frosty.db`).
2. **Integrated:** an adapter reads and writes an external service (Google Calendar, Steam).
3. **Both:** a native store plus optional adapters, with a clear rule for which side is the source of truth per record.

Adapter rules:
- An adapter lives inside the capability and is a separate module with a small interface (`list`, `get`, `create`, `update`, `delete`, `sync`).
- Every adapter declares its network hosts and keychain secrets in the manifest.
- Native mode must always work with zero network access.
- Sync conflicts with external services are resolved per capability, with last-write-wins as the default and the conflict recorded in a log.

## 15. Sync readiness (designed for, not built)

Cross-device sync is explicitly out of scope for v1, but these choices keep the door open:

- UUIDv7 primary keys, no auto-increment integers.
- `created_at`, `updated_at`, `deleted_at`, `device_id` on every capability table.
- Soft deletes only. Hard deletion happens in a later compaction step.
- Capability settings and data kept separate, so settings can sync differently from data.
- Per-device settings stay per-device (hotkeys, window positions, which capabilities are enabled), and a future sync layer can decide which categories to share.
- Candidate approaches when the time comes: a self-hostable sync server with an operation log, a CRDT layer, or file-based sync (Syncthing-style) of the SQLite export. This is not decided now.

## 16. Repo layout

```
frosty-companion/
  apps/
    desktop/             Tauri app (src-tauri for Rust, src for the shell UI)
    mobile/              later, shares the UI package
  crates/
    frosty-core/         config, storage, events, scheduler, personality
    frosty-host/         capability host and permissions
  packages/
    sdk/                 @frosty/sdk (types and runtime for capabilities)
    ui/                  shared components and design tokens
    character/           Rive runtime wrapper and skin contract
  capabilities/
    tasks/               first-party capability, same format as third party
  docs/
    PROJECT_SPEC.md
    DESIGN_SPEC.md
    capability-authoring.md
  tools/
    scaffold-capability/ generator for new capability folders
```

Tooling: pnpm workspaces for the JS side, Cargo workspace for Rust, `just` or `make` for common tasks.

## 17. Quality, testing, and performance

### Testing

- Rust unit tests for config parsing and migration, storage, scheduler, event bus, permissions.
- Contract tests: a capability fixture that exercises every SDK surface, run against every SDK version.
- Frontend component tests for shared UI and settings form generation.
- A scripted end-to-end test for the core flow (summon, open module, add item, reaction fires, dismiss) using Tauri's WebDriver support.
- Config fuzz tests: random invalid TOML must never crash the app.

### Performance targets (targets, to be measured, not promises)

- Summon to visible: under 150 ms from hotkey.
- Hidden state: near-zero CPU, animation loops paused.
- Idle visible: animation should stay smooth without noticeable CPU use.
- Cold start to tray ready: under 2 seconds on a typical laptop.

### Observability

- Local rotating logs, a "copy diagnostics" button in settings, per-capability error counters shown in the module manager.

## 18. Build and release

- CI: lint, test, and build on Windows. Rust crate tests and JS tests also run on Linux, which is cheap and keeps the core portable. macOS is added when it becomes a target.
- Releases: installers per OS from tagged builds. Auto-update later, signed.
- Versioning: semantic versioning for the app, separate `api_version` integer for the capability SDK. Breaking SDK changes bump `api_version`, and the host can load only capabilities whose version it supports.

## 19. Roadmap

Each phase has acceptance criteria so "done" is not a feeling.

### Phase 0: Foundations and spikes

Work:
- Repo, workspaces, CI, formatting, lint.
- Spike A: transparent, frameless, always-on-top Tauri window with click-through on transparent areas, on Windows 11. Also check behavior over fullscreen video and games, including how to detect a fullscreen app.
- Spike B: Rive state machine playing in the Tauri webview with a placeholder fox.
- Spike C: global hotkey summon latency measurement.
- Spike D: load a capability module from a folder outside the app bundle, render its view with the shell's React, and call a tool in a hidden logic host webview from the overlay (see 8.10).

Done when: spikes A to D work on Windows 11, and any blockers (especially transparency and click-through) are written down. Android is not part of Phase 0.

### Phase 1: The shell

Work:
- Placeholder arctic fox with idle, summon, dismiss, and a few reaction animations.
- Summon and dismiss by hotkey and tray.
- Compact mode with an empty dock. Fullscreen mode with an empty home.
- Position memory, multi-monitor behavior.
- Personality engine v0 (tone setting, microcopy packs, cooldowns) driven by a debug panel that fires fake events.
- Theme system (theme files, the built-in Frost theme plus at least one more, light and dark modes) and reduced motion.
- Config service v0 with hot reload.
- Core database with the core migrations for `device`, `kv`, and `migrations` (position memory needs `kv`).

Done when: you can summon and dismiss Frosty reliably, switch between compact and fullscreen, fire fake events from the debug panel and see the right reactions, and edit `frosty.toml` by hand and see changes live.

### Phase 2: Plugin architecture

Work:
- Capability manifest spec, validator, SDK v1, host with lifecycle and permissions.
- Module manager UI (list, toggle, status, permissions prompt, settings form generated from schema, open config file).
- Storage service with per-capability migrations.
- Scheduler core (persistent one-shot and recurring jobs) and a minimal notify path, so the SDK's `ctx.scheduler` and `ctx.notify` are real from the start.
- Scaffold tool that generates an empty capability.
- A trivial "hello" capability used as the test fixture.

Done when: you can scaffold a new capability, drop it in the folder, enable it from the UI, see its dock icon, card, and settings form, and disable it again, all without modifying core code. A deliberately broken capability shows an error and the shell survives.

### Phase 3: First real capability (Tasks and reminders)

Work:
- Tasks capability built entirely on the SDK: list, add, complete, due dates, recurring reminders, quick action.
- Scheduler and notification center fully wired: missed-job policies, do-not-disturb, fullscreen wait, rate limiting. Frosty nudges on a reminder.
- Reactions: task created, completed, streak.

Done when: you use it for your own tasks for a week, reminders fire when Frosty is hidden, and nothing in core had to special-case Tasks.

### Phase 4: Second capability and home dashboard

Work:
- Pick a very different capability (reading tracker or guitar log) to stress-test the SDK.
- Home dashboard with widgets, cross-capability events, streaks.
- Fix the SDK based on what hurt.

Done when: two capabilities with different data shapes both feel natural to build, and the SDK needed only additive changes.

### Phase 5: Onboarding and polish

Work:
- First-run onboarding flow, re-runnable from settings.
- Notification center UI, do-not-disturb.
- Export and import, backups.
- Theme editor UI (edit tokens with live preview and contrast warnings).
- Real character art replaces the placeholder (see DESIGN_SPEC.md).

### Phase 6: Android

Work:
- Spike first: Tauri Android hello world on your phone.
- Android build sharing the UI package, with the bottom-sheet card and mobile dock.
- Local notifications for reminders.
- Per-device independence confirmed (no sync).

### Phase 7: Expansion

- More capabilities (calendar, reading, guitar, files, music, bookmarks...).
- Adapter work (Google Calendar, Steam).
- Ambient awareness (optional, opt-in).
- Mirror tone mode (see 7.6.2).
- Sync.
- Open-source preparation: license choice, contributor docs, capability authoring guide, security review.

### Triggered phase: Python backends (only when the trigger in 8.9 is hit)

Work:
- Runtime adapter in the capability host for `runtime = "python"`, with process start, health check, restart policy, and clean shutdown.
- JSON-RPC over stdio protocol for tools, events, and `ctx` proxying, designed from the first real capability's needs.
- Backend status and restart controls in the module manager (DESIGN_SPEC.md section 4.3).
- A decision on how to package the Python runtime for Windows.
- One real capability built on it as proof.

Done when: that capability works end to end, a killed Python process recovers or fails soft with a clear error, and no existing TypeScript capability needed changes.

## 20. Risks and mitigations

| Risk | Impact | Mitigation |
|------|--------|-----------|
| Transparent always-on-top overlay quirks (click-through, DPI, fullscreen apps) | Core experience breaks | Windows is the primary target. Phase 0 spike A tests these on your machine before building further. macOS and Linux are later. |
| Scope creep from the capability list | Never ships | Phase gates. Only one capability until the SDK holds. |
| Plugin API designed in a vacuum | Rewrites | Dogfood it. Two very different capabilities before freezing `api_version` 1. |
| Mirror-tone feels gimmicky or wrong | Feature disappointment | Deferred. v1 ships a manual casual or neutral setting. |
| Android limits (overlay needs a special permission, background limits) | Experience mismatch | Treat Android as a companion app, not a port of the desktop overlay. Overlay is optional and later. |
| Designing a Python protocol before a real need | Wrong protocol, wasted work | Only the SDK seams are built now. The protocol waits for the first real Python capability. |
| Python sidecar lifecycle and packaging on Windows | Flaky or heavy installs | Triggered phase, personal-use install first, bundling decided later. |
| Pixel art direction changes the animation tech (Rive versus sprite sheets) | Rework of the character layer | Decide the fox art style before Phase 1 character work. Keep the skin contract abstract (state names and inputs) so either implementation fits. |
| Hot-reload config corruption | Data or settings loss | Validate before apply, backups before UI writes, keep last good. |
| Character art takes long | Looks unfinished | Placeholder skin plus a stable skin contract so real art drops in later. |
| Trusted-code assumption for capabilities | Security issue if open-sourced with a marketplace | Do not ship remote installation until sandboxing exists. |

## 21. Resolved questions and remaining open questions

### Resolved in review

| Question | Answer |
|----------|--------|
| Primary OS for the laptop | Windows 11. Windows 10 is not a target for now. |
| Phone | Android, built later, not now. No iOS. |
| How mirror tone should work | Not needed now. Deferred. v1 has a manual casual or neutral setting. |
| Dock shape | Pill attached to the fox |
| Accent color | Multiple themes instead of one fixed accent (see DESIGN_SPEC.md section 13) |
| Frontend framework | React + TypeScript |
| Capability language | TypeScript first, Python backends later when a capability needs them (see 8.9). Rust not planned. |
| First real capability | No preference stated, so Tasks and reminders stays as the default |
| Data location | Standard OS app-data folder |
| Reminders over fullscreen apps | Wait in history until the user leaves fullscreen |
| Fox and card layout | The card opens beside the fox |
| Tamil in the UI | Not in v1, but every string lives in data files so it can be added later |
| Theme look | Pixel or retro game feel, solid colors, no generic AI-generated look |
| Surface style | Subtle blur where supported with a solid fallback, set per theme |

### Still open

1. **Should the fox itself be pixel art** to match the theme direction, or stay a smooth cartoon? This decides the animation tech (see section 6 and DESIGN_SPEC.md section 17).
2. **Free-text command line** (type "log guitar 30") later? It is not a chatbot, but it is a text input. Default is no, structured quick actions only.
3. **Character skins:** should users eventually be able to swap the fox for other characters?
4. **Default tone:** the spec proposes `casual` as the default. Is that right?
5. **Themes:** which pixel themes first beyond Frost, and is a theme editor UI needed early or are theme files enough for a while? (See DESIGN_SPEC.md section 13.)

## 22. Decision log

| ID | Decision | Status |
|----|----------|--------|
| D1 | Tauri v2 for desktop and mobile | Proposed |
| D2 | React + TypeScript frontend | Confirmed in review |
| D3 | Rive for character animation | Proposed, depends on the pixel versus smooth fox decision |
| D4 | SQLite local storage, one DB per device | Proposed |
| D5 | TOML config as source of truth, UI as editor | Agreed |
| D6 | Capabilities are folders with manifest plus TS entry | Proposed |
| D7 | First-party capabilities use the public SDK | Agreed in principle |
| D8 | No AI, no chat in the core | Agreed |
| D9 | Interaction-triggered reactions only in v1, ambient later | Agreed |
| D10 | Devices independent in v1, sync designed for | Agreed |
| D11 | Native modules first, external integrations as optional adapters | Agreed |
| D12 | Compact floating mode plus fullscreen mode | Agreed |
| D13 | Tone setting with mirror mode, casual back when you are casual | Superseded by D16 (mirror deferred) |
| D14 | Windows 11 is the only v1 desktop target | Agreed, narrowed by D23 |
| D15 | Android is the only mobile target, built after the desktop app is solid. No iOS. | Agreed |
| D16 | Mirror tone deferred. v1 tone is a manual casual or neutral setting | Agreed |
| D17 | Capabilities start TypeScript only, with a reserved `runtime` field and the seams listed in 8.9 | Agreed |
| D18 | First real capability is Tasks and reminders | Default kept, no preference stated |
| D19 | Dock is a pill attached to the fox | Agreed |
| D20 | Multiple themes via theme files (see DESIGN_SPEC.md section 13). Direction is pixel or retro, no generic AI-generated look. | Agreed, theme list open |
| D21 | Python backends are added later on a real need (trigger in 8.9). Only the seams are built now, not the protocol. | Agreed |
| D22 | Rust capabilities are not planned. WASM is a later option if real sandboxing is needed. | Agreed in principle |
| D23 | Windows 11 only for now. Windows 10 is not supported or tested until there is a reason to add it. | Agreed, replaces the earlier "both Windows 10 and 11" |
| D24 | Data and config live in the standard OS app-data folder. No user-chosen folder in v1. | Agreed |
| D25 | Reminders wait in history while a fullscreen app is in front | Agreed |
| D26 | The module card opens beside the fox | Agreed |
| D27 | English only in v1. All strings live in data files so Tamil can be added later without code changes. | Agreed |
| D28 | Surfaces use subtle blur where supported, with a solid fallback. A theme can set its own surface style, and pixel themes default to solid. | Agreed, tension with the pixel look handled per theme |
| D29 | Capability logic runs in a hidden logic host webview, and views run in the overlay and main windows. Logic and views are separate entries (see 8.2, 8.10). | Proposed, verified by Phase 0 Spike D |
| D30 | Capability settings live in `capability-config/<id>.toml`, not inside the capability folder | Agreed |
| D31 | `frosty.toml` is the only store for the enabled list and permission grants. `capability_state` holds runtime facts only. | Agreed |
| D32 | Capabilities are discovered in two roots: the app bundle (built-in) and `<frosty-home>/capabilities` (user) | Agreed |
| D33 | Webviews block direct network access by content security policy. Network goes through `ctx.host.network`, enforced in Rust. | Agreed |
| D34 | CI runs the full pipeline on Windows and tests only on Linux | Agreed |
