# Frosty: Design Spec (Frontend)

Status: Draft v0.4 (adds pixel theme direction and layout answers)
Date: 2026-10-13
Companion doc: PROJECT_SPEC.md (architecture)

This document covers what Frosty looks like, how it moves, how it behaves on screen, and how capability UIs must fit into it. Dimensions and colors are starting values to be tuned once real art and a running build exist.

---

## 1. Design goals and principles

1. **Presence over interface.** Frosty is a character first. The UI around it should feel like things the fox is holding, not a dashboard with a mascot.
2. **Not a chatbot.** No chat bubbles with free-form conversation, no typing indicator, no "ask me anything". Frosty talks only in short, designed lines (microcopy) tied to events.
3. **Summoned, not nagging.** Frosty appears when you call it. It only appears on its own for reminders you set.
4. **Quiet by default, delightful when it counts.** Small reactions for small things, a bigger one for milestones.
5. **Consistent identity across capabilities.** A guitar log and a calendar must feel like the same companion.
6. **Fast.** Summon should feel instant. Animation never blocks interaction.
7. **Accessible.** Motion can be reduced, everything works with a keyboard, text is readable.
8. **Everything configurable is reachable in the UI,** and also lives in the config file.

## 2. The character: Frosty

### 2.1 Brief

- An arctic fox, cartoon style.
- Placeholder art is fine for now. You will design the final character later.
- Personality: relaxed and friendly. v1 has two tones you choose in settings, casual (proposed default) and neutral. Mirroring your energy automatically is parked for later.
- Cute but not childish. Think calm and friendly, with small expressive details (ears, tail, eyes).

### 2.2 Design notes for the real art (later)

- Silhouette must read at small sizes (it will often be around 112 to 160 px tall).
- Strong key features: big ears, fluffy tail, dark nose, soft white and blue-gray fur, a small warm accent (scarf, charm, or an ember-colored detail) so there is a recognizable color note.
- Simple enough that animation states are cheap to produce.
- Works on both light and dark backgrounds (a soft outline or subtle shadow helps on busy desktops).
- Needs a front or three-quarter pose for most states and a small number of unique poses (sleepy, celebrate).

### 2.3 Placeholder plan

Until the final art exists:
- Phase 1 uses a simple vector fox built in Rive (or a flat SVG with a few Rive-driven transforms) with the full state machine wired up. Quality of the art is secondary to the quality of the state machine.
- The fox sits behind a **skin contract** (section 2.5), so replacing the art later changes no code.

### 2.4 Animation states

Idle loops (only play while visible, paused when hidden):

| State | Description | Trigger |
|-------|-------------|---------|
| `idle_breathe` | Gentle breathing, slight body movement | Always while visible |
| `idle_blink` | Blink | Random, every 3 to 8 seconds |
| `idle_ear_twitch` | Ear twitch | Random, every 6 to 15 seconds |
| `idle_tail_flick` | Tail flick | Random, every 8 to 20 seconds |

Transitions:

| State | Description | Trigger |
|-------|-------------|---------|
| `summon_in` | Fox hops or slides in from the nearest screen edge with a small spring, dock fans out after | Summon |
| `dismiss_out` | Reverse of summon, quicker | Dismiss |
| `mode_expand` | Fox shrinks into the fullscreen rail | Enter fullscreen |
| `mode_collapse` | Fox grows back out of the rail | Exit fullscreen |

Reactions:

| State | Description | Typical use |
|-------|-------------|-------------|
| `wave` | Small wave | First summon of the day, onboarding |
| `happy_small` | Tail wag, ears up | Item created, setting saved |
| `celebrate` | Jump, sparkle burst, intensity scalable | Task completed, streak, milestone |
| `curious_tilt` | Head tilt | Empty state, something new detected |
| `nudge` | Hop and gentle paw tap toward the screen | Reminder |
| `sleepy` | Yawn, eyes droop | Do-not-disturb on, late night |
| `oops` | Ears down, small shake | Error |
| `thinking` | Small look-up loop | Loading longer than 400 ms |
| `bye` | Small wave then settle | Quit, end of onboarding |

All reactions take an `intensity` from 0 to 1. Intensity changes amplitude and extras (sparkles at high intensity), not the base pose.

### 2.5 Skin contract (Rive state machine)

Any replacement fox, or any other character, drops in if it follows this contract.

Note: this contract is written for Rive. If the fox becomes pixel art, an equivalent sprite-sheet contract with the same state names and inputs will be defined next to it (open question 1 in section 17).

| Input | Type | Meaning |
|-------|------|---------|
| `visible` | boolean | Fox on screen or hidden (pauses idle loops when false) |
| `summon` | trigger | Play `summon_in` |
| `dismiss` | trigger | Play `dismiss_out` |
| `wave` | trigger | Play wave |
| `happy` | trigger | Play happy_small |
| `celebrate` | trigger | Play celebrate |
| `curious` | trigger | Play curious_tilt |
| `nudge` | trigger | Play nudge |
| `sleepy` | boolean | Sleepy state on or off |
| `oops` | trigger | Play oops |
| `thinking` | boolean | Thinking loop on or off |
| `intensity` | number 0 to 1 | Scales the current reaction |
| `lookX`, `lookY` | number -1 to 1 | Optional eye or head follow of the pointer |

A skin is a folder in `skins/` with a `skin.toml` (name, author, artboard size, anchor point) and the Rive file. The shell validates that required inputs exist when the skin loads and falls back to the default skin if not.

### 2.6 Asset specs

- Rive file per skin, artboard 512 x 512 base, scales down by size setting.
- Anchor point at the fox's feet center so position memory and edge snapping are stable.
- Static fallback PNG or SVG per state for reduced motion and screenshots.
- Sizes: small 112 px, medium 160 px (default), large 224 px.

## 3. Modes and surfaces

### 3.1 Compact mode (the floating mode)

What it is: a transparent overlay on the desktop containing the fox, the dock, and (when open) a module card.

Behavior:
- Appears on summon on the monitor that holds the cursor, at the last remembered position for that monitor (default: bottom right with 24 px margin).
- Draggable by the fox. Optionally snaps to the nearest screen edge when released within 48 px of it.
- Only the fox, dock, and card capture the mouse. Everything else on the overlay is click-through.
- Always on top, no taskbar entry. Windows note: the overlay must behave around fullscreen apps (games, video). Decided: reminders wait in the notification history while a fullscreen app is in front, and the summon hotkey still works. The detection method is verified in the Phase 0 spike.
- Dismiss with the summon hotkey again, with Esc when nothing is focused inside a card, by clicking the fox's dismiss affordance, or from the tray.

Parts:
1. Fox (160 px default).
2. Dock: a compact row of capability icons attached to the fox (section 5.1).
3. Module card: the open capability's compact UI, anchored beside the fox (section 5.2).
4. Speech chip: a small, short-lived line of microcopy near the fox (section 8.4).
5. Quick actions: inline small controls shown in the dock (section 5.4).

### 3.2 Fullscreen mode

What it is: a normal app window for when you need space (calendar month view, reading backlog, settings).

Behavior:
- Opens maximized by default, resizable, can go true fullscreen.
- A small version of the fox lives in the rail (top left). It still reacts to events.
- Toggle with the fullscreen hotkey, from the dock, or from the tray. Switching never reloads module state.

Parts:
1. Left rail (capability icons, home, settings).
2. Top bar (page title, search placeholder for later, notification bell, mode toggle).
3. Content area (home dashboard or a capability page).
4. Optional right panel (contextual detail, used by capabilities that need master-detail).

### 3.3 Mobile (Android, later)

Android is the only planned mobile platform and it comes after the Windows app is solid. The floating overlay is optional and later. Mobile is a companion app, not a port of the overlay.

- Launch screen shows the fox at the top of a home screen with the dock as a bottom tab or icon row.
- The compact card becomes a **bottom sheet** (half height, draggable to full).
- Fullscreen is the default.
- Summon equivalents: app icon, later a home screen widget, and reminder notification tap.
- Reminders arrive as OS notifications. Tapping one opens Frosty with the `nudge` reaction.

## 4. Key flows

### 4.1 Summon to dismiss

1. Press the summon hotkey (or tray, or mobile app open).
2. Fox plays `summon_in` (about 480 ms spring). The dock fans out with a 40 ms stagger per icon.
3. Fox settles into `idle_breathe`. Optionally a short line appears in the speech chip (first summon of the day only).
4. User clicks a dock icon. The module card opens beside the fox. The fox steps back slightly (shifts 16 px away and tilts its head toward the card).
5. User works inside the card.
6. User closes the card (X, Esc, or clicking the dock icon again). The card collapses and the fox returns to the neutral position.
7. User dismisses with the hotkey. Fox plays `dismiss_out` (about 280 ms) and the window hides.

Rules:
- Summoning while already visible brings focus to the dock instead of replaying the animation.
- If the card is open when you press the hotkey, the first press closes the card, the second dismisses. (Setting: `dismiss_closes_card_first`, default on.)

### 4.2 First launch onboarding

Frosty introduces itself and walks you through choosing modules. It writes everything to `frosty.toml`.

| Step | Screen | Content |
|------|--------|---------|
| 1 | Welcome | Fox slides in and waves. Short line: introduces itself. Buttons: Continue, Skip setup. |
| 2 | Tone | Choose Casual or Neutral. Each option previews a sample line from Frosty. Casual is preselected. |
| 3 | Summon key | Shows the default hotkey, lets you rebind it by pressing a combination, and a Try it button that hides and re-summons Frosty. Warns about conflicts. |
| 4 | Pick capabilities | Grid of capability cards. Each shows icon, one-line description, and what permissions it asks for. Toggling one on adds a follow-up step. Nothing is preselected. |
| 5 | Configure (per chosen capability) | Settings form generated from the capability's schema, with defaults filled in. Skippable per capability. |
| 6 | Arrange dock | Drag to reorder icons. Skippable. |
| 7 | Done | Fox plays `celebrate` at small intensity and shows how to dismiss, how to open fullscreen, where the config file lives, and how to pin the tray icon on Windows. Button: Open the config folder. |

Onboarding can be re-run from Settings > Frosty > Run setup again. Skipping it still leaves Frosty usable with an empty dock and a hint to open the module manager.

### 4.3 Module manager

Reachable from the dock "+" button, the fullscreen rail, and Settings.

List rows:
- Icon, name, version, one-line description.
- Status chip: Enabled, Disabled, Error.
- Runtime badge: TypeScript, or Python later. For a capability with a separate backend process, also a backend status (Running, Stopped, Crashed) with a Restart button.
- Toggle.
- "Settings" button opens the generated form (or the capability's custom settings view).
- Overflow menu: Open config file, Reveal folder, View permissions, Remove.

Enabling a capability for the first time opens a permission sheet listing exactly what it asks for in plain language ("Send reminders", "Read and write its own data", "Access the clipboard"). The user grants or cancels. For a capability with a Python backend, the sheet adds a plain warning that it runs as a separate program on the computer with the same access the user has.

Error rows show the message and a Copy details button. The shell never blocks on a broken capability.

"Install from folder" lets you pick a capability folder. Remote installation is not in v1.

### 4.4 Quick action

Goal: do a common small thing without opening a card.

1. Summon.
2. Hover or long-press a dock icon that has quick actions (or click the small action chip next to it).
3. A small inline control appears above the dock: a stepper, a short text field, a toggle, or a single button, depending on the action.
4. Confirm. The capability runs the tool, emits its event, and Frosty reacts.

Example: "Log guitar practice" shows a minutes stepper with a Log button. "Add task" shows a one-line text field. This is a structured control, not a command line or chat.

### 4.5 Reminder notification

1. The scheduler fires and the notification center decides how to present it.
2. If Frosty is hidden, it appears in a minimal state (fox plus speech chip, no dock) with `nudge`.
3. The chip shows the reminder text and up to two actions (for example Done, Snooze).
4. If ignored, it stays for a configurable time (default 20 seconds), then slides away and the notification stays in the history list.
5. In do-not-disturb, nothing appears and the item waits in history. If `os_fallback` is on and the overlay cannot show, a native OS notification is used.

## 5. Layout specs

All sizes are logical pixels at 100 percent scaling.

### 5.1 Compact mode: fox and dock

```
            +--------------------------+
            |  Card (380 x 520)        |
            |                          |
            |                          |
            +--------------------------+
      (  fox 160 x 160  )
      [Tasks] [Notes] [Guitar] [+]     <- dock
```

Dock:
- Height 56, icon button 40 x 40, gap 8, horizontal padding 12, corner radius 28 (pill).
- Max 6 icons visible, the rest collapse into an overflow icon that opens a small list.
- Always includes a "+" button at the end (module manager) unless hidden in settings.
- Badges: 16 px circle, top right of the icon, count capped at 9+.
- Position: attached beneath or beside the fox depending on screen edge. If the fox is on the right edge, the dock extends to the left, and vice versa.
- Surface: subtle translucent blur where the platform allows, with a solid fallback. The active theme's `surface-style` token can set `solid` instead, and pixel themes do. In pixel themes the pill also becomes a squared bar, because the shape follows the theme's `radius-scale`.

### 5.2 Compact mode: module card

- Default 380 x 520. Resizable between 320 x 400 and 560 x 720. Size is remembered per capability.
- Anchored beside the fox on whichever side has more room, with a 12 px gap. If there is no room, it opens above.
- Structure: header (48 px: icon, title, expand-to-fullscreen button, close), scrollable body, optional footer for primary action.
- Corner radius 20. Elevation level 3.
- Opens with a 200 ms scale and fade from the dock icon. Closes with 160 ms.
- Expand button sends the user to that capability's page in fullscreen mode with state carried over.

### 5.3 Speech chip

- A small rounded rectangle near the fox's head, max width 240, max two lines.
- Appears for 3 to 5 seconds, longer for reminders with actions.
- Text is short microcopy only, never a conversation.
- Pointer-events none unless it has action buttons.
- Respects reaction level: in `subtle` mode only reminders and errors show chips.

### 5.4 Quick action tray

- Appears above the dock, height 48, same pill surface.
- Contains exactly one control plus a confirm button.
- Escape or clicking away cancels with no side effects.

### 5.5 Fullscreen layout

```
+------+-----------------------------------------------------+
| fox  |  Top bar (56): title ........ bell  mode toggle     |
| rail |-----------------------------------------------------|
| 72   |                                                     |
|      |   Content (home dashboard or capability page)       |
| icons|   max width 1200, centered, 24 px gutters           |
|      |                                                     |
| gear |                                                     |
+------+-----------------------------------------------------+
```

- Rail: 72 wide collapsed (icons only), 220 wide expanded (icons plus labels). Fox mini (56 px) at the top. Home at the top of the list, settings at the bottom.
- Top bar: 56 tall.
- Content area uses a 12-column grid, 24 px gutters, 24 px outer padding. Capability pages may use a right panel of 360 px.
- Minimum window size 900 x 600.

### 5.6 Home dashboard

- A grid of widgets supplied by capabilities (slot `widget`) plus a few core widgets (today's notifications, recent activity, streaks).
- Widget sizes in grid units: 1x1 (3 columns), 2x1 (6 columns), 2x2 (6 columns by 2 rows). Row height 160.
- Each widget has a title row, a body, and one optional action. Clicking the title opens the capability page.
- Layout is user-editable (drag to reorder, resize between allowed sizes) and saved to config.
- Empty home: the fox in `curious_tilt` with a "Add your first module" button.

### 5.7 Mobile layout

- Home: fox at the top (about 40 percent of the screen height on small phones), dock as a horizontal row of large icons, widgets below as a scroll list.
- Card becomes a bottom sheet with snap points at 50 percent and 100 percent.
- Respect safe areas (notch, home indicator). Touch targets at least 44 x 44.
- Fullscreen module pages use a standard top app bar with back navigation.

## 6. Settings UI structure

Fullscreen mode (and the sheet on mobile) has:

1. **Frosty**: tone, locale, reaction level (full, subtle, off), reduced motion, size, skin, run setup again.
2. **Hotkeys**: summon, fullscreen, open dock item N, with conflict warnings.
3. **Modules**: the module manager (section 4.3).
4. **Notifications**: do-not-disturb schedule, per-capability mute, OS fallback toggle, how long chips stay.
5. **Appearance**: theme picker with live preview, mode (system, light, dark), accent override, font scale, open themes folder, theme editor (later).
6. **Data**: Frosty Home location, export, import, backups, open config file.
7. **About**: version, diagnostics copy button, licenses.

Every setting shown has a matching key in `frosty.toml`. The settings screen shows a small "In config: `personality.tone`" hint on hover so the link between UI and file is visible.

## 7. Capability UI contract

Every capability UI follows this so the whole app feels like one thing. The shell will enforce what it can.

### 7.1 Slots

| Slot | Constraints |
|------|-------------|
| `dock` | A 24 px monochrome SVG icon that follows `currentColor`. Optional numeric badge. |
| `card` | Must work from 320 to 560 px wide. No own window chrome. Scrolls inside the body. |
| `page` | Must work from 600 px to 1200 px content width. May use the right panel. |
| `widget` | Provides 1x1 and optionally larger variants. Glanceable, no long scrolling. |
| `quick-action` | One control and one confirm action. |
| `settings` | Optional custom view. Otherwise the generated form is used. |

### 7.2 Rules

1. Use shared components from `@frosty/ui` and design tokens only. No hard-coded colors or fonts.
2. Support light and dark themes automatically through tokens.
3. Fully keyboard navigable with visible focus rings.
4. Provide loading, empty, and error states for every view (section 14).
5. Respect reduced motion and the font scale.
6. Never open extra OS windows. Navigation stays inside card or page.
7. Emit events for meaningful user actions so reactions can fire. Do not play character animations directly.
8. Destructive actions need a confirm step or an undo toast.
9. Strings go through the microcopy and i18n system so tone and Tamil locale can be added later.
10. The UI is always TypeScript and React, whatever language the capability's logic uses. UI code talks to its logic only through tools and events, never through shared in-memory state, so the logic can later move into another process without changing the UI.
11. Handle a backend that is slow or not running: show a loading state, then a clear "backend not running" error state with a Restart action. Never freeze the card.

## 8. Reactions and character behavior

### 8.1 Reaction catalog (starting set)

| Event class | Animation | Intensity | Chip shown in subtle mode |
|-------------|-----------|-----------|---------------------------|
| First summon of the day | `wave` | 0.4 | no |
| Item created | `happy_small` | 0.3 | no |
| Item completed | `celebrate` | 0.3 | no |
| Streak milestone | `celebrate` | 0.8 | yes |
| Setting saved | `happy_small` | 0.2 | no |
| Empty state shown | `curious_tilt` | 0.3 | no |
| Reminder due | `nudge` | 0.6 | yes |
| Do-not-disturb enabled | `sleepy` | 0.5 | no |
| Error | `oops` | 0.5 | yes |
| Long loading | `thinking` | n/a | no |
| Onboarding finished | `celebrate` | 0.5 | yes |
| Quit | `bye` | 0.4 | no |

### 8.2 Rules

- Cooldown: at most one reaction per 2 seconds, one `celebrate` per 10 seconds. Excess reactions are dropped, not queued.
- A reaction never blocks input. The user can keep clicking while it plays.
- Reaction level `subtle` shrinks amplitude and hides most chips. `off` plays no reactions except reminders and errors, which still show a static chip.
- Reduced motion replaces animations with a brief pose change and fade (section 12).

### 8.3 Idle behavior

- Small randomized idle loops (blink, ear twitch, tail flick) only while visible and not in a reaction.
- No wandering around the screen in v1. The fox stays where you put it.
- After 10 minutes visible with no interaction, the fox can switch to `sleepy` (setting, default on). Any interaction wakes it.

### 8.4 Microcopy and tone

All text from Frosty is short, data-driven, and varies by tone. Several variants exist per key so it does not repeat.

| Key | Casual | Neutral |
|-----|--------|---------|
| `summon.first_today` | hey, you're back. | Good to see you. |
| `tasks.completed` | nice, one down. | Task completed. |
| `tasks.created` | got it. | Task added. |
| `reminder.due` | psst, time for {title}. | Reminder: {title}. |
| `empty.generic` | nothing here yet. suspicious. | Nothing here yet. |
| `error.generic` | oops, that didn't work. | Something went wrong. |
| `dnd.on` | okay, quiet mode. | Do not disturb is on. |
| `streak.milestone` | {count} days in a row. look at you. | {count}-day streak. |

Style rules:
- One short sentence, usually under 8 words for casual.
- No exclamation overload, no emoji spam, no baby talk.
- Casual means relaxed and lowercase-friendly, not sloppy. Errors should still say what happened when it matters, with details in the module UI.
- Tamil locale packs should be authored by a person, not auto-translated, and checked for natural register.

## 9. Visual system

### 9.1 Color tokens (default theme: Frost)

The values below are the default theme, called Frost: icy blues and soft whites for the fox world, with a warm ember accent for the fox's identity color and for primary actions. Every color in the UI comes from semantic tokens, and a theme is just a set of values for those tokens (see section 13), so the whole look can change without touching components.

Core palette (Frost):

| Token | Value |
|-------|-------|
| `ice-50` | #F4F9FC |
| `ice-100` | #E6F1F8 |
| `ice-200` | #CCE3F1 |
| `ice-300` | #A6CDE6 |
| `ice-400` | #78B0D6 |
| `ice-500` | #4F92C0 |
| `ice-600` | #3A759E |
| `ice-700` | #2F5E80 |
| `ice-800` | #274D69 |
| `ice-900` | #1B3447 |
| `ember-400` | #FFB066 |
| `ember-500` | #F5913A |
| `ember-600` | #D97621 |
| `success` | #3FAE7A |
| `warning` | #E3A02B |
| `danger` | #D9534F |

Semantic tokens (Frost):

| Token | Light | Dark |
|-------|-------|------|
| `bg` | #F4F9FC | #0F1A22 |
| `surface` | #FFFFFF | #16242E |
| `surface-raised` | #FFFFFF | #1D2F3B |
| `border` | #D5E4EE | #2A3F4D |
| `text` | #1B2A35 | #E8F1F7 |
| `text-muted` | #5C7384 | #9DB4C3 |
| `accent` | ember-500 | ember-400 |
| `accent-on` (text on accent) | #1B1206 | #1B1206 |
| `focus-ring` | ice-500 | ice-300 |

All text and control combinations must be checked against WCAG AA contrast before the palette is finalized. These values are a starting point, not verified.

### 9.2 Typography

- UI text: Inter or a system UI stack.
- Display and Frosty's chips: a rounded friendly face such as Nunito for personality. Pixel themes may set a pixel font for display text and chips instead (see 13.2). Body text always stays a readable UI font, and Tamil falls back to Noto Sans Tamil because pixel fonts rarely cover Tamil.
- Tamil: Noto Sans Tamil as the fallback and as the primary face when the locale is Tamil, with line height tuned for Tamil glyphs (they need more vertical room).
- Scale (px): 12, 14, 16, 20, 24, 32. Body 14 in cards, 16 in fullscreen.
- Line height: 1.4 for body, 1.2 for headings, 1.6 for Tamil body text.
- Font scale setting multiplies the whole scale (0.9 to 1.5).

### 9.3 Spacing, radius, elevation

- Base unit 4 px. Spacing tokens: 4, 8, 12, 16, 24, 32, 48.
- Radius: 8 (controls), 12 (cards inside cards), 20 (module card), 28 and full (pills, dock).
- Elevation levels:
  1. Resting surfaces: 1 px border, no shadow.
  2. Raised (widgets): soft shadow, low spread.
  3. Floating (module card, dock): larger soft shadow plus 1 px border, tinted toward ice-900 at low opacity.
  4. Modal sheet: strongest shadow with scrim.
- Transparency and blur: used on dock and chips where the platform supports it and the theme's `surface-style` allows it. Always provide a solid fallback so legibility never depends on blur.

### 9.4 Iconography

- 24 px grid, 1.75 px stroke, rounded caps and joins, monochrome using `currentColor`.
- One consistent icon set for core UI (Lucide-style) and capabilities must follow the same grid and stroke.
- Capability icons also need to work at 20 px in the rail and 40 px in the dock button.

### 9.5 Motion tokens

| Token | Value | Use |
|-------|-------|-----|
| `motion-fast` | 120 ms | Hovers, toggles |
| `motion-base` | 200 ms | Card open, fades |
| `motion-slow` | 320 ms | Mode switches |
| `motion-character` | 480 ms | Summon spring |
| `ease-standard` | cubic-bezier(0.2, 0, 0, 1) | General |
| `ease-exit` | cubic-bezier(0.4, 0, 1, 1) | Dismiss, close |
| `spring-summon` | stiffness 260, damping 20 | Fox arrival |

Rules: animate transform and opacity only where possible. Never animate layout of large lists. Nothing loops while hidden.

## 10. Component inventory

Core components in `@frosty/ui`, all themed through tokens:

- Layout: Card, Panel, Sheet (bottom sheet and modal), Divider, Grid, Stack.
- Navigation: Rail, TopBar, TabBar (mobile), Breadcrumb.
- Inputs: Button (primary, secondary, ghost, danger), IconButton, TextField, NumberStepper, Select, MultiSelect, Toggle, Checkbox, Radio, Slider, DatePicker, TimePicker, ColorPicker, PathPicker, KeybindingRecorder, ListEditor.
- Feedback: Toast (with undo), Banner, Badge, Tooltip, Progress, Skeleton, SpeechChip.
- Data display: List, ListItem, Table, Stat, StreakBadge, Chart (minimal, for later).
- Frosty-specific: FoxView (character renderer), Dock, DockItem, QuickActionTray, ModuleCard (frame for capability cards), WidgetFrame, PermissionSheet, SettingsForm (generated from JSON Schema), BackendStatusBadge.

Every component must document its keyboard behavior and accessibility labels.

## 11. Responsive and platform behavior

| Aspect | Desktop | Mobile |
|--------|---------|--------|
| Primary surface | Overlay (compact) plus window (fullscreen) | App screens |
| Summon | Hotkey, tray | App icon, widget (later), notification tap |
| Module card | Floating card beside fox | Bottom sheet |
| Dock | Pill of icons attached to fox | Icon row on home, tab bar in modules |
| Hover states | Yes | No, use long-press for quick actions |
| Fullscreen | Optional mode | Default |

The mobile column means Android, which comes after the Windows app.

Desktop DPI: all fox and UI assets must look right at 100, 125, 150, and 200 percent scaling. Pixel assets must scale by whole numbers (use `image-rendering: pixelated`). At 125 and 150 percent Windows scaling, pick the nearest whole multiple of the asset's native size or draw at the device pixel ratio so pixels stay crisp.

Window rules on desktop: the overlay never takes keyboard focus until the user clicks into a card or field. After a quick action completes, focus returns to the previously active app.

## 12. Accessibility

- Everything reachable by keyboard: summon hotkey, Tab through dock and card, Enter to open, Esc to close, arrow keys in the dock.
- Visible focus ring on all interactive elements (2 px, `focus-ring` token, 2 px offset).
- Screen reader support: dock items have labels and badge counts, the card is a labelled dialog, focus moves into it on open and returns to the dock icon on close.
- The fox is decorative (hidden from screen readers) but every reaction that carries meaning has a text equivalent announced through a polite live region (for example "Task completed").
- Reduced motion: follows the OS setting by default, overridable. Replaces summon and reactions with a short fade and a static pose change. No parallax, no sparkle bursts.
- Contrast: AA for text and UI components in both themes. Verify with a checker before locking tokens.
- Text scaling up to 200 percent without clipping in cards (cards scroll, they do not truncate content).
- Do not rely on color alone for status (use icons or labels with the colors).
- Click-through overlay: make sure assistive tech and keyboard users can still reach the fox and dock. When the overlay is hidden it must not trap focus.
- Hotkey conflicts are explained in text, not only by color.

## 13. Theming and skins

### 13.1 Themes vs skins

- A **theme** changes the UI colors (and optionally radius and shadow feel). A **skin** changes the fox art. They are independent: any theme works with any skin.
- Themes are files, matching the config philosophy: pick one in the UI, or drop a file into the `themes/` folder.

### 13.2 Theme file

A theme is a TOML file with a name, the modes it supports (`light`, `dark`, or both), and a value for every required semantic token in each mode. Required tokens: `bg`, `surface`, `surface-raised`, `border`, `text`, `text-muted`, `accent`, `accent-on`, `focus-ring`, `success`, `warning`, `danger`. Optional: `radius-scale`, `shadow-strength`, `shadow-style` (`soft` or `hard`), `border-width`, `surface-style` (`solid` or `blur`, default `blur` where supported), `font-display`, `fox-tint`.

```toml
[theme]
id = "frost"
name = "Frost"
modes = ["light", "dark"]

[dark]
bg = "#0F1A22"
surface = "#16242E"
# remaining required tokens go here
```

Rules:
- Components use tokens only (see 7.2), so any valid theme works everywhere, including capability UIs.
- A theme missing a required token for a mode it declares is rejected with a clear message, and the previous theme stays active.
- If the user's mode is not supported by the theme, the shell uses the mode the theme does support and says so in the picker.
- The picker applies a theme live as a preview and can be cancelled.
- The theme editor (edit tokens with live preview and contrast warnings against AA) is a later phase. Until then, themes are edited as files.

### 13.3 Starting themes and the pixel direction

Direction (decided): themes use flat solid colors with a pixel or retro game feel. No gradients, no glow, no neon, and no default purple or teal. The aim is a look that someone made on purpose, not one that looks generated.

Rules for every pixel-family theme:
- Limited palette, 16 colors or fewer per theme.
- Hard borders (2 px), corner radius 0 or stepped corners, no soft rounded pills.
- Hard offset shadows (a solid block with no blur) via `shadow-style = "hard"`.
- No gradients and no glow. `surface-style = "solid"` by default.
- A pixel font for display text and chips (candidates: Pixelify Sans, Silkscreen). Body text stays a readable UI font.
- Pixel assets scale by whole numbers only.

Starting list. Seed values only, to be tuned and contrast-checked against AA before shipping:

| Theme | Modes | Character | Seed values |
|-------|-------|-----------|-------------|
| Frost | light, dark | Default. Smooth and calm, icy blue with an ember accent. Keeps the subtle blur where supported. | See section 9.1 |
| Frost Pixel | light, dark | The same hues stepped down to a limited palette, with hard borders and hard shadows | Derived from the Frost palette once the pixel art direction is set |
| Cartridge | light | Classic handheld four-tone green | bg #9BBC0F, surface #8BAC0F, border #306230, text #0F380F, accent #306230, accent-on #9BBC0F |
| Snowfield | light, dark | Limited cool whites and blues, drawn to go with a pixel fox | To be designed with the fox art |
| High Contrast | light, dark | Maximum legibility | To be defined with AA and AAA checks |

Note on the earlier starting list: the Aurora, Snowdrift, and Ember Dusk themes were dropped because they read as generic modern UI themes rather than the chosen direction.

### 13.4 Accent override

The accent token can be overridden in settings without making a new theme (a color picker with an AA check against `accent-on`). The override is stored in `frosty.toml`.

### 13.5 Skins

- A skin may optionally provide matching accent suggestions in `skin.toml`.
- Skin loading is validated against the contract in section 2.5, with a clear error message if inputs are missing.

## 14. Empty, loading, and error states

Every view that can be empty, loading, or failed needs all three.

| State | Visual | Frosty behavior |
|-------|--------|-----------------|
| Loading under 400 ms | Nothing special | None |
| Loading over 400 ms | Skeleton placeholders | `thinking` on |
| Empty | Simple illustration or fox pose, one-line explanation, one primary action | `curious_tilt` once |
| Error (recoverable) | Inline banner with plain message and Retry | `oops` once, chip only if not subtle |
| Error (capability crashed) | Card replaced with an error panel, Copy details button, Disable button | `oops` once |
| Backend not running (capability with a separate backend) | Small panel saying what stopped, a Restart button, and Copy details | `oops` once |
| Offline (for adapters) | Small banner that native data still works | None |

Error copy rules: say what happened in plain words, say what the user can do, keep technical detail behind a Copy details button.

## 15. Hotkeys (defaults, all configurable)

| Action | Windows and Linux | macOS |
|--------|-------------------|-------|
| Summon or dismiss | Ctrl+Alt+F | Cmd+Option+F |
| Toggle fullscreen | Ctrl+Alt+Shift+F | Cmd+Option+Shift+F |
| Open dock item 1 to 9 | Ctrl+Alt+1 to 9 | Cmd+Option+1 to 9 |
| Close card, then dismiss | Esc | Esc |

These defaults are placeholders. The first run and the settings UI both test for conflicts and warn. Windows is the primary target, so the Windows column is the one to test first.

## 16. Out of scope for the first design pass

- Final character art and a full animation set beyond the list above.
- Ambient behaviors (reacting to what you do outside Frosty).
- A free-text command line or any chat-like input.
- Android floating overlay.
- Marketplace or browsing UI for capabilities.
- Localization beyond English, apart from making the system ready for Tamil.

## 17. Decisions and open design questions

### Decided in review

- The dock is a horizontal pill attached to the fox. In pixel themes it becomes a squared bar.
- The module card opens beside the fox, not perched on it.
- Themes are files. The direction is flat solid colors with a pixel or retro game feel, avoiding gradients, glow, and a generic AI-generated look (section 13).
- Surfaces use subtle blur where supported, with a solid fallback. A theme can set its own surface style, and pixel themes default to solid.
- Platform: Windows 10 and Windows 11 are both supported. Android later. No iOS.
- Tone is a manual setting for v1. Mirror mode is deferred.
- Capability logic is TypeScript first and Python backends come later, so capability UIs must stay backend-agnostic (section 7.2, rules 10 and 11).
- Reminders wait in history while a fullscreen app is in front.
- Data lives in the standard app-data folder.
- English only in v1, with all strings in data files so Tamil can be added later without code changes.

### Still open

1. **Should the fox itself be pixel art** to match the theme direction, or stay a smooth cartoon? This also decides the animation tech. Pixel art usually means sprite sheets drawn to a canvas with nearest-neighbor scaling, so the skin contract in section 2.5 would need a sprite-sheet version next to the Rive one.
2. **Which pixel themes first** beyond Frost (starting ideas in 13.3), and is a theme editor UI needed early?
3. **Pixel font** for display text and chips. Candidates are Pixelify Sans and Silkscreen. Body text stays a readable UI font.
4. **Should the fox follow the cursor with its eyes** (`lookX`, `lookY`)? It adds life but costs a little CPU and needs pointer tracking over a click-through window.
5. **Sound:** any sound effects at all (optional, off by default), or silent?
6. **Idle sleepy state:** is falling asleep after 10 idle minutes charming or annoying?
7. **Home dashboard default:** should it open on home or on the last used module?
