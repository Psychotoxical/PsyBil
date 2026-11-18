//! Where the data lives: a local file, or a shared folder (for example a
//! Nextcloud folder) used by several computers.
//!
//! With a shared folder PsyBil never writes the shared file directly. It
//! works on a copy in a local cache and writes complete states back with
//! `VACUUM INTO` a temporary file next to it plus an atomic rename, so the
//! sync client never sees a half-written file. A lock file makes sure only
//! one computer writes; a changed shared file is merged, never overwritten.

use std::path::{Path, PathBuf};

use rusqlite::Transaction;
use uuid::Uuid;

use crate::dates::Clock;
use crate::db::Db;
use crate::issue::sha256_hex;
use crate::lockfile::{self, Device, LockInfo, LockState};
use crate::merge::{MergeReport, io_error};
use crate::{Error, Result};

/// File name of the database in a shared folder.
pub const SHARED_DB: &str = "psybil.db";

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Location {
    /// One computer only.
    Local { file: PathBuf },
    /// A folder several computers use; `cache` is this computer's own folder.
    Shared { dir: PathBuf, cache: PathBuf },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Access {
    ReadWrite,
    /// Another computer holds the lock; this is a copy for reading.
    ReadOnly {
        holder: LockInfo,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SyncOutcome {
    Unchanged,
    Written,
    /// The shared file had changed meanwhile and was merged in first.
    Merged(MergeReport),
}

pub struct Workspace {
    db: Db,
    location: Location,
    device: Device,
    access: Access,
    base_hash: Option<String>,
    dirty: bool,
}

/// Cloud conflict copies of the shared database in `dir` (Nextcloud names
/// them "psybil (conflicted copy …).db" or "psybil_conflict-….db").
pub fn find_conflict_copies(dir: &Path) -> Result<Vec<PathBuf>> {
    let mut found = Vec::new();
    for entry in std::fs::read_dir(dir).map_err(io_error)? {
        let entry = entry.map_err(io_error)?;
        let name = entry.file_name().to_string_lossy().to_lowercase();
        let conflict = name.contains("conflicted copy") || name.contains("_conflict-");
        if conflict && name.starts_with("psybil") && name.ends_with(".db") {
            found.push(entry.path());
        }
    }
    found.sort();
    Ok(found)
}

fn file_hash(path: &Path) -> Result<Option<String>> {
    match std::fs::read(path) {
        Ok(bytes) => Ok(Some(sha256_hex(&bytes))),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(io_error(e)),
    }
}

fn remove_db_files(path: &Path) -> Result<()> {
    for suffix in ["", "-wal", "-shm"] {
        let file = PathBuf::from(format!("{}{suffix}", path.display()));
        match std::fs::remove_file(&file) {
            Ok(()) => {}
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(e) => return Err(io_error(e)),
        }
    }
    Ok(())
}

/// Marks in the cache that the working copy holds changes the shared file
/// does not have yet. It survives a crash, so the next start keeps them.
const DIRTY_MARK: &str = "work.dirty";
const WORK_DB: &str = "work.db";
const BASE_DB: &str = "base.db";

/// Forces a written file onto the disk before it is renamed into place.
fn flush_file(path: &Path) -> Result<()> {
    std::fs::OpenOptions::new()
        .read(true)
        .write(true)
        .open(path)
        .and_then(|f| f.sync_all())
        .map_err(io_error)
}

/// Refuses a shared file written by a newer PsyBil before anything is
/// changed in the folder.
fn check_schema(dir: &Path) -> Result<()> {
    let shared = dir.join(SHARED_DB);
    if !shared.exists() {
        return Ok(());
    }
    match Db::open_read_only(&shared) {
        Ok(_) | Err(Error::OlderSchema { .. }) => Ok(()),
        Err(e) => Err(e),
    }
}

/// Copies the shared file (if any) into the cache as `name`, and with
/// `with_base` also as the base of the working copy.
fn load_copy(
    dir: &Path,
    cache: &Path,
    name: &str,
    with_base: bool,
) -> Result<(Db, Option<String>)> {
    std::fs::create_dir_all(cache).map_err(io_error)?;
    let shared = dir.join(SHARED_DB);
    let work = cache.join(name);
    let base = cache.join(BASE_DB);
    remove_db_files(&work)?;
    if with_base {
        remove_db_files(&base)?;
    }
    let hash = file_hash(&shared)?;
    if hash.is_some() {
        std::fs::copy(&shared, &work).map_err(io_error)?;
        if with_base {
            std::fs::copy(&shared, &base).map_err(io_error)?;
        }
    }
    Ok((Db::open(&work)?, hash))
}

impl Workspace {
    /// Opens the data. In a shared folder held by another computer the data
    /// opens read-only; an abandoned lock gives `Error::LockStale`, which
    /// `take_over` resolves.
    pub fn open(
        location: Location,
        device: Device,
        user: &str,
        clock: &dyn Clock,
    ) -> Result<Workspace> {
        match &location {
            Location::Local { file } => Ok(Workspace {
                db: Db::open(file)?,
                location,
                device,
                access: Access::ReadWrite,
                base_hash: None,
                dirty: false,
            }),
            Location::Shared { dir, cache } => {
                match lockfile::read_lock(dir, &device, clock.now_utc())? {
                    LockState::Held(holder) => {
                        let (db, _) = load_copy(dir, cache, "readonly.db", false)?;
                        Ok(Workspace {
                            db,
                            location,
                            device,
                            access: Access::ReadOnly { holder },
                            base_hash: None,
                            dirty: false,
                        })
                    }
                    LockState::Stale(info) => Err(Error::LockStale(info)),
                    LockState::Free | LockState::Mine(_) => {
                        check_schema(dir)?;
                        lockfile::acquire(dir, &device, user, clock.now_utc())?;
                        Self::writable(location.clone(), device)
                    }
                }
            }
        }
    }

    /// Takes over an abandoned lock and opens the data for writing.
    pub fn take_over(
        location: Location,
        device: Device,
        user: &str,
        clock: &dyn Clock,
    ) -> Result<Workspace> {
        let Location::Shared { dir, .. } = &location else {
            return Self::open(location, device, user, clock);
        };
        check_schema(dir)?;
        lockfile::take_over(dir, &device, user, clock.now_utc())?;
        let mut ws = Self::writable(location.clone(), device)?;
        let name = ws.device.name.clone();
        ws.write(|tx| {
            crate::audit::log(
                tx,
                clock,
                "database",
                Uuid::nil(),
                "take_over_lock",
                serde_json::json!({ "device": name }),
            )
        })?;
        Ok(ws)
    }

    /// Opens the working copy. Changes a crash or a failed sync left in the
    /// cache are kept; the next sync merges them like any other.
    fn writable(location: Location, device: Device) -> Result<Workspace> {
        let Location::Shared { dir, cache } = &location else {
            unreachable!("writable is only used for shared folders");
        };
        let work = cache.join(WORK_DB);
        let (db, base_hash, dirty) = if cache.join(DIRTY_MARK).exists() && work.exists() {
            (Db::open(&work)?, file_hash(&cache.join(BASE_DB))?, true)
        } else {
            let (db, hash) = load_copy(dir, cache, WORK_DB, true)?;
            (db, hash, false)
        };
        Ok(Workspace {
            db,
            location,
            device,
            access: Access::ReadWrite,
            base_hash,
            dirty,
        })
    }

    /// Notes on disk that the working copy is ahead of the shared file,
    /// before the change is made.
    fn mark_dirty(&mut self) -> Result<()> {
        if let Location::Shared { cache, .. } = &self.location
            && !self.dirty
        {
            std::fs::write(cache.join(DIRTY_MARK), b"").map_err(io_error)?;
        }
        self.dirty = true;
        Ok(())
    }

    pub fn db(&self) -> &Db {
        &self.db
    }

    pub fn access(&self) -> &Access {
        &self.access
    }

    pub fn location(&self) -> &Location {
        &self.location
    }

    /// Runs a write; refused while the data is open read-only.
    pub fn write<T>(&mut self, f: impl FnOnce(&Transaction) -> Result<T>) -> Result<T> {
        if matches!(self.access, Access::ReadOnly { .. }) {
            return Err(Error::ReadOnly);
        }
        self.mark_dirty()?;
        self.db.write(f)
    }

    /// Renews the lock; call about once a minute while open.
    pub fn heartbeat(&self, clock: &dyn Clock) -> Result<()> {
        match (&self.location, &self.access) {
            (Location::Shared { dir, .. }, Access::ReadWrite) => {
                lockfile::heartbeat(dir, &self.device, clock.now_utc())
            }
            _ => Ok(()),
        }
    }

    /// Writes the current state to the shared folder, merging first when
    /// the shared file changed since it was loaded.
    pub fn sync(&mut self, clock: &dyn Clock) -> Result<SyncOutcome> {
        let (dir, cache) = match (&self.location, &self.access) {
            (Location::Shared { dir, cache }, Access::ReadWrite) => (dir.clone(), cache.clone()),
            _ => return Ok(SyncOutcome::Unchanged),
        };
        if !matches!(
            lockfile::read_lock(&dir, &self.device, clock.now_utc())?,
            LockState::Mine(_)
        ) {
            return Err(Error::LockLost);
        }
        let tmp = self.tmp_path(&dir);
        remove_db_files(&tmp)?;
        let shared = dir.join(SHARED_DB);
        let current = file_hash(&shared)?;
        if current == self.base_hash {
            if !self.dirty {
                return Ok(SyncOutcome::Unchanged);
            }
            self.write_back(&dir, &cache)?;
            return Ok(SyncOutcome::Written);
        }
        let remote = cache.join("remote.db");
        remove_db_files(&remote)?;
        std::fs::copy(&shared, &remote).map_err(io_error)?;
        let base = cache.join("base.db");
        let base = (self.base_hash.is_some() && base.exists()).then_some(base);
        let report = self.db.merge_from(clock, &remote, base.as_deref())?;
        self.write_back(&dir, &cache)?;
        remove_db_files(&remote)?;
        Ok(SyncOutcome::Merged(report))
    }

    fn tmp_path(&self, dir: &Path) -> PathBuf {
        dir.join(format!("{SHARED_DB}.tmp-{}", self.device.id))
    }

    /// Writes a complete copy to `file`: a temporary file next to it, forced
    /// onto the disk, then renamed into place.
    fn place_copy(&self, file: &Path) -> Result<()> {
        let tmp = PathBuf::from(format!("{}.tmp-{}", file.display(), self.device.id));
        remove_db_files(&tmp)?;
        self.db
            .read()
            .execute("VACUUM INTO ?1", [tmp.to_string_lossy().to_string()])?;
        flush_file(&tmp)?;
        std::fs::rename(&tmp, file).map_err(io_error)
    }

    fn write_back(&mut self, dir: &Path, cache: &Path) -> Result<()> {
        let shared = dir.join(SHARED_DB);
        self.place_copy(&shared)?;
        let base = cache.join(BASE_DB);
        remove_db_files(&base)?;
        std::fs::copy(&shared, &base).map_err(io_error)?;
        self.base_hash = file_hash(&shared)?;
        self.dirty = false;
        match std::fs::remove_file(cache.join(DIRTY_MARK)) {
            Ok(()) => Ok(()),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(e) => Err(io_error(e)),
        }
    }

    /// Merges a cloud conflict copy (no common base known, so differences
    /// are reported as unclear) and moves the copy into `backup_dir`.
    pub fn merge_conflict_copy(
        &mut self,
        path: &Path,
        clock: &dyn Clock,
        backup_dir: &Path,
    ) -> Result<MergeReport> {
        if matches!(self.access, Access::ReadOnly { .. }) {
            return Err(Error::ReadOnly);
        }
        self.mark_dirty()?;
        let report = self.db.merge_from(clock, path, None)?;
        std::fs::create_dir_all(backup_dir).map_err(io_error)?;
        let name = path
            .file_name()
            .ok_or_else(|| Error::Io("conflict copy has no file name".into()))?;
        let target = backup_dir.join(name);
        if std::fs::rename(path, &target).is_err() {
            std::fs::copy(path, &target).map_err(io_error)?;
            std::fs::remove_file(path).map_err(io_error)?;
        }
        Ok(report)
    }

    /// Moves the data to another place. The target must not hold a PsyBil
    /// database yet; on any error this workspace stays as it was.
    pub fn relocate(&mut self, target: Location, user: &str, clock: &dyn Clock) -> Result<()> {
        let (file, lock_dir) = match &target {
            Location::Local { file } => (file.clone(), None),
            Location::Shared { dir, .. } => {
                std::fs::create_dir_all(dir).map_err(io_error)?;
                (dir.join(SHARED_DB), Some(dir.clone()))
            }
        };
        if file.exists() {
            return Err(Error::TargetNotEmpty);
        }
        if let Some(dir) = &lock_dir {
            lockfile::acquire(dir, &self.device, user, clock.now_utc())?;
        }
        let placed = self
            .place_copy(&file)
            .and_then(|()| Workspace::open(target, self.device.clone(), user, clock));
        let opened = match placed {
            Ok(opened) => opened,
            Err(e) => {
                let _ = remove_db_files(&file);
                if let Some(dir) = &lock_dir {
                    let _ = lockfile::release(dir, &self.device);
                }
                return Err(e);
            }
        };
        let mut old = std::mem::replace(self, opened);
        old.close(clock)
    }

    /// Writes back and releases the lock. On an error the workspace stays
    /// usable, so closing can be tried again.
    pub fn close(&mut self, clock: &dyn Clock) -> Result<()> {
        if let (Location::Shared { dir, .. }, Access::ReadWrite) = (&self.location, &self.access) {
            let dir = dir.clone();
            self.sync(clock)?;
            lockfile::release(&dir, &self.device)?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod conflict_tests {
    use super::*;
    use crate::masterdata::{NewCustomer, create_customer, list_customers};
    use crate::testutil::{ManualClock, sample_customer};

    fn names(db: &Db) -> Vec<String> {
        let mut n: Vec<String> = list_customers(db.read(), true)
            .unwrap()
            .into_iter()
            .map(|c| c.name)
            .collect();
        n.sort();
        n
    }

    #[test]
    fn finds_nextcloud_conflict_names() {
        let dir = tempfile::tempdir().unwrap();
        for name in [
            "psybil.db",
            "psybil (conflicted copy 2026-11-18 101500).db",
            "PsyBil_conflict-20261118-101500.db",
            "anderes (conflicted copy 2026-11-18 101500).db",
            "psybil.lock",
        ] {
            std::fs::write(dir.path().join(name), b"x").unwrap();
        }
        let mut found: Vec<String> = find_conflict_copies(dir.path())
            .unwrap()
            .into_iter()
            .map(|p| p.file_name().unwrap().to_string_lossy().to_string())
            .collect();
        found.sort();
        assert_eq!(
            found,
            vec![
                "PsyBil_conflict-20261118-101500.db",
                "psybil (conflicted copy 2026-11-18 101500).db"
            ]
        );
    }

    #[test]
    fn merge_conflict_copy_moves_file_to_backup() {
        let root = tempfile::tempdir().unwrap();
        let dir = root.path().join("Nextcloud");
        std::fs::create_dir_all(&dir).unwrap();
        let clock = ManualClock::new();
        let device = Device {
            id: Uuid::new_v4(),
            name: "PC".into(),
        };
        let location = Location::Shared {
            dir: dir.clone(),
            cache: root.path().join("cache"),
        };
        let mut ws = Workspace::open(location, device, "frank", &clock).unwrap();
        ws.write(|tx| create_customer(tx, &clock, sample_customer()))
            .unwrap();
        ws.sync(&clock).unwrap();
        let conflict = dir.join("psybil (conflicted copy 2026-11-18 101500).db");
        std::fs::copy(dir.join(SHARED_DB), &conflict).unwrap();
        clock.advance_secs(30);
        {
            let mut other = Db::open(&conflict).unwrap();
            other
                .write(|tx| {
                    create_customer(
                        tx,
                        &clock,
                        NewCustomer {
                            name: "Kunde Konflikt".into(),
                            ..sample_customer()
                        },
                    )
                })
                .unwrap();
        }
        let backup = root.path().join("backup");
        ws.merge_conflict_copy(&conflict, &clock, &backup).unwrap();
        assert_eq!(names(ws.db()), vec!["Kunde A", "Kunde Konflikt"]);
        assert!(!conflict.exists());
        assert!(backup.join(conflict.file_name().unwrap()).exists());
    }

    #[test]
    fn relocate_local_to_shared() {
        let root = tempfile::tempdir().unwrap();
        let clock = ManualClock::new();
        let device = Device {
            id: Uuid::new_v4(),
            name: "PC".into(),
        };
        let local = Location::Local {
            file: root.path().join("local.psybil"),
        };
        let mut ws = Workspace::open(local, device, "frank", &clock).unwrap();
        ws.write(|tx| create_customer(tx, &clock, sample_customer()))
            .unwrap();
        let dir = root.path().join("Nextcloud");
        let target = Location::Shared {
            dir: dir.clone(),
            cache: root.path().join("cache"),
        };
        ws.relocate(target.clone(), "frank", &clock).unwrap();
        assert_eq!(ws.location(), &target);
        assert!(matches!(ws.access(), Access::ReadWrite));
        assert_eq!(names(ws.db()), vec!["Kunde A"]);
        assert!(dir.join(SHARED_DB).exists());
    }

    #[test]
    fn relocate_into_folder_with_stale_lock_leaves_no_file() {
        use crate::dates::Clock;
        let root = tempfile::tempdir().unwrap();
        let clock = ManualClock::new();
        let device = Device {
            id: Uuid::new_v4(),
            name: "PC".into(),
        };
        let local = Location::Local {
            file: root.path().join("local.psybil"),
        };
        let mut ws = Workspace::open(local.clone(), device, "frank", &clock).unwrap();
        ws.write(|tx| create_customer(tx, &clock, sample_customer()))
            .unwrap();
        let dir = root.path().join("Nextcloud");
        std::fs::create_dir_all(&dir).unwrap();
        let other = Device {
            id: Uuid::new_v4(),
            name: "Mac".into(),
        };
        lockfile::acquire(&dir, &other, "frank", clock.now_utc()).unwrap();
        clock.advance_secs(400);
        let target = Location::Shared {
            dir: dir.clone(),
            cache: root.path().join("cache"),
        };
        assert!(matches!(
            ws.relocate(target, "frank", &clock),
            Err(Error::LockStale(_))
        ));
        assert!(!dir.join(SHARED_DB).exists());
        assert_eq!(ws.location(), &local);
    }

    #[test]
    fn relocate_refuses_existing_db() {
        let root = tempfile::tempdir().unwrap();
        let clock = ManualClock::new();
        let device = Device {
            id: Uuid::new_v4(),
            name: "PC".into(),
        };
        let local = Location::Local {
            file: root.path().join("local.psybil"),
        };
        let mut ws = Workspace::open(local.clone(), device, "frank", &clock).unwrap();
        let dir = root.path().join("Nextcloud");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join(SHARED_DB), b"schon da").unwrap();
        let target = Location::Shared {
            dir,
            cache: root.path().join("cache"),
        };
        assert!(matches!(
            ws.relocate(target, "frank", &clock),
            Err(Error::TargetNotEmpty)
        ));
        assert_eq!(ws.location(), &local);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::masterdata::{NewCustomer, create_customer, list_customers};
    use crate::testutil::{ManualClock, sample_customer};

    struct Shared {
        _root: tempfile::TempDir,
        dir: PathBuf,
        cache_a: PathBuf,
        cache_b: PathBuf,
        clock: ManualClock,
        pc: Device,
        mac: Device,
    }

    fn shared() -> Shared {
        let root = tempfile::tempdir().unwrap();
        let dir = root.path().join("Nextcloud");
        let cache_a = root.path().join("cache-a");
        let cache_b = root.path().join("cache-b");
        std::fs::create_dir_all(&dir).unwrap();
        Shared {
            _root: root,
            dir,
            cache_a,
            cache_b,
            clock: ManualClock::new(),
            pc: Device {
                id: Uuid::new_v4(),
                name: "PC".into(),
            },
            mac: Device {
                id: Uuid::new_v4(),
                name: "Mac".into(),
            },
        }
    }

    fn loc(s: &Shared, cache: &Path) -> Location {
        Location::Shared {
            dir: s.dir.clone(),
            cache: cache.to_path_buf(),
        }
    }

    fn add_customer(ws: &mut Workspace, clock: &ManualClock, name: &str) {
        ws.write(|tx| {
            create_customer(
                tx,
                clock,
                NewCustomer {
                    name: name.into(),
                    ..sample_customer()
                },
            )
        })
        .unwrap();
    }

    fn names(db: &Db) -> Vec<String> {
        let mut n: Vec<String> = list_customers(db.read(), true)
            .unwrap()
            .into_iter()
            .map(|c| c.name)
            .collect();
        n.sort();
        n
    }

    fn shared_files(s: &Shared) -> Vec<String> {
        let mut files: Vec<String> = std::fs::read_dir(&s.dir)
            .unwrap()
            .map(|e| e.unwrap().file_name().to_string_lossy().to_string())
            .collect();
        files.sort();
        files
    }

    #[test]
    fn first_open_creates_shared_db_on_sync() {
        let s = shared();
        let mut a = Workspace::open(loc(&s, &s.cache_a), s.pc.clone(), "frank", &s.clock).unwrap();
        add_customer(&mut a, &s.clock, "Kunde A");
        assert!(matches!(a.sync(&s.clock).unwrap(), SyncOutcome::Written));
        let check = Db::open_read_only(&s.dir.join(SHARED_DB)).unwrap();
        assert_eq!(names(&check), vec!["Kunde A"]);
    }

    #[test]
    fn sync_writes_complete_file_atomically() {
        let s = shared();
        let mut a = Workspace::open(loc(&s, &s.cache_a), s.pc.clone(), "frank", &s.clock).unwrap();
        add_customer(&mut a, &s.clock, "Kunde A");
        a.sync(&s.clock).unwrap();
        assert_eq!(shared_files(&s), vec!["psybil.db", "psybil.lock"]);
        assert!(matches!(a.sync(&s.clock).unwrap(), SyncOutcome::Unchanged));
    }

    #[test]
    fn interrupted_write_leaves_old_file() {
        let s = shared();
        let mut a = Workspace::open(loc(&s, &s.cache_a), s.pc.clone(), "frank", &s.clock).unwrap();
        add_customer(&mut a, &s.clock, "Kunde A");
        a.sync(&s.clock).unwrap();
        let before = std::fs::read(s.dir.join(SHARED_DB)).unwrap();
        let tmp = s.dir.join(format!("{SHARED_DB}.tmp-{}", s.pc.id));
        std::fs::write(&tmp, b"halb geschrieben").unwrap();
        assert_eq!(std::fs::read(s.dir.join(SHARED_DB)).unwrap(), before);
        add_customer(&mut a, &s.clock, "Kunde B");
        a.sync(&s.clock).unwrap();
        assert!(!tmp.exists());
        assert_eq!(
            names(&Db::open_read_only(&s.dir.join(SHARED_DB)).unwrap()),
            vec!["Kunde A", "Kunde B"]
        );
    }

    #[test]
    fn second_device_opens_read_only_while_held() {
        let s = shared();
        let mut a = Workspace::open(loc(&s, &s.cache_a), s.pc.clone(), "frank", &s.clock).unwrap();
        add_customer(&mut a, &s.clock, "Kunde A");
        a.sync(&s.clock).unwrap();
        let b = Workspace::open(loc(&s, &s.cache_b), s.mac.clone(), "frank", &s.clock).unwrap();
        match b.access() {
            Access::ReadOnly { holder } => assert_eq!(holder.device_name, "PC"),
            other => panic!("expected read-only, got {other:?}"),
        }
        assert_eq!(names(b.db()), vec!["Kunde A"]);
    }

    #[test]
    fn read_only_refuses_writes() {
        let s = shared();
        let _a = Workspace::open(loc(&s, &s.cache_a), s.pc.clone(), "frank", &s.clock).unwrap();
        let mut b = Workspace::open(loc(&s, &s.cache_b), s.mac.clone(), "frank", &s.clock).unwrap();
        let result = b.write(|tx| create_customer(tx, &s.clock, sample_customer()));
        assert!(matches!(result, Err(Error::ReadOnly)));
    }

    #[test]
    fn stale_lock_needs_take_over() {
        let s = shared();
        let _a = Workspace::open(loc(&s, &s.cache_a), s.pc.clone(), "frank", &s.clock).unwrap();
        s.clock.advance_secs(400);
        let result = Workspace::open(loc(&s, &s.cache_b), s.mac.clone(), "frank", &s.clock);
        assert!(matches!(result, Err(Error::LockStale(_))));
        let b =
            Workspace::take_over(loc(&s, &s.cache_b), s.mac.clone(), "frank", &s.clock).unwrap();
        assert!(matches!(b.access(), Access::ReadWrite));
    }

    #[test]
    fn remote_change_is_merged_not_overwritten() {
        let s = shared();
        let mut a = Workspace::open(loc(&s, &s.cache_a), s.pc.clone(), "frank", &s.clock).unwrap();
        add_customer(&mut a, &s.clock, "Kunde A");
        a.sync(&s.clock).unwrap();
        s.clock.advance_secs(30);
        {
            // A late cloud sync brings in what another computer wrote.
            let mut other = Db::open(&s.dir.join(SHARED_DB)).unwrap();
            other
                .write(|tx| {
                    create_customer(
                        tx,
                        &s.clock,
                        NewCustomer {
                            name: "Kunde Fern".into(),
                            ..sample_customer()
                        },
                    )
                })
                .unwrap();
        }
        add_customer(&mut a, &s.clock, "Kunde B");
        assert!(matches!(a.sync(&s.clock).unwrap(), SyncOutcome::Merged(_)));
        assert_eq!(names(a.db()), vec!["Kunde A", "Kunde B", "Kunde Fern"]);
        let check = Db::open_read_only(&s.dir.join(SHARED_DB)).unwrap();
        assert_eq!(names(&check), vec!["Kunde A", "Kunde B", "Kunde Fern"]);
    }

    #[test]
    fn lost_lock_stops_sync() {
        let s = shared();
        let mut a = Workspace::open(loc(&s, &s.cache_a), s.pc.clone(), "frank", &s.clock).unwrap();
        s.clock.advance_secs(400);
        let _b =
            Workspace::take_over(loc(&s, &s.cache_b), s.mac.clone(), "frank", &s.clock).unwrap();
        add_customer(&mut a, &s.clock, "Kunde A");
        assert!(matches!(a.sync(&s.clock), Err(Error::LockLost)));
    }

    #[test]
    fn crash_before_sync_keeps_work() {
        let s = shared();
        {
            let mut a =
                Workspace::open(loc(&s, &s.cache_a), s.pc.clone(), "frank", &s.clock).unwrap();
            add_customer(&mut a, &s.clock, "Kunde A");
            // No sync, no close: the program ends here.
        }
        let mut a = Workspace::open(loc(&s, &s.cache_a), s.pc.clone(), "frank", &s.clock).unwrap();
        assert_eq!(names(a.db()), vec!["Kunde A"]);
        a.sync(&s.clock).unwrap();
        assert_eq!(
            names(&Db::open_read_only(&s.dir.join(SHARED_DB)).unwrap()),
            vec!["Kunde A"]
        );
    }

    #[test]
    fn unsynced_work_survives_read_only_phase_and_merges() {
        let s = shared();
        {
            let mut a =
                Workspace::open(loc(&s, &s.cache_a), s.pc.clone(), "frank", &s.clock).unwrap();
            add_customer(&mut a, &s.clock, "Kunde A");
            a.sync(&s.clock).unwrap();
            add_customer(&mut a, &s.clock, "Kunde PC");
        }
        s.clock.advance_secs(400);
        let mut b =
            Workspace::take_over(loc(&s, &s.cache_b), s.mac.clone(), "frank", &s.clock).unwrap();
        add_customer(&mut b, &s.clock, "Kunde Mac");
        b.sync(&s.clock).unwrap();
        {
            let ro = Workspace::open(loc(&s, &s.cache_a), s.pc.clone(), "frank", &s.clock).unwrap();
            assert!(matches!(ro.access(), Access::ReadOnly { .. }));
        }
        b.close(&s.clock).unwrap();
        let mut a = Workspace::open(loc(&s, &s.cache_a), s.pc.clone(), "frank", &s.clock).unwrap();
        assert!(matches!(a.sync(&s.clock).unwrap(), SyncOutcome::Merged(_)));
        assert_eq!(names(a.db()), vec!["Kunde A", "Kunde Mac", "Kunde PC"]);
    }

    #[test]
    fn newer_shared_file_is_refused_without_taking_the_lock() {
        let s = shared();
        {
            let conn = rusqlite::Connection::open(s.dir.join(SHARED_DB)).unwrap();
            conn.execute_batch("CREATE TABLE x (a); PRAGMA user_version = 99;")
                .unwrap();
        }
        let result = Workspace::open(loc(&s, &s.cache_a), s.pc.clone(), "frank", &s.clock);
        assert!(matches!(result, Err(Error::NewerSchema { .. })));
        assert!(!s.dir.join(lockfile::LOCK_FILE).exists());
    }

    #[test]
    fn close_writes_and_releases() {
        let s = shared();
        let mut a = Workspace::open(loc(&s, &s.cache_a), s.pc.clone(), "frank", &s.clock).unwrap();
        add_customer(&mut a, &s.clock, "Kunde A");
        a.close(&s.clock).unwrap();
        assert_eq!(shared_files(&s), vec!["psybil.db"]);
        let b = Workspace::open(loc(&s, &s.cache_b), s.mac.clone(), "frank", &s.clock).unwrap();
        assert!(matches!(b.access(), Access::ReadWrite));
        assert_eq!(names(b.db()), vec!["Kunde A"]);
    }
}
