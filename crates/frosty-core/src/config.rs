//! Config service: loads, validates, and edits `frosty.toml`
//! (docs/PROJECT_SPEC.md section 7.1 and 9).
//!
//! The file is the source of truth. A bad edit never replaces the last good
//! config, UI edits keep comments and unknown keys, and every rewrite is
//! preceded by a backup.

use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};

use serde::{Deserialize, Serialize};
use toml_edit::{DocumentMut, Item, Table, Value};

pub const CURRENT_CONFIG_VERSION: u32 = 1;
const BACKUPS_KEPT: usize = 10;

#[derive(Debug, thiserror::Error)]
pub enum ConfigError {
    #[error("{message} (line {line}, column {column})")]
    Parse {
        message: String,
        line: usize,
        column: usize,
    },
    #[error(
        "config_version {found} is newer than this build understands ({CURRENT_CONFIG_VERSION})"
    )]
    TooNew { found: u32 },
    #[error("invalid config edit: {0}")]
    Edit(String),
    #[error("config file error: {0}")]
    Io(#[from] std::io::Error),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Tone {
    Casual,
    Neutral,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ReactionLevel {
    Full,
    Subtle,
    Off,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ModeSetting {
    System,
    Light,
    Dark,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ReducedMotion {
    System,
    On,
    Off,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum FoxSize {
    Small,
    Medium,
    Large,
}

impl FoxSize {
    /// Logical pixel height of the fox (docs/DESIGN_SPEC.md 2.6).
    pub fn pixels(self) -> u32 {
        match self {
            Self::Small => 112,
            Self::Medium => 160,
            Self::Large => 224,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct CoreSection {
    pub config_version: u32,
    pub device_name: String,
    pub start_on_login: bool,
}

impl Default for CoreSection {
    fn default() -> Self {
        Self {
            config_version: CURRENT_CONFIG_VERSION,
            device_name: "this device".into(),
            start_on_login: false,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct AppearanceSection {
    pub theme: String,
    pub mode: ModeSetting,
}

impl Default for AppearanceSection {
    fn default() -> Self {
        Self {
            theme: "frost".into(),
            mode: ModeSetting::System,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct HotkeysSection {
    pub summon: String,
    pub fullscreen: String,
}

impl Default for HotkeysSection {
    fn default() -> Self {
        Self {
            summon: "Ctrl+Alt+F".into(),
            fullscreen: "Ctrl+Alt+Shift+F".into(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct CharacterSection {
    pub skin: String,
    pub size: FoxSize,
    pub reactions: ReactionLevel,
    pub reduced_motion: ReducedMotion,
}

impl Default for CharacterSection {
    fn default() -> Self {
        Self {
            skin: "arctic-fox-default".into(),
            size: FoxSize::Medium,
            reactions: ReactionLevel::Full,
            reduced_motion: ReducedMotion::System,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct PersonalitySection {
    pub tone: Tone,
    pub locale: String,
    /// Event patterns Frosty never reacts to, for example `notes.*`.
    pub never_react: Vec<String>,
}

impl Default for PersonalitySection {
    fn default() -> Self {
        Self {
            tone: Tone::Casual,
            locale: "en".into(),
            never_react: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct NotificationsSection {
    pub dnd_start: String,
    pub dnd_end: String,
    pub os_fallback: bool,
    pub fullscreen_behavior: String,
}

impl Default for NotificationsSection {
    fn default() -> Self {
        Self {
            dnd_start: "23:00".into(),
            dnd_end: "07:00".into(),
            os_fallback: true,
            fullscreen_behavior: "wait".into(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct CapabilitiesSection {
    pub enabled: Vec<String>,
    pub dock_order: Vec<String>,
    pub permissions: BTreeMap<String, toml::Table>,
}

#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct FrostyConfig {
    pub core: CoreSection,
    pub appearance: AppearanceSection,
    pub hotkeys: HotkeysSection,
    pub character: CharacterSection,
    pub personality: PersonalitySection,
    pub notifications: NotificationsSection,
    pub capabilities: CapabilitiesSection,
}

/// The file written on first run. Comments here are the user's documentation.
pub const DEFAULT_CONFIG_FILE: &str = r#"# Frosty configuration. Edits are applied live.
# Every setting in the app's settings screen is a key in this file.

[core]
config_version = 1
device_name = "this device"
start_on_login = false

[appearance]
theme = "frost"          # a theme id from the themes folder
mode = "system"          # system | light | dark

[hotkeys]
summon = "Ctrl+Alt+F"
fullscreen = "Ctrl+Alt+Shift+F"

[character]
skin = "arctic-fox-default"
size = "medium"          # small | medium | large
reactions = "full"       # full | subtle | off
reduced_motion = "system" # system | on | off

[personality]
tone = "casual"          # casual | neutral
locale = "en"
never_react = []         # event patterns, for example ["notes.*"]

[notifications]
dnd_start = "23:00"
dnd_end = "07:00"
os_fallback = true
fullscreen_behavior = "wait"

[capabilities]
enabled = []
dock_order = []
"#;

fn line_and_column(text: &str, offset: usize) -> (usize, usize) {
    let offset = offset.min(text.len());
    let before = &text[..offset];
    let line = before.matches('\n').count() + 1;
    let column = before.rsplit('\n').next().map_or(0, |l| l.chars().count()) + 1;
    (line, column)
}

/// Parses and validates config text without touching the filesystem.
pub fn parse(text: &str) -> Result<FrostyConfig, ConfigError> {
    let cfg: FrostyConfig = toml::from_str(text).map_err(|e| {
        let (line, column) = e.span().map_or((1, 1), |s| line_and_column(text, s.start));
        ConfigError::Parse {
            message: e.message().to_string(),
            line,
            column,
        }
    })?;
    if cfg.core.config_version > CURRENT_CONFIG_VERSION {
        return Err(ConfigError::TooNew {
            found: cfg.core.config_version,
        });
    }
    Ok(cfg)
}

pub struct ConfigStore {
    path: PathBuf,
    backups_dir: PathBuf,
    current: FrostyConfig,
}

impl ConfigStore {
    /// Loads the config, writing the default file first if none exists. A file
    /// that exists but is invalid is an error here, so the caller can show it;
    /// there is no earlier good config to fall back to at startup.
    pub fn load(
        path: impl Into<PathBuf>,
        backups_dir: impl Into<PathBuf>,
    ) -> Result<Self, ConfigError> {
        let path = path.into();
        if !path.exists() {
            if let Some(parent) = path.parent() {
                fs::create_dir_all(parent)?;
            }
            fs::write(&path, DEFAULT_CONFIG_FILE)?;
        }
        let text = fs::read_to_string(&path)?;
        Ok(Self {
            current: parse(&text)?,
            path,
            backups_dir: backups_dir.into(),
        })
    }

    pub fn current(&self) -> &FrostyConfig {
        &self.current
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Re-reads the file. On error the previous config stays active.
    /// Returns whether the config changed.
    pub fn reload(&mut self) -> Result<bool, ConfigError> {
        let text = fs::read_to_string(&self.path)?;
        let next = parse(&text)?;
        let changed = next != self.current;
        self.current = next;
        Ok(changed)
    }

    /// Sets a dotted key such as `personality.tone`, keeping comments, key
    /// order, and unknown keys. The result is validated before it is written,
    /// so the UI cannot produce an invalid file.
    pub fn set(&mut self, key: &str, value: Value) -> Result<(), ConfigError> {
        let text = fs::read_to_string(&self.path)?;
        let mut doc: DocumentMut = text.parse().map_err(|e: toml_edit::TomlError| {
            let (line, column) = e.span().map_or((1, 1), |s| line_and_column(&text, s.start));
            ConfigError::Parse {
                message: e.message().to_string(),
                line,
                column,
            }
        })?;

        let parts: Vec<&str> = key.split('.').collect();
        let (last, parents) = parts
            .split_last()
            .filter(|(last, _)| !last.is_empty())
            .ok_or_else(|| ConfigError::Edit(format!("empty key `{key}`")))?;
        let mut table: &mut Table = doc.as_table_mut();
        for part in parents {
            let item = table
                .entry(part)
                .or_insert_with(|| Item::Table(Table::new()));
            table = item
                .as_table_mut()
                .ok_or_else(|| ConfigError::Edit(format!("`{part}` is not a table")))?;
        }
        // Keep the spacing and trailing comment of the value being replaced.
        let mut value = value;
        if let Some(Item::Value(old)) = table.get(last) {
            *value.decor_mut() = old.decor().clone();
        }
        table.insert(last, Item::Value(value));

        let new_text = doc.to_string();
        let next = parse(&new_text)?;

        self.backup(&text)?;
        let tmp = self.path.with_extension("toml.tmp");
        fs::write(&tmp, &new_text)?;
        fs::rename(&tmp, &self.path)?;
        self.current = next;
        Ok(())
    }

    fn backup(&self, previous_text: &str) -> Result<(), ConfigError> {
        fs::create_dir_all(&self.backups_dir)?;
        let millis = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_or(0, |d| d.as_millis());
        fs::write(
            self.backups_dir.join(format!("frosty-{millis:016}.toml")),
            previous_text,
        )?;
        let mut backups: Vec<PathBuf> = fs::read_dir(&self.backups_dir)?
            .filter_map(Result::ok)
            .map(|e| e.path())
            .filter(|p| {
                p.file_name()
                    .and_then(|n| n.to_str())
                    .is_some_and(|n| n.starts_with("frosty-") && n.ends_with(".toml"))
            })
            .collect();
        backups.sort();
        while backups.len() > BACKUPS_KEPT {
            fs::remove_file(backups.remove(0))?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn store(dir: &Path) -> ConfigStore {
        ConfigStore::load(dir.join("frosty.toml"), dir.join("backups")).unwrap()
    }

    #[test]
    fn default_file_parses_to_defaults() {
        assert_eq!(parse(DEFAULT_CONFIG_FILE).unwrap(), FrostyConfig::default());
    }

    #[test]
    fn missing_file_is_created_with_defaults() {
        let tmp = tempfile::tempdir().unwrap();
        let s = store(tmp.path());
        assert!(s.path().exists());
        assert_eq!(s.current().personality.tone, Tone::Casual);
    }

    #[test]
    fn parse_errors_carry_line_and_column() {
        let err =
            parse("[core]\nconfig_version = 1\n[personality]\ntone = \"loud\"\n").unwrap_err();
        match err {
            ConfigError::Parse { line, .. } => assert_eq!(line, 4),
            other => panic!("unexpected error: {other}"),
        }
    }

    #[test]
    fn newer_config_version_is_rejected() {
        assert!(matches!(
            parse("[core]\nconfig_version = 99\n"),
            Err(ConfigError::TooNew { found: 99 })
        ));
    }

    #[test]
    fn reload_keeps_last_good_config_when_file_is_broken() {
        let tmp = tempfile::tempdir().unwrap();
        let mut s = store(tmp.path());
        fs::write(s.path(), "[personality]\ntone = \"neutral\"\n").unwrap();
        assert!(s.reload().unwrap());
        assert_eq!(s.current().personality.tone, Tone::Neutral);

        fs::write(s.path(), "this is [not toml").unwrap();
        assert!(s.reload().is_err());
        assert_eq!(s.current().personality.tone, Tone::Neutral);
    }

    #[test]
    fn set_preserves_comments_and_unknown_keys() {
        let tmp = tempfile::tempdir().unwrap();
        let mut s = store(tmp.path());
        let with_extras = format!(
            "{}\n[future]\nkeep_me = 42 # from a newer build\n",
            DEFAULT_CONFIG_FILE
        );
        fs::write(s.path(), with_extras).unwrap();
        s.reload().unwrap();

        s.set("personality.tone", Value::from("neutral")).unwrap();

        let text = fs::read_to_string(s.path()).unwrap();
        assert!(text.contains("# casual | neutral"), "comment lost:\n{text}");
        assert!(text.contains("keep_me = 42 # from a newer build"));
        assert_eq!(s.current().personality.tone, Tone::Neutral);
    }

    #[test]
    fn set_creates_missing_tables() {
        let tmp = tempfile::tempdir().unwrap();
        let mut s = store(tmp.path());
        fs::write(s.path(), "[core]\nconfig_version = 1\n").unwrap();
        s.reload().unwrap();
        s.set("character.size", Value::from("large")).unwrap();
        assert_eq!(s.current().character.size, FoxSize::Large);
    }

    #[test]
    fn invalid_set_is_rejected_and_nothing_is_written() {
        let tmp = tempfile::tempdir().unwrap();
        let mut s = store(tmp.path());
        let before = fs::read_to_string(s.path()).unwrap();
        assert!(s.set("personality.tone", Value::from("shouting")).is_err());
        assert_eq!(fs::read_to_string(s.path()).unwrap(), before);
        assert_eq!(s.current().personality.tone, Tone::Casual);
    }

    #[test]
    fn backups_are_pruned() {
        let tmp = tempfile::tempdir().unwrap();
        let mut s = store(tmp.path());
        for i in 0..15 {
            let tone = if i % 2 == 0 { "neutral" } else { "casual" };
            s.set("personality.tone", Value::from(tone)).unwrap();
            std::thread::sleep(std::time::Duration::from_millis(2));
        }
        let count = fs::read_dir(tmp.path().join("backups")).unwrap().count();
        assert_eq!(count, BACKUPS_KEPT);
    }

    #[test]
    fn random_garbage_never_panics() {
        // Cheap stand-in for the fuzz test: mangled copies of a real file.
        let base = DEFAULT_CONFIG_FILE.as_bytes();
        let mut seed = 0x2545_F491_4F6C_DD1Du64;
        for _ in 0..300 {
            let mut bytes = base.to_vec();
            for _ in 0..6 {
                seed ^= seed << 13;
                seed ^= seed >> 7;
                seed ^= seed << 17;
                let i = (seed as usize) % bytes.len();
                bytes[i] = (seed >> 24) as u8;
            }
            let _ = parse(&String::from_utf8_lossy(&bytes));
        }
    }
}
