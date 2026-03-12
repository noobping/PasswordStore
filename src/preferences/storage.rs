use super::UsernameFallbackMode;
use crate::password::generation::PasswordGenerationSettings;
use adw::glib::{bool_error, BoolError};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub(super) struct PreferenceFile {
    pub(super) backend: Option<String>,
    pub(super) pass_command: Option<String>,
    pub(super) password_store_dirs: Option<Vec<String>>,
    pub(super) new_pass_file_template: Option<String>,
    pub(super) password_generation: Option<PasswordGenerationSettings>,
    pub(super) username_fallback_mode: Option<UsernameFallbackMode>,
    pub(super) ripasso_own_fingerprint: Option<String>,
}

fn config_path() -> PathBuf {
    if let Some(dir) = dirs_next::config_dir() {
        dir.join(format!("{}.toml", env!("CARGO_PKG_NAME")))
    } else {
        PathBuf::from(format!("{}.toml", env!("CARGO_PKG_NAME")))
    }
}

pub(super) fn load_file_prefs() -> PreferenceFile {
    let path = config_path();
    if let Ok(data) = fs::read_to_string(&path) {
        toml::from_str(&data).unwrap_or_default()
    } else {
        PreferenceFile::default()
    }
}

pub(super) fn save_file_prefs(cfg: &PreferenceFile) -> Result<(), BoolError> {
    let path = config_path();

    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|e| bool_error!("Failed to create config dir: {e}"))?;
    }

    let toml =
        toml::to_string_pretty(cfg).map_err(|e| bool_error!("Failed to serialize config: {e}"))?;

    fs::write(&path, toml).map_err(|e| bool_error!("Failed to write config file: {e}"))?;

    Ok(())
}
