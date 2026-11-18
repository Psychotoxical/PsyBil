//! Backing up and restoring.

use std::path::Path;

use psybil_core::backup::{self, BackupEntry, BackupInfo};
use psybil_core::dates::SystemClock;
use tauri::State;
use tauri_plugin_opener::OpenerExt;

use crate::error::{CmdError, CmdResult};
use crate::state::AppState;

pub fn backup_now_inner(state: &AppState, path: &str) -> CmdResult<String> {
    let target = Path::new(path);
    state.with_workspace(|ws| backup::backup_to(ws.db(), target))?;
    state.note_exported(target);
    Ok(path.to_string())
}

pub fn restore_backup_inner(state: &AppState, path: &str) -> CmdResult<String> {
    let backups = state.backups_dir();
    let restored = state.with_workspace(|ws| ws.restore(Path::new(path), &backups, &SystemClock));
    if let Err(error) = &restored {
        // The data could not be opened again after the swap: show the
        // problem page instead of a window that saves nothing.
        let mut guard = crate::state::lock(&state.workspace);
        if guard.as_ref().is_some_and(|ws| !ws.is_open()) {
            *guard = None;
            *crate::state::lock(&state.problem) = Some(error.clone());
        }
    }
    Ok(restored?.to_string_lossy().into_owned())
}

#[tauri::command(async, rename_all = "snake_case")]
pub fn backup_now(state: State<'_, AppState>, path: String) -> CmdResult<String> {
    backup_now_inner(&state, &path)
}

#[tauri::command(async, rename_all = "snake_case")]
pub fn list_backups(state: State<'_, AppState>) -> CmdResult<Vec<BackupEntry>> {
    Ok(backup::list_backups(&state.backups_dir())?)
}

#[tauri::command(async, rename_all = "snake_case")]
pub fn inspect_backup(path: String) -> CmdResult<BackupInfo> {
    Ok(backup::inspect_backup(Path::new(&path))?)
}

/// Restores `path` and returns the backup of the state before.
#[tauri::command(async, rename_all = "snake_case")]
pub fn restore_backup(state: State<'_, AppState>, path: String) -> CmdResult<String> {
    restore_backup_inner(&state, &path)
}

/// Opens the folder of the automatic backups in the file manager.
#[tauri::command(async, rename_all = "snake_case")]
pub fn reveal_backups_folder(app: tauri::AppHandle, state: State<'_, AppState>) -> CmdResult<()> {
    let dir = state.backups_dir();
    std::fs::create_dir_all(&dir).map_err(|e| CmdError::new("io", e.to_string()))?;
    app.opener()
        .open_path(dir.to_string_lossy(), None::<&str>)
        .map_err(|e| CmdError::new("io", e.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::tests::state_with;
    use psybil_core::masterdata::{NewCustomer, create_customer, list_customers};
    use psybil_core::settings::DataPlace;

    #[test]
    fn backup_and_restore_through_the_state() {
        let (state, dirs) = state_with(DataPlace::Local);
        state
            .write(|tx| {
                create_customer(
                    tx,
                    &SystemClock,
                    NewCustomer {
                        name: "Kunde A".into(),
                        ..NewCustomer::default()
                    },
                )
            })
            .unwrap();
        let target = dirs.0.path().join("sicherung.db");
        backup_now_inner(&state, &target.to_string_lossy()).unwrap();
        assert!(state.exported.lock().unwrap().contains(&target));
        state
            .write(|tx| create_customer(tx, &SystemClock, NewCustomer::default()))
            .unwrap();
        let saved = restore_backup_inner(&state, &target.to_string_lossy()).unwrap();
        assert!(Path::new(&saved).exists());
        let count = state.read(|c| list_customers(c, true)).unwrap().len();
        assert_eq!(count, 1);
        let kinds: Vec<_> = backup::list_backups(&state.backups_dir())
            .unwrap()
            .into_iter()
            .map(|b| b.kind)
            .collect();
        assert!(kinds.contains(&backup::BackupKind::BeforeRestore));
        assert!(kinds.contains(&backup::BackupKind::Auto));
    }

    #[test]
    fn auto_backup_runs_once_at_start_not_in_demo() {
        let data = tempfile::tempdir().unwrap();
        let config = tempfile::tempdir().unwrap();
        for _ in 0..2 {
            let state = AppState::open(data.path(), config.path(), false).unwrap();
            drop(state);
        }
        let autos = backup::list_backups(&data.path().join("backups")).unwrap();
        assert_eq!(autos.len(), 1);
        let demo_data = tempfile::tempdir().unwrap();
        let state = AppState::open(demo_data.path(), config.path(), true).unwrap();
        assert!(
            backup::list_backups(&state.backups_dir())
                .unwrap()
                .is_empty()
        );
        assert!(!demo_data.path().join("backups").exists());
    }
}
