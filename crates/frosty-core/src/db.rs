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

pub const CORE_MIGRATIONS: &[Migration] = &[Migration {
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
}];

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
}
