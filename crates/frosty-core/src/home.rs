//! Frosty Home: the per-device directory holding config, data, and
//! capabilities (docs/PROJECT_SPEC.md section 9.1).

use std::{
    io,
    path::{Path, PathBuf},
};

#[derive(Debug, Clone)]
pub struct FrostyHome {
    root: PathBuf,
}

impl FrostyHome {
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self { root: root.into() }
    }

    /// The standard app-data folder: `%APPDATA%\frosty` on Windows.
    /// `FROSTY_HOME` overrides it, which is how tests and demos run against a
    /// throwaway home without touching real data.
    pub fn resolve() -> Self {
        if let Some(custom) = std::env::var_os("FROSTY_HOME") {
            return Self::new(PathBuf::from(custom));
        }
        let base = std::env::var_os("APPDATA")
            .map(PathBuf::from)
            .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".config")))
            .unwrap_or_else(|| PathBuf::from("."));
        Self::new(base.join("frosty"))
    }

    pub fn root(&self) -> &Path {
        &self.root
    }
    pub fn config_file(&self) -> PathBuf {
        self.root.join("frosty.toml")
    }
    pub fn database_file(&self) -> PathBuf {
        self.root.join("frosty.db")
    }
    pub fn capabilities_dir(&self) -> PathBuf {
        self.root.join("capabilities")
    }
    pub fn capability_config_dir(&self) -> PathBuf {
        self.root.join("capability-config")
    }
    pub fn microcopy_dir(&self) -> PathBuf {
        self.root.join("microcopy")
    }
    pub fn skins_dir(&self) -> PathBuf {
        self.root.join("skins")
    }
    pub fn themes_dir(&self) -> PathBuf {
        self.root.join("themes")
    }
    pub fn logs_dir(&self) -> PathBuf {
        self.root.join("logs")
    }
    pub fn backups_dir(&self) -> PathBuf {
        self.root.join("backups")
    }

    /// Creates the folder layout. Safe to call on every start.
    pub fn ensure(&self) -> io::Result<()> {
        for dir in [
            self.root.clone(),
            self.capabilities_dir(),
            self.capability_config_dir(),
            self.microcopy_dir(),
            self.skins_dir(),
            self.themes_dir(),
            self.logs_dir(),
            self.backups_dir(),
        ] {
            std::fs::create_dir_all(dir)?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ensure_creates_the_layout_and_is_repeatable() {
        let tmp = tempfile::tempdir().unwrap();
        let home = FrostyHome::new(tmp.path().join("frosty"));
        home.ensure().unwrap();
        home.ensure().unwrap();
        assert!(home.themes_dir().is_dir());
        assert!(home.backups_dir().is_dir());
        assert_eq!(
            home.config_file(),
            tmp.path().join("frosty").join("frosty.toml")
        );
    }
}
