//! What the app holds while it runs: the open data and this computer's
//! settings.

use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, MutexGuard};
use std::time::Instant;

use psybil_core::backup::{AUTO_KEEP, auto_backup};
use psybil_core::dates::SystemClock;
use psybil_core::lockfile::Device;
use psybil_core::settings::{DataPlace, DeviceSettings};
use psybil_core::workspace::{Location, Workspace};

use crate::error::{CmdError, CmdResult};

/// What the background and the window report about the data, beyond the
/// workspace itself.
#[derive(Debug, Clone, Default)]
pub struct Flags {
    /// Another computer took the lock while this one was writing.
    pub lock_lost: bool,
    /// The last write-back failed with this message; it is tried again.
    pub sync_error: Option<String>,
    /// The daily backup failed with this message.
    pub backup_error: Option<String>,
}

pub struct AppState {
    /// `None` when the data could not be opened (see `problem`).
    pub workspace: Mutex<Option<Workspace>>,
    pub settings: Mutex<DeviceSettings>,
    pub settings_path: PathBuf,
    /// The app data folder (the demo folder in demo mode): local data,
    /// cache of a shared folder, backups.
    pub app_data: PathBuf,
    pub demo: bool,
    /// Why the data is not open.
    pub problem: Mutex<Option<CmdError>>,
    pub flags: Mutex<Flags>,
    /// When the last write happened that is not written back yet.
    pub last_write: Mutex<Option<Instant>>,
    /// Files PsyBil wrote in this session; only these can be shown in the
    /// file manager.
    pub exported: Mutex<HashSet<PathBuf>>,
    /// An old VibeBill database that was read and waits for "Take over".
    pub pending_import: Mutex<Option<crate::import_cmd::PendingImport>>,
    /// Last field: dropped after the workspace has closed the database.
    _demo_folder: Option<DemoFolder>,
}

/// The temporary folder of a demo run; it goes when the app state goes.
struct DemoFolder(PathBuf);

impl Drop for DemoFolder {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

/// Prefix of the temporary folders of demo runs.
pub const DEMO_PREFIX: &str = "psybil-demo-";

/// Removes demo folders that earlier runs left behind (the app can end without dropping
/// its state). Folders still in use stay, because their database cannot be deleted.
pub fn remove_stale_demo_folders(base: &Path, keep: &Path) {
    let Ok(entries) = std::fs::read_dir(base) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        let is_demo = entry
            .file_name()
            .to_str()
            .is_some_and(|n| n.starts_with(DEMO_PREFIX));
        if is_demo && path != keep && path.is_dir() {
            let _ = std::fs::remove_dir_all(&path);
        }
    }
}

/// The user name for the lock file of a shared folder.
pub(crate) fn user_name() -> String {
    std::env::var("USERNAME")
        .or_else(|_| std::env::var("USER"))
        .unwrap_or_default()
}

/// The name of this computer, shown to others when it holds the lock.
fn device_name() -> String {
    first_name([
        std::env::var("COMPUTERNAME").ok(),
        std::env::var("HOSTNAME").ok(),
        system_name(),
    ])
}

/// The first name that is not blank, trimmed; "PsyBil" when there is none.
fn first_name(names: impl IntoIterator<Item = Option<String>>) -> String {
    names
        .into_iter()
        .flatten()
        .map(|n| n.trim().to_string())
        .find(|n| !n.is_empty())
        .unwrap_or_else(|| "PsyBil".into())
}

/// The name the system gives this computer: the one shown in the Mac's
/// sharing settings, the kernel's host name on Linux.
#[cfg(target_os = "macos")]
fn system_name() -> Option<String> {
    let out = std::process::Command::new("scutil")
        .args(["--get", "ComputerName"])
        .output()
        .ok()?;
    out.status
        .success()
        .then(|| String::from_utf8_lossy(&out.stdout).into_owned())
}

#[cfg(all(unix, not(target_os = "macos")))]
fn system_name() -> Option<String> {
    ["/proc/sys/kernel/hostname", "/etc/hostname"]
        .iter()
        .find_map(|p| std::fs::read_to_string(p).ok())
}

/// Windows always sets COMPUTERNAME.
#[cfg(windows)]
fn system_name() -> Option<String> {
    None
}

/// Where the data of `place` lives. A shared folder gets its own cache, so
/// switching folders never mixes working copies.
pub fn location_for(place: &DataPlace, app_data: &Path) -> Location {
    match place {
        DataPlace::Local => Location::Local {
            file: app_data.join("psybil.db"),
        },
        DataPlace::Shared { dir } => {
            let hash = psybil_core::issue::sha256_hex(dir.as_bytes());
            Location::Shared {
                dir: PathBuf::from(dir),
                cache: app_data.join("shared-cache").join(&hash[..16]),
            }
        }
    }
}

/// Local time now, for backup names.
pub(crate) fn local_now() -> chrono::NaiveDateTime {
    chrono::Local::now().naive_local()
}

/// Takes a lock even when a thread panicked while holding it: a write that
/// panicked was rolled back with its transaction, so the data is whole.
pub(crate) fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(|e| e.into_inner())
}

impl AppState {
    /// Opens the data in `data_dir` (or the shared folder the settings name)
    /// and the settings in `config_dir`. In demo mode both live in a fresh
    /// temporary folder: sample data, and a copy of the settings, so trying
    /// out a theme or language leaves the real ones alone. Data that cannot
    /// be opened leaves the app running with a `problem`.
    pub fn open(data_dir: &Path, config_dir: &Path, demo: bool) -> Result<AppState, String> {
        let real_settings = config_dir.join("settings.json");
        let mut settings = DeviceSettings::load(&real_settings);
        if settings.device_name.trim().is_empty() {
            settings.device_name = device_name();
        }
        let demo_dir = demo
            .then(|| std::env::temp_dir().join(format!("{DEMO_PREFIX}{}", uuid::Uuid::new_v4())));
        let (settings_path, app_data) = match &demo_dir {
            Some(dir) => (dir.join("settings.json"), dir.clone()),
            None => (real_settings, data_dir.to_path_buf()),
        };
        for dir in [settings_path.parent(), Some(app_data.as_path())]
            .into_iter()
            .flatten()
        {
            std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
        }
        settings.save(&settings_path).map_err(|e| e.to_string())?;
        let place = if demo {
            DataPlace::Local
        } else {
            settings.data.clone()
        };
        let device = Device {
            id: settings.device_id,
            name: settings.device_name.clone(),
        };
        let (mut workspace, problem) = match Workspace::open_or_read(
            location_for(&place, &app_data),
            device,
            &user_name(),
            &SystemClock,
        ) {
            Ok(ws) => (Some(ws), None),
            Err(e) => (None, Some(CmdError::from(e))),
        };
        let mut flags = Flags::default();
        if let Some(ws) = workspace.as_mut() {
            if demo {
                ws.write(|tx| psybil_core::demo::seed(tx, &SystemClock))
                    .map_err(|e| e.to_string())?;
            } else if let Err(e) =
                auto_backup(ws.db(), &app_data.join("backups"), local_now(), AUTO_KEEP)
            {
                flags.backup_error = Some(e.to_string());
            }
        }
        let pending = workspace
            .as_ref()
            .is_some_and(|ws| ws.is_dirty())
            .then(Instant::now);
        Ok(AppState {
            workspace: Mutex::new(workspace),
            settings: Mutex::new(settings),
            settings_path,
            app_data,
            demo,
            problem: Mutex::new(problem),
            flags: Mutex::new(flags),
            // A working copy left with changes (a crash) is written back soon.
            last_write: Mutex::new(pending),
            exported: Mutex::new(HashSet::new()),
            pending_import: Mutex::new(None),
            _demo_folder: demo_dir.map(DemoFolder),
        })
    }

    /// This computer as the lock file names it.
    pub fn device(&self) -> Device {
        let settings = lock(&self.settings);
        Device {
            id: settings.device_id,
            name: settings.device_name.clone(),
        }
    }

    /// Where the data lives now.
    pub fn location(&self) -> Location {
        let place = if self.demo {
            DataPlace::Local
        } else {
            lock(&self.settings).data.clone()
        };
        location_for(&place, &self.app_data)
    }

    pub fn backups_dir(&self) -> PathBuf {
        self.app_data.join("backups")
    }

    /// Runs a read on the open data.
    pub fn read<T>(
        &self,
        f: impl FnOnce(&psybil_core::rusqlite::Connection) -> psybil_core::Result<T>,
    ) -> CmdResult<T> {
        let guard = lock(&self.workspace);
        let ws = guard.as_ref().ok_or_else(no_data)?;
        Ok(f(ws.db().read())?)
    }

    /// Runs a write on the open data, in one transaction, and notes it for
    /// the next write-back.
    pub fn write<T>(
        &self,
        f: impl FnOnce(&psybil_core::rusqlite::Transaction) -> psybil_core::Result<T>,
    ) -> CmdResult<T> {
        let mut guard = lock(&self.workspace);
        let ws = guard.as_mut().ok_or_else(no_data)?;
        let out = ws.write(f)?;
        *lock(&self.last_write) = Some(Instant::now());
        Ok(out)
    }

    /// Runs `f` on the open workspace itself.
    pub fn with_workspace<T>(
        &self,
        f: impl FnOnce(&mut Workspace) -> psybil_core::Result<T>,
    ) -> CmdResult<T> {
        let mut guard = lock(&self.workspace);
        let ws = guard.as_mut().ok_or_else(no_data)?;
        Ok(f(ws)?)
    }

    /// Notes a file PsyBil wrote, so the file manager may show it.
    pub fn note_exported(&self, path: &Path) {
        lock(&self.exported).insert(path.to_path_buf());
    }

    /// Writes back and releases the lock, at the end of the session.
    pub fn close_workspace(&self) -> CmdResult<()> {
        let mut guard = lock(&self.workspace);
        if let Some(ws) = guard.as_mut() {
            ws.close(&SystemClock)?;
        }
        // Closed for good: a late background beat must not take a new lock.
        *guard = None;
        *lock(&self.last_write) = None;
        Ok(())
    }
}

pub(crate) fn no_data() -> CmdError {
    CmdError::new("no_data", "the data is not open")
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use psybil_core::listing::{InvoiceFilter, list_invoices};

    /// An app state in demo mode; the folders live as long as the guard.
    pub(crate) fn demo_state() -> (AppState, (tempfile::TempDir, tempfile::TempDir)) {
        let data = tempfile::tempdir().unwrap();
        let config = tempfile::tempdir().unwrap();
        let state = AppState::open(data.path(), config.path(), true).unwrap();
        (state, (data, config))
    }

    /// A real (not demo) state whose settings name `place`.
    pub(crate) fn state_with(
        place: DataPlace,
    ) -> (AppState, (tempfile::TempDir, tempfile::TempDir)) {
        let data = tempfile::tempdir().unwrap();
        let config = tempfile::tempdir().unwrap();
        let settings = DeviceSettings {
            device_name: "PC".into(),
            data: place,
            ..DeviceSettings::default()
        };
        settings.save(&config.path().join("settings.json")).unwrap();
        let state = AppState::open(data.path(), config.path(), false).unwrap();
        (state, (data, config))
    }

    #[test]
    fn device_name_takes_first_real_name() {
        let name = first_name([None, Some("  ".into()), Some(" studio-box\n".into())]);
        assert_eq!(name, "studio-box");
        assert_eq!(first_name([None, None]), "PsyBil");
    }

    // GUI apps on macOS and Linux get no HOSTNAME variable: the system names
    // the computer instead.
    #[cfg(unix)]
    #[test]
    fn system_names_this_computer() {
        assert!(system_name().is_some_and(|n| !n.trim().is_empty()));
    }

    #[test]
    fn state_opens_local_database_in_given_dir() {
        let data = tempfile::tempdir().unwrap();
        let config = tempfile::tempdir().unwrap();
        let state = AppState::open(data.path(), config.path(), false).unwrap();
        assert!(data.path().join("psybil.db").exists());
        assert!(config.path().join("settings.json").exists());
        assert!(!state.demo);
        let settings = state.settings.lock().unwrap();
        assert!(!settings.device_name.is_empty());
    }

    #[test]
    fn demo_state_is_seeded() {
        let data = tempfile::tempdir().unwrap();
        let config = tempfile::tempdir().unwrap();
        let state = AppState::open(data.path(), config.path(), true).unwrap();
        assert!(
            !data.path().join("psybil.db").exists(),
            "demo leaves real data alone"
        );
        let today = chrono::Local::now().date_naive();
        let rows = state
            .read(|c| list_invoices(c, &InvoiceFilter::default(), today))
            .unwrap();
        assert_eq!(rows.len(), 10);
    }

    #[test]
    fn poisoned_mutex_keeps_working() {
        let (state, _dirs) = demo_state();
        let poisoned = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let _guard = state.workspace.lock().unwrap();
            panic!("a command panicked while holding the data");
        }));
        assert!(poisoned.is_err());
        assert!(state.workspace.is_poisoned());
        let today = chrono::Local::now().date_naive();
        let rows = state
            .read(|c| list_invoices(c, &InvoiceFilter::default(), today))
            .unwrap();
        assert_eq!(rows.len(), 10);
    }

    #[test]
    fn demo_folder_goes_with_the_state() {
        let data = tempfile::tempdir().unwrap();
        let config = tempfile::tempdir().unwrap();
        let state = AppState::open(data.path(), config.path(), true).unwrap();
        let folder = state.settings_path.parent().unwrap().to_path_buf();
        assert!(folder.join("psybil.db").exists());
        drop(state);
        assert!(!folder.exists(), "demo folder removed with the state");
    }

    #[test]
    fn stale_demo_folders_are_removed_but_others_stay() {
        let base = tempfile::tempdir().unwrap();
        let old = base.path().join("psybil-demo-old");
        let keep = base.path().join("psybil-demo-current");
        let other = base.path().join("something-else");
        for dir in [&old, &keep, &other] {
            std::fs::create_dir_all(dir).unwrap();
            std::fs::write(dir.join("psybil.db"), b"x").unwrap();
        }
        remove_stale_demo_folders(base.path(), &keep);
        assert!(!old.exists());
        assert!(keep.exists());
        assert!(other.exists());
    }

    #[test]
    fn demo_settings_leave_real_config_alone() {
        let data = tempfile::tempdir().unwrap();
        let config = tempfile::tempdir().unwrap();
        let real = config.path().join("settings.json");
        let mut before = DeviceSettings::load(&real);
        before.device_name = "PC".into();
        before.save(&real).unwrap();
        let bytes = std::fs::read(&real).unwrap();

        let state = AppState::open(data.path(), config.path(), true).unwrap();
        assert_ne!(state.settings_path, real, "demo writes its own copy");
        assert_eq!(state.settings.lock().unwrap().device_id, before.device_id);
        let mut changed = state.settings.lock().unwrap().clone();
        changed.language = "en".into();
        changed.save(&state.settings_path).unwrap();
        assert_eq!(
            std::fs::read(&real).unwrap(),
            bytes,
            "real settings unchanged"
        );
    }
}
