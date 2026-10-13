//! Notification center rules and history (docs/PROJECT_SPEC.md 7.5).
//!
//! Capabilities ask Frosty to notify; the center decides whether to show it
//! now, hold it for later, or drop it, and keeps every notification in a
//! history. The pure rules live here; the app supplies the clock, the
//! fullscreen state, and the window that actually shows the nudge.

use std::collections::{HashMap, VecDeque};

use rusqlite::{OptionalExtension, params};
use serde::Serialize;
use serde_json::Value;

use crate::db::{Database, DbError};

/// At most this many notifications per capability per window.
pub const RATE_LIMIT: usize = 5;
pub const RATE_WINDOW_MS: i64 = 60_000;
/// Most notifications held back before the oldest are dropped.
pub const MAX_HELD: usize = 200;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum HoldReason {
    DoNotDisturb,
    FullscreenApp,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum DropReason {
    Muted,
    RateLimited,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Delivery {
    Deliver,
    Hold(HoldReason),
    Drop(DropReason),
}

#[derive(Debug, Clone, Copy, Default)]
pub struct Conditions {
    pub dnd_active: bool,
    pub fullscreen_app: bool,
    pub muted: bool,
    pub rate_limited: bool,
}

/// Muting and rate limits drop a notification. Do-not-disturb and a
/// fullscreen app only hold it, so it is not lost (docs/DESIGN_SPEC.md 4.5).
pub fn decide(c: &Conditions) -> Delivery {
    if c.muted {
        Delivery::Drop(DropReason::Muted)
    } else if c.rate_limited {
        Delivery::Drop(DropReason::RateLimited)
    } else if c.dnd_active {
        Delivery::Hold(HoldReason::DoNotDisturb)
    } else if c.fullscreen_app {
        Delivery::Hold(HoldReason::FullscreenApp)
    } else {
        Delivery::Deliver
    }
}

/// `"23:00"` to minutes after midnight.
pub fn parse_hhmm(text: &str) -> Option<u32> {
    let (h, m) = text.trim().split_once(':')?;
    let (h, m): (u32, u32) = (h.parse().ok()?, m.parse().ok()?);
    (h < 24 && m < 60).then_some(h * 60 + m)
}

/// Whether `minutes_of_day` falls in the do-not-disturb window. The window
/// may cross midnight (23:00 to 07:00). Equal start and end means no window.
pub fn in_dnd_window(start: &str, end: &str, minutes_of_day: u32) -> bool {
    let (Some(start), Some(end)) = (parse_hhmm(start), parse_hhmm(end)) else {
        return false;
    };
    match start.cmp(&end) {
        std::cmp::Ordering::Equal => false,
        std::cmp::Ordering::Less => (start..end).contains(&minutes_of_day),
        std::cmp::Ordering::Greater => minutes_of_day >= start || minutes_of_day < end,
    }
}

/// Sliding-window limiter, one window per capability.
pub struct RateLimiter {
    limit: usize,
    window_ms: i64,
    recent: HashMap<String, VecDeque<i64>>,
}

impl Default for RateLimiter {
    fn default() -> Self {
        Self::new(RATE_LIMIT, RATE_WINDOW_MS)
    }
}

impl RateLimiter {
    pub fn new(limit: usize, window_ms: i64) -> Self {
        Self {
            limit,
            window_ms,
            recent: HashMap::new(),
        }
    }

    /// Records the attempt and returns whether it is within the limit.
    pub fn allow(&mut self, owner: &str, now_ms: i64) -> bool {
        let times = self.recent.entry(owner.to_string()).or_default();
        while times.front().is_some_and(|t| now_ms - t >= self.window_ms) {
            times.pop_front();
        }
        if times.len() >= self.limit {
            return false;
        }
        times.push_back(now_ms);
        true
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct Notification {
    pub id: String,
    pub owner: String,
    pub title: String,
    pub body: String,
    /// `[{ "label": "Done", "tool": "tasks.complete", "input": {...} }]`
    pub actions: Value,
    pub created_at: i64,
    /// `held`, `delivered`, `dismissed`, `dropped_muted`, or `dropped_rate_limited`.
    pub state: String,
    pub hold_reason: Option<String>,
    pub delivered_at: Option<i64>,
    pub read_at: Option<i64>,
    pub dismissed_at: Option<i64>,
}

const COLUMNS: &str = "id, owner, title, body, actions, created_at, state, hold_reason, delivered_at, read_at, dismissed_at";

fn from_row(r: &rusqlite::Row<'_>) -> rusqlite::Result<Notification> {
    let actions: Option<String> = r.get(4)?;
    Ok(Notification {
        id: r.get(0)?,
        owner: r.get(1)?,
        title: r.get(2)?,
        body: r.get(3)?,
        actions: actions
            .and_then(|a| serde_json::from_str(&a).ok())
            .unwrap_or(Value::Array(Vec::new())),
        created_at: r.get(5)?,
        state: r.get(6)?,
        hold_reason: r.get(7)?,
        delivered_at: r.get(8)?,
        read_at: r.get(9)?,
        dismissed_at: r.get(10)?,
    })
}

/// Stores a new notification as `held`; the caller then applies `decide`.
pub fn insert(
    db: &Database,
    owner: &str,
    title: &str,
    body: &str,
    actions: &Value,
    now_ms: i64,
) -> Result<String, DbError> {
    let id = uuid::Uuid::now_v7().to_string();
    db.connection().execute(
        "INSERT INTO notifications (id, owner, title, body, actions, created_at, state)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, 'held')",
        params![id, owner, title, body, actions.to_string(), now_ms],
    )?;
    Ok(id)
}

pub fn get(db: &Database, id: &str) -> Result<Option<Notification>, DbError> {
    Ok(db
        .connection()
        .query_row(
            &format!("SELECT {COLUMNS} FROM notifications WHERE id = ?1"),
            [id],
            from_row,
        )
        .optional()?)
}

/// Notifications waiting to be shown, oldest first.
pub fn held(db: &Database) -> Result<Vec<Notification>, DbError> {
    let mut stmt = db.connection().prepare(&format!(
        "SELECT {COLUMNS} FROM notifications WHERE state = 'held' ORDER BY created_at"
    ))?;
    let rows = stmt.query_map([], from_row)?;
    Ok(rows.collect::<Result<_, _>>()?)
}

pub fn set_hold_reason(db: &Database, id: &str, reason: HoldReason) -> Result<(), DbError> {
    let text = match reason {
        HoldReason::DoNotDisturb => "do_not_disturb",
        HoldReason::FullscreenApp => "fullscreen_app",
    };
    db.connection().execute(
        "UPDATE notifications SET hold_reason = ?1 WHERE id = ?2 AND state = 'held'",
        params![text, id],
    )?;
    Ok(())
}

pub fn mark_delivered(db: &Database, id: &str, now_ms: i64) -> Result<(), DbError> {
    db.connection().execute(
        "UPDATE notifications SET state = 'delivered', delivered_at = ?1, hold_reason = NULL
         WHERE id = ?2 AND state = 'held'",
        params![now_ms, id],
    )?;
    Ok(())
}

pub fn mark_dropped(
    db: &Database,
    id: &str,
    reason: DropReason,
    now_ms: i64,
) -> Result<(), DbError> {
    let state = match reason {
        DropReason::Muted => "dropped_muted",
        DropReason::RateLimited => "dropped_rate_limited",
    };
    db.connection().execute(
        "UPDATE notifications SET state = ?1, dismissed_at = ?2 WHERE id = ?3",
        params![state, now_ms, id],
    )?;
    Ok(())
}

pub fn mark_dismissed(db: &Database, id: &str, now_ms: i64) -> Result<(), DbError> {
    db.connection().execute(
        "UPDATE notifications SET state = 'dismissed', dismissed_at = ?1
         WHERE id = ?2 AND state IN ('held', 'delivered')",
        params![now_ms, id],
    )?;
    Ok(())
}

pub fn mark_all_read(db: &Database, now_ms: i64) -> Result<usize, DbError> {
    Ok(db.connection().execute(
        "UPDATE notifications SET read_at = ?1 WHERE read_at IS NULL",
        [now_ms],
    )?)
}

pub fn unread_count(db: &Database) -> Result<i64, DbError> {
    Ok(db.connection().query_row(
        "SELECT COUNT(*) FROM notifications WHERE read_at IS NULL AND state IN ('held', 'delivered', 'dismissed')",
        [],
        |r| r.get(0),
    )?)
}

/// Newest first.
pub fn list(db: &Database, limit: u32) -> Result<Vec<Notification>, DbError> {
    let mut stmt = db.connection().prepare(&format!(
        "SELECT {COLUMNS} FROM notifications ORDER BY created_at DESC LIMIT ?1"
    ))?;
    let rows = stmt.query_map([limit], from_row)?;
    Ok(rows.collect::<Result<_, _>>()?)
}

/// Keeps the held queue bounded by dropping the oldest as rate limited.
pub fn trim_held(db: &Database, now_ms: i64) -> Result<usize, DbError> {
    let held = held(db)?;
    let excess = held.len().saturating_sub(MAX_HELD);
    for n in held.iter().take(excess) {
        mark_dropped(db, &n.id, DropReason::RateLimited, now_ms)?;
    }
    Ok(excess)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn dnd_window_crossing_midnight() {
        let m = |h: u32, min: u32| h * 60 + min;
        assert!(in_dnd_window("23:00", "07:00", m(23, 30)));
        assert!(in_dnd_window("23:00", "07:00", m(2, 0)));
        assert!(in_dnd_window("23:00", "07:00", m(6, 59)));
        assert!(!in_dnd_window("23:00", "07:00", m(7, 0)));
        assert!(!in_dnd_window("23:00", "07:00", m(12, 0)));
        assert!(!in_dnd_window("23:00", "07:00", m(22, 59)));
    }

    #[test]
    fn dnd_window_within_a_day_and_degenerate_cases() {
        let m = |h: u32| h * 60;
        assert!(in_dnd_window("13:00", "14:00", m(13)));
        assert!(!in_dnd_window("13:00", "14:00", m(14)));
        assert!(!in_dnd_window("08:00", "08:00", m(8)));
        assert!(!in_dnd_window("nonsense", "07:00", m(3)));
        assert!(!in_dnd_window("25:00", "07:00", m(3)));
    }

    #[test]
    fn decision_order() {
        let base = Conditions::default();
        assert_eq!(decide(&base), Delivery::Deliver);
        assert_eq!(
            decide(&Conditions {
                dnd_active: true,
                ..base
            }),
            Delivery::Hold(HoldReason::DoNotDisturb)
        );
        assert_eq!(
            decide(&Conditions {
                fullscreen_app: true,
                ..base
            }),
            Delivery::Hold(HoldReason::FullscreenApp)
        );
        // Do not disturb is reported ahead of a fullscreen app.
        assert_eq!(
            decide(&Conditions {
                dnd_active: true,
                fullscreen_app: true,
                ..base
            }),
            Delivery::Hold(HoldReason::DoNotDisturb)
        );
        // A mute beats everything, including a hold.
        assert_eq!(
            decide(&Conditions {
                muted: true,
                dnd_active: true,
                ..base
            }),
            Delivery::Drop(DropReason::Muted)
        );
        assert_eq!(
            decide(&Conditions {
                rate_limited: true,
                ..base
            }),
            Delivery::Drop(DropReason::RateLimited)
        );
    }

    #[test]
    fn rate_limiter_is_per_owner_and_slides() {
        let mut r = RateLimiter::new(2, 1_000);
        assert!(r.allow("a", 0));
        assert!(r.allow("a", 100));
        assert!(!r.allow("a", 200));
        assert!(r.allow("b", 200));
        assert!(r.allow("a", 1_000));
    }

    #[test]
    fn held_notifications_wait_and_are_delivered_in_order() {
        let db = Database::open_in_memory().unwrap();
        let a = insert(&db, "tasks", "first", "", &json!([]), 100).unwrap();
        let b = insert(&db, "tasks", "second", "", &json!([]), 200).unwrap();
        set_hold_reason(&db, &a, HoldReason::FullscreenApp).unwrap();

        let waiting = held(&db).unwrap();
        assert_eq!(
            waiting.iter().map(|n| n.title.as_str()).collect::<Vec<_>>(),
            ["first", "second"]
        );
        assert_eq!(waiting[0].hold_reason.as_deref(), Some("fullscreen_app"));

        mark_delivered(&db, &a, 300).unwrap();
        assert_eq!(held(&db).unwrap().len(), 1);
        let delivered = get(&db, &a).unwrap().unwrap();
        assert_eq!(delivered.state, "delivered");
        assert_eq!(delivered.hold_reason, None);
        assert_eq!(delivered.delivered_at, Some(300));
        let _ = b;
    }

    #[test]
    fn actions_round_trip_and_history_is_newest_first() {
        let db = Database::open_in_memory().unwrap();
        let actions =
            json!([{ "label": "Done", "tool": "tasks.complete", "input": { "id": "1" } }]);
        insert(&db, "tasks", "old", "", &json!([]), 1).unwrap();
        let id = insert(&db, "tasks", "new", "body", &actions, 2).unwrap();
        let list = list(&db, 10).unwrap();
        assert_eq!(list[0].id, id);
        assert_eq!(list[0].actions[0]["tool"], "tasks.complete");
        assert_eq!(list[1].title, "old");
    }

    #[test]
    fn dropping_dismissing_and_reading() {
        let db = Database::open_in_memory().unwrap();
        let a = insert(&db, "tasks", "a", "", &json!([]), 1).unwrap();
        let b = insert(&db, "tasks", "b", "", &json!([]), 2).unwrap();
        mark_dropped(&db, &a, DropReason::Muted, 5).unwrap();
        assert_eq!(get(&db, &a).unwrap().unwrap().state, "dropped_muted");
        // A dropped notification cannot be "delivered" afterwards.
        mark_delivered(&db, &a, 6).unwrap();
        assert_eq!(get(&db, &a).unwrap().unwrap().state, "dropped_muted");

        mark_delivered(&db, &b, 7).unwrap();
        assert_eq!(unread_count(&db).unwrap(), 1);
        mark_dismissed(&db, &b, 8).unwrap();
        assert_eq!(get(&db, &b).unwrap().unwrap().state, "dismissed");
        assert_eq!(mark_all_read(&db, 9).unwrap(), 2);
        assert_eq!(unread_count(&db).unwrap(), 0);
    }

    #[test]
    fn the_held_queue_is_bounded() {
        let db = Database::open_in_memory().unwrap();
        for i in 0..(MAX_HELD as i64 + 3) {
            insert(&db, "t", &format!("n{i}"), "", &json!([]), i).unwrap();
        }
        assert_eq!(trim_held(&db, 9_999).unwrap(), 3);
        assert_eq!(held(&db).unwrap().len(), MAX_HELD);
        // The oldest were the ones dropped.
        assert_eq!(held(&db).unwrap()[0].title, "n3");
    }
}
