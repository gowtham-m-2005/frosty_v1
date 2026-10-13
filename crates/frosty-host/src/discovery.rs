//! Finds capabilities in the two roots (docs/PROJECT_SPEC.md 7.9 and 8.8):
//! the app bundle for built-ins and `<frosty-home>/capabilities` for users.

use std::{
    fs,
    path::{Path, PathBuf},
};

use crate::manifest::{LoadedCapability, Origin, load_capability};

#[derive(Debug, Clone)]
pub struct Rejected {
    pub dir: PathBuf,
    pub origin: Origin,
    pub errors: Vec<String>,
}

#[derive(Debug, Default)]
pub struct Discovery {
    pub loaded: Vec<LoadedCapability>,
    pub rejected: Vec<Rejected>,
}

fn scan(root: &Path, origin: Origin, out: &mut Discovery) {
    let Ok(entries) = fs::read_dir(root) else {
        return;
    };
    let mut dirs: Vec<PathBuf> = entries
        .filter_map(Result::ok)
        .map(|e| e.path())
        .filter(|p| p.join("capability.toml").is_file())
        .collect();
    dirs.sort();

    for dir in dirs {
        match load_capability(&dir, origin) {
            Err(errors) => out.rejected.push(Rejected {
                dir,
                origin,
                errors,
            }),
            Ok(cap) => {
                let id = cap.id().to_string();
                if let Some(existing) = out.loaded.iter().find(|c| c.id() == id) {
                    let message = if existing.origin == Origin::BuiltIn && origin == Origin::User {
                        format!("id `{id}` is already provided by a built-in capability")
                    } else {
                        format!("id `{id}` is already used by {}", existing.dir.display())
                    };
                    out.rejected.push(Rejected {
                        dir,
                        origin,
                        errors: vec![message],
                    });
                } else {
                    out.loaded.push(cap);
                }
            }
        }
    }
}

/// Built-ins are scanned first so they win an id clash.
pub fn discover(builtin_root: Option<&Path>, user_root: &Path) -> Discovery {
    let mut out = Discovery::default();
    if let Some(root) = builtin_root {
        scan(root, Origin::BuiltIn, &mut out);
    }
    scan(user_root, Origin::User, &mut out);
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::manifest::test_support::{GOOD, write_good};

    fn make(root: &Path, folder: &str, id: &str) {
        let dir = root.join(folder);
        write_good(&dir);
        let text = GOOD
            .replace(r#"id = "notes""#, &format!(r#"id = "{id}""#))
            .replace("notes.add", &format!("{id}.add"))
            .replace("notes.created", &format!("{id}.created"));
        fs::write(dir.join("capability.toml"), text).unwrap();
    }

    #[test]
    fn finds_capabilities_in_both_roots() {
        let tmp = tempfile::tempdir().unwrap();
        let (builtin, user) = (tmp.path().join("builtin"), tmp.path().join("user"));
        make(&builtin, "notes", "notes");
        make(&user, "guitar", "guitar");
        let d = discover(Some(&builtin), &user);
        assert_eq!(d.loaded.len(), 2);
        assert!(d.rejected.is_empty());
        assert_eq!(d.loaded[0].origin, Origin::BuiltIn);
        assert_eq!(d.loaded[1].origin, Origin::User);
    }

    #[test]
    fn user_capability_cannot_shadow_a_built_in() {
        let tmp = tempfile::tempdir().unwrap();
        let (builtin, user) = (tmp.path().join("builtin"), tmp.path().join("user"));
        make(&builtin, "notes", "notes");
        make(&user, "my-notes", "notes");
        let d = discover(Some(&builtin), &user);
        assert_eq!(d.loaded.len(), 1);
        assert_eq!(d.rejected.len(), 1);
        assert!(d.rejected[0].errors[0].contains("built-in"));
    }

    #[test]
    fn a_broken_capability_is_rejected_without_hiding_the_good_ones() {
        let tmp = tempfile::tempdir().unwrap();
        let user = tmp.path().join("user");
        make(&user, "good", "good");
        fs::create_dir_all(user.join("broken")).unwrap();
        fs::write(user.join("broken/capability.toml"), "not toml [").unwrap();
        let d = discover(None, &user);
        assert_eq!(d.loaded.len(), 1);
        assert_eq!(d.rejected.len(), 1);
    }

    #[test]
    fn missing_roots_are_fine() {
        let tmp = tempfile::tempdir().unwrap();
        let d = discover(Some(&tmp.path().join("nope")), &tmp.path().join("nada"));
        assert!(d.loaded.is_empty() && d.rejected.is_empty());
    }
}
