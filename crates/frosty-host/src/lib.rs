//! Frosty capability host: discovery, manifest validation, lifecycle,
//! permission checks, and the namespaced data store
//! (docs/PROJECT_SPEC.md section 8). Pure Rust; the app wires it to windows.

pub mod capconfig;
pub mod discovery;
pub mod manifest;
pub mod permissions;
pub mod registry;
pub mod schema;
pub mod store;
