//! Settings of this computer.

use psybil_core::settings::DeviceSettings;
use tauri::State;

use crate::state::AppState;

#[tauri::command]
pub fn load_settings(state: State<'_, AppState>) -> DeviceSettings {
    state.settings.lock().expect("settings lock").clone()
}

#[tauri::command]
pub fn save_settings(state: State<'_, AppState>, settings: DeviceSettings) -> Result<(), String> {
    settings
        .save(&state.settings_path)
        .map_err(|e| e.to_string())?;
    *state.settings.lock().expect("settings lock") = settings;
    Ok(())
}
