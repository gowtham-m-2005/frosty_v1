import { describe, expect, it } from "vitest";

import { LogicHost, type CapabilityRecord } from "./host";
import { defineCapability } from "./index";
import type { Bridge, CapabilityDefinition, Ctx } from "./types";

class FakeBridge implements Bridge {
  calls: Array<{ command: string; args: Record<string, unknown> | undefined }> = [];
  private listeners = new Map<string, Array<(payload: never) => void>>();
  responses = new Map<string, (args: Record<string, unknown> | undefined) => unknown>();
  capabilities: CapabilityRecord[] = [];

  async invoke<T>(command: string, args?: Record<string, unknown>): Promise<T> {
    this.calls.push({ command, args });
    if (command === "list_capabilities") {
      return { capabilities: this.capabilities } as T;
    }
    const respond = this.responses.get(command);
    return (respond ? respond(args) : null) as T;
  }

  async listen<T>(event: string, handler: (payload: T) => void): Promise<() => void> {
    const list = this.listeners.get(event) ?? [];
    list.push(handler as (payload: never) => void);
    this.listeners.set(event, list);
    return () => {
      this.listeners.set(
        event,
        (this.listeners.get(event) ?? []).filter((h) => h !== handler),
      );
    };
  }

  emit(event: string, payload: unknown): void {
    for (const h of this.listeners.get(event) ?? []) h(payload as never);
  }

  of(command: string) {
    return this.calls.filter((c) => c.command === command).map((c) => c.args);
  }
}

function setup(modules: Record<string, CapabilityDefinition | Error>) {
  const bridge = new FakeBridge();
  bridge.capabilities = Object.keys(modules).map((id) => ({
    id,
    state: "enabling" as const,
    entry_url: `mem://${id}`,
  }));
  const host = new LogicHost(bridge, async (url) => {
    const found = modules[url.replace("mem://", "")];
    if (found instanceof Error) throw found;
    return { default: found };
  });
  return { bridge, host };
}

const tick = () => new Promise((r) => setTimeout(r, 0));

describe("LogicHost", () => {
  it("activates enabling capabilities and reports success", async () => {
    let seen: Ctx | undefined;
    const { bridge, host } = setup({
      notes: defineCapability({ id: "notes", onActivate: (ctx) => void (seen = ctx) }),
    });
    await host.start();
    expect(seen?.id).toBe("notes");
    expect(bridge.of("capability_activated")).toEqual([{ id: "notes", ok: true }]);
    expect(host.isRunning("notes")).toBe(true);
  });

  it("reports a throwing onActivate and keeps going", async () => {
    const { bridge, host } = setup({
      bad: defineCapability({
        id: "bad",
        onActivate: () => {
          throw new Error("boom");
        },
      }),
      good: defineCapability({ id: "good", onActivate: () => {} }),
    });
    await host.start();
    const reports = bridge.of("capability_activated");
    expect(reports).toContainEqual({ id: "bad", ok: false, error: "onActivate failed: boom" });
    expect(reports).toContainEqual({ id: "good", ok: true });
    expect(host.isRunning("bad")).toBe(false);
    expect(host.isRunning("good")).toBe(true);
  });

  it("reports a module that fails to load", async () => {
    const { bridge, host } = setup({ gone: new Error("404 not found") });
    await host.start();
    expect(bridge.of("capability_activated")[0]).toMatchObject({ ok: false });
  });

  it("rejects a module whose id does not match its manifest", async () => {
    const { bridge, host } = setup({
      notes: defineCapability({ id: "something_else", onActivate: () => {} }),
    });
    await host.start();
    const [report] = bridge.of("capability_activated");
    expect(report).toMatchObject({ id: "notes", ok: false });
    expect(String((report as { error: string }).error)).toContain("something_else");
  });

  it("routes tool calls and returns results and errors", async () => {
    const { bridge, host } = setup({
      notes: defineCapability({
        id: "notes",
        async onActivate(ctx) {
          await ctx.tools.register("notes.add", ({ title }: { title: string }) => ({
            added: title,
          }));
          await ctx.tools.register("notes.fail", () => {
            throw new Error("nope");
          });
        },
      }),
    });
    await host.start();

    bridge.emit("frosty://tool-call", {
      id: 1,
      capability: "notes",
      tool: "notes.add",
      input: { title: "x" },
    });
    bridge.emit("frosty://tool-call", {
      id: 2,
      capability: "notes",
      tool: "notes.fail",
      input: {},
    });
    bridge.emit("frosty://tool-call", {
      id: 3,
      capability: "notes",
      tool: "notes.ghost",
      input: {},
    });
    await tick();

    const results = bridge.of("tool_result");
    expect(results).toContainEqual({ id: 1, result: { ok: true, value: { added: "x" } } });
    expect(results).toContainEqual({ id: 2, result: { ok: false, error: "nope" } });
    expect(results.find((r) => (r as { id: number }).id === 3)).toMatchObject({
      result: { ok: false },
    });
  });

  it("maps ctx.db onto structured cap_db calls", async () => {
    let ctx!: Ctx;
    const { bridge, host } = setup({
      notes: defineCapability({ id: "notes", onActivate: (c) => void (ctx = c) }),
    });
    await host.start();

    await ctx.db.insert("items", { title: "a" });
    await ctx.db.query("items", {
      filter: { done: false },
      orderBy: "title",
      descending: true,
      limit: 5,
    });
    await ctx.db.update("items", "r1", { title: "b" });
    await ctx.db.softDelete("items", "r1");

    const calls = bridge.of("cap_db");
    expect(calls[0]).toMatchObject({
      capability: "notes",
      op: "insert",
      table: "items",
      args: { record: { title: "a" } },
    });
    expect(calls[1]).toMatchObject({
      op: "query",
      args: { filter: { done: false }, order_by: "title", descending: true, limit: 5 },
    });
    expect(calls[2]).toMatchObject({ op: "update", args: { id: "r1", patch: { title: "b" } } });
    expect(calls[3]).toMatchObject({ op: "soft_delete", args: { id: "r1" } });
  });

  it("sends events, scheduler jobs, badge, and notifications through the core", async () => {
    let ctx!: Ctx;
    const { bridge, host } = setup({
      notes: defineCapability({ id: "notes", onActivate: (c) => void (ctx = c) }),
    });
    await host.start();

    await ctx.events.emit("notes.created", { id: "1" }, 0.4);
    await ctx.scheduler.at(5000, { event: "notes.due" }, { missed: "skip" });
    await ctx.scheduler.every(60_000, { tool: "notes.sync" });
    await ctx.scheduler.cancel("job-1");
    await ctx.views.setBadge(3.7);
    await ctx.notify("Hello", "body");

    expect(bridge.of("cap_emit")[0]).toEqual({
      capability: "notes",
      name: "notes.created",
      payload: { id: "1" },
      significance: 0.4,
    });
    const schedule = bridge.of("cap_schedule");
    expect(schedule[0]).toMatchObject({ op: "at", args: { at_ms: 5000, missed: "skip" } });
    expect(schedule[1]).toMatchObject({ op: "every", args: { interval_ms: 60_000 } });
    expect(schedule[2]).toMatchObject({ op: "cancel", args: { job_id: "job-1" } });
    expect(bridge.of("cap_badge")[0]).toEqual({ capability: "notes", value: 3 });
    expect(bridge.of("cap_notify")[0]).toEqual({
      capability: "notes",
      title: "Hello",
      body: "body",
    });
  });

  it("delivers only matching events to a subscription, and stops after unsubscribe", async () => {
    let ctx!: Ctx;
    const { bridge, host } = setup({
      notes: defineCapability({ id: "notes", onActivate: (c) => void (ctx = c) }),
    });
    await host.start();

    const got: string[] = [];
    const off = await ctx.events.subscribe("tasks.*", (e) => got.push(e.name));
    const event = (name: string) => ({
      capability: "notes",
      event: { id: "1", name, source: name.split(".")[0], at: 0, payload: {}, significance: null },
    });
    bridge.emit("frosty://cap-event", event("tasks.completed"));
    bridge.emit("frosty://cap-event", event("guitar.logged"));
    off();
    bridge.emit("frosty://cap-event", event("tasks.created"));
    expect(got).toEqual(["tasks.completed"]);
  });

  it("a throwing event handler does not stop the others", async () => {
    let ctx!: Ctx;
    const { bridge, host } = setup({
      notes: defineCapability({ id: "notes", onActivate: (c) => void (ctx = c) }),
    });
    await host.start();
    const got: string[] = [];
    await ctx.events.subscribe("*", () => {
      throw new Error("bad handler");
    });
    await ctx.events.subscribe("*", (e) => got.push(e.name));
    bridge.emit("frosty://cap-event", {
      capability: "notes",
      event: {
        id: "1",
        name: "core.summoned",
        source: "core",
        at: 0,
        payload: {},
        significance: null,
      },
    });
    expect(got).toEqual(["core.summoned"]);
    expect(bridge.of("cap_log")[0]).toMatchObject({ level: "error" });
  });

  it("deactivation runs onDeactivate and drops every handler", async () => {
    let deactivated = false;
    const { bridge, host } = setup({
      notes: defineCapability({
        id: "notes",
        async onActivate(ctx) {
          await ctx.tools.register("notes.add", () => "ok");
        },
        onDeactivate() {
          deactivated = true;
        },
      }),
    });
    await host.start();
    bridge.emit("frosty://capability-disable", { id: "notes" });
    await tick();
    expect(deactivated).toBe(true);
    expect(host.isRunning("notes")).toBe(false);

    bridge.emit("frosty://tool-call", { id: 9, capability: "notes", tool: "notes.add", input: {} });
    await tick();
    expect(bridge.of("tool_result").at(-1)).toMatchObject({ id: 9, result: { ok: false } });
  });

  it("passes settings changes to onChange handlers", async () => {
    let ctx!: Ctx;
    const { bridge, host } = setup({
      notes: defineCapability({ id: "notes", onActivate: (c) => void (ctx = c) }),
    });
    await host.start();
    const seen: unknown[] = [];
    await ctx.config.onChange((v) => seen.push(v));
    bridge.emit("frosty://capability-config", { id: "notes", values: { sort: "oldest" } });
    bridge.emit("frosty://capability-config", { id: "other", values: { sort: "x" } });
    expect(seen).toEqual([{ sort: "oldest" }]);
  });

  it("activates a capability enabled while running", async () => {
    const { bridge, host } = setup({
      late: defineCapability({ id: "late", onActivate: () => {} }),
    });
    bridge.capabilities = [];
    await host.start();
    expect(host.isRunning("late")).toBe(false);
    bridge.capabilities = [{ id: "late", state: "enabling", entry_url: "mem://late" }];
    bridge.emit("frosty://capability-enable", { id: "late" });
    await tick();
    expect(host.isRunning("late")).toBe(true);
  });
});
