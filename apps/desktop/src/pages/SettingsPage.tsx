import { useEffect, useState } from "react";
import { Button, Field, Segmented, Select, Toggle } from "@frosty/ui";

import {
  api,
  type FoxSize,
  type FrostyConfig,
  type ModeSetting,
  type Paths,
  type ReactionLevel,
  type ReducedMotion,
  type ThemeInfo,
  type Tone,
} from "../bridge";

function set(key: string, value: unknown) {
  void api.setConfigValue(key, value).catch((e) => console.error(`could not set ${key}`, e));
}

/** Saves on Enter or blur, so half-typed hotkeys are never applied. */
function HotkeyField({
  configKey,
  value,
  label,
}: {
  configKey: string;
  value: string;
  label: string;
}) {
  // `draft` is only set while the user is typing; otherwise show the config.
  const [draft, setDraft] = useState<string | null>(null);
  const text = draft ?? value;
  const setText = (next: string) => setDraft(next);
  const commit = () => {
    if (draft !== null && draft.trim() && draft !== value) set(configKey, draft.trim());
    setDraft(null);
  };
  return (
    <input
      className="f-input"
      aria-label={label}
      value={text}
      onChange={(e) => setText(e.target.value)}
      onBlur={commit}
      onKeyDown={(e) => e.key === "Enter" && commit()}
    />
  );
}

/** A time of day, saved when it changes. */
function TimeField({
  configKey,
  value,
  label,
}: {
  configKey: string;
  value: string;
  label: string;
}) {
  return (
    <input
      className="f-input"
      type="time"
      aria-label={label}
      value={value}
      onChange={(e) => e.target.value && set(configKey, e.target.value)}
    />
  );
}

export function SettingsPage({ config }: { config: FrostyConfig }) {
  const [themes, setThemes] = useState<ThemeInfo[]>([]);
  const [paths, setPaths] = useState<Paths | null>(null);

  useEffect(() => {
    void api.listThemes().then(setThemes);
    void api.getPaths().then(setPaths);
  }, []);

  return (
    <div className="settings">
      <section aria-labelledby="s-frosty">
        <h2 id="s-frosty">Frosty</h2>
        <Field label="Tone" hint="How Frosty talks." configKey="personality.tone">
          <Segmented<Tone>
            label="Tone"
            value={config.personality.tone}
            options={[
              { value: "casual", label: "Casual" },
              { value: "neutral", label: "Neutral" },
            ]}
            onChange={(v) => set("personality.tone", v)}
          />
        </Field>
        <Field
          label="Reactions"
          hint="Subtle shrinks them; off keeps only reminders and errors."
          configKey="character.reactions"
        >
          <Segmented<ReactionLevel>
            label="Reactions"
            value={config.character.reactions}
            options={[
              { value: "full", label: "Full" },
              { value: "subtle", label: "Subtle" },
              { value: "off", label: "Off" },
            ]}
            onChange={(v) => set("character.reactions", v)}
          />
        </Field>
        <Field label="Size" configKey="character.size">
          <Segmented<FoxSize>
            label="Size"
            value={config.character.size}
            options={[
              { value: "small", label: "Small" },
              { value: "medium", label: "Medium" },
              { value: "large", label: "Large" },
            ]}
            onChange={(v) => set("character.size", v)}
          />
        </Field>
        <Field
          label="Reduced motion"
          hint="System follows your Windows setting."
          configKey="character.reduced_motion"
        >
          <Segmented<ReducedMotion>
            label="Reduced motion"
            value={config.character.reduced_motion}
            options={[
              { value: "system", label: "System" },
              { value: "on", label: "On" },
              { value: "off", label: "Off" },
            ]}
            onChange={(v) => set("character.reduced_motion", v)}
          />
        </Field>
        <Field label="Start on login" configKey="core.start_on_login">
          <Toggle
            label="Start on login"
            checked={config.core.start_on_login}
            onChange={(v) => set("core.start_on_login", v)}
          />
        </Field>
      </section>

      <section aria-labelledby="s-hotkeys">
        <h2 id="s-hotkeys">Hotkeys</h2>
        <Field
          label="Summon or dismiss"
          hint="Press Enter to apply. Conflicts are reported at the top of the page."
          configKey="hotkeys.summon"
        >
          <HotkeyField
            label="Summon hotkey"
            configKey="hotkeys.summon"
            value={config.hotkeys.summon}
          />
        </Field>
        <Field label="Toggle fullscreen" configKey="hotkeys.fullscreen">
          <HotkeyField
            label="Fullscreen hotkey"
            configKey="hotkeys.fullscreen"
            value={config.hotkeys.fullscreen}
          />
        </Field>
      </section>

      <section aria-labelledby="s-notifications">
        <h2 id="s-notifications">Notifications</h2>
        <Field
          label="Nightly do not disturb"
          hint="Reminders wait until the window ends."
          configKey="notifications.dnd_schedule_enabled"
        >
          <Toggle
            label="Nightly do not disturb"
            checked={config.notifications.dnd_schedule_enabled}
            onChange={(v) => set("notifications.dnd_schedule_enabled", v)}
          />
        </Field>
        <Field label="From" configKey="notifications.dnd_start">
          <TimeField
            label="Do not disturb starts"
            configKey="notifications.dnd_start"
            value={config.notifications.dnd_start}
          />
        </Field>
        <Field label="Until" configKey="notifications.dnd_end">
          <TimeField
            label="Do not disturb ends"
            configKey="notifications.dnd_end"
            value={config.notifications.dnd_end}
          />
        </Field>
        <Field
          label="Reminders stay on screen for"
          hint="Then they move to the notification list."
          configKey="notifications.chip_seconds"
        >
          <input
            className="f-input"
            type="number"
            min={5}
            max={120}
            aria-label="Seconds a reminder stays on screen"
            value={config.notifications.chip_seconds}
            onChange={(e) => {
              const n = Number(e.target.value);
              if (n >= 5 && n <= 120) set("notifications.chip_seconds", n);
            }}
          />
        </Field>
        <Field
          label="Use a Windows notification if Frosty cannot appear"
          configKey="notifications.os_fallback"
        >
          <Toggle
            label="Use a Windows notification as a fallback"
            checked={config.notifications.os_fallback}
            onChange={(v) => set("notifications.os_fallback", v)}
          />
        </Field>
      </section>

      <section aria-labelledby="s-appearance">
        <h2 id="s-appearance">Appearance</h2>
        <Field label="Theme" configKey="appearance.theme">
          <Select
            label="Theme"
            value={config.appearance.theme}
            options={themes.map((t) => ({ value: t.id, label: t.name }))}
            onChange={(v) => set("appearance.theme", v)}
          />
        </Field>
        <Field label="Mode" configKey="appearance.mode">
          <Segmented<ModeSetting>
            label="Mode"
            value={config.appearance.mode}
            options={[
              { value: "system", label: "System" },
              { value: "light", label: "Light" },
              { value: "dark", label: "Dark" },
            ]}
            onChange={(v) => set("appearance.mode", v)}
          />
        </Field>
      </section>

      <section aria-labelledby="s-data">
        <h2 id="s-data">Data</h2>
        <Field label="Frosty Home" hint={paths?.home}>
          <Button onClick={() => void api.reveal("home")}>Open folder</Button>
        </Field>
        <Field label="Config file" hint={paths?.config_file}>
          <Button onClick={() => void api.reveal("config")}>Show file</Button>
        </Field>
        <Field label="Themes folder" hint={paths?.themes}>
          <Button onClick={() => void api.reveal("themes")}>Open folder</Button>
        </Field>
      </section>
    </div>
  );
}
