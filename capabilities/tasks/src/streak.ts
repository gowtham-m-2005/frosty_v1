/** Local calendar date as YYYY-MM-DD. */
export function localDate(ms: number): string {
  const d = new Date(ms);
  const mm = String(d.getMonth() + 1).padStart(2, "0");
  const dd = String(d.getDate()).padStart(2, "0");
  return `${d.getFullYear()}-${mm}-${dd}`;
}

function previousDay(date: string): string {
  const [y, m, d] = date.split("-").map(Number);
  return localDate(new Date(y ?? 1970, (m ?? 1) - 1, (d ?? 1) - 1, 12).getTime());
}

/**
 * Consecutive days with at least one completion, ending today. If nothing
 * was done yet today, a streak that ran up to yesterday is still alive and
 * still counts.
 */
export function computeStreak(days: ReadonlySet<string>, today: string): number {
  let day = days.has(today) ? today : previousDay(today);
  let count = 0;
  while (days.has(day)) {
    count++;
    day = previousDay(day);
  }
  return count;
}

/** Streak lengths worth a celebration. */
export const MILESTONES: readonly number[] = [3, 7, 14, 30, 60, 100, 365];

export function isMilestone(streak: number): boolean {
  return MILESTONES.includes(streak);
}
