use adw::gio::{self, prelude::*, Settings};
use adw::glib::BoolError;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};

const APP_ID: &str = "dev.noobping.passwordstore";
const DEFAULT_CMD: &str = "pass";

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
struct PreferenceFile {
    pass_command: Option<String>,
    password_store_dirs: Option<Vec<String>>,
}

#[derive(Debug, Clone)]
pub struct Preferences {
    settings: Option<Settings>,
}

impl Preferences {
    pub fn new() -> Self {
        Self {
            settings: Self::try_settings(),
        }
    }

    fn try_settings() -> Option<Settings> {
        let source = gio::SettingsSchemaSource::default()?;
        let _schema = source.lookup(APP_ID, true)?;
        Some(Settings::new(APP_ID))
    }

    pub fn command(&self) -> String {
        match &self.settings {
            Some(s) => s.string("pass-command").to_string(),
            None => DEFAULT_CMD.to_string(),
        }
    }

    pub fn stores(&self) -> Vec<String> {
        match &self.settings {
            Some(s) => s
                .strv("password-store-dirs")
                .iter()
                .map(|g| g.to_string())
                .collect(),
            None => {
                if let Ok(home) = std::env::var("HOME") {
                    vec![format!("{home}/.password-store")]
                } else {
                    Vec::new()
                }
            }
        }
    }

    pub fn paths(&self) -> Vec<PathBuf> {
        match &self.settings {
            Some(s) => s
                .strv("password-store-dirs")
                .iter()
                .map(|g| PathBuf::from(g.as_str()))
                .collect(),
            None => {
                if let Ok(home) = std::env::var("HOME") {
                    vec![PathBuf::from(format!("{home}/.password-store"))]
                } else {
                    Vec::new()
                }
            }
        }
    }

    pub fn set_command(&self, cmd: &str) -> Result<(), BoolError> {
        if let Some(s) = &self.settings {
            s.set_string("pass-command", cmd)
        } else {
            // TODO: Save settings
            Ok(())
        }
    }

    pub fn set_stores(&self, stores: Vec<String>) -> Result<(), BoolError> {
        if let Some(s) = &self.settings {
            s.set_strv("password-store-dirs", stores)
        } else {
            // TODO: Save settings
            Ok(())
        }
    }

    pub fn has_references(&self) -> bool {
        self.settings.is_some()
    }
}

fn config_path() -> PathBuf {
    if let Some(dir) = std::env::var_os("XDG_CONFIG_HOME") {
        PathBuf::from(dir).join(format!("{}/config.toml", APP_ID))
    } else if let Some(home) = std::env::var_os("HOME") {
        PathBuf::from(home)
            .join(".config")
            .join(format!("{}/config.toml", APP_ID))
    } else {
        // Super fallback: current dir
        PathBuf::from(format!("{}.toml", APP_ID))
    }
}

fn load_file_prefs() -> FilePreferences {
    let path = config_path();

    if let Ok(data) = fs::read_to_string(&path) {
        toml::from_str(&data).unwrap_or_default()
    } else {
        FilePreferences::default()
    }
}

fn save_file_prefs(cfg: &FilePreferences) -> Result<(), BoolError> {
    use adw::glib::bool_error; // macro to construct BoolError

    let path = config_path();

    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .map_err(|e| bool_error!(format!("Failed to create config dir: {e}")))?;
    }

    let toml = toml::to_string_pretty(cfg)
        .map_err(|e| bool_error!(format!("Failed to serialize config: {e}")))?;

    fs::write(&path, toml).map_err(|e| bool_error!(format!("Failed to write config file: {e}")))?;

    Ok(())
}
