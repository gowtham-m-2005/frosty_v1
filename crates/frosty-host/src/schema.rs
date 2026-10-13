//! JSON Schema validation for tool input and output, event payloads, and
//! capability settings.

use serde_json::{Map, Value};

/// Returns every problem found, as readable sentences.
pub fn validate(schema: &Value, instance: &Value) -> Result<(), Vec<String>> {
    let validator =
        jsonschema::validator_for(schema).map_err(|e| vec![format!("invalid schema: {e}")])?;
    let errors: Vec<String> = validator
        .iter_errors(instance)
        .map(|e| e.to_string())
        .collect();
    if errors.is_empty() {
        Ok(())
    } else {
        Err(errors)
    }
}

/// Fills in `default` values for top-level properties that are missing.
pub fn apply_defaults(schema: &Value, values: &Map<String, Value>) -> Map<String, Value> {
    let mut out = values.clone();
    if let Some(props) = schema.get("properties").and_then(Value::as_object) {
        for (name, prop) in props {
            if !out.contains_key(name)
                && let Some(default) = prop.get("default")
            {
                out.insert(name.clone(), default.clone());
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn validates_and_reports_all_errors() {
        let schema = json!({
            "type": "object",
            "required": ["title", "minutes"],
            "properties": { "title": {"type": "string"}, "minutes": {"type": "integer", "minimum": 1} }
        });
        assert!(validate(&schema, &json!({"title": "x", "minutes": 5})).is_ok());
        let errors = validate(&schema, &json!({"minutes": 0})).unwrap_err();
        assert_eq!(errors.len(), 2, "{errors:?}");
    }

    #[test]
    fn defaults_fill_only_missing_keys() {
        let schema = json!({"properties": {"a": {"default": 1}, "b": {"default": "x"}}});
        let mut given = Map::new();
        given.insert("a".into(), json!(9));
        let out = apply_defaults(&schema, &given);
        assert_eq!(out["a"], 9);
        assert_eq!(out["b"], "x");
    }
}
