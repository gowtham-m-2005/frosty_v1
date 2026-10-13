import type { ComponentType } from "react";

/** A value that can cross the bridge. */
export type Json = null | boolean | number | string | Json[] | { [key: string]: Json };

/** Every row carries the sync columns the host fills in. */
export interface Row {
  id: string;
  created_at: number;
  updated_at: number;
  deleted_at: number | null;
  device_id: string;
  [column: string]: unknown;
}

export interface QueryOptions {
  /** Equality filters. `null` matches NULL. */
  filter?: Record<string, Json>;
  orderBy?: string;
  descending?: boolean;
  limit?: number;
}

export interface FrostyEvent {
  id: string;
  name: string;
  source: string;
  /** Unix time in milliseconds. */
  at: number;
  payload: Record<string, unknown>;
  significance: number | null;
}

export type Unsubscribe = () => void;

/** What a scheduled job does when it fires. */
export type ScheduledAction =
  | { event: string; payload?: Record<string, Json> }
  | { tool: string; input?: Record<string, Json> };

export interface ScheduleOptions {
  /** What to do for a run that was missed while Frosty was closed. */
  missed?: "fire_once_on_start" | "skip" | "fire_all";
}

export interface Db {
  insert(table: string, record: Record<string, Json>): Promise<Row>;
  get(table: string, id: string): Promise<Row | null>;
  query(table: string, options?: QueryOptions): Promise<Row[]>;
  update(table: string, id: string, patch: Record<string, Json>): Promise<Row | null>;
  /** Marks the row deleted; the data stays so a later sync can see it. */
  softDelete(table: string, id: string): Promise<boolean>;
}

/**
 * What a capability's logic can do. Every call is async, even though v1 runs
 * in the same process, so a capability keeps working if its logic later moves
 * to another process (docs/PROJECT_SPEC.md 8.2 and 8.9).
 *
 * Only what the manifest's permissions allow actually works; the host checks
 * every call.
 */
export interface Ctx {
  readonly id: string;
  db: Db;
  events: {
    /** The event must be declared in the manifest. */
    emit(name: string, payload?: Record<string, Json>, significance?: number): Promise<void>;
    /** Needs `events_subscribe` for namespaces other than your own. */
    subscribe(pattern: string, handler: (event: FrostyEvent) => void): Promise<Unsubscribe>;
  };
  tools: {
    register(id: string, handler: ToolHandler): Promise<void>;
    call(tool: string, input?: Record<string, Json>): Promise<unknown>;
  };
  views: {
    /** The number shown on the dock icon. 0 hides it. */
    setBadge(count: number): Promise<void>;
  };
  scheduler: {
    at(whenMs: number, action: ScheduledAction, options?: ScheduleOptions): Promise<string>;
    every(
      intervalMs: number,
      action: ScheduledAction,
      options?: ScheduleOptions & { firstAtMs?: number },
    ): Promise<string>;
    cancel(jobId: string): Promise<boolean>;
  };
  notify(title: string, body?: string): Promise<void>;
  config: {
    get(): Promise<Record<string, unknown>>;
    onChange(handler: (values: Record<string, unknown>) => void): Promise<Unsubscribe>;
  };
  log: {
    info(message: string): Promise<void>;
    warn(message: string): Promise<void>;
    error(message: string): Promise<void>;
  };
}

export type ToolHandler = (input: never) => Promise<unknown> | unknown;

export interface CapabilityDefinition {
  id: string;
  onActivate(ctx: Ctx): Promise<void> | void;
  /** Release timers and subscriptions. Data is kept. */
  onDeactivate?(ctx: Ctx): Promise<void> | void;
}

/** Props every view component receives. */
export interface ViewProps {
  /** Which slot is rendering: `card`, `page`, or `widget`. */
  slot: "card" | "page" | "widget";
}

/** The slots a capability can fill (docs/DESIGN_SPEC.md 7.1). */
export interface ViewsDefinition {
  card?: ComponentType<ViewProps>;
  page?: ComponentType<ViewProps>;
  widget?: ComponentType<ViewProps>;
  settings?: ComponentType<ViewProps>;
}

/** The bridge to the Frosty core. The shell supplies the real one. */
export interface Bridge {
  invoke<T = unknown>(command: string, args?: Record<string, unknown>): Promise<T>;
  listen<T = unknown>(event: string, handler: (payload: T) => void): Promise<() => void>;
}

/** Matches `*`, `ns.*`, `*.suffix`, or an exact event name. */
export function matchesPattern(pattern: string, name: string): boolean {
  if (pattern === "*") return true;
  if (pattern.endsWith(".*")) return name.split(".")[0] === pattern.slice(0, -2);
  if (pattern.startsWith("*.")) return name.split(".").slice(1).join(".") === pattern.slice(2);
  return pattern === name;
}
