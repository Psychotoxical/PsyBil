//! Backups: one complete SQLite file per backup, written with `VACUUM INTO`.
//! Backups always stay on this computer, also when the data lives in a
//! shared folder.

use std::path::{Path, PathBuf};

use chrono::NaiveDateTime;
use rusqlite::Connection;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::db::{Db, SCHEMA_VERSION};
use crate::merge::io_error;
use crate::workspace::{flush_file, remove_db_files};
use crate::{Error, Result};

const STAMP: &str = "%Y-%m-%d-%H%M%S";

/// Prefix of backups made with "Back up now".
pub const MANUAL_PREFIX: &str = "psybil-backup";
/// Prefix of the daily backup at start.
pub const AUTO_PREFIX: &str = "auto";
pub const BEFORE_RESTORE_PREFIX: &str = "before-restore";
pub const BEFORE_IMPORT_PREFIX: &str = "before-import";
/// The local file left behind when the data moved to a shared folder.
pub const BEFORE_MOVE_PREFIX: &str = "before-move";
/// Daily backups kept.
pub const AUTO_KEEP: usize = 10;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[cfg_attr(test, derive(ts_rs::TS), ts(export))]
pub enum BackupKind {
    Manual,
    Auto,
    BeforeRestore,
    BeforeImport,
    BeforeMove,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(test, derive(ts_rs::TS), ts(export))]
pub struct BackupEntry {
    pub path: String,
    pub file_name: String,
    pub kind: BackupKind,
    #[cfg_attr(test, ts(type = "number"))]
    pub size: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(test, derive(ts_rs::TS), ts(export))]
pub struct BackupInfo {
    pub path: String,
    pub schema_version: i32,
    /// When the data was last changed: the newest entry of the log.
    pub data_as_of: Option<String>,
    /// Issued invoices and cancellation documents.
    pub issued: u32,
    pub drafts: u32,
    pub businesses: u32,
}

impl Db {
    /// Writes a complete copy to `file`: a temporary file next to it, forced
    /// onto the disk, then renamed into place.
    pub(crate) fn place_copy(&self, file: &Path, tag: &str) -> Result<()> {
        let tmp = PathBuf::from(format!("{}.tmp-{tag}", file.display()));
        remove_db_files(&tmp)?;
        let written = self
            .read()
            .execute("VACUUM INTO ?1", [tmp.to_string_lossy().to_string()])
            .map_err(Error::from)
            .and_then(|_| flush_file(&tmp))
            .and_then(|()| std::fs::rename(&tmp, file).map_err(io_error));
        if written.is_err() {
            let _ = remove_db_files(&tmp);
        }
        written
    }
}

/// Writes a backup of `db` to `target`.
pub fn backup_to(db: &Db, target: &Path) -> Result<()> {
    db.place_copy(target, &Uuid::new_v4().to_string())
}

/// `psybil-backup-2026-11-18-143005.db` for `prefix` `psybil-backup`.
pub fn backup_file_name(prefix: &str, now: NaiveDateTime) -> String {
    format!("{prefix}-{}.db", now.format(STAMP))
}

/// Kind and time stamp of a backup file name; `None` for other files.
fn classify(name: &str) -> Option<(BackupKind, NaiveDateTime)> {
    let stem = name.strip_suffix(".db")?;
    [
        (MANUAL_PREFIX, BackupKind::Manual),
        (AUTO_PREFIX, BackupKind::Auto),
        (BEFORE_RESTORE_PREFIX, BackupKind::BeforeRestore),
        (BEFORE_IMPORT_PREFIX, BackupKind::BeforeImport),
        (BEFORE_MOVE_PREFIX, BackupKind::BeforeMove),
    ]
    .into_iter()
    .find_map(|(prefix, kind)| {
        let stamp = stem.strip_prefix(prefix)?.strip_prefix('-')?;
        NaiveDateTime::parse_from_str(stamp, STAMP)
            .ok()
            .map(|t| (kind, t))
    })
}

/// The daily backup: made once per day into `dir`, keeping the newest
/// `keep` daily backups. Other files in `dir` are left alone.
pub fn auto_backup(
    db: &Db,
    dir: &Path,
    now: NaiveDateTime,
    keep: usize,
) -> Result<Option<PathBuf>> {
    std::fs::create_dir_all(dir).map_err(io_error)?;
    let autos = |dir: &Path| -> Result<Vec<BackupEntry>> {
        let mut found: Vec<BackupEntry> = list_backups(dir)?
            .into_iter()
            .filter(|e| e.kind == BackupKind::Auto)
            .collect();
        found.reverse();
        Ok(found)
    };
    let today = now.date();
    let made_today = autos(dir)?
        .iter()
        .any(|e| classify(&e.file_name).is_some_and(|(_, t)| t.date() == today));
    if made_today {
        return Ok(None);
    }
    let path = dir.join(backup_file_name(AUTO_PREFIX, now));
    backup_to(db, &path)?;
    let all = autos(dir)?;
    let surplus = all.len().saturating_sub(keep);
    for old in &all[..surplus] {
        std::fs::remove_file(&old.path).map_err(io_error)?;
    }
    Ok(Some(path))
}

/// The backups in `dir`, newest first.
pub fn list_backups(dir: &Path) -> Result<Vec<BackupEntry>> {
    let entries = match std::fs::read_dir(dir) {
        Ok(entries) => entries,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(e) => return Err(io_error(e)),
    };
    let mut found = Vec::new();
    for entry in entries {
        let entry = entry.map_err(io_error)?;
        let file_name = entry.file_name().to_string_lossy().into_owned();
        let Some((kind, stamp)) = classify(&file_name) else {
            continue;
        };
        let meta = entry.metadata().map_err(io_error)?;
        if !meta.is_file() {
            continue;
        }
        found.push((
            stamp,
            BackupEntry {
                path: entry.path().to_string_lossy().into_owned(),
                file_name,
                kind,
                size: meta.len(),
            },
        ));
    }
    found.sort_by(|a, b| {
        b.0.cmp(&a.0)
            .then_with(|| a.1.file_name.cmp(&b.1.file_name))
    });
    Ok(found.into_iter().map(|(_, e)| e).collect())
}

/// A private folder that goes when it is dropped.
struct Scratch(PathBuf);

impl Scratch {
    fn new() -> Result<Scratch> {
        let dir = std::env::temp_dir().join(format!("psybil-inspect-{}", Uuid::new_v4()));
        std::fs::create_dir_all(&dir).map_err(io_error)?;
        Ok(Scratch(dir))
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn damaged(error: rusqlite::Error) -> Error {
    Error::BackupDamaged(error.to_string())
}

/// Integrity, tables and version of a database copy, before it is opened
/// as PsyBil data.
fn check_copy(copy: &Path) -> Result<i32> {
    let conn = Connection::open(copy).map_err(damaged)?;
    let check: String = conn
        .query_row("PRAGMA integrity_check", [], |r| r.get(0))
        .map_err(damaged)?;
    if check != "ok" {
        return Err(Error::BackupDamaged(check));
    }
    let tables: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM sqlite_master WHERE type = 'table'
             AND name IN ('businesses', 'invoices', 'audit_log')",
            [],
            |r| r.get(0),
        )
        .map_err(damaged)?;
    let version: i32 = conn
        .query_row("PRAGMA user_version", [], |r| r.get(0))
        .map_err(damaged)?;
    if tables < 3 || version < 1 {
        return Err(Error::NotABackup);
    }
    if version > SCHEMA_VERSION {
        return Err(Error::NewerSchema {
            found: version,
            supported: SCHEMA_VERSION,
        });
    }
    Ok(version)
}

/// Checks a backup and tells what is in it. Works on a private copy, so the
/// file itself is never opened or changed.
pub fn inspect_backup(path: &Path) -> Result<BackupInfo> {
    let scratch = Scratch::new()?;
    let copy = scratch.0.join("inspect.db");
    std::fs::copy(path, &copy).map_err(io_error)?;
    let schema_version = check_copy(&copy)?;
    let db = Db::open(&copy)?;
    let conn = db.read();
    let count = |sql: &str| -> Result<u32> { Ok(conn.query_row(sql, [], |r| r.get(0))?) };
    Ok(BackupInfo {
        path: path.to_string_lossy().into_owned(),
        schema_version,
        data_as_of: conn.query_row("SELECT MAX(at) FROM audit_log", [], |r| r.get(0))?,
        issued: count("SELECT COUNT(*) FROM invoices WHERE state = 'issued'")?,
        drafts: count("SELECT COUNT(*) FROM invoices WHERE state = 'draft'")?,
        businesses: count("SELECT COUNT(*) FROM businesses")?,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Error;
    use crate::dates::Clock;
    use crate::lockfile::{Device, LockState, read_lock};
    use crate::masterdata::{NewCustomer, create_customer, list_customers};
    use crate::testutil::{ManualClock, sample_customer};
    use crate::workspace::{Access, Location, SHARED_DB, Workspace};
    use chrono::NaiveDate;
    use uuid::Uuid;

    fn at(day: u32, h: u32) -> NaiveDateTime {
        NaiveDate::from_ymd_opt(2026, 11, day)
            .unwrap()
            .and_hms_opt(h, 30, 5)
            .unwrap()
    }

    fn device() -> Device {
        Device {
            id: Uuid::new_v4(),
            name: "PC".into(),
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

    fn file_names(dir: &Path) -> Vec<String> {
        let mut n: Vec<String> = std::fs::read_dir(dir)
            .unwrap()
            .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
            .collect();
        n.sort();
        n
    }

    /// A local workspace with one customer, and its backups folder.
    fn local() -> (tempfile::TempDir, Workspace, ManualClock, PathBuf) {
        let root = tempfile::tempdir().unwrap();
        let clock = ManualClock::new();
        let file = root.path().join("psybil.db");
        let mut ws = Workspace::open(Location::Local { file }, device(), "nutzer", &clock).unwrap();
        add_customer(&mut ws, &clock, "Kunde A");
        let backups = root.path().join("backups");
        (root, ws, clock, backups)
    }

    #[test]
    fn backup_is_complete_copy_without_wal() {
        let (root, ws, _, _) = local();
        let target = root.path().join("out").join("sicherung.db");
        std::fs::create_dir_all(target.parent().unwrap()).unwrap();
        backup_to(ws.db(), &target).unwrap();
        assert_eq!(file_names(target.parent().unwrap()), ["sicherung.db"]);
        let copy = Db::open_read_only(&target).unwrap();
        assert_eq!(names(&copy), ["Kunde A"]);
        assert_eq!(
            backup_file_name(MANUAL_PREFIX, at(18, 14)),
            "psybil-backup-2026-11-18-143005.db"
        );
    }

    #[test]
    fn auto_backup_once_per_day_keeps_ten() {
        let (_root, ws, _, backups) = local();
        for day in 1..=12 {
            assert!(
                auto_backup(ws.db(), &backups, at(day, 9), AUTO_KEEP)
                    .unwrap()
                    .is_some()
            );
            assert!(
                auto_backup(ws.db(), &backups, at(day, 18), AUTO_KEEP)
                    .unwrap()
                    .is_none()
            );
        }
        let files = file_names(&backups);
        assert_eq!(files.len(), 10);
        assert_eq!(files[0], "auto-2026-11-03-093005.db");
        assert_eq!(files[9], "auto-2026-11-12-093005.db");
    }

    #[test]
    fn auto_backup_never_deletes_other_backups() {
        let (_root, ws, _, backups) = local();
        std::fs::create_dir_all(&backups).unwrap();
        for name in [
            "psybil-backup-2026-10-01-100000.db",
            "before-restore-2026-10-02-100000.db",
            "before-import-2026-10-03-100000.db",
            "notizen.txt",
        ] {
            std::fs::write(backups.join(name), b"x").unwrap();
        }
        for day in 1..=12 {
            auto_backup(ws.db(), &backups, at(day, 9), 2).unwrap();
        }
        let files = file_names(&backups);
        assert_eq!(files.len(), 6, "{files:?}");
        assert!(files.contains(&"notizen.txt".to_string()));
        let listed = list_backups(&backups).unwrap();
        let kinds: Vec<BackupKind> = listed.iter().map(|e| e.kind).collect();
        assert_eq!(
            kinds,
            [
                BackupKind::Auto,
                BackupKind::Auto,
                BackupKind::BeforeImport,
                BackupKind::BeforeRestore,
                BackupKind::Manual
            ]
        );
        assert_eq!(listed[0].file_name, "auto-2026-11-12-093005.db");
    }

    #[test]
    fn inspect_reports_counts_and_date() {
        let (root, ws, _, _) = local();
        let target = root.path().join("b.db");
        backup_to(ws.db(), &target).unwrap();
        let info = inspect_backup(&target).unwrap();
        assert_eq!(info.schema_version, crate::db::SCHEMA_VERSION);
        assert_eq!(info.issued, 0);
        assert_eq!(info.drafts, 0);
        assert_eq!(info.businesses, 0);
        assert!(info.data_as_of.is_some_and(|t| t.starts_with("2026-11-18")));
        assert_eq!(info.path, target.to_string_lossy());
    }

    #[test]
    fn inspect_refuses_damaged_file() {
        let (root, ws, _, _) = local();
        let good = root.path().join("good.db");
        backup_to(ws.db(), &good).unwrap();
        let bytes = std::fs::read(&good).unwrap();
        let cut = root.path().join("cut.db");
        std::fs::write(&cut, &bytes[..bytes.len() / 2]).unwrap();
        assert!(matches!(inspect_backup(&cut), Err(Error::BackupDamaged(_))));
        let junk = root.path().join("junk.db");
        std::fs::write(&junk, b"this is not a database at all, just text ......").unwrap();
        assert!(matches!(
            inspect_backup(&junk),
            Err(Error::BackupDamaged(_))
        ));
    }

    #[test]
    fn inspect_refuses_foreign_sqlite() {
        let root = tempfile::tempdir().unwrap();
        let foreign = root.path().join("foreign.db");
        rusqlite::Connection::open(&foreign)
            .unwrap()
            .execute_batch("CREATE TABLE songs (title TEXT);")
            .unwrap();
        assert!(matches!(inspect_backup(&foreign), Err(Error::NotABackup)));
        let empty = root.path().join("empty.db");
        std::fs::write(&empty, b"").unwrap();
        assert!(matches!(inspect_backup(&empty), Err(Error::NotABackup)));
    }

    #[test]
    fn inspect_refuses_newer_schema() {
        let (root, ws, _, _) = local();
        let newer = root.path().join("newer.db");
        backup_to(ws.db(), &newer).unwrap();
        rusqlite::Connection::open(&newer)
            .unwrap()
            .execute_batch("PRAGMA user_version = 99;")
            .unwrap();
        assert!(matches!(
            inspect_backup(&newer),
            Err(Error::NewerSchema { found: 99, .. })
        ));
    }

    #[test]
    fn inspect_leaves_backup_file_untouched() {
        let (root, ws, _, _) = local();
        let target = root.path().join("b.db");
        backup_to(ws.db(), &target).unwrap();
        let before = std::fs::read(&target).unwrap();
        inspect_backup(&target).unwrap();
        assert_eq!(std::fs::read(&target).unwrap(), before);
        assert_eq!(
            file_names(root.path())
                .iter()
                .filter(|n| n.starts_with("b.db"))
                .count(),
            1
        );
    }

    #[test]
    fn restore_saves_current_state_first() {
        let (root, mut ws, clock, backups) = local();
        let old = root.path().join("old.db");
        backup_to(ws.db(), &old).unwrap();
        add_customer(&mut ws, &clock, "Kunde B");
        let before = ws.restore(&old, &backups, &clock).unwrap();
        assert!(
            before
                .file_name()
                .unwrap()
                .to_string_lossy()
                .starts_with("before-restore-")
        );
        let saved = Db::open_read_only(&before).unwrap();
        assert_eq!(names(&saved), ["Kunde A", "Kunde B"]);
    }

    #[test]
    fn restore_replaces_data_and_reopens() {
        let (root, mut ws, clock, backups) = local();
        let old = root.path().join("old.db");
        backup_to(ws.db(), &old).unwrap();
        add_customer(&mut ws, &clock, "Kunde B");
        ws.restore(&old, &backups, &clock).unwrap();
        assert_eq!(names(ws.db()), ["Kunde A"]);
        // Still writable, and the restore is in the log.
        add_customer(&mut ws, &clock, "Kunde C");
        let logged: i64 = ws
            .db()
            .read()
            .query_row(
                "SELECT COUNT(*) FROM audit_log WHERE action = 'restore_backup'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(logged, 1);
        // The original backup file is unchanged and still a backup.
        assert!(inspect_backup(&old).is_ok());
    }

    #[test]
    fn restore_failure_leaves_current_data() {
        let (root, mut ws, clock, backups) = local();
        let junk = root.path().join("junk.db");
        std::fs::write(&junk, b"no database here, only some words ....").unwrap();
        assert!(ws.restore(&junk, &backups, &clock).is_err());
        assert_eq!(names(ws.db()), ["Kunde A"]);
        add_customer(&mut ws, &clock, "Kunde B");
        assert!(!backups.exists() || file_names(&backups).is_empty());
    }

    #[test]
    fn restore_migrates_older_schema() {
        let (root, mut ws, clock, backups) = local();
        let old = root.path().join("v2.db");
        backup_to(ws.db(), &old).unwrap();
        rusqlite::Connection::open(&old)
            .unwrap()
            .execute_batch(
                "DROP TRIGGER invoice_attachments_no_update;
                 DROP TRIGGER invoice_attachments_no_delete;
                 DROP TABLE invoice_attachments; DROP TABLE outside_revenue;
                 PRAGMA user_version = 2;",
            )
            .unwrap();
        assert_eq!(inspect_backup(&old).unwrap().schema_version, 2);
        ws.restore(&old, &backups, &clock).unwrap();
        let version: i32 = ws
            .db()
            .read()
            .query_row("PRAGMA user_version", [], |r| r.get(0))
            .unwrap();
        assert_eq!(version, crate::db::SCHEMA_VERSION);
        assert_eq!(names(ws.db()), ["Kunde A"]);
    }

    #[test]
    fn restore_in_shared_folder_writes_back_and_keeps_lock() {
        let root = tempfile::tempdir().unwrap();
        let dir = root.path().join("Nextcloud");
        std::fs::create_dir_all(&dir).unwrap();
        let clock = ManualClock::new();
        let pc = device();
        let location = Location::Shared {
            dir: dir.clone(),
            cache: root.path().join("cache"),
        };
        let mut ws = Workspace::open(location, pc.clone(), "nutzer", &clock).unwrap();
        add_customer(&mut ws, &clock, "Kunde A");
        ws.sync(&clock).unwrap();
        let old = root.path().join("old.db");
        backup_to(ws.db(), &old).unwrap();
        add_customer(&mut ws, &clock, "Kunde B");
        ws.sync(&clock).unwrap();
        ws.restore(&old, &root.path().join("backups"), &clock)
            .unwrap();
        let shared = Db::open_read_only(&dir.join(SHARED_DB)).unwrap();
        assert_eq!(names(&shared), ["Kunde A"]);
        assert!(matches!(
            read_lock(&dir, &pc, clock.now_utc()).unwrap(),
            LockState::Mine(_)
        ));
        assert!(matches!(ws.access(), Access::ReadWrite));
    }

    #[test]
    fn restore_read_only_refused() {
        let root = tempfile::tempdir().unwrap();
        let dir = root.path().join("Nextcloud");
        std::fs::create_dir_all(&dir).unwrap();
        let clock = ManualClock::new();
        let location = |cache: &str| Location::Shared {
            dir: dir.clone(),
            cache: root.path().join(cache),
        };
        let mut writer = Workspace::open(location("a"), device(), "nutzer", &clock).unwrap();
        add_customer(&mut writer, &clock, "Kunde A");
        writer.sync(&clock).unwrap();
        let old = root.path().join("old.db");
        backup_to(writer.db(), &old).unwrap();
        let mut reader = Workspace::open(location("b"), device(), "band", &clock).unwrap();
        assert!(matches!(reader.access(), Access::ReadOnly { .. }));
        assert!(matches!(
            reader.restore(&old, &root.path().join("backups"), &clock),
            Err(Error::ReadOnly)
        ));
    }
}
