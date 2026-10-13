import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import type { ResolvedTheme } from "@frosty/ui";

export type Tone = "casual" | "neutral";
export type ReactionLevel = "full" | "subtle" | "off";
export type ModeSetting = "system" | "light" | "dark";
export type ReducedMotion = "system" | "on" | "off";
export type FoxSize = "small" | "medium" | "large";

/** Mirrors `FrostyConfig` in crates/frosty-core/src/config.rs. */
export interface FrostyConfig {
  core: { config_version: number; device_name: string; start_on_login: boolean };
  appearance: { theme: string; mode: ModeSetting };
  hotkeys: { summon: string; fullscreen: string };
  character: {
    skin: string;
    size: FoxSize;
    reactions: ReactionLevel;
    reduced_motion: ReducedMotion;
  };
  personality: { tone: Tone; locale: string; never_react: string[] };
  notifications: {
    dnd_start: string;
    dnd_end: string;
    os_fallback: boolean;
    fullscreen_behavior: string;
  };
  capabilities: { enabled: string[]; dock_order: string[] };
}

export interface Notice {
  source: string;
  level: "error" | "info";
  message: string;
}

export interface ThemeInfo {
  id: string;
  name: string;
  modes: ("light" | "dark")[];
}

export interface Paths {
  home: string;
  config_file: string;
  themes: string;
}

export type Decision =
  | {
      kind: "play";
      animation: string | null;
      intensity: number;
      text: string | null;
      show_chip: boolean;
      key: string | null;
    }
  | { kind: "dropped"; reason: string };

export interface DecisionPayload {
  event: string;
  decision: Decision;
}

export interface HitRect {
  x: number;
  y: number;
  w: number;
  h: number;
}

export const api = {
  getConfig: () => invoke<FrostyConfig>("get_config"),
  setConfigValue: (key: string, value: unknown) => invoke<void>("set_config_value", { key, value }),
  fireEvent: (name: string, payload: unknown = {}, significance?: number) =>
    invoke<void>("fire_event", { name, payload, significance }),
  reportHitRects: (rects: HitRect[]) => invoke<void>("report_hit_rects", { rects }),
  overlayHidden: () => invoke<void>("overlay_hidden"),
  toggleSummon: () => invoke<void>("toggle_summon"),
  toggleFullscreen: () => invoke<void>("toggle_fullscreen"),
  openMain: (page?: string) => invoke<void>("open_main", { page }),
  listThemes: () => invoke<ThemeInfo[]>("list_themes"),
  resolveTheme: (systemDark: boolean) => invoke<ResolvedTheme>("resolve_theme", { systemDark }),
  getNotices: () => invoke<Notice[]>("get_notices"),
  getFoxPosition: () => invoke<string | null>("get_fox_position"),
  saveFoxPosition: (position: string) => invoke<void>("save_fox_position", { position }),
  getPaths: () => invoke<Paths>("get_paths"),
  reveal: (which: "home" | "config" | "themes") => invoke<void>("reveal", { which }),
};

export function on<T>(event: string, handler: (payload: T) => void): Promise<UnlistenFn> {
  return listen<T>(event, (e) => handler(e.payload));
}
