//! In-process event bus (docs/PROJECT_SPEC.md section 7.3 and 11.1).

use std::{
    collections::VecDeque,
    sync::{Arc, Mutex},
    time::{SystemTime, UNIX_EPOCH},
};

use serde::{Deserialize, Serialize};

const DEFAULT_RECENT_CAPACITY: usize = 256;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Event {
    pub id: String,
    pub name: String,
    pub source: String,
    /// Unix time in milliseconds.
    pub at: u64,
    pub payload: serde_json::Value,
    /// Optional 0 to 1 hint that lets the personality engine scale reactions.
    pub significance: Option<f32>,
}

impl Event {
    /// The source is taken from the part of the name before the first dot.
    pub fn new(name: &str, payload: serde_json::Value) -> Self {
        Self {
            id: uuid::Uuid::now_v7().to_string(),
            name: name.to_string(),
            source: name.split('.').next().unwrap_or_default().to_string(),
            at: SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .map_or(0, |d| d.as_millis() as u64),
            payload,
            significance: None,
        }
    }

    pub fn with_significance(mut self, significance: f32) -> Self {
        self.significance = Some(significance.clamp(0.0, 1.0));
        self
    }
}

/// Event names are `<namespace>.<event>`, lowercase letters, digits and `_`.
pub fn is_valid_event_name(name: &str) -> bool {
    let mut parts = name.split('.');
    let valid = |p: &str| {
        !p.is_empty()
            && p.chars()
                .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_')
    };
    match (parts.next(), parts.next(), parts.next()) {
        (Some(a), Some(b), None) => valid(a) && valid(b),
        _ => false,
    }
}

/// `*` matches everything, `ns.*` matches one namespace, `*.suffix` matches an
/// event name suffix across namespaces, anything else must match exactly.
pub fn pattern_matches(pattern: &str, name: &str) -> bool {
    if pattern == "*" {
        return true;
    }
    if let Some(prefix) = pattern.strip_suffix(".*") {
        return name.split('.').next() == Some(prefix);
    }
    if let Some(suffix) = pattern.strip_prefix("*.") {
        return name.split_once('.').is_some_and(|(_, rest)| rest == suffix);
    }
    pattern == name
}

type Handler = Arc<dyn Fn(&Event) + Send + Sync>;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SubscriptionId(u64);

struct Inner {
    subs: Vec<(u64, String, Handler)>,
    next_id: u64,
    recent: VecDeque<Event>,
    capacity: usize,
}

pub struct EventBus {
    inner: Mutex<Inner>,
}

impl Default for EventBus {
    fn default() -> Self {
        Self::with_capacity(DEFAULT_RECENT_CAPACITY)
    }
}

impl EventBus {
    pub fn with_capacity(capacity: usize) -> Self {
        Self {
            inner: Mutex::new(Inner {
                subs: Vec::new(),
                next_id: 0,
                recent: VecDeque::new(),
                capacity,
            }),
        }
    }

    pub fn subscribe(
        &self,
        pattern: &str,
        handler: impl Fn(&Event) + Send + Sync + 'static,
    ) -> SubscriptionId {
        let mut inner = self.inner.lock().unwrap();
        let id = inner.next_id;
        inner.next_id += 1;
        inner
            .subs
            .push((id, pattern.to_string(), Arc::new(handler)));
        SubscriptionId(id)
    }

    pub fn unsubscribe(&self, id: SubscriptionId) {
        self.inner
            .lock()
            .unwrap()
            .subs
            .retain(|(i, _, _)| *i != id.0);
    }

    /// Records the event and calls matching handlers outside the lock, so a
    /// handler may publish or subscribe without deadlocking.
    pub fn publish(&self, event: Event) {
        let handlers: Vec<Handler> = {
            let mut inner = self.inner.lock().unwrap();
            inner.recent.push_back(event.clone());
            while inner.recent.len() > inner.capacity {
                inner.recent.pop_front();
            }
            inner
                .subs
                .iter()
                .filter(|(_, pattern, _)| pattern_matches(pattern, &event.name))
                .map(|(_, _, h)| h.clone())
                .collect()
        };
        for handler in handlers {
            handler(&event);
        }
    }

    pub fn recent(&self) -> Vec<Event> {
        self.inner.lock().unwrap().recent.iter().cloned().collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};

    #[test]
    fn name_validation() {
        assert!(is_valid_event_name("tasks.completed"));
        assert!(is_valid_event_name("core.fullscreen_entered"));
        assert!(!is_valid_event_name("tasks"));
        assert!(!is_valid_event_name("Tasks.Completed"));
        assert!(!is_valid_event_name("a.b.c"));
        assert!(!is_valid_event_name(".x"));
    }

    #[test]
    fn pattern_matching() {
        assert!(pattern_matches("*", "tasks.completed"));
        assert!(pattern_matches("tasks.*", "tasks.completed"));
        assert!(!pattern_matches("tasks.*", "notes.completed"));
        assert!(pattern_matches("*.completed", "guitar.completed"));
        assert!(pattern_matches("tasks.completed", "tasks.completed"));
        assert!(!pattern_matches("tasks.created", "tasks.completed"));
    }

    #[test]
    fn delivers_only_to_matching_subscribers_and_unsubscribes() {
        let bus = EventBus::default();
        let hits = Arc::new(AtomicUsize::new(0));
        let h = hits.clone();
        let id = bus.subscribe("tasks.*", move |_| {
            h.fetch_add(1, Ordering::SeqCst);
        });
        bus.publish(Event::new("tasks.created", serde_json::json!({})));
        bus.publish(Event::new("notes.created", serde_json::json!({})));
        assert_eq!(hits.load(Ordering::SeqCst), 1);
        bus.unsubscribe(id);
        bus.publish(Event::new("tasks.created", serde_json::json!({})));
        assert_eq!(hits.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn recent_log_is_bounded() {
        let bus = EventBus::with_capacity(3);
        for i in 0..5 {
            bus.publish(Event::new("core.tick", serde_json::json!({ "i": i })));
        }
        let recent = bus.recent();
        assert_eq!(recent.len(), 3);
        assert_eq!(recent[0].payload["i"], 2);
    }

    #[test]
    fn handler_may_publish_without_deadlock() {
        let bus = Arc::new(EventBus::default());
        let inner = bus.clone();
        bus.subscribe("a.start", move |_| {
            inner.publish(Event::new("a.follow_up", serde_json::json!({})));
        });
        bus.publish(Event::new("a.start", serde_json::json!({})));
        assert_eq!(bus.recent().len(), 2);
    }

    #[test]
    fn source_and_significance() {
        let e = Event::new("tasks.completed", serde_json::json!({})).with_significance(3.0);
        assert_eq!(e.source, "tasks");
        assert_eq!(e.significance, Some(1.0));
    }
}
