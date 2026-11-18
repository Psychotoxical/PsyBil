//! Settings of this computer (theme, language, panel states). They live
//! in a small file next to the app, not in the database, because the
//! database may be shared by several computers.

use std::path::Path;

use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::Result;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
#[cfg_attr(test, derive(ts_rs::TS), ts(export))]
pub enum ThemeChoice {
    #[default]
    System,
    Dark,
    Light,
}

/// Where this computer keeps the data.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(tag = "kind", rename_all = "snake_case")]
#[cfg_attr(test, derive(ts_rs::TS), ts(export))]
pub enum DataPlace {
    /// The app data folder of this computer.
    #[default]
    Local,
    /// A folder several computers reach, for example in a Nextcloud.
    Shared { dir: String },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
#[cfg_attr(test, derive(ts_rs::TS), ts(export))]
pub struct DeviceSettings {
    pub device_id: Uuid,
    pub device_name: String,
    pub theme: ThemeChoice,
    pub language: String,
    pub sidebar_collapsed: bool,
    pub preview_open: bool,
    pub data: DataPlace,
    /// The first start's questions have been answered.
    pub setup_done: bool,
}

impl Default for DeviceSettings {
    fn default() -> Self {
        DeviceSettings {
            device_id: Uuid::new_v4(),
            device_name: String::new(),
            theme: ThemeChoice::System,
            language: "de".into(),
            sidebar_collapsed: false,
            preview_open: true,
            data: DataPlace::Local,
            setup_done: false,
        }
    }
}

impl DeviceSettings {
    /// The settings in `path`; defaults when the file is missing or broken.
    pub fn load(path: &Path) -> DeviceSettings {
        std::fs::read_to_string(path)
            .ok()
            .and_then(|text| serde_json::from_str(&text).ok())
            .unwrap_or_default()
    }

    /// Writes the settings via a temporary file and a rename.
    pub fn save(&self, path: &Path) -> Result<()> {
        use crate::merge::io_error;
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir).map_err(io_error)?;
        }
        let tmp = path.with_extension("json.tmp");
        std::fs::write(&tmp, serde_json::to_string_pretty(self)?).map_err(io_error)?;
        std::fs::rename(&tmp, path).map_err(io_error)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn device_settings_roundtrip() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("settings.json");
        let settings = DeviceSettings {
            theme: ThemeChoice::Light,
            language: "en".into(),
            sidebar_collapsed: true,
            ..DeviceSettings::default()
        };
        settings.save(&path).unwrap();
        assert_eq!(DeviceSettings::load(&path), settings);
        assert_eq!(std::fs::read_dir(dir.path()).unwrap().count(), 1);
    }

    #[test]
    fn broken_settings_file_gives_defaults() {
        let dir = tempfile::tempdir().unwrap();
        let missing = DeviceSettings::load(&dir.path().join("fehlt.json"));
        assert_eq!(
            (missing.language.as_str(), missing.theme),
            ("de", ThemeChoice::System)
        );
        let broken = dir.path().join("kaputt.json");
        std::fs::write(&broken, b"{ kein json").unwrap();
        let loaded = DeviceSettings::load(&broken);
        assert_eq!(loaded.language, "de");
        assert_ne!(loaded.device_id, Uuid::nil());
    }

    #[test]
    fn old_settings_file_loads_with_local_place() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("settings.json");
        std::fs::write(
            &path,
            br#"{"device_id":"6f1c1d2e-0000-4000-8000-000000000001","device_name":"PC",
                "theme":"dark","language":"en","sidebar_collapsed":false,"preview_open":true}"#,
        )
        .unwrap();
        let loaded = DeviceSettings::load(&path);
        assert_eq!(loaded.theme, ThemeChoice::Dark);
        assert_eq!(loaded.data, DataPlace::Local);
        assert!(!loaded.setup_done);
        let shared = DeviceSettings {
            data: DataPlace::Shared {
                dir: "C:/Daten/Nextcloud/PsyBil".into(),
            },
            setup_done: true,
            ..loaded
        };
        shared.save(&path).unwrap();
        assert_eq!(DeviceSettings::load(&path), shared);
    }
}
