//! The background beat for a shared folder: write back shortly after the
//! last change, renew the lock, and notice when a foreign lock goes away.
//! `tick` does one beat; the thread in `lib.rs` calls it every second.

use std::time::{Duration, Instant};

use psybil_core::Error;
use psybil_core::dates::Clock;
use psybil_core::lockfile::LockState;
use psybil_core::merge::MergeReport;
use psybil_core::workspace::{Access, Location, SyncOutcome, Workspace};

use crate::state::{AppState, lock, user_name};

/// Quiet time after the last write before it is written back.
pub const SYNC_AFTER: Duration = Duration::from_secs(3);
pub const HEARTBEAT_EVERY: Duration = Duration::from_secs(60);
/// How often a read-only computer looks whether the lock became free.
pub const LOCK_CHECK_EVERY: Duration = Duration::from_secs(15);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Seen {
    Free,
    Held,
    Stale,
}

pub struct Ticker {
    last_heartbeat: Instant,
    last_lock_check: Instant,
    seen: Option<Seen>,
}

impl Ticker {
    pub fn new(now: Instant) -> Ticker {
        Ticker {
            last_heartbeat: now,
            last_lock_check: now,
            seen: None,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum Event {
    /// Something in `workspace_status` changed.
    Status,
    /// The shared file had changed meanwhile and was merged in.
    Merged(MergeReport),
}

/// One beat. Returns what the window should hear about.
pub fn tick(state: &AppState, ticker: &mut Ticker, now: Instant, clock: &dyn Clock) -> Vec<Event> {
    let mut events = Vec::new();
    let mut guard = lock(&state.workspace);
    let Some(ws) = guard.as_mut() else {
        return events;
    };
    if matches!(ws.location(), Location::Local { .. }) {
        *lock(&state.last_write) = None;
        return events;
    }
    if matches!(ws.access(), Access::ReadOnly { .. }) {
        if now.duration_since(ticker.last_lock_check) >= LOCK_CHECK_EVERY {
            ticker.last_lock_check = now;
            let seen = match ws.lock_state(clock) {
                Ok(Some(LockState::Free | LockState::Mine(_))) => Seen::Free,
                Ok(Some(LockState::Stale(_))) => Seen::Stale,
                _ => Seen::Held,
            };
            if ticker.seen != Some(seen) {
                // A lock that is free at the first look is news as well.
                if ticker.seen.is_some() || seen == Seen::Free {
                    events.push(Event::Status);
                }
                ticker.seen = Some(seen);
            }
        }
        return events;
    }
    let mut lost = false;
    let due = lock(&state.last_write).is_some_and(|t| now.duration_since(t) >= SYNC_AFTER);
    if due {
        match ws.sync(clock) {
            Ok(outcome) => {
                *lock(&state.last_write) = None;
                if lock(&state.flags).sync_error.take().is_some() {
                    events.push(Event::Status);
                }
                if let SyncOutcome::Merged(report) = outcome {
                    events.push(Event::Merged(report));
                }
            }
            Err(Error::LockLost) => lost = true,
            Err(e) => {
                let message = e.to_string();
                let mut flags = lock(&state.flags);
                if flags.sync_error.as_deref() != Some(message.as_str()) {
                    flags.sync_error = Some(message);
                    events.push(Event::Status);
                }
            }
        }
    }
    if !lost && now.duration_since(ticker.last_heartbeat) >= HEARTBEAT_EVERY {
        ticker.last_heartbeat = now;
        if let Err(Error::LockLost) = ws.heartbeat(clock) {
            lost = true;
        }
    }
    if lost {
        // The changes stay in the working copy, marked as not written back;
        // they are merged the next time this computer takes the lock.
        let location = ws.location().clone();
        *guard = None;
        *lock(&state.last_write) = None;
        match Workspace::open_or_read(location, state.device(), &user_name(), clock) {
            Ok(read_only) => *guard = Some(read_only),
            Err(e) => *lock(&state.problem) = Some(e.into()),
        }
        lock(&state.flags).lock_lost = true;
        ticker.seen = None;
        events.push(Event::Status);
    }
    events
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::tests::state_with;
    use crate::workspace_cmd::status;
    use chrono::Utc;
    use psybil_core::dates::SystemClock;
    use psybil_core::db::Db;
    use psybil_core::lockfile::{self, Device};
    use psybil_core::masterdata::{NewCustomer, create_customer, list_customers};
    use psybil_core::settings::DataPlace;
    use psybil_core::workspace::SHARED_DB;

    fn shared_dir() -> tempfile::TempDir {
        tempfile::tempdir().unwrap()
    }

    fn place(dir: &tempfile::TempDir) -> DataPlace {
        DataPlace::Shared {
            dir: dir.path().to_string_lossy().into_owned(),
        }
    }

    fn other() -> Device {
        Device {
            id: uuid::Uuid::new_v4(),
            name: "Mac".into(),
        }
    }

    fn add_customer(state: &AppState, name: &str) {
        state
            .write(|tx| {
                create_customer(
                    tx,
                    &SystemClock,
                    NewCustomer {
                        name: name.into(),
                        ..NewCustomer::default()
                    },
                )
            })
            .unwrap();
    }

    fn shared_names(dir: &tempfile::TempDir) -> Vec<String> {
        let path = dir.path().join(SHARED_DB);
        if !path.exists() {
            return Vec::new();
        }
        let db = Db::open_read_only(&path).unwrap();
        list_customers(db.read(), true)
            .unwrap()
            .into_iter()
            .map(|c| c.name)
            .collect()
    }

    #[test]
    fn tick_syncs_three_seconds_after_last_write() {
        let dir = shared_dir();
        let (state, _dirs) = state_with(place(&dir));
        let start = Instant::now();
        let mut ticker = Ticker::new(start);
        add_customer(&state, "Kunde A");
        let written = lock(&state.last_write).unwrap();
        assert!(
            tick(
                &state,
                &mut ticker,
                written + Duration::from_secs(2),
                &SystemClock
            )
            .is_empty()
        );
        assert!(shared_names(&dir).is_empty());
        tick(&state, &mut ticker, written + SYNC_AFTER, &SystemClock);
        assert_eq!(shared_names(&dir), ["Kunde A"]);
        assert!(lock(&state.last_write).is_none());
    }

    #[test]
    fn tick_reports_lock_lost_and_turns_read_only() {
        let dir = shared_dir();
        let (state, _dirs) = state_with(place(&dir));
        let mut ticker = Ticker::new(Instant::now());
        add_customer(&state, "Kunde A");
        lockfile::force_take_over(dir.path(), &other(), "band", Utc::now()).unwrap();
        let written = lock(&state.last_write).unwrap();
        let events = tick(&state, &mut ticker, written + SYNC_AFTER, &SystemClock);
        assert_eq!(events, [Event::Status]);
        let s = status(&state);
        assert_eq!(s.access, "read_only");
        assert!(s.lock_lost);
        assert_eq!(s.holder.unwrap().device_name, "Mac");
    }

    #[test]
    fn first_look_announces_a_free_lock() {
        let dir = shared_dir();
        let mac = other();
        lockfile::acquire(dir.path(), &mac, "band", Utc::now()).unwrap();
        let (state, _dirs) = state_with(place(&dir));
        lockfile::release(dir.path(), &mac).unwrap();
        let start = Instant::now();
        let mut ticker = Ticker::new(start);
        let events = tick(&state, &mut ticker, start + LOCK_CHECK_EVERY, &SystemClock);
        assert_eq!(events, [Event::Status]);
    }

    #[test]
    fn dirty_copy_is_written_back_after_start() {
        let dir = shared_dir();
        let (state, dirs) = state_with(place(&dir));
        add_customer(&state, "Kunde A");
        // A crash: the write is in the working copy, never written back.
        drop(state);
        lockfile::release(
            dir.path(),
            &Device {
                id: psybil_core::settings::DeviceSettings::load(
                    &dirs.1.path().join("settings.json"),
                )
                .device_id,
                name: "PC".into(),
            },
        )
        .unwrap();
        let state = AppState::open(dirs.0.path(), dirs.1.path(), false).unwrap();
        assert!(
            lock(&state.last_write).is_some(),
            "the start notes the pending write-back"
        );
        let start = Instant::now();
        let mut ticker = Ticker::new(start);
        tick(&state, &mut ticker, start + SYNC_AFTER, &SystemClock);
        assert_eq!(shared_names(&dir), ["Kunde A"]);
    }

    #[test]
    fn tick_notices_free_lock_while_read_only() {
        let dir = shared_dir();
        let mac = other();
        lockfile::acquire(dir.path(), &mac, "band", Utc::now()).unwrap();
        let (state, _dirs) = state_with(place(&dir));
        assert_eq!(status(&state).access, "read_only");
        let start = Instant::now();
        let mut ticker = Ticker::new(start);
        assert!(tick(&state, &mut ticker, start + LOCK_CHECK_EVERY, &SystemClock).is_empty());
        lockfile::release(dir.path(), &mac).unwrap();
        let events = tick(
            &state,
            &mut ticker,
            start + LOCK_CHECK_EVERY * 2,
            &SystemClock,
        );
        assert_eq!(events, [Event::Status]);
        assert!(status(&state).lock_free);
    }
}
