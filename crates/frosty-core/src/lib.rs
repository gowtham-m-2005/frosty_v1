//! Frosty Core: the services that exist with zero capabilities installed.
//!
//! Everything here is plain Rust with no UI or Tauri dependency, so it can be
//! unit-tested on its own (docs/PROJECT_SPEC.md section 7).

pub mod config;
pub mod db;
pub mod events;
pub mod home;
pub mod personality;
pub mod scheduler;
pub mod theme;
