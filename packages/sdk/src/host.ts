/**
 * The logic host: runs inside the hidden webview, loads each enabled
 * capability's logic entry, and serves tool calls and events to it. The shell
 * uses this; capabilities never import it.
 */
import type {
  Bridge,
  CapabilityDefinition,
  Ctx,
  FrostyEvent,
  Json,
  QueryOptions,
  Row,
  ToolHandler,
  Unsubscribe,
} from "./types";
import { matchesPattern } from "./types";

/** One capability as the core lists it. */
export interface CapabilityRecord {
  id: string;
  state: "disabled" | "enabling" | "enabled" | "error";
  entry_url: string;
}

interface ListResult {
  capabilities: CapabilityRecord[];
}

type Loader = (url: string) => Promise<{ default?: CapabilityDefinition }>;

interface Running {
  definition: CapabilityDefinition;
  ctx: Ctx;
  tools: Map<string, ToolHandler>;
  subscriptions: Set<{ pattern: string; handler: (event: FrostyEvent) => void }>;
  configHandlers: Set<(values: Record<string, unknown>) => void>;
}

function describeError(e: unknown): string {
  if (e instanceof Error) return e.message || e.name;
  return String(e);
}

/** Builds the `ctx` object for one capability. Exported for tests. */
export function createContext(
  id: string,
  bridge: Bridge,
  state: Pick<Running, "tools" | "subscriptions" | "configHandlers">,
): Ctx {
  const db = (op: string, table: string, args: Record<string, unknown>) =>
    bridge.invoke("cap_db", { capability: id, op, table, args });
  const log = (level: string) => (message: string) =>
    bridge.invoke<void>("cap_log", { capability: id, level, message });

  return {
    id,
    db: {
      insert: (table, record) => db("insert", table, { record }) as Promise<Row>,
      get: (table, rowId) => db("get", table, { id: rowId }) as Promise<Row | null>,
      query: (table, options?: QueryOptions) =>
        db("query", table, {
          filter: options?.filter,
          order_by: options?.orderBy,
          descending: options?.descending,
          limit: options?.limit,
        }) as Promise<Row[]>,
      update: (table, rowId, patch) =>
        db("update", table, { id: rowId, patch }) as Promise<Row | null>,
      softDelete: (table, rowId) => db("soft_delete", table, { id: rowId }) as Promise<boolean>,
      restore: (table, rowId) => db("restore", table, { id: rowId }) as Promise<Row | null>,
    },
    events: {
      emit: (name, payload, significance) =>
        bridge.invoke<void>("cap_emit", {
          capability: id,
          name,
          payload: payload ?? {},
          significance,
        }),
      subscribe: async (pattern, handler) => {
        const entry = { pattern, handler };
        state.subscriptions.add(entry);
        const stop: Unsubscribe = () => {
          state.subscriptions.delete(entry);
        };
        return stop;
      },
    },
    tools: {
      register: async (toolId, handler) => {
        state.tools.set(toolId, handler);
      },
      call: (tool, input) => bridge.invoke("call_tool", { caller: id, tool, input: input ?? {} }),
    },
    views: {
      setBadge: (count) =>
        bridge.invoke<void>("cap_badge", { capability: id, value: Math.max(0, Math.trunc(count)) }),
    },
    scheduler: {
      at: (whenMs, action, options) =>
        bridge.invoke<string>("cap_schedule", {
          capability: id,
          op: "at",
          args: { at_ms: Math.trunc(whenMs), action, missed: options?.missed },
        }),
      every: (intervalMs, action, options) =>
        bridge.invoke<string>("cap_schedule", {
          capability: id,
          op: "every",
          args: {
            interval_ms: Math.trunc(intervalMs),
            first_at_ms: options?.firstAtMs,
            action,
            missed: options?.missed,
          },
        }),
      cancel: (jobId) =>
        bridge.invoke<boolean>("cap_schedule", {
          capability: id,
          op: "cancel",
          args: { job_id: jobId },
        }),
    },
    notify: (title, body, options) =>
      bridge.invoke<void>("cap_notify", {
        capability: id,
        title,
        body: body ?? "",
        actions: options?.actions ?? [],
      }),
    config: {
      get: () => bridge.invoke<Record<string, unknown>>("get_capability_settings", { id }),
      onChange: async (handler) => {
        state.configHandlers.add(handler);
        const stop: Unsubscribe = () => {
          state.configHandlers.delete(handler);
        };
        return stop;
      },
    },
    log: {
      info: log("info"),
      warn: log("warn"),
      error: log("error"),
    },
  };
}

export class LogicHost {
  private running = new Map<string, Running>();
  private starting = new Set<string>();
  private unlisten: Array<() => void> = [];

  constructor(
    private bridge: Bridge,
    private load: Loader,
  ) {}

  /** Subscribes to the core's messages and activates what is already enabled. */
  async start(): Promise<void> {
    const b = this.bridge;
    this.unlisten.push(
      await b.listen<{ id: string; capability: string; tool: string; input: unknown }>(
        "frosty://tool-call",
        (call) => void this.handleToolCall(call),
      ),
      await b.listen<{ capability: string; event: FrostyEvent }>("frosty://cap-event", (m) =>
        this.handleEvent(m.capability, m.event),
      ),
      await b.listen<{ id: string }>(
        "frosty://capability-enable",
        ({ id }) => void this.enable(id),
      ),
      await b.listen<{ id: string }>(
        "frosty://capability-disable",
        ({ id }) => void this.deactivate(id),
      ),
      await b.listen<{ id: string; values: Record<string, unknown> }>(
        "frosty://capability-config",
        (m) => this.handleConfig(m.id, m.values),
      ),
    );

    const list = await b.invoke<ListResult>("list_capabilities");
    for (const cap of list.capabilities) {
      if (cap.state === "enabling" || cap.state === "enabled") {
        await this.activate(cap);
      }
    }
  }

  stop(): void {
    for (const off of this.unlisten) off();
    this.unlisten = [];
  }

  isRunning(id: string): boolean {
    return this.running.has(id);
  }

  private async enable(id: string): Promise<void> {
    const list = await this.bridge.invoke<ListResult>("list_capabilities");
    const cap = list.capabilities.find((c) => c.id === id);
    if (cap) await this.activate(cap);
  }

  async activate(cap: CapabilityRecord): Promise<void> {
    if (this.running.has(cap.id) || this.starting.has(cap.id)) return;
    this.starting.add(cap.id);
    const state = {
      tools: new Map<string, ToolHandler>(),
      subscriptions: new Set<{ pattern: string; handler: (event: FrostyEvent) => void }>(),
      configHandlers: new Set<(values: Record<string, unknown>) => void>(),
    };
    try {
      const mod = await this.load(cap.entry_url);
      const definition = mod.default;
      if (!definition || typeof definition.onActivate !== "function") {
        throw new Error("the logic entry must export default defineCapability({...})");
      }
      if (definition.id !== cap.id) {
        throw new Error(
          `the logic entry says its id is \`${definition.id}\`, but the manifest says \`${cap.id}\``,
        );
      }
      const ctx = createContext(cap.id, this.bridge, state);
      await definition.onActivate(ctx);
      this.running.set(cap.id, { definition, ctx, ...state });
      await this.bridge.invoke("capability_activated", { id: cap.id, ok: true });
    } catch (e) {
      // One broken capability must not take the host down.
      await this.bridge.invoke("capability_activated", {
        id: cap.id,
        ok: false,
        error: `onActivate failed: ${describeError(e)}`,
      });
    } finally {
      this.starting.delete(cap.id);
    }
  }

  async deactivate(id: string): Promise<void> {
    const run = this.running.get(id);
    if (!run) return;
    this.running.delete(id);
    try {
      await run.definition.onDeactivate?.(run.ctx);
    } catch (e) {
      await this.bridge.invoke("cap_log", {
        capability: id,
        level: "error",
        message: `onDeactivate failed: ${describeError(e)}`,
      });
    }
    run.tools.clear();
    run.subscriptions.clear();
    run.configHandlers.clear();
  }

  private async handleToolCall(call: {
    id: number | string;
    capability: string;
    tool: string;
    input: unknown;
  }): Promise<void> {
    const handler = this.running.get(call.capability)?.tools.get(call.tool);
    let result: Json;
    if (!handler) {
      result = { ok: false, error: `no handler registered for ${call.tool}` };
    } else {
      try {
        const value = await (handler as (input: unknown) => unknown)(call.input);
        result = { ok: true, value: (value ?? null) as Json };
      } catch (e) {
        result = { ok: false, error: describeError(e) };
      }
    }
    await this.bridge.invoke("tool_result", { id: call.id, result });
  }

  private handleEvent(capability: string, event: FrostyEvent): void {
    const run = this.running.get(capability);
    if (!run) return;
    for (const sub of run.subscriptions) {
      if (!matchesPattern(sub.pattern, event.name)) continue;
      try {
        sub.handler(event);
      } catch (e) {
        void this.bridge.invoke("cap_log", {
          capability,
          level: "error",
          message: `event handler for ${event.name} threw: ${describeError(e)}`,
        });
      }
    }
  }

  private handleConfig(id: string, values: Record<string, unknown>): void {
    for (const handler of this.running.get(id)?.configHandlers ?? []) {
      try {
        handler(values);
      } catch {
        // A settings handler that throws only affects itself.
      }
    }
  }
}
