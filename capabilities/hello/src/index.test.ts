/* eslint-disable @typescript-eslint/no-explicit-any --
   The fake core below stands in for untyped bridge payloads. */
// Contract test: the hello capability against the real SDK context, with an
// in-memory stand-in for the Rust core. It exercises db, events, tools,
// views, and config together.
import { describe, expect, it } from "vitest";

import { createContext } from "@frosty/sdk/host";
import type { Bridge, ToolHandler } from "@frosty/sdk";

import hello from "./index";

function fakeCore(settings: Record<string, unknown> = {}) {
  const rows: Record<string, unknown>[] = [];
  const emitted: { name: string; payload: unknown }[] = [];
  const badges: number[] = [];
  let next = 1;

  const bridge: Bridge = {
    async invoke(command, args) {
      const a = (args ?? {}) as Record<string, any>;
      switch (command) {
        case "get_capability_settings":
          return settings as never;
        case "cap_db": {
          const { op, args: p } = a;
          if (op === "insert") {
            const row = { id: `row-${next++}`, deleted_at: null, ...p.record };
            rows.push(row);
            return row as never;
          }
          if (op === "query") return rows.filter((r) => r.deleted_at === null) as never;
          if (op === "soft_delete") {
            const row = rows.find((r) => r.id === p.id);
            if (row) row.deleted_at = 1;
            return true as never;
          }
          throw new Error(`unexpected db op ${op}`);
        }
        case "cap_emit":
          emitted.push({ name: a.name, payload: a.payload });
          return undefined as never;
        case "cap_badge":
          badges.push(a.value);
          return undefined as never;
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
  return { bridge, rows, emitted, badges };
}

async function activate(settings?: Record<string, unknown>) {
  const core = fakeCore(settings);
  const tools = new Map<string, ToolHandler>();
  const ctx = createContext("hello", core.bridge, {
    tools,
    subscriptions: new Set(),
    configHandlers: new Set(),
  });
  await hello.onActivate(ctx);
  const call = (tool: string, input: unknown = {}) =>
    (tools.get(tool) as (i: unknown) => Promise<any>)(input);
  return { ...core, ctx, call, tools };
}

describe("hello capability", () => {
  it("registers every tool its manifest declares", async () => {
    const { tools } = await activate();
    expect([...tools.keys()].sort()).toEqual([
      "hello.clear",
      "hello.greet",
      "hello.list",
      "hello.ping",
    ]);
  });

  it("greets, stores, emits, and updates the badge", async () => {
    const { call, rows, emitted, badges } = await activate({ greeting: "Hi" });
    const result = await call("hello.greet", { name: "Ada" });
    expect(result.text).toBe("Hi, Ada");
    expect(rows).toHaveLength(1);
    expect(emitted).toEqual([{ name: "hello.greeted", payload: { id: "row-1", text: "Hi, Ada" } }]);
    expect(badges.at(-1)).toBe(1);
  });

  it("applies settings: shout, repeat, nicknames", async () => {
    const { call } = await activate({ greeting: "Yo", shout: true, repeat: 2, nicknames: ["Bee"] });
    const { text } = await call("hello.greet", { name: "Ada" });
    expect(text).toBe("YO, ADA. YO, ADA (AND BEE)");
  });

  it("clears greetings and resets the badge", async () => {
    const { call, badges } = await activate();
    await call("hello.greet", { name: "A" });
    await call("hello.greet", { name: "B" });
    expect((await call("hello.clear")).cleared).toBe(2);
    expect(await call("hello.list")).toHaveLength(0);
    expect(badges.at(-1)).toBe(0);
  });

  it("clears the badge when deactivated", async () => {
    const { ctx, badges } = await activate();
    await hello.onDeactivate?.(ctx);
    expect(badges.at(-1)).toBe(0);
  });
});
