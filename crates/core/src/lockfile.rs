//! The lock file in a shared data folder: only one computer writes at a time.
//! The holder renews a heartbeat every minute; after five minutes without
//! one the lock counts as abandoned.

use std::path::Path;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::merge::io_error;
use crate::{Error, Result};

pub const LOCK_FILE: &str = "psybil.lock";

/// Seconds without a heartbeat after which a lock counts as abandoned.
pub const LOCK_STALE_AFTER_SECS: i64 = 300;

/// This computer.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Device {
    pub id: Uuid,
    pub name: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(test, derive(ts_rs::TS), ts(export))]
pub struct LockInfo {
    pub device_id: Uuid,
    pub device_name: String,
    pub user: String,
    pub started: DateTime<Utc>,
    pub heartbeat: DateTime<Utc>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LockState {
    Free,
    Mine(LockInfo),
    Held(LockInfo),
    Stale(LockInfo),
}

fn unreadable(now: DateTime<Utc>) -> LockInfo {
    LockInfo {
        device_id: Uuid::nil(),
        device_name: "unbekannt".into(),
        user: String::new(),
        started: now,
        heartbeat: DateTime::<Utc>::MIN_UTC,
    }
}

fn read_raw(dir: &Path) -> Result<Option<std::result::Result<LockInfo, ()>>> {
    let path = dir.join(LOCK_FILE);
    match std::fs::read_to_string(&path) {
        Ok(text) => Ok(Some(serde_json::from_str(&text).map_err(|_| ()))),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(io_error(e)),
    }
}

/// Who holds the lock of `dir` at `now`, seen from `me`.
pub fn read_lock(dir: &Path, me: &Device, now: DateTime<Utc>) -> Result<LockState> {
    Ok(match read_raw(dir)? {
        None => LockState::Free,
        Some(Err(())) => LockState::Stale(unreadable(now)),
        Some(Ok(info)) if info.device_id == me.id => LockState::Mine(info),
        Some(Ok(info)) => {
            if (now - info.heartbeat).num_seconds() > LOCK_STALE_AFTER_SECS {
                LockState::Stale(info)
            } else {
                LockState::Held(info)
            }
        }
    })
}

fn write(dir: &Path, me: &Device, info: &LockInfo) -> Result<()> {
    use std::io::Write;
    let tmp = dir.join(format!("{LOCK_FILE}.tmp-{}", me.id));
    let text = serde_json::to_string_pretty(info)?;
    std::fs::File::create(&tmp)
        .and_then(|mut f| {
            f.write_all(text.as_bytes())?;
            f.sync_all()
        })
        .map_err(io_error)?;
    std::fs::rename(&tmp, dir.join(LOCK_FILE)).map_err(io_error)
}

fn fresh(me: &Device, user: &str, now: DateTime<Utc>) -> LockInfo {
    LockInfo {
        device_id: me.id,
        device_name: me.name.clone(),
        user: user.into(),
        started: now,
        heartbeat: now,
    }
}

/// Takes a free lock (or renews one's own). A lock held by someone else is
/// refused; an abandoned one only goes through `take_over`.
pub fn acquire(dir: &Path, me: &Device, user: &str, now: DateTime<Utc>) -> Result<LockInfo> {
    match read_lock(dir, me, now)? {
        LockState::Held(info) => Err(Error::LockHeld(info)),
        LockState::Stale(info) => Err(Error::LockStale(info)),
        LockState::Free | LockState::Mine(_) => {
            let info = fresh(me, user, now);
            write(dir, me, &info)?;
            Ok(info)
        }
    }
}

/// Takes over an abandoned or unreadable lock. A fresh lock of someone else
/// is still refused.
pub fn take_over(dir: &Path, me: &Device, user: &str, now: DateTime<Utc>) -> Result<LockInfo> {
    match read_lock(dir, me, now)? {
        LockState::Held(info) => Err(Error::LockHeld(info)),
        _ => {
            let info = fresh(me, user, now);
            write(dir, me, &info)?;
            Ok(info)
        }
    }
}

/// Takes the lock whoever holds it ("take over anyway").
pub fn force_take_over(
    dir: &Path,
    me: &Device,
    user: &str,
    now: DateTime<Utc>,
) -> Result<LockInfo> {
    let info = fresh(me, user, now);
    write(dir, me, &info)?;
    Ok(info)
}

/// Renews the heartbeat of one's own lock.
pub fn heartbeat(dir: &Path, me: &Device, now: DateTime<Utc>) -> Result<()> {
    match read_lock(dir, me, now)? {
        LockState::Mine(mut info) => {
            info.heartbeat = now;
            write(dir, me, &info)
        }
        _ => Err(Error::LockLost),
    }
}

/// Removes the lock, but only one's own.
pub fn release(dir: &Path, me: &Device) -> Result<()> {
    if let Some(Ok(info)) = read_raw(dir)?
        && info.device_id == me.id
    {
        std::fs::remove_file(dir.join(LOCK_FILE)).map_err(io_error)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn t(secs: i64) -> DateTime<Utc> {
        DateTime::parse_from_rfc3339("2026-11-18T10:00:00Z")
            .unwrap()
            .with_timezone(&Utc)
            + chrono::Duration::seconds(secs)
    }

    fn device(name: &str) -> Device {
        Device {
            id: Uuid::new_v4(),
            name: name.into(),
        }
    }

    #[test]
    fn free_then_acquire_then_mine() {
        let dir = tempfile::tempdir().unwrap();
        let me = device("PC");
        assert_eq!(read_lock(dir.path(), &me, t(0)).unwrap(), LockState::Free);
        let info = acquire(dir.path(), &me, "nutzer", t(0)).unwrap();
        assert_eq!(info.device_name, "PC");
        assert!(matches!(
            read_lock(dir.path(), &me, t(10)).unwrap(),
            LockState::Mine(_)
        ));
    }

    #[test]
    fn other_fresh_lock_is_held() {
        let dir = tempfile::tempdir().unwrap();
        let (pc, mac) = (device("PC"), device("Mac"));
        acquire(dir.path(), &pc, "nutzer", t(0)).unwrap();
        assert!(matches!(
            read_lock(dir.path(), &mac, t(240)).unwrap(),
            LockState::Held(_)
        ));
        assert!(matches!(
            acquire(dir.path(), &mac, "nutzer", t(240)),
            Err(Error::LockHeld(_))
        ));
        assert!(matches!(
            take_over(dir.path(), &mac, "nutzer", t(240)),
            Err(Error::LockHeld(_))
        ));
    }

    #[test]
    fn lock_older_than_five_minutes_is_stale() {
        let dir = tempfile::tempdir().unwrap();
        let (pc, mac) = (device("PC"), device("Mac"));
        acquire(dir.path(), &pc, "nutzer", t(0)).unwrap();
        assert!(matches!(
            read_lock(dir.path(), &mac, t(301)).unwrap(),
            LockState::Stale(_)
        ));
        assert!(matches!(
            acquire(dir.path(), &mac, "nutzer", t(301)),
            Err(Error::LockStale(_))
        ));
    }

    #[test]
    fn take_over_replaces_stale() {
        let dir = tempfile::tempdir().unwrap();
        let (pc, mac) = (device("PC"), device("Mac"));
        acquire(dir.path(), &pc, "nutzer", t(0)).unwrap();
        take_over(dir.path(), &mac, "nutzer", t(400)).unwrap();
        assert!(matches!(
            read_lock(dir.path(), &mac, t(401)).unwrap(),
            LockState::Mine(_)
        ));
        assert!(matches!(
            read_lock(dir.path(), &pc, t(401)).unwrap(),
            LockState::Held(_)
        ));
    }

    #[test]
    fn heartbeat_keeps_fresh() {
        let dir = tempfile::tempdir().unwrap();
        let (pc, mac) = (device("PC"), device("Mac"));
        acquire(dir.path(), &pc, "nutzer", t(0)).unwrap();
        heartbeat(dir.path(), &pc, t(280)).unwrap();
        assert!(matches!(
            read_lock(dir.path(), &mac, t(500)).unwrap(),
            LockState::Held(_)
        ));
        assert!(matches!(
            heartbeat(dir.path(), &mac, t(500)),
            Err(Error::LockLost)
        ));
    }

    #[test]
    fn release_only_own_lock() {
        let dir = tempfile::tempdir().unwrap();
        let (pc, mac) = (device("PC"), device("Mac"));
        acquire(dir.path(), &pc, "nutzer", t(0)).unwrap();
        release(dir.path(), &mac).unwrap();
        assert!(matches!(
            read_lock(dir.path(), &mac, t(1)).unwrap(),
            LockState::Held(_)
        ));
        release(dir.path(), &pc).unwrap();
        assert_eq!(read_lock(dir.path(), &mac, t(2)).unwrap(), LockState::Free);
    }

    #[test]
    fn garbage_lock_file_is_stale() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join(LOCK_FILE), "kein json").unwrap();
        let me = device("PC");
        assert!(matches!(
            read_lock(dir.path(), &me, t(0)).unwrap(),
            LockState::Stale(_)
        ));
        take_over(dir.path(), &me, "nutzer", t(0)).unwrap();
        assert!(matches!(
            read_lock(dir.path(), &me, t(0)).unwrap(),
            LockState::Mine(_)
        ));
    }
}
