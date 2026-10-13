/**
 * Repeating tasks. A recurrence is stored as JSON on the task:
 *
 *   { "kind": "daily" }
 *   { "kind": "weekly", "days": [0, 2, 4] }       0 = Monday ... 6 = Sunday
 *   { "kind": "every_n_days", "n": 3 }
 *
 * Each may carry `"time": "HH:MM"`; without one the task keeps the time of
 * day it was due. All arithmetic is in local time, built from calendar
 * fields, so a daylight saving change does not shift a 09:00 task.
 */

export type Recurrence =
  | { kind: "daily"; time?: string }
  | { kind: "weekly"; days: number[]; time?: string }
  | { kind: "every_n_days"; n: number; time?: string };

const TIME = /^([01]\d|2[0-3]):([0-5]\d)$/;

/** Throws a readable error for anything that is not a valid recurrence. */
export function parseRecurrence(input: unknown): Recurrence {
  if (typeof input !== "object" || input === null) {
    throw new Error("recurrence must be an object");
  }
  const r = input as Record<string, unknown>;
  if (r.time !== undefined && (typeof r.time !== "string" || !TIME.test(r.time))) {
    throw new Error("recurrence time must look like 09:30");
  }
  const time = r.time as string | undefined;

  switch (r.kind) {
    case "daily":
      return { kind: "daily", ...(time ? { time } : {}) };
    case "weekly": {
      const days = r.days;
      if (
        !Array.isArray(days) ||
        days.length === 0 ||
        !days.every((d) => Number.isInteger(d) && d >= 0 && d <= 6)
      ) {
        throw new Error("weekly recurrence needs days from 0 (Monday) to 6 (Sunday)");
      }
      return {
        kind: "weekly",
        days: [...new Set(days as number[])].sort((a, b) => a - b),
        ...(time ? { time } : {}),
      };
    }
    case "every_n_days": {
      const n = r.n;
      if (typeof n !== "number" || !Number.isInteger(n) || n < 1 || n > 365) {
        throw new Error("every_n_days needs n from 1 to 365");
      }
      return { kind: "every_n_days", n, ...(time ? { time } : {}) };
    }
    default:
      throw new Error(`unknown recurrence kind ${String(r.kind)}`);
  }
}

/** Monday is 0, matching the stored `days`. */
export function weekdayOf(ms: number): number {
  return (new Date(ms).getDay() + 6) % 7;
}

function at(day: Date, hour: number, minute: number, plusDays = 0): Date {
  return new Date(day.getFullYear(), day.getMonth(), day.getDate() + plusDays, hour, minute, 0, 0);
}

/**
 * The next time the task is due after completing it.
 *
 * It is always in the future and always after `due`, so finishing a task days
 * late gives the next slot, not a backlog of missed ones.
 */
export function nextOccurrence(rec: Recurrence, due: number, now: number): number {
  const base = new Date(due);
  const [hour, minute] = rec.time
    ? rec.time.split(":").map(Number)
    : [base.getHours(), base.getMinutes()];
  const h = hour ?? 0;
  const m = minute ?? 0;
  const start = Math.max(due, now);
  const startDay = new Date(start);

  switch (rec.kind) {
    case "daily": {
      for (let i = 0; i < 3; i++) {
        const candidate = at(startDay, h, m, i);
        if (candidate.getTime() > start) return candidate.getTime();
      }
      break;
    }
    case "weekly": {
      for (let i = 0; i < 9; i++) {
        const candidate = at(startDay, h, m, i);
        if (candidate.getTime() > start && rec.days.includes(weekdayOf(candidate.getTime()))) {
          return candidate.getTime();
        }
      }
      break;
    }
    case "every_n_days": {
      // Keep the original rhythm: step from the due date in whole n-day
      // jumps until the result is after both the due date and now.
      let candidate = at(base, h, m);
      let guard = 0;
      while (candidate.getTime() <= start && guard++ < 4000) {
        candidate = at(candidate, h, m, rec.n);
      }
      return candidate.getTime();
    }
  }
  // Unreachable for valid input; fall back to one day later.
  return at(startDay, h, m, 1).getTime();
}

const DAY_NAMES = ["Mon", "Tue", "Wed", "Thu", "Fri", "Sat", "Sun"];

/** A short label for a task row. */
export function describeRecurrence(rec: Recurrence): string {
  switch (rec.kind) {
    case "daily":
      return "Every day";
    case "weekly":
      return rec.days.length === 7
        ? "Every day"
        : `Every ${rec.days.map((d) => DAY_NAMES[d]).join(", ")}`;
    case "every_n_days":
      return rec.n === 1 ? "Every day" : `Every ${rec.n} days`;
  }
}
