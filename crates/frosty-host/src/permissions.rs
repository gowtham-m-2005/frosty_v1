//! Permissions: what a manifest asks for, what the user granted, and the
//! plain-language lines for the permission sheet
//! (docs/PROJECT_SPEC.md 8.6, docs/DESIGN_SPEC.md 4.3).

use serde::Serialize;

use crate::manifest::Permissions;

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct PermissionLine {
    pub key: String,
    pub text: String,
    /// Shown with extra emphasis and, for `shell`, a second confirmation.
    pub sensitive: bool,
}

/// Reads a `[capabilities.permissions.<id>]` table from `frosty.toml`.
/// Anything missing or malformed counts as not granted.
pub fn from_table(table: Option<&toml::Table>) -> Permissions {
    table
        .and_then(|t| toml::Value::Table(t.clone()).try_into::<Permissions>().ok())
        .unwrap_or_default()
}

/// What the capability asks for, as sentences a person can judge.
pub fn describe(p: &Permissions) -> Vec<PermissionLine> {
    let mut lines = Vec::new();
    let mut add = |key: &str, text: String, sensitive: bool| {
        lines.push(PermissionLine {
            key: key.to_string(),
            text,
            sensitive,
        });
    };
    if p.storage {
        add("storage", "Read and write its own data".into(), false);
    }
    if p.scheduler {
        add(
            "scheduler",
            "Run things on a schedule while Frosty is open".into(),
            false,
        );
    }
    if p.notifications {
        add("notifications", "Send reminders".into(), false);
    }
    if p.clipboard {
        add("clipboard", "Access the clipboard".into(), false);
    }
    for host in &p.network {
        add(
            &format!("network:{host}"),
            format!("Connect to {host}"),
            false,
        );
    }
    for path in &p.fs {
        add(
            &format!("fs:{path}"),
            format!("Read and write files in {path}"),
            false,
        );
    }
    for ns in &p.events_subscribe {
        add(
            &format!("events_subscribe:{ns}"),
            format!("Listen to {ns} events from other modules"),
            false,
        );
    }
    for tool in &p.tools_call {
        add(
            &format!("tools_call:{tool}"),
            format!("Use the {tool} action of another module"),
            false,
        );
    }
    if p.shell {
        add(
            "shell",
            "Run programs on your computer, with the same access you have".into(),
            true,
        );
    }
    lines
}

/// True when `granted` includes everything in `requested`.
pub fn covers(granted: &Permissions, requested: &Permissions) -> bool {
    let all = |want: &[String], have: &[String]| want.iter().all(|w| have.contains(w));
    (!requested.storage || granted.storage)
        && (!requested.scheduler || granted.scheduler)
        && (!requested.notifications || granted.notifications)
        && (!requested.clipboard || granted.clipboard)
        && (!requested.shell || granted.shell)
        && all(&requested.network, &granted.network)
        && all(&requested.fs, &granted.fs)
        && all(&requested.events_subscribe, &granted.events_subscribe)
        && all(&requested.tools_call, &granted.tools_call)
}

/// Whether `namespace` is one this capability may listen to: its own, or one
/// it was granted. `*` grants everything.
pub fn may_listen_to(granted: &Permissions, own_id: &str, event_name: &str) -> bool {
    let ns = event_name.split('.').next().unwrap_or_default();
    ns == own_id
        || ns == "core"
        || granted
            .events_subscribe
            .iter()
            .any(|g| g == "*" || g == ns || g.strip_suffix(".*") == Some(ns))
}

pub fn may_call_tool(granted: &Permissions, own_id: &str, tool: &str) -> bool {
    tool.split('.').next() == Some(own_id) || granted.tools_call.iter().any(|t| t == tool)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn perms() -> Permissions {
        Permissions {
            storage: true,
            notifications: true,
            network: vec!["api.example.com".into()],
            ..Default::default()
        }
    }

    #[test]
    fn descriptions_are_plain_language() {
        let lines = describe(&perms());
        let texts: Vec<_> = lines.iter().map(|l| l.text.as_str()).collect();
        assert_eq!(
            texts,
            [
                "Read and write its own data",
                "Send reminders",
                "Connect to api.example.com"
            ]
        );
        assert!(lines.iter().all(|l| !l.sensitive));
    }

    #[test]
    fn grants_read_back_from_config_tables() {
        let table: toml::Table = toml::from_str(
            "storage = true
network = [\"a.example\"]
",
        )
        .unwrap();
        let p = from_table(Some(&table));
        assert!(p.storage && !p.shell);
        assert_eq!(p.network, vec!["a.example"]);
        assert_eq!(from_table(None), Permissions::default());
    }

    #[test]
    fn shell_is_flagged_sensitive() {
        let lines = describe(&Permissions {
            shell: true,
            ..Default::default()
        });
        assert!(lines[0].sensitive);
    }

    #[test]
    fn covers_requires_every_request() {
        let requested = perms();
        assert!(covers(&requested, &requested));
        assert!(!covers(&Permissions::default(), &requested));
        let partial = Permissions {
            storage: true,
            notifications: true,
            ..Default::default()
        };
        assert!(!covers(&partial, &requested));
    }

    #[test]
    fn listening_rules() {
        let granted = Permissions {
            events_subscribe: vec!["tasks".into()],
            ..Default::default()
        };
        assert!(may_listen_to(&granted, "guitar", "guitar.logged"));
        assert!(may_listen_to(&granted, "guitar", "core.summoned"));
        assert!(may_listen_to(&granted, "guitar", "tasks.completed"));
        assert!(!may_listen_to(&granted, "guitar", "notes.created"));
    }

    #[test]
    fn calling_rules() {
        let granted = Permissions {
            tools_call: vec!["tasks.add".into()],
            ..Default::default()
        };
        assert!(may_call_tool(&granted, "guitar", "guitar.log"));
        assert!(may_call_tool(&granted, "guitar", "tasks.add"));
        assert!(!may_call_tool(&granted, "guitar", "tasks.delete"));
    }
}
