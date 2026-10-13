//! `capability.toml`: parsing and validation (docs/PROJECT_SPEC.md 8.1).

use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
};

use serde::{Deserialize, Serialize};
use serde_json::Value;

/// The SDK contract version this host understands.
pub const SUPPORTED_API_VERSION: &str = "1";

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Manifest {
    pub capability: CapabilitySection,
    #[serde(default)]
    pub permissions: Permissions,
    #[serde(default)]
    pub storage: StorageSection,
    #[serde(default)]
    pub settings: SettingsSection,
    #[serde(default)]
    pub tools: Vec<ToolDecl>,
    #[serde(default)]
    pub events: Vec<EventDecl>,
    #[serde(default)]
    pub reactions: Vec<ReactionDecl>,
}

fn default_runtime() -> String {
    "ts".into()
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct CapabilitySection {
    pub id: String,
    pub name: String,
    pub version: String,
    pub api_version: String,
    #[serde(default = "default_runtime")]
    pub runtime: String,
    #[serde(default)]
    pub description: String,
    /// Logic entry, loaded in the hidden logic host.
    pub entry: String,
    /// Views entry, loaded in the overlay and main windows.
    #[serde(default)]
    pub views: Option<String>,
    #[serde(default)]
    pub icon: Option<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize, Serialize)]
#[serde(default)]
pub struct Permissions {
    pub storage: bool,
    pub scheduler: bool,
    pub notifications: bool,
    pub clipboard: bool,
    pub network: Vec<String>,
    pub fs: Vec<String>,
    pub shell: bool,
    pub events_subscribe: Vec<String>,
    /// Tool ids this capability may call in other capabilities.
    pub tools_call: Vec<String>,
}

#[derive(Debug, Clone, Default, Deserialize, Serialize)]
#[serde(default)]
pub struct StorageSection {
    pub migrations: Option<String>,
}

#[derive(Debug, Clone, Default, Deserialize, Serialize)]
#[serde(default)]
pub struct SettingsSection {
    pub schema: Option<String>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ToolDecl {
    pub id: String,
    pub title: String,
    #[serde(default)]
    pub input_schema: Option<String>,
    #[serde(default)]
    pub output_schema: Option<String>,
    #[serde(default)]
    pub quick_action: bool,
    /// `stepper`, `text`, `toggle`, or `button`.
    #[serde(default)]
    pub quick_action_control: Option<String>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct EventDecl {
    pub id: String,
    #[serde(default)]
    pub payload_schema: Option<String>,
}

/// A number from 0 to 1, or `small`, `medium`, `large`.
#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
#[serde(untagged)]
pub enum IntensityDecl {
    Number(f64),
    Named(String),
}

impl IntensityDecl {
    pub fn value(&self) -> Option<f32> {
        match self {
            Self::Number(n) if (0.0..=1.0).contains(n) => Some(*n as f32),
            Self::Number(_) => None,
            Self::Named(name) => match name.as_str() {
                "small" => Some(0.3),
                "medium" => Some(0.5),
                "large" => Some(0.8),
                _ => None,
            },
        }
    }
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ReactionDecl {
    pub on: String,
    pub animation: String,
    pub intensity: IntensityDecl,
    #[serde(default)]
    pub microcopy: Option<String>,
}

const ANIMATIONS: [&str; 9] = [
    "wave",
    "happy_small",
    "celebrate",
    "curious_tilt",
    "nudge",
    "sleepy",
    "oops",
    "thinking",
    "bye",
];

pub fn is_valid_capability_id(id: &str) -> bool {
    let mut chars = id.chars();
    matches!(chars.next(), Some(c) if c.is_ascii_lowercase())
        && id.len() <= 32
        && chars.all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_')
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Origin {
    BuiltIn,
    User,
}

/// A manifest plus everything it references, read and checked.
#[derive(Debug, Clone)]
pub struct LoadedCapability {
    pub manifest: Manifest,
    pub dir: PathBuf,
    pub origin: Origin,
    pub tool_schemas: BTreeMap<String, ToolSchemas>,
    pub event_schemas: BTreeMap<String, Value>,
    pub settings_schema: Option<Value>,
    /// `(version, sql)` sorted by version.
    pub migrations: Vec<(String, String)>,
}

#[derive(Debug, Clone, Default)]
pub struct ToolSchemas {
    pub input: Option<Value>,
    pub output: Option<Value>,
}

impl LoadedCapability {
    pub fn id(&self) -> &str {
        &self.manifest.capability.id
    }
}

fn line_and_column(text: &str, offset: usize) -> (usize, usize) {
    let offset = offset.min(text.len());
    let before = &text[..offset];
    let line = before.matches('\n').count() + 1;
    let column = before.rsplit('\n').next().map_or(0, |l| l.chars().count()) + 1;
    (line, column)
}

pub fn parse_manifest(text: &str) -> Result<Manifest, String> {
    toml::from_str::<Manifest>(text).map_err(|e| match e.span() {
        Some(span) => {
            let (line, column) = line_and_column(text, span.start);
            format!(
                "capability.toml: {} (line {line}, column {column})",
                e.message()
            )
        }
        None => format!("capability.toml: {}", e.message()),
    })
}

/// Rejects paths that could point outside the capability folder.
fn safe_relative(path: &str) -> bool {
    let p = Path::new(path);
    !path.is_empty()
        && p.is_relative()
        && p.components()
            .all(|c| matches!(c, std::path::Component::Normal(_)))
}

fn read_schema(dir: &Path, rel: &str, what: &str, errors: &mut Vec<String>) -> Option<Value> {
    if !safe_relative(rel) {
        errors.push(format!(
            "{what}: `{rel}` must be a relative path inside the capability folder"
        ));
        return None;
    }
    let text = match fs::read_to_string(dir.join(rel)) {
        Ok(t) => t,
        Err(e) => {
            errors.push(format!("{what}: cannot read `{rel}` ({e})"));
            return None;
        }
    };
    let value: Value = match serde_json::from_str(&text) {
        Ok(v) => v,
        Err(e) => {
            errors.push(format!("{what}: `{rel}` is not valid JSON ({e})"));
            return None;
        }
    };
    if let Err(e) = jsonschema::validator_for(&value) {
        errors.push(format!("{what}: `{rel}` is not a valid JSON Schema ({e})"));
        return None;
    }
    Some(value)
}

/// Reads and validates one capability folder. Every problem found is
/// reported, not just the first, so the module manager can show them all.
pub fn load_capability(dir: &Path, origin: Origin) -> Result<LoadedCapability, Vec<String>> {
    let text = fs::read_to_string(dir.join("capability.toml"))
        .map_err(|e| vec![format!("cannot read capability.toml ({e})")])?;
    let manifest = parse_manifest(&text).map_err(|e| vec![e])?;
    let c = &manifest.capability;
    let mut errors = Vec::new();

    if !is_valid_capability_id(&c.id) {
        errors.push(format!(
            "id `{}` is not valid: use lowercase letters, digits and `_`, starting with a letter, at most 32 characters",
            c.id
        ));
    }
    if c.name.trim().is_empty() {
        errors.push("name must not be empty".into());
    }
    if c.api_version != SUPPORTED_API_VERSION {
        errors.push(format!(
            "api_version `{}` is not supported (this Frosty understands `{SUPPORTED_API_VERSION}`)",
            c.api_version
        ));
    }
    if c.runtime != "ts" {
        errors.push(format!(
            "runtime `{}` is not supported yet; only `ts` is available",
            c.runtime
        ));
    }

    for (what, rel) in [
        ("entry", Some(&c.entry)),
        ("views", c.views.as_ref()),
        ("icon", c.icon.as_ref()),
    ] {
        let Some(rel) = rel else { continue };
        if !safe_relative(rel) {
            errors.push(format!(
                "{what}: `{rel}` must be a relative path inside the capability folder"
            ));
        } else if !dir.join(rel).is_file() {
            errors.push(format!(
                "{what}: file `{rel}` does not exist (has the capability been built?)"
            ));
        }
    }

    let prefix = format!("{}.", c.id);
    let mut tool_schemas = BTreeMap::new();
    for tool in &manifest.tools {
        if !tool.id.starts_with(&prefix) {
            errors.push(format!("tool `{}` must start with `{prefix}`", tool.id));
        }
        if let Some(control) = &tool.quick_action_control
            && !["stepper", "text", "toggle", "button"].contains(&control.as_str())
        {
            errors.push(format!(
                "tool `{}`: quick_action_control `{control}` is not one of stepper, text, toggle, button",
                tool.id
            ));
        }
        let mut schemas = ToolSchemas::default();
        if let Some(rel) = &tool.input_schema {
            schemas.input = read_schema(
                dir,
                rel,
                &format!("tool `{}` input_schema", tool.id),
                &mut errors,
            );
        }
        if let Some(rel) = &tool.output_schema {
            schemas.output = read_schema(
                dir,
                rel,
                &format!("tool `{}` output_schema", tool.id),
                &mut errors,
            );
        }
        tool_schemas.insert(tool.id.clone(), schemas);
    }

    let mut event_schemas = BTreeMap::new();
    for event in &manifest.events {
        if !event.id.starts_with(&prefix) {
            errors.push(format!("event `{}` must start with `{prefix}`", event.id));
        }
        if let Some(rel) = &event.payload_schema
            && let Some(schema) = read_schema(
                dir,
                rel,
                &format!("event `{}` payload_schema", event.id),
                &mut errors,
            )
        {
            event_schemas.insert(event.id.clone(), schema);
        }
    }

    for reaction in &manifest.reactions {
        if !ANIMATIONS.contains(&reaction.animation.as_str()) {
            errors.push(format!(
                "reaction on `{}`: unknown animation `{}`",
                reaction.on, reaction.animation
            ));
        }
        if reaction.intensity.value().is_none() {
            errors.push(format!(
                "reaction on `{}`: intensity must be a number from 0 to 1, or small, medium, large",
                reaction.on
            ));
        }
    }

    let settings_schema = manifest
        .settings
        .schema
        .as_deref()
        .and_then(|rel| read_schema(dir, rel, "settings schema", &mut errors));

    let mut migrations = Vec::new();
    if let Some(rel) = &manifest.storage.migrations {
        if !safe_relative(rel) {
            errors.push(format!(
                "storage migrations: `{rel}` must be a relative folder"
            ));
        } else if let Ok(entries) = fs::read_dir(dir.join(rel)) {
            for path in entries.filter_map(Result::ok).map(|e| e.path()) {
                if path.extension().and_then(|e| e.to_str()) != Some("sql") {
                    continue;
                }
                let version = path
                    .file_stem()
                    .and_then(|s| s.to_str())
                    .unwrap_or_default()
                    .to_string();
                match fs::read_to_string(&path) {
                    Ok(sql) => migrations.push((version, sql)),
                    Err(e) => {
                        errors.push(format!("migration {}: cannot read ({e})", path.display()))
                    }
                }
            }
            migrations.sort();
        } else {
            errors.push(format!("storage migrations: folder `{rel}` does not exist"));
        }
    }

    if !errors.is_empty() {
        return Err(errors);
    }
    Ok(LoadedCapability {
        manifest,
        dir: dir.to_path_buf(),
        origin,
        tool_schemas,
        event_schemas,
        settings_schema,
        migrations,
    })
}

#[cfg(test)]
pub(crate) mod test_support {
    use std::{fs, path::Path};

    pub const GOOD: &str = r#"
[capability]
id = "notes"
name = "Notes"
version = "0.1.0"
api_version = "1"
description = "Quick notes"
entry = "dist/index.js"
views = "dist/views.js"

[permissions]
storage = true

[storage]
migrations = "migrations/"

[settings]
schema = "settings.schema.json"

[[tools]]
id = "notes.add"
title = "Add note"
input_schema = "schemas/add.input.json"
quick_action = true
quick_action_control = "text"

[[events]]
id = "notes.created"

[[reactions]]
on = "notes.created"
animation = "happy_small"
intensity = "small"
"#;

    pub fn write_good(dir: &Path) {
        fs::create_dir_all(dir.join("dist")).unwrap();
        fs::create_dir_all(dir.join("migrations")).unwrap();
        fs::create_dir_all(dir.join("schemas")).unwrap();
        fs::write(dir.join("capability.toml"), GOOD).unwrap();
        fs::write(dir.join("dist/index.js"), "export default {};").unwrap();
        fs::write(dir.join("dist/views.js"), "export default {};").unwrap();
        fs::write(
            dir.join("migrations/0001_init.sql"),
            "CREATE TABLE cap_notes_items (id TEXT PRIMARY KEY, created_at INTEGER, updated_at INTEGER, deleted_at INTEGER, device_id TEXT, title TEXT);",
        )
        .unwrap();
        fs::write(
            dir.join("settings.schema.json"),
            r#"{"type":"object","properties":{}}"#,
        )
        .unwrap();
        fs::write(
            dir.join("schemas/add.input.json"),
            r#"{"type":"object","required":["title"],"properties":{"title":{"type":"string"}}}"#,
        )
        .unwrap();
    }
}

#[cfg(test)]
mod tests {
    use super::{test_support::*, *};

    #[test]
    fn a_good_capability_loads() {
        let tmp = tempfile::tempdir().unwrap();
        write_good(tmp.path());
        let cap = load_capability(tmp.path(), Origin::User).unwrap();
        assert_eq!(cap.id(), "notes");
        assert_eq!(cap.migrations.len(), 1);
        assert!(cap.tool_schemas["notes.add"].input.is_some());
        assert!(cap.settings_schema.is_some());
    }

    #[test]
    fn all_problems_are_reported_together() {
        let tmp = tempfile::tempdir().unwrap();
        write_good(tmp.path());
        let bad = GOOD
            .replace(r#"id = "notes""#, r#"id = "Bad Id""#)
            .replace(r#"api_version = "1""#, r#"api_version = "9""#)
            .replace(r#"id = "notes.add""#, r#"id = "other.add""#)
            .replace(r#"animation = "happy_small""#, r#"animation = "dance""#);
        fs::write(tmp.path().join("capability.toml"), bad).unwrap();
        let errors = load_capability(tmp.path(), Origin::User).unwrap_err();
        let all = errors.join("\n");
        assert!(all.contains("id `Bad Id`"), "{all}");
        assert!(all.contains("api_version `9`"), "{all}");
        assert!(all.contains("must start with"), "{all}");
        assert!(all.contains("unknown animation `dance`"), "{all}");
    }

    #[test]
    fn missing_entry_file_points_at_the_build() {
        let tmp = tempfile::tempdir().unwrap();
        write_good(tmp.path());
        fs::remove_file(tmp.path().join("dist/index.js")).unwrap();
        let errors = load_capability(tmp.path(), Origin::User).unwrap_err();
        assert!(
            errors[0].contains("has the capability been built"),
            "{errors:?}"
        );
    }

    #[test]
    fn python_runtime_is_not_supported_yet() {
        let tmp = tempfile::tempdir().unwrap();
        write_good(tmp.path());
        let text = GOOD.replace(
            "api_version = \"1\"",
            "api_version = \"1\"\nruntime = \"python\"",
        );
        fs::write(tmp.path().join("capability.toml"), text).unwrap();
        let errors = load_capability(tmp.path(), Origin::User).unwrap_err();
        assert!(errors.iter().any(|e| e.contains("not supported yet")));
    }

    #[test]
    fn paths_cannot_escape_the_folder() {
        let tmp = tempfile::tempdir().unwrap();
        write_good(tmp.path());
        let text = GOOD.replace("dist/index.js", "../outside.js");
        fs::write(tmp.path().join("capability.toml"), text).unwrap();
        let errors = load_capability(tmp.path(), Origin::User).unwrap_err();
        assert!(
            errors
                .iter()
                .any(|e| e.contains("inside the capability folder")),
            "{errors:?}"
        );
    }

    #[test]
    fn invalid_toml_reports_a_line() {
        let err = parse_manifest("[capability\nid = 1").unwrap_err();
        assert!(err.contains("line"), "{err}");
    }

    #[test]
    fn intensity_values() {
        assert_eq!(IntensityDecl::Named("large".into()).value(), Some(0.8));
        assert_eq!(IntensityDecl::Number(0.25).value(), Some(0.25));
        assert_eq!(IntensityDecl::Number(1.5).value(), None);
        assert_eq!(IntensityDecl::Named("huge".into()).value(), None);
    }

    #[test]
    fn id_rules() {
        assert!(is_valid_capability_id("guitar_log"));
        assert!(!is_valid_capability_id("1abc"));
        assert!(!is_valid_capability_id("a-b"));
        assert!(!is_valid_capability_id(""));
    }
}
