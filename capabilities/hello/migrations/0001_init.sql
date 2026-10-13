-- Every capability table is `cap_<id>_<name>` and carries the sync columns.
CREATE TABLE cap_hello_greetings (
    id         TEXT PRIMARY KEY,
    created_at INTEGER NOT NULL,
    updated_at INTEGER NOT NULL,
    deleted_at INTEGER,
    device_id  TEXT NOT NULL,
    name       TEXT NOT NULL,
    text       TEXT NOT NULL
);
CREATE INDEX cap_hello_greetings_created ON cap_hello_greetings (created_at);
