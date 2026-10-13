//! The structured data API behind `ctx.db`. No raw SQL crosses the bridge:
//! the host adds the `cap_<id>_` prefix, fills the sync columns, and only
//! touches the calling capability's own tables
//! (docs/PROJECT_SPEC.md 7.2 and 8.2).

use rusqlite::{Connection, params_from_iter, types::Value as Sql, types::ValueRef};
use serde_json::{Map, Number, Value};

#[derive(Debug, thiserror::Error)]
pub enum StoreError {
    #[error("{0}")]
    Invalid(String),
    #[error("database error: {0}")]
    Db(#[from] rusqlite::Error),
}

const RESERVED: [&str; 5] = ["id", "created_at", "updated_at", "deleted_at", "device_id"];
const DEFAULT_LIMIT: u32 = 1_000;
const MAX_LIMIT: u32 = 10_000;

fn now_millis() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_millis() as i64)
}

fn valid_ident(s: &str) -> bool {
    let mut chars = s.chars();
    matches!(chars.next(), Some(c) if c.is_ascii_lowercase())
        && s.len() <= 48
        && chars.all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_')
}

fn to_sql(v: &Value) -> Sql {
    match v {
        Value::Null => Sql::Null,
        Value::Bool(b) => Sql::Integer(i64::from(*b)),
        Value::Number(n) => match n.as_i64() {
            Some(i) => Sql::Integer(i),
            None => Sql::Real(n.as_f64().unwrap_or(0.0)),
        },
        Value::String(s) => Sql::Text(s.clone()),
        // Objects and lists are stored as JSON text.
        other => Sql::Text(other.to_string()),
    }
}

fn from_ref(v: ValueRef<'_>) -> Value {
    match v {
        ValueRef::Null => Value::Null,
        ValueRef::Integer(i) => Value::from(i),
        ValueRef::Real(f) => Number::from_f64(f).map_or(Value::Null, Value::Number),
        ValueRef::Text(t) => Value::String(String::from_utf8_lossy(t).into_owned()),
        ValueRef::Blob(_) => Value::Null,
    }
}

#[derive(Debug, Default, Clone)]
pub struct Query {
    /// Equality filters. A `null` value matches NULL.
    pub filter: Map<String, Value>,
    pub order_by: Option<String>,
    pub descending: bool,
    pub limit: Option<u32>,
}

pub struct CapStore<'a> {
    conn: &'a Connection,
    capability_id: &'a str,
    device_id: &'a str,
}

impl<'a> CapStore<'a> {
    pub fn new(conn: &'a Connection, capability_id: &'a str, device_id: &'a str) -> Self {
        Self {
            conn,
            capability_id,
            device_id,
        }
    }

    fn table(&self, table: &str) -> Result<(String, Vec<String>), StoreError> {
        if !valid_ident(table) {
            return Err(StoreError::Invalid(format!(
                "`{table}` is not a valid table name"
            )));
        }
        let full = format!("cap_{}_{table}", self.capability_id);
        let mut stmt = self
            .conn
            .prepare(&format!("PRAGMA table_info(\"{full}\")"))?;
        let columns: Vec<String> = stmt
            .query_map([], |r| r.get::<_, String>(1))?
            .collect::<Result<_, _>>()?;
        if columns.is_empty() {
            return Err(StoreError::Invalid(format!(
                "this capability has no table `{table}`"
            )));
        }
        Ok((full, columns))
    }

    fn check_columns(
        &self,
        record: &Map<String, Value>,
        columns: &[String],
    ) -> Result<(), StoreError> {
        for key in record.keys() {
            if RESERVED.contains(&key.as_str()) {
                return Err(StoreError::Invalid(format!(
                    "`{key}` is managed by Frosty and cannot be set"
                )));
            }
            if !columns.iter().any(|c| c == key) {
                return Err(StoreError::Invalid(format!("unknown column `{key}`")));
            }
        }
        Ok(())
    }

    fn read_rows(&self, sql: &str, params: Vec<Sql>) -> Result<Vec<Value>, StoreError> {
        let mut stmt = self.conn.prepare(sql)?;
        let names: Vec<String> = stmt
            .column_names()
            .iter()
            .map(ToString::to_string)
            .collect();
        let rows = stmt.query_map(params_from_iter(params), |row| {
            let mut object = Map::new();
            for (i, name) in names.iter().enumerate() {
                object.insert(name.clone(), from_ref(row.get_ref(i)?));
            }
            Ok(Value::Object(object))
        })?;
        Ok(rows.collect::<Result<_, _>>()?)
    }

    pub fn insert(&self, table: &str, record: &Map<String, Value>) -> Result<Value, StoreError> {
        let (full, columns) = self.table(table)?;
        self.check_columns(record, &columns)?;
        let id = uuid::Uuid::now_v7().to_string();
        let now = now_millis();

        let mut names = vec!["id", "created_at", "updated_at", "device_id"]
            .into_iter()
            .map(str::to_string)
            .collect::<Vec<_>>();
        let mut values = vec![
            Sql::Text(id.clone()),
            Sql::Integer(now),
            Sql::Integer(now),
            Sql::Text(self.device_id.to_string()),
        ];
        for (k, v) in record {
            names.push(k.clone());
            values.push(to_sql(v));
        }
        let quoted: Vec<String> = names.iter().map(|n| format!("\"{n}\"")).collect();
        let marks = vec!["?"; names.len()].join(", ");
        self.conn.execute(
            &format!(
                "INSERT INTO \"{full}\" ({}) VALUES ({marks})",
                quoted.join(", ")
            ),
            params_from_iter(values),
        )?;
        self.get(table, &id)?
            .ok_or_else(|| StoreError::Invalid("row vanished after insert".into()))
    }

    pub fn get(&self, table: &str, id: &str) -> Result<Option<Value>, StoreError> {
        let (full, _) = self.table(table)?;
        let rows = self.read_rows(
            &format!("SELECT * FROM \"{full}\" WHERE id = ? AND deleted_at IS NULL"),
            vec![Sql::Text(id.to_string())],
        )?;
        Ok(rows.into_iter().next())
    }

    pub fn query(&self, table: &str, query: &Query) -> Result<Vec<Value>, StoreError> {
        let (full, columns) = self.table(table)?;
        let mut clauses = vec!["deleted_at IS NULL".to_string()];
        let mut params = Vec::new();
        for (key, value) in &query.filter {
            if !columns.iter().any(|c| c == key) {
                return Err(StoreError::Invalid(format!("unknown column `{key}`")));
            }
            if value.is_null() {
                clauses.push(format!("\"{key}\" IS NULL"));
            } else {
                clauses.push(format!("\"{key}\" = ?"));
                params.push(to_sql(value));
            }
        }
        let order = match &query.order_by {
            Some(col) if columns.iter().any(|c| c == col) => col.as_str(),
            Some(col) => {
                return Err(StoreError::Invalid(format!("unknown column `{col}`")));
            }
            None => "created_at",
        };
        let limit = query.limit.unwrap_or(DEFAULT_LIMIT).min(MAX_LIMIT);
        let sql = format!(
            "SELECT * FROM \"{full}\" WHERE {} ORDER BY \"{order}\" {} LIMIT {limit}",
            clauses.join(" AND "),
            if query.descending { "DESC" } else { "ASC" },
        );
        self.read_rows(&sql, params)
    }

    pub fn update(
        &self,
        table: &str,
        id: &str,
        patch: &Map<String, Value>,
    ) -> Result<Option<Value>, StoreError> {
        let (full, columns) = self.table(table)?;
        self.check_columns(patch, &columns)?;
        let mut sets = vec!["\"updated_at\" = ?".to_string()];
        let mut params = vec![Sql::Integer(now_millis())];
        for (k, v) in patch {
            sets.push(format!("\"{k}\" = ?"));
            params.push(to_sql(v));
        }
        params.push(Sql::Text(id.to_string()));
        let changed = self.conn.execute(
            &format!(
                "UPDATE \"{full}\" SET {} WHERE id = ? AND deleted_at IS NULL",
                sets.join(", ")
            ),
            params_from_iter(params),
        )?;
        if changed == 0 {
            return Ok(None);
        }
        self.get(table, id)
    }

    /// Soft delete: the row stays, marked, so a later sync can see it.
    pub fn soft_delete(&self, table: &str, id: &str) -> Result<bool, StoreError> {
        let (full, _) = self.table(table)?;
        let now = now_millis();
        let changed = self.conn.execute(
            &format!(
                "UPDATE \"{full}\" SET deleted_at = ?1, updated_at = ?1 WHERE id = ?2 AND deleted_at IS NULL"
            ),
            rusqlite::params![now, id],
        )?;
        Ok(changed > 0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use frosty_core::db::Database;
    use serde_json::json;

    fn setup() -> Database {
        let mut db = Database::open_in_memory().unwrap();
        let migration = vec![(
            "0001".to_string(),
            "CREATE TABLE cap_notes_items (
                id TEXT PRIMARY KEY, created_at INTEGER, updated_at INTEGER,
                deleted_at INTEGER, device_id TEXT,
                title TEXT, done INTEGER, minutes REAL, meta TEXT);"
                .to_string(),
        )];
        db.run_capability_migrations("notes", &migration).unwrap();
        db
    }

    fn obj(v: Value) -> Map<String, Value> {
        v.as_object().unwrap().clone()
    }

    #[test]
    fn insert_fills_sync_columns() {
        let db = setup();
        let store = CapStore::new(db.connection(), "notes", "dev-1");
        let row = store
            .insert("items", &obj(json!({"title": "milk", "done": false})))
            .unwrap();
        assert_eq!(row["title"], "milk");
        assert_eq!(row["done"], 0);
        assert_eq!(row["device_id"], "dev-1");
        assert!(row["id"].as_str().unwrap().len() == 36);
        assert!(row["created_at"].as_i64().unwrap() > 0);
        assert_eq!(row["deleted_at"], Value::Null);
    }

    #[test]
    fn query_filters_orders_and_hides_deleted() {
        let db = setup();
        let store = CapStore::new(db.connection(), "notes", "d");
        let a = store
            .insert("items", &obj(json!({"title": "a", "done": false})))
            .unwrap();
        store
            .insert("items", &obj(json!({"title": "b", "done": true})))
            .unwrap();
        store
            .insert("items", &obj(json!({"title": "c", "done": false})))
            .unwrap();
        store
            .soft_delete("items", a["id"].as_str().unwrap())
            .unwrap();

        let open = store
            .query(
                "items",
                &Query {
                    filter: obj(json!({"done": false})),
                    ..Default::default()
                },
            )
            .unwrap();
        assert_eq!(open.len(), 1);
        assert_eq!(open[0]["title"], "c");

        let desc = store
            .query(
                "items",
                &Query {
                    order_by: Some("title".into()),
                    descending: true,
                    ..Default::default()
                },
            )
            .unwrap();
        assert_eq!(desc[0]["title"], "c");
        assert_eq!(desc.len(), 2);
    }

    #[test]
    fn update_changes_fields_and_bumps_updated_at() {
        let db = setup();
        let store = CapStore::new(db.connection(), "notes", "d");
        let row = store.insert("items", &obj(json!({"title": "a"}))).unwrap();
        let id = row["id"].as_str().unwrap();
        std::thread::sleep(std::time::Duration::from_millis(3));
        let after = store
            .update("items", id, &obj(json!({"title": "z"})))
            .unwrap()
            .unwrap();
        assert_eq!(after["title"], "z");
        assert!(after["updated_at"].as_i64() > row["updated_at"].as_i64());
        assert!(
            store
                .update("items", "nope", &obj(json!({"title": "z"})))
                .unwrap()
                .is_none()
        );
    }

    #[test]
    fn objects_are_stored_as_json_text() {
        let db = setup();
        let store = CapStore::new(db.connection(), "notes", "d");
        let row = store
            .insert("items", &obj(json!({"meta": {"tags": ["x"]}})))
            .unwrap();
        let parsed: Value = serde_json::from_str(row["meta"].as_str().unwrap()).unwrap();
        assert_eq!(parsed["tags"][0], "x");
    }

    #[test]
    fn rejects_reserved_unknown_and_foreign_tables() {
        let db = setup();
        let store = CapStore::new(db.connection(), "notes", "d");
        assert!(store.insert("items", &obj(json!({"id": "mine"}))).is_err());
        assert!(store.insert("items", &obj(json!({"nope": 1}))).is_err());
        assert!(store.insert("missing", &obj(json!({}))).is_err());
        assert!(store.insert("kv", &obj(json!({}))).is_err());
        assert!(
            store
                .insert("items\"; DROP TABLE kv; --", &obj(json!({})))
                .is_err()
        );
        // Another capability using the same table name sees nothing of ours.
        let other = CapStore::new(db.connection(), "guitar", "d");
        assert!(other.query("items", &Query::default()).is_err());
    }

    #[test]
    fn sql_injection_in_values_and_filters_is_inert() {
        let db = setup();
        let store = CapStore::new(db.connection(), "notes", "d");
        store
            .insert(
                "items",
                &obj(json!({"title": "x'); DROP TABLE cap_notes_items; --"})),
            )
            .unwrap();
        assert_eq!(store.query("items", &Query::default()).unwrap().len(), 1);
        let bad_filter = Query {
            filter: obj(json!({"title\" = 1 OR \"1": 1})),
            ..Default::default()
        };
        assert!(store.query("items", &bad_filter).is_err());
    }

    #[test]
    fn limit_is_capped() {
        let db = setup();
        let store = CapStore::new(db.connection(), "notes", "d");
        for i in 0..5 {
            store
                .insert("items", &obj(json!({"title": i.to_string()})))
                .unwrap();
        }
        let rows = store
            .query(
                "items",
                &Query {
                    limit: Some(2),
                    ..Default::default()
                },
            )
            .unwrap();
        assert_eq!(rows.len(), 2);
    }
}
