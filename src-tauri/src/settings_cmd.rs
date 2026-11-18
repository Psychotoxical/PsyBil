//! Settings of this computer.

use psybil_core::settings::DeviceSettings;
use tauri::State;

use crate::state::{AppState, lock};

#[tauri::command(async)]
pub fn load_settings(state: State<'_, AppState>) -> DeviceSettings {
    lock(&state.settings).clone()
}

/// Saves what the window changes (theme, language, panels, device name).
/// Device id, data place and setup state belong to the app and stay as
/// they are, so an older copy in the window cannot undo a folder change.
pub fn save_settings_inner(state: &AppState, incoming: DeviceSettings) -> Result<(), String> {
    let mut current = lock(&state.settings);
    let merged = DeviceSettings {
        device_id: current.device_id,
        data: current.data.clone(),
        setup_done: current.setup_done,
        ..incoming
    };
    merged
        .save(&state.settings_path)
        .map_err(|e| e.to_string())?;
    *current = merged;
    Ok(())
}

#[tauri::command(async)]
pub fn save_settings(state: State<'_, AppState>, settings: DeviceSettings) -> Result<(), String> {
    save_settings_inner(&state, settings)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::tests::state_with;
    use psybil_core::settings::DataPlace;

    #[test]
    fn save_settings_keeps_data_place() {
        let dir = tempfile::tempdir().unwrap();
        let place = DataPlace::Shared {
            dir: dir.path().to_string_lossy().into_owned(),
        };
        let (state, _dirs) = state_with(place.clone());
        lock(&state.settings).setup_done = true;
        let stale = DeviceSettings {
            language: "en".into(),
            data: DataPlace::Local,
            setup_done: false,
            ..lock(&state.settings).clone()
        };
        save_settings_inner(&state, stale).unwrap();
        let saved = DeviceSettings::load(&state.settings_path);
        assert_eq!(saved.language, "en");
        assert_eq!(saved.data, place);
        assert!(saved.setup_done);
    }
}
