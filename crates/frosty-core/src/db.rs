//! Storage service: one SQLite database per device with forward-only
//! migrations per owner (docs/PROJECT_SPEC.md section 7.2 and 10).

use std::{
    fs,
    path::{Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};

use rusqlite::{Connection, OptionalExtension, params};

#[derive(Debug, thiserror::Error)]
pub enum DbError {
    #[error("database error: {0}")]
    Sqlite(#[from] rusqlite::Error),
    #[error("database file error: {0}")]
    Io(#[from] std::io::Error),
}

#[derive(Debug, Clone, Copy)]
pub struct Migration {
    /// Sortable version, for example `0001_core`.
    pub version: &'static str,
    pub sql: &'static str,
}

pub const CORE_OWNER: &str = "core";

pub const CORE_MIGRATIONS: &[Migration] = &[
    Migration {
        version: "0001_core",
        sql: "
        CREATE TABLE device (
            id TEXT PRIMARY KEY,
            name TEXT NOT NULL,
            created_at INTEGER NOT NULL
        );
        CREATE TABLE kv (
            key TEXT PRIMARY KEY,
            value TEXT NOT NULL,
            updated_at INTEGER NOT NULL
        );
    ",
    },
    Migration {
        version: "0002_scheduler_notifications",
        sql: "
        CREATE TABLE scheduler_jobs (
            id TEXT PRIMARY KEY,
            owner TEXT NOT NULL,
            kind TEXT NOT NULL,
            fire_at INTEGER NOT NULL,
            interval_ms INTEGER,
            action TEXT NOT NULL,
            missed_policy TEXT NOT NULL DEFAULT 'fire_once_on_start',
            created_at INTEGER NOT NULL
        );
        CREATE INDEX scheduler_jobs_due ON scheduler_jobs (fire_at);
        CREATE TABLE notifications (
            id TEXT PRIMARY KEY,
            owner TEXT NOT NULL,
            title TEXT NOT NULL,
            body TEXT NOT NULL,
            created_at INTEGER NOT NULL,
            read_at INTEGER,
            dismissed_at INTEGER
        );
        CREATE TABLE capability_state (
            id TEXT PRIMARY KEY,
            installed_version TEXT,
            state TEXT NOT NULL,
            last_error TEXT,
            error_count INTEGER NOT NULL DEFAULT 0,
            updated_at INTEGER NOT NULL
        );
    ",
    },
    Migration {
        version: "0003_notification_center",
        sql: "
        ALTER TABLE notifications ADD COLUMN actions TEXT;
        ALTER TABLE notifications ADD COLUMN state TEXT NOT NULL DEFAULT 'held';
        ALTER TABLE notifications ADD COLUMN hold_reason TEXT;
        ALTER TABLE notifications ADD COLUMN delivered_at INTEGER;
        CREATE INDEX notifications_state ON notifications (state, created_at);
    ",
    },
];

/// Columns every capability table must carry so its rows can sync later
/// (docs/PROJECT_SPEC.md 7.2 and 15).
pub const SYNC_COLUMNS: [&str; 5] = ["id", "created_at", "updated_at", "deleted_at", "device_id"];

#[derive(Debug, thiserror::Error)]
pub enum CapabilityMigrationError {
    #[error(transparent)]
    Db(#[from] DbError),
    #[error("migration {version} failed: {message}")]
    Sql { version: String, message: String },
    #[error(
        "migration {version} touches `{object}`, which is outside this capability. Its objects must start with `{prefix}`."
    )]
    OutsideNamespace {
        version: String,
        object: String,
        prefix: String,
    },
    #[error("table `{table}` is missing required column `{column}`")]
    MissingSyncColumn { table: String, column: String },
}

fn now_millis() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_millis() as i64)
}

pub struct Database {
    conn: Connection,
    path: Option<PathBuf>,
    backups_dir: Option<PathBuf>,
}

impl Database {
    /// Opens (creating if needed) the database and applies core migrations.
    /// When migrations are pending and the file already exists, a copy is
    /// placed in `backups_dir` first.
    pub fn open(path: &Path, backups_dir: &Path) -> Result<Self, DbError> {
        let mut db = Self {
            conn: Connection::open(path)?,
            path: Some(path.to_path_buf()),
            backups_dir: Some(backups_dir.to_path_buf()),
        };
        db.init()?;
        Ok(db)
    }

    pub fn open_in_memory() -> Result<Self, DbError> {
        let mut db = Self {
            conn: Connection::open_in_memory()?,
            path: None,
            backups_dir: None,
        };
        db.init()?;
        Ok(db)
    }

    fn init(&mut self) -> Result<(), DbError> {
        self.conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS migrations (
                owner TEXT NOT NULL,
                version TEXT NOT NULL,
                applied_at INTEGER NOT NULL,
                PRIMARY KEY (owner, version)
            );",
        )?;
        self.run_migrations(CORE_OWNER, CORE_MIGRATIONS)?;
        Ok(())
    }

    fn applied(&self, owner: &str) -> Result<Vec<String>, DbError> {
        let mut stmt = self
            .conn
            .prepare("SELECT version FROM migrations WHERE owner = ?1")?;
        let rows = stmt.query_map([owner], |r| r.get::<_, String>(0))?;
        Ok(rows.collect::<Result<_, _>>()?)
    }

    /// Applies pending migrations for `owner` in order, all or nothing.
    /// Returns how many were applied.
    pub fn run_migrations(
        &mut self,
        owner: &str,
        migrations: &[Migration],
    ) -> Result<usize, DbError> {
        let applied = self.applied(owner)?;
        let mut pending: Vec<&Migration> = migrations
            .iter()
            .filter(|m| !applied.iter().any(|a| a == m.version))
            .collect();
        pending.sort_by_key(|m| m.version);
        if pending.is_empty() {
            return Ok(0);
        }

        // Back up an existing database before changing its schema. A brand
        // new database has nothing worth copying yet.
        if let (Some(path), Some(dir)) = (&self.path, &self.backups_dir) {
            let has_history = !self.all_applied_is_empty()?;
            if has_history && path.exists() {
                fs::create_dir_all(dir)?;
                let name = format!("frosty-db-{:016}.db", now_millis());
                fs::copy(path, dir.join(name))?;
            }
        }

        let tx = self.conn.transaction()?;
        for m in &pending {
            tx.execute_batch(m.sql)?;
            tx.execute(
                "INSERT INTO migrations (owner, version, applied_at) VALUES (?1, ?2, ?3)",
                params![owner, m.version, now_millis()],
            )?;
        }
        tx.commit()?;
        Ok(pending.len())
    }

    /// Applies a capability's own migrations. Beyond `run_migrations`, this
    /// guarantees the capability stays inside its namespace: every object it
    /// creates must belong to a `cap_<id>_` table, nothing else in the
    /// database may change, and new tables must carry the sync columns.
    /// Any violation rolls the whole batch back.
    pub fn run_capability_migrations(
        &mut self,
        capability_id: &str,
        migrations: &[(String, String)],
    ) -> Result<usize, CapabilityMigrationError> {
        let owner = format!("cap:{capability_id}");
        let prefix = format!("cap_{capability_id}_");
        let applied = self.applied(&owner)?;
        let mut pending: Vec<&(String, String)> = migrations
            .iter()
            .filter(|(version, _)| !applied.iter().any(|a| a == version))
            .collect();
        pending.sort_by(|a, b| a.0.cmp(&b.0));
        if pending.is_empty() {
            return Ok(0);
        }

        if let (Some(path), Some(dir)) = (&self.path, &self.backups_dir)
            && path.exists()
        {
            fs::create_dir_all(dir).map_err(DbError::from)?;
            let name = format!("frosty-db-{:016}.db", now_millis());
            fs::copy(path, dir.join(name)).map_err(DbError::from)?;
        }

        let snapshot = |conn: &Connection| -> Result<Vec<(String, String, String)>, DbError> {
            let mut stmt = conn.prepare(
                "SELECT name, tbl_name, COALESCE(sql, '') FROM sqlite_master ORDER BY name",
            )?;
            let rows = stmt.query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))?;
            Ok(rows.collect::<Result<_, _>>()?)
        };

        let tx = self.conn.transaction().map_err(DbError::from)?;
        let before = snapshot(&tx)?;

        for (version, sql) in &pending {
            tx.execute_batch(sql)
                .map_err(|e| CapabilityMigrationError::Sql {
                    version: version.clone(),
                    message: e.to_string(),
                })?;
            let after = snapshot(&tx)?;
            for (name, tbl_name, sql) in &after {
                let existed = before.iter().any(|(n, _, s)| n == name && s == sql);
                if !existed && !tbl_name.starts_with(&prefix) {
                    return Err(CapabilityMigrationError::OutsideNamespace {
                        version: version.clone(),
                        object: name.clone(),
                        prefix,
                    });
                }
            }
            // Dropping or altering someone else's object also shows up as a
            // missing (name, sql) pair.
            for (name, tbl_name, sql) in &before {
                if tbl_name.starts_with(&prefix) {
                    continue;
                }
                if !after.iter().any(|(n, _, s)| n == name && s == sql) {
                    return Err(CapabilityMigrationError::OutsideNamespace {
                        version: version.clone(),
                        object: name.clone(),
                        prefix,
                    });
                }
            }
            tx.execute(
                "INSERT INTO migrations (owner, version, applied_at) VALUES (?1, ?2, ?3)",
                params![owner, version, now_millis()],
            )
            .map_err(DbError::from)?;
        }

        // Every table this capability owns must carry the sync columns.
        let tables: Vec<String> = {
            let mut stmt = tx
                .prepare("SELECT name FROM sqlite_master WHERE type = 'table'")
                .map_err(DbError::from)?;
            let names: Vec<String> = stmt
                .query_map([], |r| r.get::<_, String>(0))
                .map_err(DbError::from)?
                .collect::<Result<_, _>>()
                .map_err(DbError::from)?;
            names
                .into_iter()
                .filter(|n| n.starts_with(&prefix))
                .collect()
        };
        for table in tables {
            let columns: Vec<String> = {
                let mut stmt = tx
                    .prepare(&format!("PRAGMA table_info(\"{table}\")"))
                    .map_err(DbError::from)?;
                stmt.query_map([], |r| r.get::<_, String>(1))
                    .map_err(DbError::from)?
                    .collect::<Result<_, _>>()
                    .map_err(DbError::from)?
            };
            for required in SYNC_COLUMNS {
                if !columns.iter().any(|c| c == required) {
                    return Err(CapabilityMigrationError::MissingSyncColumn {
                        table,
                        column: required.to_string(),
                    });
                }
            }
        }

        tx.commit().map_err(DbError::from)?;
        Ok(pending.len())
    }

    /// Drops every table a capability owns and forgets its migration history.
    /// Used by "remove and delete data".
    pub fn drop_capability_data(&mut self, capability_id: &str) -> Result<(), DbError> {
        let prefix = format!("cap_{capability_id}_");
        let tables: Vec<String> = {
            let mut stmt = self
                .conn
                .prepare("SELECT name FROM sqlite_master WHERE type = 'table'")?;
            stmt.query_map([], |r| r.get::<_, String>(0))?
                .collect::<Result<Vec<_>, _>>()?
                .into_iter()
                .filter(|n| n.starts_with(&prefix))
                .collect()
        };
        let tx = self.conn.transaction()?;
        for table in tables {
            tx.execute_batch(&format!("DROP TABLE \"{table}\""))?;
        }
        tx.execute(
            "DELETE FROM migrations WHERE owner = ?1",
            [format!("cap:{capability_id}")],
        )?;
        tx.commit()?;
        Ok(())
    }

    fn all_applied_is_empty(&self) -> Result<bool, DbError> {
        let n: i64 = self
            .conn
            .query_row("SELECT COUNT(*) FROM migrations", [], |r| r.get(0))?;
        Ok(n == 0)
    }

    pub fn kv_get(&self, key: &str) -> Result<Option<String>, DbError> {
        Ok(self
            .conn
            .query_row("SELECT value FROM kv WHERE key = ?1", [key], |r| r.get(0))
            .optional()?)
    }

    pub fn kv_set(&self, key: &str, value: &str) -> Result<(), DbError> {
        self.conn.execute(
            "INSERT INTO kv (key, value, updated_at) VALUES (?1, ?2, ?3)
             ON CONFLICT(key) DO UPDATE SET value = excluded.value, updated_at = excluded.updated_at",
            params![key, value, now_millis()],
        )?;
        Ok(())
    }

    /// This device's id, created on first call. UUIDv7 so it sorts by time
    /// and is safe for future sync.
    pub fn device_id(&self, device_name: &str) -> Result<String, DbError> {
        if let Some(id) = self
            .conn
            .query_row("SELECT id FROM device LIMIT 1", [], |r| {
                r.get::<_, String>(0)
            })
            .optional()?
        {
            return Ok(id);
        }
        let id = uuid::Uuid::now_v7().to_string();
        self.conn.execute(
            "INSERT INTO device (id, name, created_at) VALUES (?1, ?2, ?3)",
            params![id, device_name, now_millis()],
        )?;
        Ok(id)
    }

    /// For tests and the capability storage layer that comes later.
    pub fn connection(&self) -> &Connection {
        &self.conn
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn core_migrations_apply_once() {
        let mut db = Database::open_in_memory().unwrap();
        assert_eq!(db.run_migrations(CORE_OWNER, CORE_MIGRATIONS).unwrap(), 0);
        assert!(db.kv_get("missing").unwrap().is_none());
    }

    #[test]
    fn kv_roundtrip_and_overwrite() {
        let db = Database::open_in_memory().unwrap();
        db.kv_set("window.main", "{\"x\":1}").unwrap();
        db.kv_set("window.main", "{\"x\":2}").unwrap();
        assert_eq!(
            db.kv_get("window.main").unwrap().as_deref(),
            Some("{\"x\":2}")
        );
    }

    #[test]
    fn device_id_is_stable() {
        let db = Database::open_in_memory().unwrap();
        let a = db.device_id("laptop").unwrap();
        let b = db.device_id("renamed").unwrap();
        assert_eq!(a, b);
    }

    #[test]
    fn failed_migration_rolls_back_everything() {
        let mut db = Database::open_in_memory().unwrap();
        let bad = [
            Migration {
                version: "0001_ok",
                sql: "CREATE TABLE cap_x_a (id TEXT);",
            },
            Migration {
                version: "0002_bad",
                sql: "THIS IS NOT SQL;",
            },
        ];
        assert!(db.run_migrations("x", &bad).is_err());
        let exists: i64 = db
            .connection()
            .query_row(
                "SELECT COUNT(*) FROM sqlite_master WHERE name = 'cap_x_a'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(exists, 0);
        assert!(db.applied("x").unwrap().is_empty());
    }

    #[test]
    fn owners_are_tracked_separately() {
        let mut db = Database::open_in_memory().unwrap();
        let m = [Migration {
            version: "0001",
            sql: "CREATE TABLE cap_t_items (id TEXT);",
        }];
        assert_eq!(db.run_migrations("t", &m).unwrap(), 1);
        assert_eq!(db.run_migrations("t", &m).unwrap(), 0);
    }

    #[test]
    fn existing_database_is_backed_up_before_new_migrations() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("frosty.db");
        let backups = tmp.path().join("backups");
        {
            let mut db = Database::open(&path, &backups).unwrap();
            // First open of a fresh file: nothing worth backing up.
            assert!(!backups.exists());
            let m = [Migration {
                version: "0001",
                sql: "CREATE TABLE cap_t_items (id TEXT);",
            }];
            db.run_migrations("t", &m).unwrap();
        }
        assert_eq!(fs::read_dir(&backups).unwrap().count(), 1);
    }

    #[test]
    fn capability_migrations_create_namespaced_tables() {
        let mut db = Database::open_in_memory().unwrap();
        let m = vec![(
            "0001".to_string(),
            "CREATE TABLE cap_notes_items (
                id TEXT PRIMARY KEY, created_at INTEGER, updated_at INTEGER,
                deleted_at INTEGER, device_id TEXT, title TEXT);
             CREATE INDEX cap_notes_items_title ON cap_notes_items (title);"
                .to_string(),
        )];
        assert_eq!(db.run_capability_migrations("notes", &m).unwrap(), 1);
        assert_eq!(db.run_capability_migrations("notes", &m).unwrap(), 0);
    }

    #[test]
    fn capability_cannot_create_tables_outside_its_namespace() {
        let mut db = Database::open_in_memory().unwrap();
        let m = vec![(
            "0001".to_string(),
            "CREATE TABLE evil (id TEXT);".to_string(),
        )];
        let err = db.run_capability_migrations("notes", &m).unwrap_err();
        assert!(
            matches!(err, CapabilityMigrationError::OutsideNamespace { .. }),
            "{err}"
        );
    }

    #[test]
    fn capability_cannot_touch_core_or_other_capability_tables() {
        let mut db = Database::open_in_memory().unwrap();
        for sql in [
            "DROP TABLE kv;",
            "ALTER TABLE kv ADD COLUMN x TEXT;",
            "DROP TABLE scheduler_jobs;",
        ] {
            let m = vec![("0001".to_string(), sql.to_string())];
            assert!(db.run_capability_migrations("notes", &m).is_err(), "{sql}");
        }
        // Core is intact.
        db.kv_set("still", "here").unwrap();
    }

    #[test]
    fn capability_tables_need_sync_columns() {
        let mut db = Database::open_in_memory().unwrap();
        let m = vec![(
            "0001".to_string(),
            "CREATE TABLE cap_notes_items (id TEXT PRIMARY KEY, title TEXT);".to_string(),
        )];
        let err = db.run_capability_migrations("notes", &m).unwrap_err();
        assert!(
            matches!(err, CapabilityMigrationError::MissingSyncColumn { .. }),
            "{err}"
        );
        // And nothing was left behind.
        assert!(db.applied("cap:notes").unwrap().is_empty());
    }

    #[test]
    fn dropping_capability_data_removes_only_its_tables() {
        let mut db = Database::open_in_memory().unwrap();
        let table = |id: &str| {
            vec![(
                "0001".to_string(),
                format!(
                    "CREATE TABLE cap_{id}_items (id TEXT PRIMARY KEY, created_at INTEGER,
                      updated_at INTEGER, deleted_at INTEGER, device_id TEXT);"
                ),
            )]
        };
        db.run_capability_migrations("a", &table("a")).unwrap();
        db.run_capability_migrations("b", &table("b")).unwrap();
        db.drop_capability_data("a").unwrap();
        let names: Vec<String> = {
            let mut stmt = db
                .connection()
                .prepare("SELECT name FROM sqlite_master WHERE type = 'table'")
                .unwrap();
            let all: Vec<String> = stmt
                .query_map([], |r| r.get(0))
                .unwrap()
                .map(Result::unwrap)
                .collect();
            all.into_iter().filter(|n| n.starts_with("cap_")).collect()
        };
        assert_eq!(names, vec!["cap_b_items"]);
        // The history is gone too, so a reinstall would migrate again.
        assert_eq!(db.run_capability_migrations("a", &table("a")).unwrap(), 1);
    }
}
