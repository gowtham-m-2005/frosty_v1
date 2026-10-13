import { describe, expect, it } from "vitest";

import { describeRecurrence, nextOccurrence, parseRecurrence, weekdayOf } from "./recurrence";

// Local-time helper: month is 1 to 12.
const at = (y: number, mo: number, d: number, h = 9, mi = 0) =>
  new Date(y, mo - 1, d, h, mi, 0, 0).getTime();

describe("parseRecurrence", () => {
  it("accepts the three kinds and normalizes weekly days", () => {
    expect(parseRecurrence({ kind: "daily" })).toEqual({ kind: "daily" });
    expect(parseRecurrence({ kind: "weekly", days: [4, 0, 4] })).toEqual({
      kind: "weekly",
      days: [0, 4],
    });
    expect(parseRecurrence({ kind: "every_n_days", n: 3, time: "07:30" })).toEqual({
      kind: "every_n_days",
      n: 3,
      time: "07:30",
    });
  });

  it("rejects nonsense with a readable message", () => {
    expect(() => parseRecurrence(null)).toThrow("object");
    expect(() => parseRecurrence({ kind: "hourly" })).toThrow("unknown recurrence kind");
    expect(() => parseRecurrence({ kind: "weekly", days: [] })).toThrow("days");
    expect(() => parseRecurrence({ kind: "weekly", days: [7] })).toThrow("days");
    expect(() => parseRecurrence({ kind: "every_n_days", n: 0 })).toThrow("n from 1");
    expect(() => parseRecurrence({ kind: "daily", time: "25:00" })).toThrow("time");
  });
});

describe("nextOccurrence", () => {
  it("daily: completing early moves to the next day, keeping the time", () => {
    const due = at(2026, 10, 14, 9, 0);
    const now = at(2026, 10, 14, 8, 0);
    expect(nextOccurrence({ kind: "daily" }, due, now)).toBe(at(2026, 10, 15, 9, 0));
  });

  it("daily: completing late the same day still gives tomorrow", () => {
    const due = at(2026, 10, 14, 9, 0);
    const now = at(2026, 10, 14, 14, 0);
    expect(nextOccurrence({ kind: "daily" }, due, now)).toBe(at(2026, 10, 15, 9, 0));
  });

  it("daily: days late gives the next future slot, not a backlog", () => {
    const due = at(2026, 10, 10, 9, 0);
    const now = at(2026, 10, 14, 14, 0);
    expect(nextOccurrence({ kind: "daily" }, due, now)).toBe(at(2026, 10, 15, 9, 0));
  });

  it("daily: an explicit time overrides the due time", () => {
    const due = at(2026, 10, 14, 9, 0);
    const now = at(2026, 10, 14, 9, 5);
    expect(nextOccurrence({ kind: "daily", time: "18:30" }, due, now)).toBe(
      at(2026, 10, 14, 18, 30),
    );
  });

  it("weekly: picks the next listed weekday", () => {
    // 2026-10-14 is a Wednesday (index 2).
    expect(weekdayOf(at(2026, 10, 14))).toBe(2);
    const due = at(2026, 10, 14, 9, 0);
    const now = at(2026, 10, 14, 9, 30);
    const rec = parseRecurrence({ kind: "weekly", days: [0, 4] }); // Mon, Fri
    expect(nextOccurrence(rec, due, now)).toBe(at(2026, 10, 16, 9, 0)); // Friday
    const afterFri = at(2026, 10, 16, 10, 0);
    expect(nextOccurrence(rec, at(2026, 10, 16, 9, 0), afterFri)).toBe(at(2026, 10, 19, 9, 0)); // Monday
  });

  it("weekly with one day repeats exactly a week later", () => {
    const due = at(2026, 10, 14, 9, 0);
    const rec = { kind: "weekly" as const, days: [weekdayOf(due)] };
    expect(nextOccurrence(rec, due, due)).toBe(at(2026, 10, 21, 9, 0));
  });

  it("every n days keeps the rhythm from the original due date", () => {
    const due = at(2026, 10, 1, 9, 0);
    const rec = { kind: "every_n_days" as const, n: 3 };
    // Completed on the 4th at noon: the 4th 09:00 has passed, so the 7th.
    expect(nextOccurrence(rec, due, at(2026, 10, 4, 12, 0))).toBe(at(2026, 10, 7, 9, 0));
    // Completed early on the 2nd: the 4th.
    expect(nextOccurrence(rec, due, at(2026, 10, 2, 8, 0))).toBe(at(2026, 10, 4, 9, 0));
  });

  it("is always after both the due date and now", () => {
    const recs = [
      { kind: "daily" as const },
      { kind: "weekly" as const, days: [1, 3, 5] },
      { kind: "every_n_days" as const, n: 2 },
    ];
    const due = at(2026, 3, 28, 23, 30);
    for (const rec of recs) {
      for (let d = 0; d < 20; d++) {
        const now = due + d * 7 * 3_600_000;
        const next = nextOccurrence(rec, due, now);
        expect(next).toBeGreaterThan(due);
        expect(next).toBeGreaterThan(now);
      }
    }
  });

  it("keeps the wall-clock time across a clock change", () => {
    // Late March crosses a daylight saving change in many zones; in zones
    // without one this still holds trivially.
    const start = at(2026, 3, 25, 9, 0);
    let due = start;
    for (let i = 0; i < 10; i++) {
      due = nextOccurrence({ kind: "daily" }, due, due);
      const d = new Date(due);
      expect([d.getHours(), d.getMinutes()]).toEqual([9, 0]);
    }
  });
});

describe("describeRecurrence", () => {
  it("is short and readable", () => {
    expect(describeRecurrence({ kind: "daily" })).toBe("Every day");
    expect(describeRecurrence({ kind: "weekly", days: [0, 2] })).toBe("Every Mon, Wed");
    expect(describeRecurrence({ kind: "every_n_days", n: 3 })).toBe("Every 3 days");
  });
});
