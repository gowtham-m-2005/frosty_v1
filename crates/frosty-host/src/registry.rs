//! Capability lifecycle state (docs/PROJECT_SPEC.md 8.3). A failure at any
//! step puts only that capability in `Error`; nothing else is affected.

use std::collections::BTreeMap;

use serde::Serialize;

use crate::{
    discovery::{Discovery, Rejected},
    manifest::LoadedCapability,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum State {
    Disabled,
    /// Granted and migrated; waiting for the logic host to confirm activation.
    Enabling,
    Enabled,
    Error,
}

#[derive(Debug, Clone)]
pub struct Entry {
    pub loaded: LoadedCapability,
    pub state: State,
    pub error: Option<String>,
    pub error_count: u32,
}

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum TransitionError {
    #[error("no capability `{0}`")]
    Unknown(String),
    #[error("capability `{id}` cannot go from {from:?} to {to:?}")]
    Invalid { id: String, from: State, to: State },
}

#[derive(Debug, Default)]
pub struct Registry {
    entries: BTreeMap<String, Entry>,
    rejected: Vec<Rejected>,
}

impl Registry {
    pub fn from_discovery(d: Discovery) -> Self {
        let entries = d
            .loaded
            .into_iter()
            .map(|loaded| {
                (
                    loaded.id().to_string(),
                    Entry {
                        loaded,
                        state: State::Disabled,
                        error: None,
                        error_count: 0,
                    },
                )
            })
            .collect();
        Self {
            entries,
            rejected: d.rejected,
        }
    }

    /// Replaces the set of known capabilities after a rescan, keeping the
    /// state of ones that are still present.
    pub fn refresh(&mut self, d: Discovery) {
        let mut next = Self::from_discovery(d);
        for (id, entry) in &mut next.entries {
            if let Some(old) = self.entries.get(id) {
                entry.state = old.state;
                entry.error = old.error.clone();
                entry.error_count = old.error_count;
            }
        }
        *self = next;
    }

    pub fn get(&self, id: &str) -> Option<&Entry> {
        self.entries.get(id)
    }

    pub fn iter(&self) -> impl Iterator<Item = &Entry> {
        self.entries.values()
    }

    pub fn rejected(&self) -> &[Rejected] {
        &self.rejected
    }

    pub fn state(&self, id: &str) -> Option<State> {
        self.entries.get(id).map(|e| e.state)
    }

    /// The enabled capability that declares `tool`, if any.
    pub fn tool_owner(&self, tool: &str) -> Option<&Entry> {
        self.entries.values().find(|e| {
            e.state == State::Enabled && e.loaded.manifest.tools.iter().any(|t| t.id == tool)
        })
    }

    fn transition(
        &mut self,
        id: &str,
        to: State,
        allowed: &[State],
    ) -> Result<(), TransitionError> {
        let entry = self
            .entries
            .get_mut(id)
            .ok_or_else(|| TransitionError::Unknown(id.to_string()))?;
        if !allowed.contains(&entry.state) {
            return Err(TransitionError::Invalid {
                id: id.to_string(),
                from: entry.state,
                to,
            });
        }
        entry.state = to;
        if to != State::Error {
            entry.error = None;
        }
        Ok(())
    }

    pub fn begin_enable(&mut self, id: &str) -> Result<(), TransitionError> {
        // Error is allowed so a fixed capability can be retried.
        self.transition(id, State::Enabling, &[State::Disabled, State::Error])
    }

    pub fn mark_enabled(&mut self, id: &str) -> Result<(), TransitionError> {
        self.transition(id, State::Enabled, &[State::Enabling])
    }

    pub fn mark_disabled(&mut self, id: &str) -> Result<(), TransitionError> {
        self.transition(
            id,
            State::Disabled,
            &[
                State::Enabled,
                State::Enabling,
                State::Error,
                State::Disabled,
            ],
        )
    }

    /// Any state can fail; the message and a running count are kept.
    pub fn mark_error(
        &mut self,
        id: &str,
        message: impl Into<String>,
    ) -> Result<(), TransitionError> {
        let entry = self
            .entries
            .get_mut(id)
            .ok_or_else(|| TransitionError::Unknown(id.to_string()))?;
        entry.state = State::Error;
        entry.error = Some(message.into());
        entry.error_count += 1;
        Ok(())
    }

    pub fn forget(&mut self, id: &str) {
        self.entries.remove(id);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{discovery::discover, manifest::test_support::write_good};

    fn registry() -> (tempfile::TempDir, Registry) {
        let tmp = tempfile::tempdir().unwrap();
        write_good(&tmp.path().join("notes"));
        let r = Registry::from_discovery(discover(None, tmp.path()));
        (tmp, r)
    }

    #[test]
    fn happy_path() {
        let (_tmp, mut r) = registry();
        assert_eq!(r.state("notes"), Some(State::Disabled));
        r.begin_enable("notes").unwrap();
        r.mark_enabled("notes").unwrap();
        assert!(r.tool_owner("notes.add").is_some());
        r.mark_disabled("notes").unwrap();
        assert!(r.tool_owner("notes.add").is_none());
    }

    #[test]
    fn cannot_skip_activation() {
        let (_tmp, mut r) = registry();
        assert!(matches!(
            r.mark_enabled("notes"),
            Err(TransitionError::Invalid { .. })
        ));
    }

    #[test]
    fn errors_are_counted_and_can_be_retried() {
        let (_tmp, mut r) = registry();
        r.begin_enable("notes").unwrap();
        r.mark_error("notes", "onActivate threw").unwrap();
        r.begin_enable("notes").unwrap();
        r.mark_error("notes", "again").unwrap();
        let e = r.get("notes").unwrap();
        assert_eq!(e.error_count, 2);
        assert_eq!(e.error.as_deref(), Some("again"));
        r.begin_enable("notes").unwrap();
        assert!(r.get("notes").unwrap().error.is_none());
    }

    #[test]
    fn unknown_ids_are_errors() {
        let (_tmp, mut r) = registry();
        assert_eq!(
            r.begin_enable("ghost"),
            Err(TransitionError::Unknown("ghost".into()))
        );
    }

    #[test]
    fn refresh_keeps_state_of_survivors() {
        let (tmp, mut r) = registry();
        r.begin_enable("notes").unwrap();
        r.mark_enabled("notes").unwrap();
        r.refresh(discover(None, tmp.path()));
        assert_eq!(r.state("notes"), Some(State::Enabled));
    }
}
