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
    dnd_schedule_enabled: boolean;
    dnd_start: string;
    dnd_end: string;
    os_fallback: boolean;
    fullscreen_behavior: string;
    muted: string[];
    chip_seconds: number;
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

export type CapabilityState = "disabled" | "enabling" | "enabled" | "error";

export interface PermissionLine {
  key: string;
  text: string;
  sensitive: boolean;
}

export interface ToolInfo {
  id: string;
  title: string;
  quick_action: boolean;
  quick_action_control: string | null;
  primary_field: string | null;
  primary_min: number | null;
  primary_max: number | null;
}

export interface NotificationAction {
  label: string;
  tool: string;
  input?: Record<string, unknown>;
}

export interface NotificationRecord {
  id: string;
  owner: string;
  title: string;
  body: string;
  actions: NotificationAction[];
  created_at: number;
  state: "held" | "delivered" | "dismissed" | "dropped_muted" | "dropped_rate_limited";
  hold_reason: "do_not_disturb" | "fullscreen_app" | null;
  delivered_at: number | null;
  read_at: number | null;
  dismissed_at: number | null;
}

/** What the core sends the overlay when a notification is shown. */
export interface NudgeInfo {
  id: string;
  owner: string;
  title: string;
  body: string;
  actions: NotificationAction[];
  seconds: number;
}

export interface DndStatus {
  manual: boolean;
  active: boolean;
  fullscreen_app: boolean;
}

export interface CapabilityInfo {
  id: string;
  name: string;
  version: string;
  description: string;
  runtime: string;
  origin: "built_in" | "user";
  state: CapabilityState;
  error: string | null;
  error_count: number;
  tools: ToolInfo[];
  permissions: PermissionLine[];
  has_settings: boolean;
  entry_url: string;
  views_url: string | null;
  icon_url: string | null;
  badge: number;
}

export interface CapabilityList {
  capabilities: CapabilityInfo[];
  rejected: { dir: string; errors: string[] }[];
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
  summonPainted: () => invoke<void>("summon_painted"),
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
  listCapabilities: () => invoke<CapabilityList>("list_capabilities"),
  enableCapability: (id: string, confirmedShell = false) =>
    invoke<void>("enable_capability", { id, confirmedShell }),
  disableCapability: (id: string) => invoke<void>("disable_capability", { id }),
  capabilityFailed: (id: string, message: string) =>
    invoke<void>("capability_failed", { id, message }),
  rescanCapabilities: () => invoke<void>("rescan_capabilities"),
  removeCapability: (id: string, deleteData: boolean) =>
    invoke<void>("remove_capability", { id, deleteData }),
  installCapabilityFromFolder: () => invoke<string | null>("install_capability_from_folder"),
  capabilityReveal: (id: string, which: "folder" | "config") =>
    invoke<void>("capability_reveal", { id, which }),
  getCapabilitySchema: (id: string) => invoke<unknown>("get_capability_schema", { id }),
  getCapabilitySettings: (id: string) =>
    invoke<Record<string, unknown>>("get_capability_settings", { id }),
  setCapabilitySettings: (id: string, values: Record<string, unknown>) =>
    invoke<void>("set_capability_settings", { id, values }),
  callTool: (tool: string, input: Record<string, unknown> = {}) =>
    invoke<unknown>("call_tool", { tool, input }),
  reveal: (which: "home" | "config" | "themes") => invoke<void>("reveal", { which }),
  listNotifications: (limit = 50) => invoke<NotificationRecord[]>("list_notifications", { limit }),
  markNotificationsRead: () => invoke<void>("notifications_mark_read"),
  dismissNotification: (id: string) => invoke<void>("dismiss_notification", { id }),
  runNotificationAction: (id: string, index: number) =>
    invoke<unknown>("run_notification_action", { id, index }),
  getDnd: () => invoke<DndStatus>("get_dnd"),
  setDndManual: (on: boolean) => invoke<void>("set_dnd_manual", { on }),
  restorePreviousFocus: () => invoke<void>("restore_previous_focus"),
};

export function on<T>(event: string, handler: (payload: T) => void): Promise<UnlistenFn> {
  return listen<T>(event, (e) => handler(e.payload));
}
