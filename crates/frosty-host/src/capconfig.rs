//! Per-capability settings files: `capability-config/<id>.toml`
//! (docs/PROJECT_SPEC.md 8.7). Writes keep comments and unknown keys, like
//! the main config.

use std::{fs, io, path::Path};

use serde_json::{Map, Value};
use toml_edit::{DocumentMut, Item, Table};

#[derive(Debug, thiserror::Error)]
pub enum CapConfigError {
    #[error("settings file is not valid TOML: {0}")]
    Parse(String),
    #[error("settings file error: {0}")]
    Io(#[from] io::Error),
    #[error("unsupported setting value for `{0}`")]
    Unsupported(String),
}

/// Reads the file as a JSON object. A missing file is an empty object.
pub fn read(path: &Path) -> Result<Map<String, Value>, CapConfigError> {
    let text = match fs::read_to_string(path) {
        Ok(t) => t,
        Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(Map::new()),
        Err(e) => return Err(e.into()),
    };
    let table: toml::Table =
        toml::from_str(&text).map_err(|e| CapConfigError::Parse(e.to_string()))?;
    match serde_json::to_value(table) {
        Ok(Value::Object(map)) => Ok(map),
        _ => Ok(Map::new()),
    }
}

fn json_to_item(key: &str, value: &Value) -> Result<Item, CapConfigError> {
    use toml_edit::{Array, InlineTable, Value as T};
    fn to_value(key: &str, value: &Value) -> Result<T, CapConfigError> {
        Ok(match value {
            Value::Bool(b) => T::from(*b),
            Value::String(s) => T::from(s.as_str()),
            Value::Number(n) => match (n.as_i64(), n.as_f64()) {
                (Some(i), _) => T::from(i),
                (None, Some(f)) => T::from(f),
                _ => return Err(CapConfigError::Unsupported(key.to_string())),
            },
            Value::Array(items) => {
                let mut array = Array::new();
                for item in items {
                    array.push(to_value(key, item)?);
                }
                T::from(array)
            }
            Value::Object(map) => {
                let mut table = InlineTable::new();
                for (k, v) in map {
                    table.insert(k, to_value(k, v)?);
                }
                T::from(table)
            }
            Value::Null => return Err(CapConfigError::Unsupported(key.to_string())),
        })
    }
    Ok(Item::Value(to_value(key, value)?))
}

/// Sets the given top-level keys, leaving everything else in the file alone.
pub fn write(path: &Path, values: &Map<String, Value>) -> Result<(), CapConfigError> {
    let existing = match fs::read_to_string(path) {
        Ok(t) => t,
        Err(e) if e.kind() == io::ErrorKind::NotFound => String::new(),
        Err(e) => return Err(e.into()),
    };
    let mut doc: DocumentMut = existing
        .parse()
        .map_err(|e: toml_edit::TomlError| CapConfigError::Parse(e.to_string()))?;
    let table: &mut Table = doc.as_table_mut();
    for (key, value) in values {
        let mut item = json_to_item(key, value)?;
        // Keep the comment on the line of a value being replaced.
        if let (Some(Item::Value(old)), Item::Value(new)) = (table.get(key), &mut item) {
            *new.decor_mut() = old.decor().clone();
        }
        // Replace in place so the key keeps its leading comment.
        match table.get_mut(key) {
            Some(slot) => *slot = item,
            None => {
                table.insert(key, item);
            }
        }
    }
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let tmp = path.with_extension("toml.tmp");
    fs::write(&tmp, doc.to_string())?;
    fs::rename(&tmp, path)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn missing_file_reads_as_empty() {
        let tmp = tempfile::tempdir().unwrap();
        assert!(read(&tmp.path().join("nope.toml")).unwrap().is_empty());
    }

    #[test]
    fn roundtrip_and_comments_survive() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("notes.toml");
        fs::write(
            &path,
            "# my notes\nsort = \"newest\" # or oldest\nunknown = 7\n",
        )
        .unwrap();
        let mut values = Map::new();
        values.insert("sort".into(), json!("oldest"));
        values.insert("show_done".into(), json!(true));
        values.insert("tags".into(), json!(["a", "b"]));
        write(&path, &values).unwrap();

        let text = fs::read_to_string(&path).unwrap();
        assert!(text.contains("# my notes"), "{text}");
        assert!(text.contains("# or oldest"), "{text}");
        assert!(text.contains("unknown = 7"), "{text}");

        let read_back = read(&path).unwrap();
        assert_eq!(read_back["sort"], "oldest");
        assert_eq!(read_back["show_done"], true);
        assert_eq!(read_back["tags"], json!(["a", "b"]));
        assert_eq!(read_back["unknown"], 7);
    }

    #[test]
    fn null_is_rejected() {
        let tmp = tempfile::tempdir().unwrap();
        let mut values = Map::new();
        values.insert("x".into(), Value::Null);
        assert!(matches!(
            write(&tmp.path().join("a.toml"), &values),
            Err(CapConfigError::Unsupported(_))
        ));
    }
}
