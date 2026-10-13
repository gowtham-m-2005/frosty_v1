import { defineCapability } from "@frosty/sdk";
import type { Ctx, Json, Row } from "@frosty/sdk";

import { nextOccurrence, parseRecurrence, type Recurrence } from "./recurrence";
import { computeStreak, isMilestone, localDate } from "./streak";

interface Settings {
  remind_at_due?: boolean;
  snooze_minutes?: number;
}

export interface Task {
  id: string;
  title: string;
  notes: string | null;
  due_at: number | null;
  completed_at: number | null;
  recurrence: Recurrence | null;
  snoozed_until: number | null;
  created_at: number;
}

const DEFAULT_SNOOZE_MINUTES = 10;
const MINUTE = 60_000;

function toTask(row: Row): Task {
  let recurrence: Recurrence | null = null;
  if (typeof row.recurrence === "string" && row.recurrence) {
    try {
      recurrence = parseRecurrence(JSON.parse(row.recurrence));
    } catch {
      // A damaged value is treated as "does not repeat" rather than breaking the list.
    }
  }
  return {
    id: row.id,
    title: String(row.title),
    notes: (row.notes as string | null) ?? null,
    due_at: (row.due_at as number | null) ?? null,
    completed_at: (row.completed_at as number | null) ?? null,
    recurrence,
    snoozed_until: (row.snoozed_until as number | null) ?? null,
    created_at: row.created_at,
  };
}

/** Open tasks first, soonest due first, undated last; done tasks newest first. */
export function sortTasks(tasks: Task[]): Task[] {
  return [...tasks].sort((a, b) => {
    const aDone = a.completed_at !== null;
    const bDone = b.completed_at !== null;
    if (aDone !== bDone) return aDone ? 1 : -1;
    if (aDone && bDone) return (b.completed_at ?? 0) - (a.completed_at ?? 0);
    const aDue = a.due_at ?? Number.POSITIVE_INFINITY;
    const bDue = b.due_at ?? Number.POSITIVE_INFINITY;
    return aDue - bDue || b.created_at - a.created_at;
  });
}

export default defineCapability({
  id: "tasks",

  async onActivate(ctx) {
    const settings = async (): Promise<Settings> => (await ctx.config.get()) as Settings;
    const snoozeMinutes = async () => (await settings()).snooze_minutes ?? DEFAULT_SNOOZE_MINUTES;

    const openCount = async () => {
      const rows = await ctx.db.query("items", { filter: { completed_at: null } });
      return rows.length;
    };
    const refreshBadge = async () => ctx.views.setBadge(await openCount());
    const changed = (id: string) => ctx.events.emit("tasks.changed", { id });

    const load = async (id: string): Promise<Row> => {
      const row = await ctx.db.get("items", id);
      if (!row) throw new Error("There is no such task.");
      return row;
    };

    /** Cancels the old reminder job and, if there is a time, schedules one. */
    const setReminder = async (row: Row, when: number | null): Promise<void> => {
      if (typeof row.remind_job === "string" && row.remind_job) {
        await ctx.scheduler.cancel(row.remind_job);
      }
      let job: string | null = null;
      if (when !== null && when > Date.now()) {
        job = await ctx.scheduler.at(
          when,
          { tool: "tasks.remind", input: { id: row.id } },
          { missed: "fire_once_on_start" },
        );
      }
      await ctx.db.update("items", row.id, { remind_job: job });
    };

    const reminderTime = async (due: number | null): Promise<number | null> =>
      due !== null && (await settings()).remind_at_due !== false ? due : null;

    const streak = async (): Promise<number> => {
      const rows = await ctx.db.query("completions", { limit: 4000 });
      const days = new Set(rows.map((r) => String(r.completed_on)));
      return computeStreak(days, localDate(Date.now()));
    };

    await ctx.tools.register(
      "tasks.add",
      async (input: { title: string; notes?: string; due_at?: number; recurrence?: unknown }) => {
        const title = input.title.trim();
        if (!title) throw new Error("A task needs a title.");
        const recurrence = input.recurrence ? parseRecurrence(input.recurrence) : null;
        if (recurrence && input.due_at === undefined) {
          throw new Error("A repeating task needs a due date to repeat from.");
        }
        const row = await ctx.db.insert("items", {
          title,
          notes: input.notes ?? null,
          due_at: input.due_at ?? null,
          recurrence: recurrence ? JSON.stringify(recurrence) : null,
        });
        await setReminder(row, await reminderTime(input.due_at ?? null));
        await ctx.events.emit("tasks.created", { id: row.id, title });
        await refreshBadge();
        return toTask((await ctx.db.get("items", row.id)) ?? row);
      },
    );

    await ctx.tools.register(
      "tasks.list",
      async (input: { filter?: "open" | "done" | "all"; limit?: number }) => {
        const filter = input?.filter ?? "open";
        const rows = await ctx.db.query("items", {
          orderBy: "created_at",
          descending: true,
          limit: 1000,
        });
        let tasks = rows.map(toTask);
        if (filter === "open") tasks = tasks.filter((t) => t.completed_at === null);
        if (filter === "done") tasks = tasks.filter((t) => t.completed_at !== null);
        return sortTasks(tasks).slice(0, input?.limit ?? 100);
      },
    );

    await ctx.tools.register("tasks.complete", async ({ id }: { id: string }) => {
      const row = await load(id);
      const task = toTask(row);
      if (task.completed_at !== null) return { task, streak: await streak() };

      const now = Date.now();
      const today = localDate(now);
      const before = await ctx.db.query("completions", {
        filter: { completed_on: today },
        limit: 1,
      });
      await ctx.db.insert("completions", { task_id: id, completed_on: today });

      if (task.recurrence) {
        // A repeating task reopens at its next slot instead of finishing.
        const next = nextOccurrence(task.recurrence, task.due_at ?? now, now);
        await ctx.db.update("items", id, { due_at: next, completed_at: null, snoozed_until: null });
        await setReminder(row, await reminderTime(next));
      } else {
        await ctx.db.update("items", id, { completed_at: now, snoozed_until: null });
        await setReminder(row, null);
      }

      const count = await streak();
      await ctx.events.emit("tasks.completed", {
        id,
        title: task.title,
        recurring: task.recurrence !== null,
      });
      // Only the first completion of a day can reach a milestone.
      if (before.length === 0 && isMilestone(count)) {
        await ctx.events.emit(
          "tasks.streak_milestone",
          { count, days: count },
          Math.min(1, 0.6 + count / 100),
        );
      }
      await changed(id);
      await refreshBadge();
      return { task: toTask((await ctx.db.get("items", id)) ?? row), streak: count };
    });

    await ctx.tools.register("tasks.uncomplete", async ({ id }: { id: string }) => {
      const row = await load(id);
      await ctx.db.update("items", id, { completed_at: null });
      // Take back today's completion so the streak does not count it.
      const today = localDate(Date.now());
      const todays = await ctx.db.query("completions", {
        filter: { task_id: id, completed_on: today },
      });
      for (const c of todays) await ctx.db.softDelete("completions", c.id);
      await setReminder(row, await reminderTime((row.due_at as number | null) ?? null));
      await changed(id);
      await refreshBadge();
      return toTask((await ctx.db.get("items", id)) ?? row);
    });

    await ctx.tools.register(
      "tasks.update",
      async (input: {
        id: string;
        title?: string;
        notes?: string | null;
        due_at?: number | null;
        recurrence?: unknown;
      }) => {
        const row = await load(input.id);
        const patch: Record<string, Json> = {};
        if (input.title !== undefined) {
          const title = input.title.trim();
          if (!title) throw new Error("A task needs a title.");
          patch.title = title;
        }
        if (input.notes !== undefined) patch.notes = input.notes;
        if (input.due_at !== undefined) patch.due_at = input.due_at;
        if (input.recurrence !== undefined) {
          patch.recurrence =
            input.recurrence === null ? null : JSON.stringify(parseRecurrence(input.recurrence));
        }
        const dueAfter =
          input.due_at !== undefined ? input.due_at : ((row.due_at as number | null) ?? null);
        if (patch.recurrence && dueAfter === null) {
          throw new Error("A repeating task needs a due date to repeat from.");
        }
        patch.snoozed_until = null;
        const updated = await ctx.db.update("items", input.id, patch);
        if (input.due_at !== undefined) {
          await setReminder(row, await reminderTime(input.due_at));
        }
        await changed(input.id);
        return toTask(updated ?? row);
      },
    );

    await ctx.tools.register("tasks.delete", async ({ id }: { id: string }) => {
      const row = await load(id);
      await setReminder(row, null);
      await ctx.db.softDelete("items", id);
      await changed(id);
      await refreshBadge();
      return { id };
    });

    await ctx.tools.register("tasks.restore", async ({ id }: { id: string }) => {
      const row = await ctx.db.restore("items", id);
      if (!row) throw new Error("There is nothing to restore.");
      const task = toTask(row);
      if (task.completed_at === null) await setReminder(row, await reminderTime(task.due_at));
      await changed(id);
      await refreshBadge();
      return task;
    });

    await ctx.tools.register(
      "tasks.snooze",
      async ({ id, minutes }: { id: string; minutes?: number }) => {
        const row = await load(id);
        const until = Date.now() + (minutes ?? (await snoozeMinutes())) * MINUTE;
        await ctx.db.update("items", id, { snoozed_until: until });
        await setReminder(row, until);
        await changed(id);
        return { id, snoozed_until: until };
      },
    );

    await ctx.tools.register("tasks.stats", async () => {
      const rows = (await ctx.db.query("items", { limit: 1000 })).map(toTask);
      const open = rows.filter((t) => t.completed_at === null);
      const now = Date.now();
      const today = localDate(now);
      const completions = await ctx.db.query("completions", { filter: { completed_on: today } });
      return {
        open: open.length,
        overdue: open.filter((t) => t.due_at !== null && t.due_at < now).length,
        due_today: open.filter((t) => t.due_at !== null && localDate(t.due_at) === today).length,
        completed_today: completions.length,
        streak: await streak(),
      };
    });

    // Called by the scheduler at the due time.
    await ctx.tools.register(
      "tasks.remind",
      async ({ id, late }: { id: string; late?: boolean }) => {
        const row = await ctx.db.get("items", id);
        const task = row ? toTask(row) : null;
        if (!task || task.completed_at !== null) return { skipped: true };
        await ctx.db.update("items", id, { remind_job: null, snoozed_until: null });
        const minutes = await snoozeMinutes();
        await ctx.notify(task.title, late ? "This was due while Frosty was away." : "Due now.", {
          actions: [
            { label: "Done", tool: "tasks.complete", input: { id } },
            { label: `Snooze ${minutes} min`, tool: "tasks.snooze", input: { id, minutes } },
          ],
        });
        return { notified: true };
      },
    );

    await reconcileReminders(ctx);
    await refreshBadge();
  },

  async onDeactivate(ctx) {
    await ctx.views.setBadge(0);
  },
});

/**
 * Reminder jobs live in the core, so they normally survive restarts. This
 * covers tasks that have a future due date but lost their job, for example
 * after the reminder setting was switched back on.
 */
async function reconcileReminders(ctx: Ctx): Promise<void> {
  const settings = (await ctx.config.get()) as Settings;
  if (settings.remind_at_due === false) return;
  const rows = await ctx.db.query("items", { filter: { completed_at: null }, limit: 1000 });
  const now = Date.now();
  for (const row of rows) {
    const due = row.due_at as number | null;
    if (due !== null && due > now && !row.remind_job) {
      const job = await ctx.scheduler.at(
        due,
        { tool: "tasks.remind", input: { id: row.id } },
        { missed: "fire_once_on_start" },
      );
      await ctx.db.update("items", row.id, { remind_job: job });
    }
  }
}
