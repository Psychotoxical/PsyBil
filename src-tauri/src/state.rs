//! What the app holds while it runs: the open data and this computer's
//! settings.

use std::path::{Path, PathBuf};
use std::sync::Mutex;

use psybil_core::dates::SystemClock;
use psybil_core::lockfile::Device;
use psybil_core::settings::DeviceSettings;
use psybil_core::workspace::{Location, Workspace};

use crate::error::{CmdError, CmdResult};

pub struct AppState {
    pub workspace: Mutex<Workspace>,
    pub settings: Mutex<DeviceSettings>,
    pub settings_path: PathBuf,
    pub demo: bool,
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
fn user_name() -> String {
    std::env::var("USERNAME")
        .or_else(|_| std::env::var("USER"))
        .unwrap_or_default()
}

/// The name of this computer, shown to others when it holds the lock.
fn device_name() -> String {
    std::env::var("COMPUTERNAME")
        .or_else(|_| std::env::var("HOSTNAME"))
        .unwrap_or_else(|_| "PsyBil".into())
}

impl AppState {
    /// Opens the data in `data_dir` and the settings in `config_dir`. In demo
    /// mode both live in a fresh temporary folder: sample data, and a copy of
    /// the settings, so trying out a theme or language leaves the real ones alone.
    pub fn open(data_dir: &Path, config_dir: &Path, demo: bool) -> Result<AppState, String> {
        let real_settings = config_dir.join("settings.json");
        let mut settings = DeviceSettings::load(&real_settings);
        if settings.device_name.trim().is_empty() {
            settings.device_name = device_name();
        }
        let demo_dir = demo
            .then(|| std::env::temp_dir().join(format!("{DEMO_PREFIX}{}", uuid::Uuid::new_v4())));
        let (settings_path, file) = match &demo_dir {
            Some(dir) => (dir.join("settings.json"), dir.join("psybil.db")),
            None => (real_settings, data_dir.join("psybil.db")),
        };
        for path in [&settings_path, &file] {
            if let Some(dir) = path.parent() {
                std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
            }
        }
        settings.save(&settings_path).map_err(|e| e.to_string())?;
        let device = Device {
            id: settings.device_id,
            name: settings.device_name.clone(),
        };
        let mut workspace =
            Workspace::open(Location::Local { file }, device, &user_name(), &SystemClock)
                .map_err(|e| e.to_string())?;
        if demo {
            workspace
                .write(|tx| psybil_core::demo::seed(tx, &SystemClock))
                .map_err(|e| e.to_string())?;
        }
        Ok(AppState {
            workspace: Mutex::new(workspace),
            settings: Mutex::new(settings),
            settings_path,
            demo,
            _demo_folder: demo_dir.map(DemoFolder),
        })
    }
}

impl AppState {
    /// Runs a read on the open data.
    pub fn read<T>(
        &self,
        f: impl FnOnce(&psybil_core::rusqlite::Connection) -> psybil_core::Result<T>,
    ) -> CmdResult<T> {
        let ws = self
            .workspace
            .lock()
            .map_err(|_| CmdError::new("busy", "the data is not available"))?;
        Ok(f(ws.db().read())?)
    }

    /// Runs a write on the open data, in one transaction.
    pub fn write<T>(
        &self,
        f: impl FnOnce(&psybil_core::rusqlite::Transaction) -> psybil_core::Result<T>,
    ) -> CmdResult<T> {
        let mut ws = self
            .workspace
            .lock()
            .map_err(|_| CmdError::new("busy", "the data is not available"))?;
        Ok(ws.write(f)?)
    }
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
        let ws = state.workspace.lock().unwrap();
        let today = chrono::Local::now().date_naive();
        let rows = list_invoices(ws.db().read(), &InvoiceFilter::default(), today).unwrap();
        assert_eq!(rows.len(), 6);
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
