/* eslint-disable @typescript-eslint/no-explicit-any --
   Bridge payloads and tool results are untyped JSON in these tests. */
/**
 * An in-memory stand-in for the Frosty core, for tests. It implements just
 * enough of the bridge for a capability: a table store with the same
 * filter, ordering, soft-delete and restore rules as the Rust host, a
 * recording scheduler and notifier, and an event log.
 */
import { createContext } from "@frosty/sdk/host";
import type { Bridge, Ctx, ToolHandler } from "@frosty/sdk";

type Row = Record<string, unknown> & { id: string };

export interface RecordedJob {
  id: string;
  at: number;
  action: { tool?: string; event?: string; input?: Record<string, unknown> };
  cancelled: boolean;
}

export interface RecordedNotification {
  title: string;
  body: string;
  actions: { label: string; tool: string; input?: Record<string, unknown> }[];
}

export function fakeCore(settings: Record<string, unknown> = {}) {
  const tables = new Map<string, Row[]>();
  const jobs: RecordedJob[] = [];
  const notifications: RecordedNotification[] = [];
  const events: { name: string; payload: Record<string, unknown>; significance?: number }[] = [];
  const badges: number[] = [];
  let nextId = 1;
  let nextJob = 1;
  let clock = Date.now();

  const rowsOf = (table: string) => {
    let rows = tables.get(table);
    if (!rows) {
      rows = [];
      tables.set(table, rows);
    }
    return rows;
  };
  const live = (table: string) => rowsOf(table).filter((r) => r.deleted_at === null);

  const bridge: Bridge = {
    async invoke(command, args) {
      const a = (args ?? {}) as Record<string, any>;
      switch (command) {
        case "get_capability_settings":
          return settings as never;
        case "cap_db": {
          const { op, table, args: p } = a;
          clock += 1;
          if (op === "insert") {
            const row: Row = {
              id: `row-${nextId++}`,
              created_at: clock,
              updated_at: clock,
              deleted_at: null,
              device_id: "test",
              ...p.record,
            };
            rowsOf(table).push(row);
            return { ...row } as never;
          }
          if (op === "get") {
            const row = live(table).find((r) => r.id === p.id);
            return (row ? { ...row } : null) as never;
          }
          if (op === "query") {
            let rows = live(table);
            for (const [k, v] of Object.entries(p.filter ?? {})) {
              rows = rows.filter((r) =>
                v === null ? r[k] === null || r[k] === undefined : r[k] === v,
              );
            }
            const by = p.order_by ?? "created_at";
            rows = [...rows].sort((x, y) => {
              const d = Number(x[by] ?? 0) - Number(y[by] ?? 0);
              return p.descending ? -d : d;
            });
            return rows.slice(0, p.limit ?? 1000).map((r) => ({ ...r })) as never;
          }
          if (op === "update") {
            const row = live(table).find((r) => r.id === p.id);
            if (!row) return null as never;
            Object.assign(row, p.patch, { updated_at: clock });
            return { ...row } as never;
          }
          if (op === "soft_delete") {
            const row = live(table).find((r) => r.id === p.id);
            if (!row) return false as never;
            row.deleted_at = clock;
            return true as never;
          }
          if (op === "restore") {
            const row = rowsOf(table).find((r) => r.id === p.id && r.deleted_at !== null);
            if (!row) return null as never;
            row.deleted_at = null;
            return { ...row } as never;
          }
          throw new Error(`unexpected db op ${op}`);
        }
        case "cap_emit":
          events.push({ name: a.name, payload: a.payload, significance: a.significance });
          return undefined as never;
        case "cap_badge":
          badges.push(a.value);
          return undefined as never;
        case "cap_notify":
          notifications.push({ title: a.title, body: a.body, actions: a.actions ?? [] });
          return undefined as never;
        case "cap_schedule": {
          if (a.op === "at") {
            const job: RecordedJob = {
              id: `job-${nextJob++}`,
              at: a.args.at_ms,
              action: a.args.action,
              cancelled: false,
            };
            jobs.push(job);
            return job.id as never;
          }
          if (a.op === "cancel") {
            const job = jobs.find((j) => j.id === a.args.job_id && !j.cancelled);
            if (job) job.cancelled = true;
            return Boolean(job) as never;
          }
          throw new Error(`unexpected scheduler op ${a.op}`);
        }
        case "cap_log":
          return undefined as never;
        default:
          throw new Error(`unexpected command ${command}`);
      }
    },
    async listen() {
      return () => {};
    },
  };

  return {
    bridge,
    jobs,
    notifications,
    events,
    badges,
    /** Active (not cancelled) reminder jobs. */
    pendingJobs: () => jobs.filter((j) => !j.cancelled),
    rows: (table: string) => live(table),
  };
}

/** Activates a capability against the fake core and returns a tool caller. */
export async function activate(
  definition: { onActivate(ctx: Ctx): Promise<void> | void },
  id: string,
  settings?: Record<string, unknown>,
) {
  const core = fakeCore(settings);
  const tools = new Map<string, ToolHandler>();
  const ctx = createContext(id, core.bridge, {
    tools,
    subscriptions: new Set(),
    configHandlers: new Set(),
  });
  await definition.onActivate(ctx);
  const call = async (tool: string, input: unknown = {}): Promise<any> => {
    const handler = tools.get(tool);
    if (!handler) throw new Error(`no tool ${tool}`);
    return (handler as (i: unknown) => Promise<unknown>)(input);
  };
  return { ...core, ctx, call, tools };
}
