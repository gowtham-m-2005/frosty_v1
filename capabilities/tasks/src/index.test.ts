import { describe, expect, it } from "vitest";

import tasks, { sortTasks, type Task } from "./index";
import { activate } from "./testing";
import { localDate } from "./streak";

const HOUR = 3_600_000;
const DAY = 24 * HOUR;

const start = (settings?: Record<string, unknown>) => activate(tasks, "tasks", settings);

describe("tasks: adding", () => {
  it("adds a task, emits an event, and updates the badge", async () => {
    const t = await start();
    const task = await t.call("tasks.add", { title: "  Buy milk  " });
    expect(task.title).toBe("Buy milk");
    expect(t.events.map((e) => e.name)).toEqual(["tasks.created"]);
    expect(t.badges.at(-1)).toBe(1);
  });

  it("schedules a reminder only for a future due date", async () => {
    const t = await start();
    const soon = Date.now() + HOUR;
    await t.call("tasks.add", { title: "future", due_at: soon });
    await t.call("tasks.add", { title: "past", due_at: Date.now() - HOUR });
    await t.call("tasks.add", { title: "undated" });
    const jobs = t.pendingJobs();
    expect(jobs).toHaveLength(1);
    expect(jobs[0]?.at).toBe(soon);
    expect(jobs[0]?.action).toMatchObject({ tool: "tasks.remind" });
  });

  it("does not schedule a reminder when reminders are switched off", async () => {
    const t = await start({ remind_at_due: false });
    await t.call("tasks.add", { title: "x", due_at: Date.now() + HOUR });
    expect(t.pendingJobs()).toHaveLength(0);
  });

  it("rejects an empty title and a repeating task with no due date", async () => {
    const t = await start();
    await expect(t.call("tasks.add", { title: "   " })).rejects.toThrow("title");
    await expect(
      t.call("tasks.add", { title: "x", recurrence: { kind: "daily" } }),
    ).rejects.toThrow("due date");
    await expect(
      t.call("tasks.add", { title: "x", due_at: 1, recurrence: { kind: "hourly" } }),
    ).rejects.toThrow("unknown recurrence");
  });
});

describe("tasks: completing", () => {
  it("completes, cancels the reminder, and records the day", async () => {
    const t = await start();
    const task = await t.call("tasks.add", { title: "x", due_at: Date.now() + HOUR });
    const result = await t.call("tasks.complete", { id: task.id });
    expect(result.task.completed_at).not.toBeNull();
    expect(result.streak).toBe(1);
    expect(t.pendingJobs()).toHaveLength(0);
    expect(t.events.some((e) => e.name === "tasks.completed")).toBe(true);
    expect(t.badges.at(-1)).toBe(0);
    expect(t.rows("completions")[0]?.completed_on).toBe(localDate(Date.now()));
  });

  it("completing twice does not double count", async () => {
    const t = await start();
    const task = await t.call("tasks.add", { title: "x" });
    await t.call("tasks.complete", { id: task.id });
    await t.call("tasks.complete", { id: task.id });
    expect(t.rows("completions")).toHaveLength(1);
  });

  it("a repeating task reopens at its next slot with a new reminder", async () => {
    const t = await start();
    const due = Date.now() + HOUR;
    const task = await t.call("tasks.add", {
      title: "stretch",
      due_at: due,
      recurrence: { kind: "daily" },
    });
    const result = await t.call("tasks.complete", { id: task.id });
    expect(result.task.completed_at).toBeNull();
    expect(result.task.due_at).toBeGreaterThan(due);
    expect(result.task.due_at).toBeLessThanOrEqual(due + 2 * DAY);
    const jobs = t.pendingJobs();
    expect(jobs).toHaveLength(1);
    expect(jobs[0]?.at).toBe(result.task.due_at);
    expect(t.badges.at(-1)).toBe(1);
  });

  it("uncompleting reopens and takes back today's completion", async () => {
    const t = await start();
    const task = await t.call("tasks.add", { title: "x" });
    await t.call("tasks.complete", { id: task.id });
    const reopened = await t.call("tasks.uncomplete", { id: task.id });
    expect(reopened.completed_at).toBeNull();
    expect(t.rows("completions")).toHaveLength(0);
    expect(t.badges.at(-1)).toBe(1);
  });

  it("an unknown task is a readable error", async () => {
    const t = await start();
    await expect(t.call("tasks.complete", { id: "nope" })).rejects.toThrow("no such task");
  });
});

describe("tasks: streaks", () => {
  it("emits a milestone on the first completion that reaches one", async () => {
    const t = await start();
    // Two earlier days already completed.
    const day = (n: number) => localDate(Date.now() - n * DAY);
    t.rows("completions"); // ensure the table exists in the fake
    await t.ctx.db.insert("completions", { task_id: "old", completed_on: day(1) });
    await t.ctx.db.insert("completions", { task_id: "old", completed_on: day(2) });

    const task = await t.call("tasks.add", { title: "today" });
    const result = await t.call("tasks.complete", { id: task.id });
    expect(result.streak).toBe(3);
    const milestone = t.events.find((e) => e.name === "tasks.streak_milestone");
    expect(milestone?.payload).toMatchObject({ count: 3 });
    expect(milestone?.significance).toBeGreaterThan(0.6);
  });

  it("a second completion the same day does not repeat the milestone", async () => {
    const t = await start();
    const day = (n: number) => localDate(Date.now() - n * DAY);
    await t.ctx.db.insert("completions", { task_id: "old", completed_on: day(1) });
    await t.ctx.db.insert("completions", { task_id: "old", completed_on: day(2) });
    const a = await t.call("tasks.add", { title: "a" });
    const b = await t.call("tasks.add", { title: "b" });
    await t.call("tasks.complete", { id: a.id });
    await t.call("tasks.complete", { id: b.id });
    expect(t.events.filter((e) => e.name === "tasks.streak_milestone")).toHaveLength(1);
  });
});

describe("tasks: editing, snoozing, deleting", () => {
  it("changing the due date moves the reminder", async () => {
    const t = await start();
    const first = Date.now() + HOUR;
    const second = Date.now() + 3 * HOUR;
    const task = await t.call("tasks.add", { title: "x", due_at: first });
    await t.call("tasks.update", { id: task.id, due_at: second });
    const jobs = t.pendingJobs();
    expect(jobs).toHaveLength(1);
    expect(jobs[0]?.at).toBe(second);
  });

  it("clearing the due date cancels the reminder", async () => {
    const t = await start();
    const task = await t.call("tasks.add", { title: "x", due_at: Date.now() + HOUR });
    await t.call("tasks.update", { id: task.id, due_at: null });
    expect(t.pendingJobs()).toHaveLength(0);
  });

  it("snooze reschedules the reminder using the configured length", async () => {
    const t = await start({ snooze_minutes: 25 });
    const task = await t.call("tasks.add", { title: "x", due_at: Date.now() + HOUR });
    const before = Date.now();
    const { snoozed_until } = await t.call("tasks.snooze", { id: task.id });
    expect(snoozed_until).toBeGreaterThanOrEqual(before + 25 * 60_000);
    expect(snoozed_until).toBeLessThan(before + 26 * 60_000);
    expect(t.pendingJobs()).toHaveLength(1);
    expect(t.pendingJobs()[0]?.at).toBe(snoozed_until);
  });

  it("delete cancels the reminder and restore brings it back", async () => {
    const t = await start();
    const due = Date.now() + HOUR;
    const task = await t.call("tasks.add", { title: "x", due_at: due });
    await t.call("tasks.delete", { id: task.id });
    expect(t.pendingJobs()).toHaveLength(0);
    expect(await t.call("tasks.list", { filter: "all" })).toHaveLength(0);
    expect(t.badges.at(-1)).toBe(0);

    await t.call("tasks.restore", { id: task.id });
    expect(await t.call("tasks.list", { filter: "all" })).toHaveLength(1);
    expect(t.pendingJobs()).toHaveLength(1);
    expect(t.badges.at(-1)).toBe(1);
  });

  it("restoring something that was not deleted is an error", async () => {
    const t = await start();
    const task = await t.call("tasks.add", { title: "x" });
    await expect(t.call("tasks.restore", { id: task.id })).rejects.toThrow("nothing to restore");
  });
});

describe("tasks: reminders", () => {
  it("a reminder notifies with Done and Snooze actions", async () => {
    const t = await start({ snooze_minutes: 15 });
    const task = await t.call("tasks.add", { title: "Water plants", due_at: Date.now() + HOUR });
    const result = await t.call("tasks.remind", { id: task.id });
    expect(result).toEqual({ notified: true });
    const n = t.notifications[0];
    expect(n?.title).toBe("Water plants");
    expect(n?.actions.map((a) => a.tool)).toEqual(["tasks.complete", "tasks.snooze"]);
    expect(n?.actions[0]?.input).toEqual({ id: task.id });
    expect(n?.actions[1]?.input).toEqual({ id: task.id, minutes: 15 });
  });

  it("no reminder for a task that is already done or gone", async () => {
    const t = await start();
    const task = await t.call("tasks.add", { title: "x" });
    await t.call("tasks.complete", { id: task.id });
    expect(await t.call("tasks.remind", { id: task.id })).toEqual({ skipped: true });
    expect(await t.call("tasks.remind", { id: "ghost" })).toEqual({ skipped: true });
    expect(t.notifications).toHaveLength(0);
  });

  it("a late reminder says it was due while Frosty was away", async () => {
    const t = await start();
    const task = await t.call("tasks.add", { title: "x" });
    await t.call("tasks.remind", { id: task.id, late: true });
    expect(t.notifications[0]?.body).toContain("while Frosty was away");
  });

  it("on activation, a future task with no job gets one", async () => {
    // Simulate a task whose job was lost: insert it directly, then activate.
    const core = (await import("./testing")).fakeCore();
    const due = Date.now() + 2 * HOUR;
    await core.bridge.invoke("cap_db", {
      capability: "tasks",
      op: "insert",
      table: "items",
      args: { record: { title: "orphan", due_at: due, completed_at: null, remind_job: null } },
    });
    const { createContext } = await import("@frosty/sdk/host");
    const ctx = createContext("tasks", core.bridge, {
      tools: new Map(),
      subscriptions: new Set(),
      configHandlers: new Set(),
    });
    await tasks.onActivate(ctx);
    expect(core.pendingJobs()).toHaveLength(1);
    expect(core.pendingJobs()[0]?.at).toBe(due);
  });
});

describe("tasks: listing and stats", () => {
  it("sorts open tasks by due date with undated last, then done tasks", () => {
    const base = { notes: null, recurrence: null, snoozed_until: null };
    const list: Task[] = [
      { ...base, id: "done", title: "done", due_at: null, completed_at: 5, created_at: 1 },
      { ...base, id: "none", title: "undated", due_at: null, completed_at: null, created_at: 2 },
      { ...base, id: "late", title: "later", due_at: 200, completed_at: null, created_at: 3 },
      { ...base, id: "soon", title: "sooner", due_at: 100, completed_at: null, created_at: 4 },
    ];
    expect(sortTasks(list).map((t) => t.id)).toEqual(["soon", "late", "none", "done"]);
  });

  it("filters and reports stats", async () => {
    const t = await start();
    const a = await t.call("tasks.add", { title: "a", due_at: Date.now() - HOUR });
    await t.call("tasks.add", { title: "b" });
    await t.call("tasks.complete", { id: a.id });
    expect((await t.call("tasks.list", { filter: "open" })).map((x: Task) => x.title)).toEqual([
      "b",
    ]);
    expect((await t.call("tasks.list", { filter: "done" })).map((x: Task) => x.title)).toEqual([
      "a",
    ]);
    expect(await t.call("tasks.list", { filter: "all" })).toHaveLength(2);
    const stats = await t.call("tasks.stats");
    expect(stats).toMatchObject({ open: 1, completed_today: 1, streak: 1 });
  });

  it("registers every tool its manifest declares", async () => {
    const t = await start();
    expect([...t.tools.keys()].sort()).toEqual([
      "tasks.add",
      "tasks.complete",
      "tasks.delete",
      "tasks.list",
      "tasks.remind",
      "tasks.restore",
      "tasks.snooze",
      "tasks.stats",
      "tasks.uncomplete",
      "tasks.update",
    ]);
  });
});
