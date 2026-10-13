import { describe, expect, it } from "vitest";

import { computeStreak, isMilestone, localDate } from "./streak";

describe("computeStreak", () => {
  const days = (...d: string[]) => new Set(d);

  it("is zero with no completions", () => {
    expect(computeStreak(days(), "2026-10-14")).toBe(0);
  });

  it("counts consecutive days ending today", () => {
    expect(computeStreak(days("2026-10-12", "2026-10-13", "2026-10-14"), "2026-10-14")).toBe(3);
  });

  it("a streak that ran up to yesterday is still alive", () => {
    expect(computeStreak(days("2026-10-12", "2026-10-13"), "2026-10-14")).toBe(2);
  });

  it("a gap of a full day ends it", () => {
    expect(computeStreak(days("2026-10-11", "2026-10-12"), "2026-10-14")).toBe(0);
  });

  it("only the run that touches today counts", () => {
    expect(
      computeStreak(
        days("2026-10-01", "2026-10-02", "2026-10-03", "2026-10-13", "2026-10-14"),
        "2026-10-14",
      ),
    ).toBe(2);
  });

  it("crosses month and year boundaries", () => {
    expect(computeStreak(days("2025-12-30", "2025-12-31", "2026-01-01"), "2026-01-01")).toBe(3);
    expect(computeStreak(days("2026-02-27", "2026-02-28", "2026-03-01"), "2026-03-01")).toBe(3);
  });
});

describe("localDate", () => {
  it("formats local calendar dates with zero padding", () => {
    expect(localDate(new Date(2026, 0, 5, 23, 59).getTime())).toBe("2026-01-05");
    expect(localDate(new Date(2026, 9, 14, 0, 0).getTime())).toBe("2026-10-14");
  });
});

describe("isMilestone", () => {
  it("celebrates round streaks", () => {
    expect(isMilestone(7)).toBe(true);
    expect(isMilestone(8)).toBe(false);
  });
});
