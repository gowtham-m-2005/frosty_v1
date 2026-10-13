//! Contract tests for the capability host, using only its public API: the
//! path a real capability takes from discovery to disabling, and the ways a
//! broken capability must fail without touching anything else.

use std::{fs, path::Path};

use frosty_core::db::Database;
use frosty_host::{
    discovery::discover,
    manifest::Origin,
    permissions,
    registry::{Registry, State},
    store::{CapStore, Query},
};
use serde_json::json;

fn manifest(id: &str, extra: &str) -> String {
    format!(
        r#"
[capability]
id = "{id}"
name = "{id}"
version = "0.1.0"
api_version = "1"
entry = "dist/index.js"

[permissions]
storage = true

[storage]
migrations = "migrations/"

{extra}
"#
    )
}

fn good_migration(id: &str) -> String {
    format!(
        "CREATE TABLE cap_{id}_items (id TEXT PRIMARY KEY, created_at INTEGER, updated_at INTEGER, \
         deleted_at INTEGER, device_id TEXT, title TEXT);"
    )
}

fn write_capability(root: &Path, id: &str, manifest_text: &str, migration: &str) {
    let dir = root.join(id);
    fs::create_dir_all(dir.join("dist")).unwrap();
    fs::create_dir_all(dir.join("migrations")).unwrap();
    fs::write(dir.join("capability.toml"), manifest_text).unwrap();
    fs::write(dir.join("dist/index.js"), "export default {};").unwrap();
    fs::write(dir.join("migrations/0001_init.sql"), migration).unwrap();
}

#[test]
fn a_capability_goes_from_discovery_to_data_and_back_to_disabled() {
    let tmp = tempfile::tempdir().unwrap();
    write_capability(
        tmp.path(),
        "notes",
        &manifest("notes", ""),
        &good_migration("notes"),
    );

    let discovery = discover(None, tmp.path());
    assert_eq!(discovery.loaded.len(), 1);
    let mut registry = Registry::from_discovery(discovery);

    let entry = registry.get("notes").unwrap();
    let lines = permissions::describe(&entry.loaded.manifest.permissions);
    assert_eq!(lines[0].text, "Read and write its own data");
    let migrations = entry.loaded.migrations.clone();

    let mut db = Database::open_in_memory().unwrap();
    db.run_capability_migrations("notes", &migrations).unwrap();
    registry.begin_enable("notes").unwrap();
    registry.mark_enabled("notes").unwrap();
    assert_eq!(registry.state("notes"), Some(State::Enabled));

    let store = CapStore::new(db.connection(), "notes", "dev");
    let row = store
        .insert("items", json!({"title": "milk"}).as_object().unwrap())
        .unwrap();
    assert_eq!(row["title"], "milk");

    // Disabling keeps the data.
    registry.mark_disabled("notes").unwrap();
    let rows = store.query("items", &Query::default()).unwrap();
    assert_eq!(rows.len(), 1);
}

#[test]
fn a_broken_manifest_is_rejected_and_the_rest_still_load() {
    let tmp = tempfile::tempdir().unwrap();
    write_capability(
        tmp.path(),
        "good",
        &manifest("good", ""),
        &good_migration("good"),
    );
    write_capability(
        tmp.path(),
        "broken",
        &manifest("broken", "").replace("api_version = \"1\"", "api_version = \"7\""),
        &good_migration("broken"),
    );
    let discovery = discover(None, tmp.path());
    assert_eq!(discovery.loaded.len(), 1);
    assert_eq!(discovery.rejected.len(), 1);
    assert!(
        discovery.rejected[0]
            .errors
            .join(" ")
            .contains("api_version")
    );
}

#[test]
fn a_failing_migration_errors_only_that_capability() {
    let tmp = tempfile::tempdir().unwrap();
    write_capability(
        tmp.path(),
        "good",
        &manifest("good", ""),
        &good_migration("good"),
    );
    write_capability(tmp.path(), "bad", &manifest("bad", ""), "THIS IS NOT SQL;");

    let mut registry = Registry::from_discovery(discover(None, tmp.path()));
    let mut db = Database::open_in_memory().unwrap();

    for id in ["bad", "good"] {
        let migrations = registry.get(id).unwrap().loaded.migrations.clone();
        match db.run_capability_migrations(id, &migrations) {
            Ok(_) => {
                registry.begin_enable(id).unwrap();
                registry.mark_enabled(id).unwrap();
            }
            Err(e) => registry.mark_error(id, e.to_string()).unwrap(),
        }
    }

    assert_eq!(registry.state("bad"), Some(State::Error));
    assert!(
        registry
            .get("bad")
            .unwrap()
            .error
            .as_deref()
            .unwrap()
            .contains("migration")
    );
    assert_eq!(registry.state("good"), Some(State::Enabled));
    // The good one's table exists and works.
    CapStore::new(db.connection(), "good", "d")
        .insert("items", json!({"title": "x"}).as_object().unwrap())
        .unwrap();
}

#[test]
fn a_migration_reaching_outside_its_namespace_is_refused() {
    let tmp = tempfile::tempdir().unwrap();
    write_capability(
        tmp.path(),
        "greedy",
        &manifest("greedy", ""),
        "CREATE TABLE someone_elses (id TEXT);",
    );
    let registry = Registry::from_discovery(discover(None, tmp.path()));
    let mut db = Database::open_in_memory().unwrap();
    let migrations = registry.get("greedy").unwrap().loaded.migrations.clone();
    assert!(db.run_capability_migrations("greedy", &migrations).is_err());
}

#[test]
fn an_unbuilt_capability_says_so() {
    let tmp = tempfile::tempdir().unwrap();
    write_capability(
        tmp.path(),
        "raw",
        &manifest("raw", ""),
        &good_migration("raw"),
    );
    fs::remove_file(tmp.path().join("raw/dist/index.js")).unwrap();
    let d = discover(None, tmp.path());
    assert!(d.rejected[0].errors.join(" ").contains("been built"));
}

/// Every built-in capability in this repo must load. Build them first with
/// `pnpm build:capabilities`; an unbuilt one fails here with a clear message.
#[test]
fn built_in_capabilities_in_this_repo_are_valid() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../capabilities");
    let d = discover(
        Some(&root),
        &std::env::temp_dir().join("frosty-no-such-user-root"),
    );
    assert!(
        !d.loaded.is_empty(),
        "no built-in capabilities found in {}",
        root.display()
    );
    assert!(
        d.rejected.is_empty(),
        "built-in capabilities failed to load: {:#?}",
        d.rejected
    );
    assert!(d.loaded.iter().all(|c| c.origin == Origin::BuiltIn));
    assert!(d.loaded.iter().any(|c| c.id() == "hello"));
}
