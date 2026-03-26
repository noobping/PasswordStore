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
    pub(super) window_width: Option<i32>,
    pub(super) window_height: Option<i32>,
    pub(super) new_pass_file_template: Option<String>,
    pub(super) clear_empty_fields_before_save: Option<bool>,
    pub(super) password_generation: Option<PasswordGenerationSettings>,
    pub(super) username_fallback_mode: Option<UsernameFallbackMode>,
    pub(super) ripasso_own_fingerprint: Option<String>,
    pub(super) sync_private_keys_with_host: Option<bool>,
}

fn config_path() -> PathBuf {
    dirs_next::config_dir().map_or_else(
        || PathBuf::from(format!("{}.toml", env!("CARGO_PKG_NAME"))),
        |dir| dir.join(format!("{}.toml", env!("CARGO_PKG_NAME"))),
    )
}

pub(super) fn load_file_prefs() -> PreferenceFile {
    let path = config_path();
    fs::read_to_string(&path).map_or_else(
        |_| PreferenceFile::default(),
        |data| toml::from_str(&data).unwrap_or_default(),
    )
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
