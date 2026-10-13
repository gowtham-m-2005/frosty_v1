-- Tasks. Every table carries the sync columns (docs/PROJECT_SPEC.md 15).
CREATE TABLE cap_tasks_items (
    id            TEXT PRIMARY KEY,
    created_at    INTEGER NOT NULL,
    updated_at    INTEGER NOT NULL,
    deleted_at    INTEGER,
    device_id     TEXT NOT NULL,
    title         TEXT NOT NULL,
    notes         TEXT,
    -- Unix milliseconds. NULL means no due date.
    due_at        INTEGER,
    completed_at  INTEGER,
    -- JSON, see src/recurrence.ts. NULL means it does not repeat.
    recurrence    TEXT,
    -- The scheduler job that will remind about this task, if any.
    remind_job    TEXT,
    snoozed_until INTEGER
);
CREATE INDEX cap_tasks_items_due ON cap_tasks_items (due_at);

-- One row per completion, so a streak survives a repeating task reopening.
CREATE TABLE cap_tasks_completions (
    id           TEXT PRIMARY KEY,
    created_at   INTEGER NOT NULL,
    updated_at   INTEGER NOT NULL,
    deleted_at   INTEGER,
    device_id    TEXT NOT NULL,
    task_id      TEXT NOT NULL,
    -- Local date, YYYY-MM-DD.
    completed_on TEXT NOT NULL
);
CREATE INDEX cap_tasks_completions_day ON cap_tasks_completions (completed_on);
