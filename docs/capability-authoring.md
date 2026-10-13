# Writing a Frosty capability

A capability is a folder. It gives Frosty an ability (tasks, a guitar log, a reading list) without touching the core. This guide covers what is built in v1. The design is in PROJECT_SPEC.md section 8.

## Quick start

```bash
node tools/scaffold-capability/scaffold.mjs guitar_log --name "Guitar Log"
```

That creates `%APPDATA%\frosty\capabilities\guitar_log`, builds it, and tells you what to do next: open Frosty, go to **Modules**, press **Rescan**, and turn it on. Pass `--out capabilities` to make it a built-in of this repo instead.

Rebuild after editing source:

```bash
node tools/capability-build/build.mjs <capability folder>
```

For a built-in in this repo, `pnpm build:capabilities` builds all of them. The app does this automatically before `tauri dev` and `tauri build`.

## Folder layout

```
guitar_log/
  capability.toml        manifest (required)
  settings.schema.json   settings, as JSON Schema
  schemas/               JSON Schemas for tool input and event payloads
  migrations/            SQL, applied in file-name order
  microcopy/en.json      phrases Frosty can say, per tone
  assets/icon.svg        24 px monochrome icon
  src/index.ts           logic entry  -> dist/index.js
  src/views.tsx          views entry  -> dist/views.js
```

Only `dist/` and `assets/` are ever served to a window. Everything else stays on disk.

## The manifest

See `capabilities/hello/capability.toml` for a working one. The fields:

| Section | What it does |
|---------|--------------|
| `[capability]` | `id` (lowercase letters, digits, `_`, at most 32), `name`, `version`, `api_version = "1"`, `runtime = "ts"`, `entry`, optional `views` and `icon` |
| `[permissions]` | `storage`, `scheduler`, `notifications`, `clipboard`, `shell`, and lists `network`, `fs`, `events_subscribe`, `tools_call` |
| `[storage]` | `migrations = "migrations/"` |
| `[settings]` | `schema = "settings.schema.json"` |
| `[[tools]]` | `id` (starts with `<id>.`), `title`, `input_schema`, `output_schema`, `quick_action`, `quick_action_control` |
| `[[events]]` | `id` (starts with `<id>.`), `payload_schema` |
| `[[reactions]]` | `on`, `animation`, `intensity` (0 to 1, or `small` / `medium` / `large`), `microcopy` |

A manifest with several problems reports all of them in the module manager, not just the first.

## Logic: `src/index.ts`

```ts
import { defineCapability } from "@frosty/sdk";

export default defineCapability({
  id: "guitar_log",

  async onActivate(ctx) {
    await ctx.tools.register("guitar_log.add", async ({ minutes }: { minutes: number }) => {
      const row = await ctx.db.insert("sessions", { minutes });
      await ctx.events.emit("guitar_log.logged", { id: row.id });
      return { id: row.id };
    });
  },

  async onDeactivate(ctx) {
    // release timers and subscriptions; data is kept
  },
});
```

The logic runs in a hidden window, whether or not Frosty is visible, so reminders and scheduled work keep running.

Every `ctx` call returns a promise and is checked in Rust against the manifest and your grants.

| Call | Needs | Notes |
|------|-------|-------|
| `ctx.db.insert / get / query / update / softDelete / restore` | `storage` | Structured, no raw SQL. Tables are `cap_<id>_<table>`; Frosty fills `id`, `created_at`, `updated_at`, `deleted_at`, `device_id`. Deletes are soft. |
| `ctx.events.emit(name, payload, significance?)` | | The event must be declared. Payload is validated against its schema. |
| `ctx.events.subscribe(pattern, handler)` | `events_subscribe` for other namespaces | Your own namespace and `core.*` are always allowed. |
| `ctx.tools.register(id, handler)` | | Only tools declared in the manifest are routable. |
| `ctx.tools.call(tool, input)` | `tools_call` for other modules | Input is validated against the tool's schema. |
| `ctx.views.setBadge(n)` | | The number on the dock icon. 0 hides it. |
| `ctx.scheduler.at / every / cancel` | `scheduler` | Jobs fire an event or a tool call and survive restarts. Cancelled when the module is turned off. A run that is more than a minute late follows `missed`: `fire_once_on_start` (default, a tool call gets `late: true`), `skip`, or `fire_all` (replays a recurring job's missed runs, at most 10). |
| `ctx.notify(title, body?, { actions })` | `notifications` | Frosty appears and nudges. Up to two `{ label, tool, input }` actions become buttons; each runs one of your tools. It may be held for do-not-disturb or while a fullscreen app is open, and the user can mute your module. |
| `ctx.config.get / onChange` | | The module's settings, with schema defaults applied. |
| `ctx.log.info / warn / error` | | Written to `logs/capabilities.log`. |

`ctx.host.*` (clipboard, filesystem, shell, network) is **not built yet**. The manifest accepts those permissions and the module manager shows them, but there are no calls behind them in this phase.

## Views: `src/views.tsx`

```tsx
import { defineViews } from "@frosty/sdk";
import { useTool, useToolData, useEvent } from "@frosty/sdk/views";

function Sessions() {
  const add = useTool<{ minutes: number }>("guitar_log.add");
  const { data, loading, error, refresh } = useToolData("guitar_log.list", {}, ["guitar_log.logged"]);
  // ...
}

export default defineViews({ card: Sessions, page: Sessions });
```

Rules (DESIGN_SPEC.md 7.2):

- A view never gets `ctx`. It reaches its logic through tools and events only, so the logic can later move to another process.
- Handle loading, empty, and error states. `useToolData` gives you all three.
- Use the shared CSS classes (`f-input`, `f-button`, `f-button-primary`) and theme tokens (`var(--text)`, `var(--accent)`). No hard-coded colors.
- Do not open extra windows.
- A crashing view is contained: the card is replaced by an error panel with **Copy details** and **Turn off this module**, and the rest of Frosty keeps running.

Slots in v1: `card` (opens beside the fox) and `page` (the fullscreen module page; falls back to `card`). `widget` arrives with the home dashboard.

**Quick actions.** A tool with `quick_action = true` gets a small `+` beside its dock icon that opens a one-line control above the dock. `quick_action_control` picks it: `text`, `stepper`, `toggle`, or `button`. The control fills the first required property in the tool's input schema, so a quick action tool should have one obvious field. After it runs, focus goes back to the app the user was in.

## Settings

Declare them as JSON Schema and the module manager generates the form. Extra hints:

| Keyword | Meaning |
|---------|---------|
| `x-frosty-widget` | `toggle`, `text`, `number`, `select`, `multi-select`, `time`, `date`, `color`, `path`, `list`, `keybinding` |
| `x-frosty-group` | A heading to group fields under |
| `x-frosty-order` | A number to sort by |

Values live in `<frosty-home>\capability-config\<id>.toml`. Editing that file by hand has the same effect as the form and reaches `ctx.config.onChange`.

## Migrations

Plain SQL in `migrations/`, applied once in file-name order. Rules, enforced by the host:

- Every table, index and trigger you create must belong to a `cap_<id>_` table.
- Every table must have `id`, `created_at`, `updated_at`, `deleted_at`, `device_id`.
- You cannot change or drop anything outside your namespace.
- A failing migration rolls back and puts only your module in the Error state.

## Permissions and trust

Permissions are shown in plain language when a user turns a module on, and stored in `frosty.toml`. The host enforces them on every call, and the webviews run under a content security policy that blocks direct network access and loading code from other origins.

v1 treats capability code as **trusted local code**: permissions are a safety net and a clarity tool, not a sandbox. Only install modules you trust. Real sandboxing is a later phase.

## A worked example: Tasks

`capabilities/tasks` is a real module built only on this SDK: reminders through `ctx.scheduler.at` and `ctx.notify` with Done and Snooze actions, repeating tasks that reopen at the next slot, a streak computed from its own completion table, and delete with undo using `softDelete` and `restore`. Its pure logic (`recurrence.ts`, `streak.ts`) is separate from the tools so it can be tested without any host, and `src/testing.ts` is an in-memory stand-in for the core that you can copy.

## Testing

`capabilities/hello/src/index.test.ts` shows the pattern: build a context with `createContext` from `@frosty/sdk/host` over a fake bridge, activate your module, and call its tools. The Rust side has contract tests in `crates/frosty-host/tests/contract.rs`, and `tools/smoke/phase2.ps1` runs the real app against a throwaway home (set with the `FROSTY_HOME` environment variable).
