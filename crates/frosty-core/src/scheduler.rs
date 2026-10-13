//! Persistent job scheduler (docs/PROJECT_SPEC.md 7.4).
//!
//! Jobs live in the `scheduler_jobs` table so they survive restarts. This
//! module only stores and selects jobs; the app drives it with a timer and
//! turns due jobs into events or tool calls.
//!
//! Missed jobs: a job that is more than a minute late (Frosty was closed or
//! the computer slept) follows its policy. `fire_once_on_start` fires once
//! and a recurring job moves to its next future slot, `skip` drops the missed
//! run silently, and `fire_all` replays the missed runs of a recurring job, up
//! to a cap so a long absence cannot flood the screen.

use rusqlite::{OptionalExtension, params};
use serde::{Deserialize, Serialize};

use crate::db::{Database, DbError};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MissedPolicy {
    FireOnceOnStart,
    Skip,
    FireAll,
}

impl MissedPolicy {
    fn as_str(self) -> &'static str {
        match self {
            Self::FireOnceOnStart => "fire_once_on_start",
            Self::Skip => "skip",
            Self::FireAll => "fire_all",
        }
    }

    fn parse(s: &str) -> Self {
        match s {
            "skip" => Self::Skip,
            "fire_all" => Self::FireAll,
            _ => Self::FireOnceOnStart,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Job {
    pub id: String,
    pub owner: String,
    /// Unix time in milliseconds.
    pub fire_at: i64,
    /// Present for recurring jobs.
    pub interval_ms: Option<i64>,
    /// `{"event": "tasks.due", "payload": {...}}` or `{"tool": "tasks.add", "input": {...}}`.
    pub action: serde_json::Value,
    pub missed_policy: MissedPolicy,
}

fn now_millis() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_millis() as i64)
}

pub fn schedule_at(
    db: &Database,
    owner: &str,
    at_ms: i64,
    action: &serde_json::Value,
    policy: MissedPolicy,
) -> Result<String, DbError> {
    insert(db, owner, at_ms, None, action, policy)
}

/// First run is one interval from now unless `first_at_ms` is given.
pub fn schedule_every(
    db: &Database,
    owner: &str,
    interval_ms: i64,
    first_at_ms: Option<i64>,
    action: &serde_json::Value,
    policy: MissedPolicy,
) -> Result<String, DbError> {
    let interval_ms = interval_ms.max(1_000);
    let first = first_at_ms.unwrap_or_else(|| now_millis() + interval_ms);
    insert(db, owner, first, Some(interval_ms), action, policy)
}

fn insert(
    db: &Database,
    owner: &str,
    fire_at: i64,
    interval_ms: Option<i64>,
    action: &serde_json::Value,
    policy: MissedPolicy,
) -> Result<String, DbError> {
    let id = uuid::Uuid::now_v7().to_string();
    db.connection().execute(
        "INSERT INTO scheduler_jobs (id, owner, kind, fire_at, interval_ms, action, missed_policy, created_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
        params![
            id,
            owner,
            if interval_ms.is_some() { "every" } else { "at" },
            fire_at,
            interval_ms,
            action.to_string(),
            policy.as_str(),
            now_millis(),
        ],
    )?;
    Ok(id)
}

/// Cancels one job, but only if it belongs to `owner`.
pub fn cancel(db: &Database, owner: &str, job_id: &str) -> Result<bool, DbError> {
    let n = db.connection().execute(
        "DELETE FROM scheduler_jobs WHERE id = ?1 AND owner = ?2",
        params![job_id, owner],
    )?;
    Ok(n > 0)
}

/// Used when a capability is disabled.
pub fn cancel_owner(db: &Database, owner: &str) -> Result<usize, DbError> {
    Ok(db
        .connection()
        .execute("DELETE FROM scheduler_jobs WHERE owner = ?1", [owner])?)
}

/// Jobs whose time has come, oldest first.
pub fn due(db: &Database, now_ms: i64) -> Result<Vec<Job>, DbError> {
    let mut stmt = db.connection().prepare(
        "SELECT id, owner, fire_at, interval_ms, action, missed_policy
         FROM scheduler_jobs WHERE fire_at <= ?1 ORDER BY fire_at",
    )?;
    let rows = stmt.query_map([now_ms], |r| {
        let action: String = r.get(4)?;
        let policy: String = r.get(5)?;
        Ok(Job {
            id: r.get(0)?,
            owner: r.get(1)?,
            fire_at: r.get(2)?,
            interval_ms: r.get(3)?,
            action: serde_json::from_str(&action).unwrap_or(serde_json::Value::Null),
            missed_policy: MissedPolicy::parse(&policy),
        })
    })?;
    Ok(rows.collect::<Result<_, _>>()?)
}

/// Call after firing a job: one-shot jobs are removed, recurring jobs move to
/// their next slot after `now_ms` (never replaying a backlog).
pub fn after_fire(db: &Database, job: &Job, now_ms: i64) -> Result<(), DbError> {
    match job.interval_ms {
        None => {
            db.connection()
                .execute("DELETE FROM scheduler_jobs WHERE id = ?1", [&job.id])?;
        }
        Some(interval) => {
            let behind = (now_ms - job.fire_at).max(0);
            let next = job.fire_at + (behind / interval + 1) * interval;
            db.connection().execute(
                "UPDATE scheduler_jobs SET fire_at = ?1 WHERE id = ?2",
                params![next, job.id],
            )?;
        }
    }
    Ok(())
}

/// A job later than this was missed rather than merely waiting for the tick.
pub const LATE_AFTER_MS: i64 = 60_000;
/// Most runs a `fire_all` job replays after an absence.
pub const FIRE_ALL_CAP: i64 = 10;

/// One thing to run now, as decided by the job's missed-run policy.
#[derive(Debug, Clone, PartialEq)]
pub struct Fire {
    pub job: Job,
    /// The run was missed and is being caught up.
    pub late: bool,
}

/// Collects what is due, applies each job's policy, and advances or removes
/// the jobs, so the caller only has to run the returned actions. Jobs whose
/// owner is not ready (a capability still starting) are left untouched.
pub fn take_due(
    db: &Database,
    now_ms: i64,
    owner_ready: impl Fn(&str) -> bool,
) -> Result<Vec<Fire>, DbError> {
    let mut out = Vec::new();
    for job in due(db, now_ms)? {
        if !owner_ready(&job.owner) {
            continue;
        }
        let behind = now_ms - job.fire_at;
        let late = behind > LATE_AFTER_MS;
        let runs = match (late, job.missed_policy) {
            (false, _) => 1,
            (true, MissedPolicy::Skip) => 0,
            (true, MissedPolicy::FireOnceOnStart) => 1,
            (true, MissedPolicy::FireAll) => match job.interval_ms {
                Some(interval) => (behind / interval + 1).min(FIRE_ALL_CAP),
                None => 1,
            },
        };
        for _ in 0..runs {
            out.push(Fire {
                job: job.clone(),
                late,
            });
        }
        after_fire(db, &job, now_ms)?;
    }
    Ok(out)
}

pub fn count_for_owner(db: &Database, owner: &str) -> Result<i64, DbError> {
    Ok(db
        .connection()
        .query_row(
            "SELECT COUNT(*) FROM scheduler_jobs WHERE owner = ?1",
            [owner],
            |r| r.get(0),
        )
        .optional()?
        .unwrap_or(0))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn db() -> Database {
        Database::open_in_memory().unwrap()
    }

    #[test]
    fn one_shot_fires_once_then_disappears() {
        let db = db();
        let action = json!({ "event": "tasks.due", "payload": { "title": "x" } });
        schedule_at(&db, "tasks", 1_000, &action, MissedPolicy::FireOnceOnStart).unwrap();
        assert!(due(&db, 999).unwrap().is_empty());
        let jobs = due(&db, 1_000).unwrap();
        assert_eq!(jobs.len(), 1);
        assert_eq!(jobs[0].action["event"], "tasks.due");
        after_fire(&db, &jobs[0], 1_000).unwrap();
        assert!(due(&db, 10_000).unwrap().is_empty());
    }

    #[test]
    fn recurring_jobs_skip_a_backlog_and_keep_the_grid() {
        let db = db();
        schedule_every(
            &db,
            "g",
            10_000,
            Some(0),
            &json!({"event": "g.tick"}),
            MissedPolicy::FireOnceOnStart,
        )
        .unwrap();
        // Frosty was closed for 95 seconds: one catch-up fire, not ten.
        let jobs = due(&db, 95_000).unwrap();
        assert_eq!(jobs.len(), 1);
        after_fire(&db, &jobs[0], 95_000).unwrap();
        assert!(due(&db, 99_999).unwrap().is_empty());
        let next = due(&db, 100_000).unwrap();
        assert_eq!(next.len(), 1);
        assert_eq!(next[0].fire_at, 100_000);
    }

    #[test]
    fn cancel_is_scoped_to_the_owner() {
        let db = db();
        let id = schedule_at(&db, "a", 1, &json!({}), MissedPolicy::Skip).unwrap();
        assert!(!cancel(&db, "b", &id).unwrap());
        assert!(cancel(&db, "a", &id).unwrap());
    }

    #[test]
    fn cancel_owner_removes_only_that_owners_jobs() {
        let db = db();
        schedule_at(&db, "a", 1, &json!({}), MissedPolicy::Skip).unwrap();
        schedule_at(&db, "a", 2, &json!({}), MissedPolicy::Skip).unwrap();
        schedule_at(&db, "b", 3, &json!({}), MissedPolicy::Skip).unwrap();
        assert_eq!(cancel_owner(&db, "a").unwrap(), 2);
        assert_eq!(count_for_owner(&db, "b").unwrap(), 1);
    }

    #[test]
    fn jobs_survive_reopening_the_database() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("frosty.db");
        let backups = tmp.path().join("backups");
        {
            let db = Database::open(&path, &backups).unwrap();
            schedule_at(
                &db,
                "tasks",
                5_000,
                &json!({"event": "tasks.due"}),
                MissedPolicy::FireOnceOnStart,
            )
            .unwrap();
        }
        let db = Database::open(&path, &backups).unwrap();
        assert_eq!(due(&db, 6_000).unwrap().len(), 1);
    }

    fn fires(db: &Database, now: i64) -> usize {
        take_due(db, now, |_| true).unwrap().len()
    }

    #[test]
    fn on_time_jobs_always_fire() {
        for policy in [
            MissedPolicy::FireOnceOnStart,
            MissedPolicy::Skip,
            MissedPolicy::FireAll,
        ] {
            let db = db();
            schedule_at(&db, "a", 10_000, &json!({"event": "a.x"}), policy).unwrap();
            assert_eq!(fires(&db, 10_500), 1);
            assert_eq!(
                fires(&db, 20_000),
                0,
                "a one-shot job is gone after it fires"
            );
        }
    }

    #[test]
    fn a_missed_one_shot_follows_its_policy() {
        let late = 10_000 + LATE_AFTER_MS + 1;
        let db = db();
        schedule_at(&db, "a", 10_000, &json!({}), MissedPolicy::FireOnceOnStart).unwrap();
        let fired = take_due(&db, late, |_| true).unwrap();
        assert_eq!(fired.len(), 1);
        assert!(fired[0].late);

        let db = db_with_skip();
        assert_eq!(fires(&db, late), 0);
        assert!(
            due(&db, late).unwrap().is_empty(),
            "a skipped one-shot is removed"
        );
    }

    fn db_with_skip() -> Database {
        let db = db();
        schedule_at(&db, "a", 10_000, &json!({}), MissedPolicy::Skip).unwrap();
        db
    }

    #[test]
    fn a_missed_recurring_job_follows_its_policy() {
        let interval = 10_000;
        let now = 95_000 + LATE_AFTER_MS; // 15 runs behind
        let make = |policy| {
            let db = db();
            schedule_every(
                &db,
                "g",
                interval,
                Some(0),
                &json!({"event": "g.tick"}),
                policy,
            )
            .unwrap();
            db
        };

        let once = make(MissedPolicy::FireOnceOnStart);
        assert_eq!(fires(&once, now), 1);

        let skip = make(MissedPolicy::Skip);
        assert_eq!(fires(&skip, now), 0);
        // Skipping still moves the job to its next future slot.
        assert!(due(&skip, now).unwrap().is_empty());
        assert_eq!(due(&skip, now + interval).unwrap().len(), 1);

        let all = make(MissedPolicy::FireAll);
        assert_eq!(fires(&all, now) as i64, FIRE_ALL_CAP);
    }

    #[test]
    fn jobs_of_a_capability_that_is_not_ready_wait() {
        let db = db();
        schedule_at(&db, "slow", 1_000, &json!({}), MissedPolicy::Skip).unwrap();
        assert!(
            take_due(&db, 2_000, |owner| owner != "slow")
                .unwrap()
                .is_empty()
        );
        // Still there, and fires once the owner is ready.
        assert_eq!(take_due(&db, 2_000, |_| true).unwrap().len(), 1);
    }

    #[test]
    fn fire_all_replays_fewer_runs_when_few_were_missed() {
        let db = db();
        schedule_every(&db, "g", 30_000, Some(0), &json!({}), MissedPolicy::FireAll).unwrap();
        // 100 s late with a 30 s interval: runs at 0, 30, 60, 90.
        assert_eq!(fires(&db, 100_000), 4);
    }
}
