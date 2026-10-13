import { useEffect, useRef, useState } from "react";
import { defineViews } from "@frosty/sdk";
import { useEvent, useSettings, useTool, useToolData } from "@frosty/sdk/views";

import { describeRecurrence, weekdayOf, type Recurrence } from "./recurrence";

interface Task {
  id: string;
  title: string;
  notes: string | null;
  due_at: number | null;
  completed_at: number | null;
  recurrence: Recurrence | null;
}

interface Stats {
  open: number;
  overdue: number;
  due_today: number;
  completed_today: number;
  streak: number;
}

type RepeatChoice = "none" | "daily" | "weekly";

const UNDO_MS = 6000;

/** `datetime-local` gives local time without a zone: 2026-10-14T09:30. */
function fromLocalInput(value: string): number | null {
  if (!value) return null;
  const ms = new Date(value).getTime();
  return Number.isNaN(ms) ? null : ms;
}

function dueLabel(due: number | null, now: number): { text: string; overdue: boolean } {
  if (due === null) return { text: "", overdue: false };
  const d = new Date(due);
  const time = d.toLocaleTimeString([], { hour: "2-digit", minute: "2-digit" });
  const start = (x: Date) => new Date(x.getFullYear(), x.getMonth(), x.getDate()).getTime();
  const days = Math.round((start(d) - start(new Date(now))) / 86_400_000);
  const day =
    days === 0
      ? "Today"
      : days === 1
        ? "Tomorrow"
        : days === -1
          ? "Yesterday"
          : d.toLocaleDateString([], { weekday: "short", month: "short", day: "numeric" });
  return { text: `${day}, ${time}`, overdue: due < now };
}

function recurrenceFor(choice: RepeatChoice, due: number | null): Recurrence | null {
  if (choice === "none" || due === null) return null;
  if (choice === "daily") return { kind: "daily" };
  return { kind: "weekly", days: [weekdayOf(due)] };
}

function AddForm({ onAdded }: { onAdded: () => void }) {
  const add = useTool<Record<string, never>, Task>("tasks.add");
  const [title, setTitle] = useState("");
  const [due, setDue] = useState("");
  const [repeat, setRepeat] = useState<RepeatChoice>("none");
  const [error, setError] = useState<string | null>(null);
  const dueMs = fromLocalInput(due);

  const submit = () => {
    if (!title.trim()) return;
    setError(null);
    const input: Record<string, unknown> = { title: title.trim() };
    if (dueMs !== null) input.due_at = dueMs;
    const recurrence = recurrenceFor(repeat, dueMs);
    if (recurrence) input.recurrence = recurrence;
    (add as unknown as (i: Record<string, unknown>) => Promise<Task>)(input).then(
      () => {
        setTitle("");
        setDue("");
        setRepeat("none");
        onAdded();
      },
      (e: unknown) => setError(e instanceof Error ? e.message : String(e)),
    );
  };

  return (
    <form
      className="tasks-add"
      onSubmit={(e) => {
        e.preventDefault();
        submit();
      }}
    >
      <div style={{ display: "flex", gap: 8 }}>
        <input
          className="f-input"
          aria-label="New task"
          placeholder="Add a task"
          value={title}
          onChange={(e) => setTitle(e.target.value)}
          style={{ flex: 1 }}
        />
        <button type="submit" className="f-button f-button-primary">
          Add
        </button>
      </div>
      <div style={{ display: "flex", gap: 8, marginTop: 8, flexWrap: "wrap" }}>
        <input
          className="f-input"
          type="datetime-local"
          aria-label="Due"
          value={due}
          onChange={(e) => setDue(e.target.value)}
        />
        <select
          className="f-select"
          aria-label="Repeat"
          value={repeat}
          disabled={dueMs === null}
          title={dueMs === null ? "Set a due time to repeat" : undefined}
          onChange={(e) => setRepeat(e.target.value as RepeatChoice)}
        >
          <option value="none">Does not repeat</option>
          <option value="daily">Every day</option>
          <option value="weekly">Every week</option>
        </select>
      </div>
      {error ? <p role="alert">{error}</p> : null}
    </form>
  );
}

function Row({
  task,
  now,
  onComplete,
  onReopen,
  onDelete,
}: {
  task: Task;
  now: number;
  onComplete: (id: string) => void;
  onReopen: (id: string) => void;
  onDelete: (task: Task) => void;
}) {
  const done = task.completed_at !== null;
  const due = dueLabel(task.due_at, now);
  return (
    <li
      className="task-row"
      style={{ display: "flex", gap: 8, alignItems: "flex-start", padding: "6px 0" }}
    >
      <input
        type="checkbox"
        checked={done}
        aria-label={done ? `Reopen ${task.title}` : `Complete ${task.title}`}
        onChange={() => (done ? onReopen(task.id) : onComplete(task.id))}
      />
      <div style={{ flex: 1, minWidth: 0 }}>
        <div
          style={done ? { textDecoration: "line-through", color: "var(--text-muted)" } : undefined}
        >
          {task.title}
        </div>
        <div
          style={{
            fontSize: 12,
            color: due.overdue && !done ? "var(--danger)" : "var(--text-muted)",
          }}
        >
          {due.text ? `${due.overdue && !done ? "Overdue · " : ""}${due.text}` : null}
          {task.recurrence ? ` · ${describeRecurrence(task.recurrence)}` : null}
        </div>
      </div>
      <button
        type="button"
        className="f-button f-button-ghost"
        aria-label={`Delete ${task.title}`}
        onClick={() => onDelete(task)}
      >
        ×
      </button>
    </li>
  );
}

function Tasks({ compact }: { compact: boolean }) {
  const settings = useSettings<{ show_completed?: boolean }>();
  const list = useToolData<Task[]>("tasks.list", { filter: "all" }, ["tasks.*"]);
  const stats = useToolData<Stats>("tasks.stats", {}, ["tasks.*"]);
  const complete = useTool<{ id: string }>("tasks.complete");
  const reopen = useTool<{ id: string }>("tasks.uncomplete");
  const remove = useTool<{ id: string }>("tasks.delete");
  const restore = useTool<{ id: string }>("tasks.restore");

  const [undo, setUndo] = useState<{ id: string; title: string } | null>(null);
  const [problem, setProblem] = useState<string | null>(null);
  const [showDone, setShowDone] = useState(false);
  const undoTimer = useRef<number | undefined>(undefined);
  const [now, setNow] = useState(() => Date.now());

  // Keep "overdue" and "today" honest while the card stays open.
  useEffect(() => {
    const t = window.setInterval(() => setNow(Date.now()), 30_000);
    return () => window.clearInterval(t);
  }, []);
  useEvent("tasks.changed", () => setNow(Date.now()));

  const run = (p: Promise<unknown>) =>
    p.catch((e: unknown) => setProblem(e instanceof Error ? e.message : String(e)));

  const open = (list.data ?? []).filter((t) => t.completed_at === null);
  const done = (list.data ?? []).filter((t) => t.completed_at !== null);
  const doneShown = settings?.show_completed === true || showDone;

  const onDelete = (task: Task) => {
    setProblem(null);
    void run(remove({ id: task.id }));
    window.clearTimeout(undoTimer.current);
    setUndo({ id: task.id, title: task.title });
    undoTimer.current = window.setTimeout(() => setUndo(null), UNDO_MS);
  };

  return (
    <div style={{ display: "grid", gap: 12 }}>
      <div style={{ display: "flex", justifyContent: "space-between", color: "var(--text-muted)" }}>
        <span>
          {stats.data
            ? `${stats.data.open} open${stats.data.overdue ? `, ${stats.data.overdue} overdue` : ""}`
            : "…"}
        </span>
        <span aria-label="Streak">
          {stats.data && stats.data.streak > 0
            ? `Streak: ${stats.data.streak} ${stats.data.streak === 1 ? "day" : "days"}`
            : "No streak yet"}
        </span>
      </div>

      <AddForm onAdded={list.refresh} />

      {problem ? <p role="alert">{problem}</p> : null}
      {list.error ? (
        <div role="alert">
          <p>Could not load your tasks: {list.error}</p>
          <button type="button" className="f-button" onClick={list.refresh}>
            Retry
          </button>
        </div>
      ) : null}
      {list.loading && !list.data ? <p className="muted">Loading…</p> : null}
      {list.data && open.length === 0 ? (
        <p className="muted">Nothing to do. Add something above.</p>
      ) : null}

      <ul style={{ margin: 0, padding: 0, listStyle: "none" }}>
        {open.slice(0, compact ? 12 : 100).map((t) => (
          <Row
            key={t.id}
            task={t}
            now={now}
            onComplete={(id) => void run(complete({ id }))}
            onReopen={(id) => void run(reopen({ id }))}
            onDelete={onDelete}
          />
        ))}
      </ul>

      {done.length > 0 ? (
        <div>
          <button
            type="button"
            className="f-button f-button-ghost"
            onClick={() => setShowDone((s) => !s)}
          >
            {doneShown ? "Hide" : "Show"} completed ({done.length})
          </button>
          {doneShown ? (
            <ul style={{ margin: 0, padding: 0, listStyle: "none" }}>
              {done.slice(0, compact ? 5 : 50).map((t) => (
                <Row
                  key={t.id}
                  task={t}
                  now={now}
                  onComplete={(id) => void run(complete({ id }))}
                  onReopen={(id) => void run(reopen({ id }))}
                  onDelete={onDelete}
                />
              ))}
            </ul>
          ) : null}
        </div>
      ) : null}

      {undo ? (
        <div
          role="status"
          style={{
            display: "flex",
            justifyContent: "space-between",
            alignItems: "center",
            padding: "8px 12px",
            border: "var(--border-width) solid var(--border)",
            background: "var(--surface-raised)",
          }}
        >
          <span>Deleted “{undo.title}”</span>
          <button
            type="button"
            className="f-button"
            onClick={() => {
              window.clearTimeout(undoTimer.current);
              void run(restore({ id: undo.id }));
              setUndo(null);
            }}
          >
            Undo
          </button>
        </div>
      ) : null}
    </div>
  );
}

// Kept so a bad stored recurrence is caught in one place when editing lands.
export default defineViews({
  card: () => <Tasks compact />,
  page: () => <Tasks compact={false} />,
});
