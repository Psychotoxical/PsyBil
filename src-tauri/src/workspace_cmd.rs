//! Where the data lives, the lock of a shared folder, and merging.

use std::path::{Path, PathBuf};

use psybil_core::backup::{BEFORE_MOVE_PREFIX, MANUAL_PREFIX, backup_file_name, backup_to};
use psybil_core::dates::{Clock, SystemClock};
use psybil_core::lockfile::{LockInfo, LockState};
use psybil_core::merge::{MergeReport, QuarantineEntry};
use psybil_core::settings::DataPlace;
use psybil_core::workspace::{Access, Location, WORK_DB, Workspace, find_conflict_copies};
use serde::{Deserialize, Serialize};
use tauri::State;

use crate::error::{CmdError, CmdResult};
use crate::state::{AppState, local_now, location_for, lock, user_name};

/// What the window shows about the data: where it is, who may write, and
/// what needs attention.
#[derive(Debug, Clone, Serialize)]
pub struct WorkspaceStatus {
    pub place: DataPlace,
    /// `read_write`, `read_only` or `none` (not open, see `problem`).
    pub access: &'static str,
    /// The computer that holds the lock while this one only reads.
    pub holder: Option<LockInfo>,
    /// The holder has not renewed its lock for 5 minutes.
    pub stale: bool,
    /// The lock is free now; reloading opens the data for writing.
    pub lock_free: bool,
    /// Another computer took over while this one was writing.
    pub lock_lost: bool,
    pub sync_error: Option<String>,
    pub backup_error: Option<String>,
    /// Cloud conflict copies of the shared file, offered for merging.
    pub conflict_copies: Vec<String>,
    pub problem: Option<CmdError>,
    pub demo: bool,
    pub setup_done: bool,
}

pub fn status(state: &AppState) -> WorkspaceStatus {
    let place = if state.demo {
        DataPlace::Local
    } else {
        lock(&state.settings).data.clone()
    };
    let setup_done = lock(&state.settings).setup_done;
    let flags = lock(&state.flags).clone();
    let guard = lock(&state.workspace);
    let mut out = WorkspaceStatus {
        place,
        access: "none",
        holder: None,
        stale: false,
        lock_free: false,
        lock_lost: flags.lock_lost,
        sync_error: flags.sync_error,
        backup_error: flags.backup_error,
        conflict_copies: Vec::new(),
        problem: lock(&state.problem).clone(),
        demo: state.demo,
        setup_done,
    };
    let Some(ws) = guard.as_ref() else {
        return out;
    };
    match ws.access() {
        Access::ReadWrite => {
            out.access = "read_write";
            if let Location::Shared { dir, .. } = ws.location() {
                out.conflict_copies = find_conflict_copies(dir)
                    .unwrap_or_default()
                    .into_iter()
                    .map(|p| p.to_string_lossy().into_owned())
                    .collect();
            }
        }
        Access::ReadOnly { holder, .. } => {
            out.access = "read_only";
            match ws.lock_state(&SystemClock) {
                Ok(Some(LockState::Free | LockState::Mine(_))) => out.lock_free = true,
                Ok(Some(LockState::Stale(info))) => {
                    out.stale = true;
                    out.holder = Some(info);
                }
                Ok(Some(LockState::Held(info))) => out.holder = Some(info),
                _ => out.holder = Some(holder.clone()),
            }
        }
    }
    out
}

/// Replaces the open workspace with what `open` returns for the current
/// place. When that fails, the data opens read-only if it can.
fn reopen(
    state: &AppState,
    open: impl FnOnce(Location) -> psybil_core::Result<Workspace>,
) -> CmdResult<()> {
    let location = state.location();
    let mut guard = lock(&state.workspace);
    drop(guard.take());
    match open(location.clone()) {
        Ok(ws) => {
            note_pending(state, &ws);
            *guard = Some(ws);
            *lock(&state.problem) = None;
            lock(&state.flags).lock_lost = false;
            Ok(())
        }
        Err(e) => {
            let error = CmdError::from(e);
            match Workspace::open_or_read(location, state.device(), &user_name(), &SystemClock) {
                Ok(ws) => *guard = Some(ws),
                Err(_) => *lock(&state.problem) = Some(error.clone()),
            }
            Err(error)
        }
    }
}

/// Opens the data again: a fresh copy while reading, or the data for
/// writing once the lock is free. Data that is open for writing stays.
pub fn reload_inner(state: &AppState) -> CmdResult<WorkspaceStatus> {
    let writing = lock(&state.workspace)
        .as_ref()
        .is_some_and(|ws| matches!(ws.access(), Access::ReadWrite));
    if !writing {
        let device = state.device();
        reopen(state, |l| {
            Workspace::open_or_read(l, device, &user_name(), &SystemClock)
        })?;
    }
    Ok(status(state))
}

/// Takes over an abandoned lock, or with `force` any lock.
pub fn take_over_inner(state: &AppState, force: bool) -> CmdResult<WorkspaceStatus> {
    let device = state.device();
    reopen(state, |l| {
        if force {
            Workspace::force_take_over(l, device, &user_name(), &SystemClock)
        } else {
            Workspace::take_over(l, device, &user_name(), &SystemClock)
        }
    })?;
    Ok(status(state))
}

fn conflicts_dir(state: &AppState) -> PathBuf {
    state.backups_dir().join("conflicts")
}

pub fn merge_conflict_copies_inner(
    state: &AppState,
    paths: &[String],
) -> CmdResult<Vec<MergeReport>> {
    let target = conflicts_dir(state);
    let reports = state.with_workspace(|ws| {
        paths
            .iter()
            .map(|p| ws.merge_conflict_copy(Path::new(p), &SystemClock, &target))
            .collect()
    })?;
    *lock(&state.last_write) = Some(std::time::Instant::now());
    Ok(reports)
}

/// How to switch to another data place.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PlaceMode {
    /// Move the data there; the target must not hold PsyBil data yet.
    Move,
    /// Open the data that is already there; the current data stays as a backup.
    Open,
    /// Like `Open`, and merge the current data into it.
    OpenAndMerge,
}

/// A working copy with changes not written back yet is written back soon.
pub(crate) fn note_pending(state: &AppState, ws: &Workspace) {
    if ws.is_dirty() {
        *lock(&state.last_write) = Some(std::time::Instant::now());
    }
}

/// After a switch, messages about the old place no longer apply.
fn clear_flags(state: &AppState) {
    let mut flags = lock(&state.flags);
    flags.lock_lost = false;
    flags.sync_error = None;
}

/// Moves the local file the data left behind into the backups, so going
/// back to this computer later moves the newest state instead of opening
/// this old one.
fn put_aside(state: &AppState, file: &Path) {
    if !file.exists() {
        return;
    }
    let dir = state.backups_dir();
    let target = dir.join(backup_file_name(BEFORE_MOVE_PREFIX, local_now()));
    let moved = std::fs::create_dir_all(&dir).and_then(|()| std::fs::rename(file, &target));
    if let Err(e) = moved {
        eprintln!("PsyBil: the old local data could not be put aside: {e}");
        return;
    }
    for suffix in ["-wal", "-shm"] {
        let _ = std::fs::remove_file(format!("{}{suffix}", file.display()));
    }
}

/// Working on this computer while the shared folder is away starts from
/// this computer's working copy of it, not from an old or empty file.
fn seed_local_from_cache(state: &AppState, current: &Location) -> CmdResult<()> {
    let Location::Shared { cache, .. } = current else {
        return Ok(());
    };
    let file = state.app_data.join("psybil.db");
    let work = cache.join(WORK_DB);
    if file.exists() || !work.exists() {
        return Ok(());
    }
    std::fs::copy(&work, &file).map_err(|e| CmdError::new("io", e.to_string()))?;
    let wal = PathBuf::from(format!("{}-wal", work.display()));
    if wal.exists() {
        std::fs::copy(&wal, format!("{}-wal", file.display()))
            .map_err(|e| CmdError::new("io", e.to_string()))?;
    }
    Ok(())
}

fn save_place(state: &AppState, place: DataPlace) -> CmdResult<()> {
    let mut settings = lock(&state.settings);
    settings.data = place;
    settings.save(&state.settings_path)?;
    Ok(())
}

pub fn choose_data_place_inner(
    state: &AppState,
    place: DataPlace,
    mode: PlaceMode,
) -> CmdResult<Option<MergeReport>> {
    if state.demo {
        return Err(CmdError::new(
            "demo",
            "the data place cannot change in demo mode",
        ));
    }
    let target = location_for(&place, &state.app_data);
    let current = state.location();
    // Merging needs the lock there; a folder another computer holds is
    // refused before anything changes here.
    if mode == PlaceMode::OpenAndMerge
        && let Location::Shared { dir, .. } = &target
        && let LockState::Held(holder) =
            psybil_core::lockfile::read_lock(dir, &state.device(), SystemClock.now_utc())?
    {
        return Err(psybil_core::Error::LockHeld(holder).into());
    }
    if mode == PlaceMode::Move {
        state.with_workspace(|ws| ws.relocate(target, &user_name(), &SystemClock))?;
        save_place(state, place)?;
        clear_flags(state);
        if let Location::Local { file } = &current {
            put_aside(state, file);
        }
        return Ok(None);
    }
    // The current data is kept as a backup before anything else happens.
    let saved = {
        let guard = lock(&state.workspace);
        match guard.as_ref() {
            Some(ws) => {
                let dir = state.backups_dir();
                std::fs::create_dir_all(&dir).map_err(|e| CmdError::new("io", e.to_string()))?;
                let path = dir.join(backup_file_name(MANUAL_PREFIX, local_now()));
                backup_to(ws.db(), &path)?;
                Some(path)
            }
            None => None,
        }
    };
    {
        let mut guard = lock(&state.workspace);
        if let Some(mut ws) = guard.take()
            && let Err(e) = ws.close(&SystemClock)
        {
            *guard = Some(ws);
            return Err(e.into());
        }
    }
    if place == DataPlace::Local {
        seed_local_from_cache(state, &current)?;
    }
    let opened = Workspace::open_or_read(target, state.device(), &user_name(), &SystemClock);
    let ws = match opened {
        Ok(ws) => ws,
        Err(e) => {
            let device = state.device();
            let _ = reopen(state, |l| {
                Workspace::open_or_read(l, device, &user_name(), &SystemClock)
            });
            return Err(e.into());
        }
    };
    note_pending(state, &ws);
    *lock(&state.workspace) = Some(ws);
    *lock(&state.problem) = None;
    save_place(state, place)?;
    clear_flags(state);
    if mode == PlaceMode::Open {
        return Ok(None);
    }
    let Some(saved) = saved else {
        return Ok(None);
    };
    // The merge moves the copy away, so it gets a copy of the backup.
    let copy = state.backups_dir().join(format!(
        "{}.merge",
        saved.file_name().unwrap_or_default().to_string_lossy()
    ));
    std::fs::copy(&saved, &copy).map_err(|e| CmdError::new("io", e.to_string()))?;
    let target = conflicts_dir(state);
    let report = state.with_workspace(|ws| ws.merge_conflict_copy(&copy, &SystemClock, &target))?;
    *lock(&state.last_write) = Some(std::time::Instant::now());
    Ok(Some(report))
}

pub fn finish_setup_inner(state: &AppState) -> CmdResult<()> {
    let mut settings = lock(&state.settings);
    settings.setup_done = true;
    settings.save(&state.settings_path)?;
    Ok(())
}

#[tauri::command(async, rename_all = "snake_case")]
pub fn workspace_status(state: State<'_, AppState>) -> WorkspaceStatus {
    status(&state)
}

#[tauri::command(async, rename_all = "snake_case")]
pub fn reload_workspace(state: State<'_, AppState>) -> CmdResult<WorkspaceStatus> {
    reload_inner(&state)
}

#[tauri::command(async, rename_all = "snake_case")]
pub fn take_over_lock(state: State<'_, AppState>) -> CmdResult<WorkspaceStatus> {
    take_over_inner(&state, false)
}

#[tauri::command(async, rename_all = "snake_case")]
pub fn force_take_over_lock(state: State<'_, AppState>) -> CmdResult<WorkspaceStatus> {
    take_over_inner(&state, true)
}

#[tauri::command(async, rename_all = "snake_case")]
pub fn merge_conflict_copies(
    state: State<'_, AppState>,
    paths: Vec<String>,
) -> CmdResult<Vec<MergeReport>> {
    merge_conflict_copies_inner(&state, &paths)
}

#[tauri::command(async, rename_all = "snake_case")]
pub fn keep_other_version(
    state: State<'_, AppState>,
    table: String,
    row: serde_json::Value,
) -> CmdResult<()> {
    state.write(|tx| psybil_core::merge::keep_other_version(tx, &SystemClock, &table, &row))
}

#[tauri::command(async, rename_all = "snake_case")]
pub fn quarantine_entries(
    state: State<'_, AppState>,
    ids: Vec<String>,
) -> CmdResult<Vec<QuarantineEntry>> {
    state.read(|c| psybil_core::merge::quarantine_entries(c, &ids))
}

#[tauri::command(async, rename_all = "snake_case")]
pub fn choose_data_place(
    state: State<'_, AppState>,
    place: DataPlace,
    mode: PlaceMode,
) -> CmdResult<Option<MergeReport>> {
    choose_data_place_inner(&state, place, mode)
}

#[tauri::command(async, rename_all = "snake_case")]
pub fn finish_setup(state: State<'_, AppState>) -> CmdResult<()> {
    finish_setup_inner(&state)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::tests::state_with;
    use chrono::Utc;
    use psybil_core::db::Db;
    use psybil_core::lockfile::{self, Device};
    use psybil_core::masterdata::{NewCustomer, create_customer, list_customers};
    use psybil_core::workspace::SHARED_DB;

    fn shared(dir: &Path) -> DataPlace {
        DataPlace::Shared {
            dir: dir.to_string_lossy().into_owned(),
        }
    }

    fn mac() -> Device {
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

    fn names(state: &AppState) -> Vec<String> {
        let mut n: Vec<String> = state
            .read(|c| list_customers(c, true))
            .unwrap()
            .into_iter()
            .map(|c| c.name)
            .collect();
        n.sort();
        n
    }

    /// A shared folder that another computer already filled and closed.
    fn filled_shared(name: &str) -> tempfile::TempDir {
        let dir = tempfile::tempdir().unwrap();
        let cache = tempfile::tempdir().unwrap();
        let location = Location::Shared {
            dir: dir.path().to_path_buf(),
            cache: cache.path().to_path_buf(),
        };
        let mut ws = Workspace::open(location, mac(), "band", &SystemClock).unwrap();
        ws.write(|tx| {
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
        ws.close(&SystemClock).unwrap();
        dir
    }

    #[test]
    fn start_in_shared_folder_takes_lock() {
        let dir = tempfile::tempdir().unwrap();
        let (state, _dirs) = state_with(shared(dir.path()));
        let s = status(&state);
        assert_eq!(s.access, "read_write");
        assert!(dir.path().join("psybil.lock").exists());
    }

    #[test]
    fn start_with_foreign_fresh_lock_is_read_only() {
        let dir = filled_shared("Kunde Fern");
        lockfile::acquire(dir.path(), &mac(), "band", Utc::now()).unwrap();
        let (state, _dirs) = state_with(shared(dir.path()));
        let s = status(&state);
        assert_eq!(s.access, "read_only");
        assert!(!s.stale);
        assert_eq!(s.holder.unwrap().device_name, "Mac");
        assert_eq!(names(&state), ["Kunde Fern"]);
        assert_eq!(
            state
                .write(|tx| create_customer(tx, &SystemClock, NewCustomer::default()))
                .unwrap_err()
                .code,
            "read_only"
        );
    }

    #[test]
    fn stale_lock_can_be_taken_over() {
        let dir = filled_shared("Kunde Fern");
        let old = Utc::now() - chrono::Duration::minutes(10);
        lockfile::acquire(dir.path(), &mac(), "band", old).unwrap();
        let (state, _dirs) = state_with(shared(dir.path()));
        let s = status(&state);
        assert_eq!(s.access, "read_only");
        assert!(s.stale);
        let after = take_over_inner(&state, false).unwrap();
        assert_eq!(after.access, "read_write");
        add_customer(&state, "Kunde A");
    }

    #[test]
    fn force_take_over_from_fresh_lock() {
        let dir = filled_shared("Kunde Fern");
        lockfile::acquire(dir.path(), &mac(), "band", Utc::now()).unwrap();
        let (state, _dirs) = state_with(shared(dir.path()));
        assert_eq!(
            take_over_inner(&state, false).unwrap_err().code,
            "lock_held"
        );
        assert_eq!(status(&state).access, "read_only");
        assert_eq!(take_over_inner(&state, true).unwrap().access, "read_write");
    }

    #[test]
    fn unreachable_folder_gives_problem_not_crash() {
        let root = tempfile::tempdir().unwrap();
        let missing = root.path().join("not-mounted").join("PsyBil");
        let (state, _dirs) = state_with(shared(&missing));
        let s = status(&state);
        assert_eq!(s.access, "none");
        assert!(s.problem.is_some());
        assert_eq!(names_err(&state), "no_data");
        assert!(!missing.exists(), "nothing created in the missing folder");
        // Working on this computer only gets the app going again.
        choose_data_place_inner(&state, DataPlace::Local, PlaceMode::Open).unwrap();
        assert_eq!(status(&state).access, "read_write");
        assert!(status(&state).problem.is_none());
    }

    fn names_err(state: &AppState) -> String {
        state.read(|c| list_customers(c, true)).unwrap_err().code
    }

    #[test]
    fn exit_writes_back_and_releases_lock() {
        let dir = tempfile::tempdir().unwrap();
        let (state, _dirs) = state_with(shared(dir.path()));
        add_customer(&state, "Kunde A");
        state.close_workspace().unwrap();
        assert!(!dir.path().join("psybil.lock").exists());
        let db = Db::open_read_only(&dir.path().join(SHARED_DB)).unwrap();
        assert_eq!(list_customers(db.read(), true).unwrap().len(), 1);
    }

    #[test]
    fn choose_shared_moves_data() {
        let (state, _dirs) = state_with(DataPlace::Local);
        add_customer(&state, "Kunde A");
        let dir = tempfile::tempdir().unwrap();
        choose_data_place_inner(&state, shared(dir.path()), PlaceMode::Move).unwrap();
        assert!(dir.path().join(SHARED_DB).exists());
        assert_eq!(lock(&state.settings).data, shared(dir.path()));
        let saved = psybil_core::settings::DeviceSettings::load(&state.settings_path);
        assert_eq!(saved.data, shared(dir.path()));
        assert_eq!(names(&state), ["Kunde A"]);
        assert_eq!(status(&state).access, "read_write");
    }

    #[test]
    fn choose_existing_shared_open_keeps_local_backup() {
        let (state, _dirs) = state_with(DataPlace::Local);
        add_customer(&state, "Kunde Lokal");
        let dir = filled_shared("Kunde Fern");
        assert_eq!(
            choose_data_place_inner(&state, shared(dir.path()), PlaceMode::Move)
                .unwrap_err()
                .code,
            "target_not_empty"
        );
        assert_eq!(names(&state), ["Kunde Lokal"]);
        choose_data_place_inner(&state, shared(dir.path()), PlaceMode::Open).unwrap();
        assert_eq!(names(&state), ["Kunde Fern"]);
        let backups = psybil_core::backup::list_backups(&state.backups_dir()).unwrap();
        assert!(
            backups
                .iter()
                .any(|b| b.kind == psybil_core::backup::BackupKind::Manual)
        );
    }

    #[test]
    fn open_and_merge_brings_local_data_in() {
        let (state, _dirs) = state_with(DataPlace::Local);
        add_customer(&state, "Kunde Lokal");
        let dir = filled_shared("Kunde Fern");
        let report =
            choose_data_place_inner(&state, shared(dir.path()), PlaceMode::OpenAndMerge).unwrap();
        assert!(report.is_some_and(|r| r.added >= 1));
        assert_eq!(names(&state), ["Kunde Fern", "Kunde Lokal"]);
    }

    #[test]
    fn cache_dir_is_per_shared_folder() {
        let data = Path::new("C:/Daten");
        let a = location_for(&shared(Path::new("D:/A")), data);
        let b = location_for(&shared(Path::new("D:/B")), data);
        let (Location::Shared { cache: ca, .. }, Location::Shared { cache: cb, .. }) = (a, b)
        else {
            panic!("shared locations expected");
        };
        assert_ne!(ca, cb);
        assert!(ca.starts_with(data.join("shared-cache")));
    }

    #[test]
    fn demo_refuses_place_change() {
        let (state, _dirs) = crate::state::tests::demo_state();
        let dir = tempfile::tempdir().unwrap();
        assert_eq!(
            choose_data_place_inner(&state, shared(dir.path()), PlaceMode::Move)
                .unwrap_err()
                .code,
            "demo"
        );
    }

    #[test]
    fn move_to_shared_puts_old_local_file_aside() {
        let (state, _dirs) = state_with(DataPlace::Local);
        add_customer(&state, "Kunde A");
        let local = state.app_data.join("psybil.db");
        let dir = tempfile::tempdir().unwrap();
        choose_data_place_inner(&state, shared(dir.path()), PlaceMode::Move).unwrap();
        assert!(!local.exists(), "the old local file is put aside");
        let kinds: Vec<_> = psybil_core::backup::list_backups(&state.backups_dir())
            .unwrap()
            .into_iter()
            .map(|b| b.kind)
            .collect();
        assert!(kinds.contains(&psybil_core::backup::BackupKind::BeforeMove));
        add_customer(&state, "Kunde B");
        // Back to this computer moves the newest state, not an old copy.
        choose_data_place_inner(&state, DataPlace::Local, PlaceMode::Move).unwrap();
        assert_eq!(names(&state), ["Kunde A", "Kunde B"]);
    }

    #[test]
    fn working_offline_starts_from_the_shared_working_copy() {
        let dir = tempfile::tempdir().unwrap();
        let data = tempfile::tempdir().unwrap();
        let config = tempfile::tempdir().unwrap();
        let settings = psybil_core::settings::DeviceSettings {
            device_name: "PC".into(),
            data: shared(dir.path()),
            ..psybil_core::settings::DeviceSettings::default()
        };
        settings.save(&config.path().join("settings.json")).unwrap();
        {
            let state = AppState::open(data.path(), config.path(), false).unwrap();
            add_customer(&state, "Kunde A");
            state.close_workspace().unwrap();
        }
        // The cloud has not brought the file back yet.
        std::fs::remove_file(dir.path().join(SHARED_DB)).unwrap();
        let state = AppState::open(data.path(), config.path(), false).unwrap();
        let s = status(&state);
        assert_eq!(s.access, "none");
        assert_eq!(s.problem.unwrap().code, "shared_file_missing");
        choose_data_place_inner(&state, DataPlace::Local, PlaceMode::Open).unwrap();
        assert_eq!(names(&state), ["Kunde A"]);
    }

    #[test]
    fn open_and_merge_into_held_folder_changes_nothing() {
        let (state, _dirs) = state_with(DataPlace::Local);
        add_customer(&state, "Kunde Lokal");
        let dir = filled_shared("Kunde Fern");
        lockfile::acquire(dir.path(), &mac(), "band", Utc::now()).unwrap();
        let err = choose_data_place_inner(&state, shared(dir.path()), PlaceMode::OpenAndMerge)
            .unwrap_err();
        assert_eq!(err.code, "lock_held");
        assert_eq!(lock(&state.settings).data, DataPlace::Local);
        assert_eq!(names(&state), ["Kunde Lokal"]);
        assert_eq!(status(&state).access, "read_write");
    }

    #[test]
    fn place_switch_clears_old_flags() {
        let (state, _dirs) = state_with(DataPlace::Local);
        {
            let mut flags = lock(&state.flags);
            flags.sync_error = Some("old".into());
            flags.lock_lost = true;
        }
        let dir = tempfile::tempdir().unwrap();
        choose_data_place_inner(&state, shared(dir.path()), PlaceMode::Move).unwrap();
        let s = status(&state);
        assert!(s.sync_error.is_none());
        assert!(!s.lock_lost);
    }

    #[test]
    fn exit_takes_the_data_out() {
        let dir = tempfile::tempdir().unwrap();
        let (state, _dirs) = state_with(shared(dir.path()));
        state.close_workspace().unwrap();
        // A late beat finds nothing to renew and takes no new lock.
        let mut ticker = crate::background::Ticker::new(std::time::Instant::now());
        let later = std::time::Instant::now() + crate::background::HEARTBEAT_EVERY;
        crate::background::tick(&state, &mut ticker, later, &SystemClock);
        assert!(!dir.path().join("psybil.lock").exists());
    }

    #[test]
    fn finish_setup_is_saved() {
        let (state, _dirs) = state_with(DataPlace::Local);
        assert!(!status(&state).setup_done);
        finish_setup_inner(&state).unwrap();
        assert!(status(&state).setup_done);
        assert!(psybil_core::settings::DeviceSettings::load(&state.settings_path).setup_done);
    }
}
