use adw::gio::{self, prelude::*, Settings};
use adw::glib::{bool_error, BoolError};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::fs;

use crate::config::APP_ID;

#[cfg(feature = "flatpak")]
mod flatpak;
#[cfg(not(feature = "flatpak"))]
mod standard;

#[cfg(feature = "flatpak")]
use self::flatpak as platform_defaults;
#[cfg(not(feature = "flatpak"))]
use self::standard as platform_defaults;
use self::platform_defaults::default_store_dirs;

const DEFAULT_NEW_PASS_FILE_TEMPLATE: &str = "username:\nurl:";

#[cfg(not(feature = "flatpak"))]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum BackendKind {
    Integrated,
    HostCommand,
}

#[cfg(not(feature = "flatpak"))]
fn default_backend_kind() -> BackendKind {
    BackendKind::Integrated
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
struct PreferenceFile {
    backend: Option<String>,
    pass_command: Option<String>,
    password_store_dirs: Option<Vec<String>>,
    new_pass_file_template: Option<String>,
    ripasso_own_fingerprint: Option<String>,
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

    fn expand_path(s: &str) -> String {
        shellexpand::full(s)
            .map(|c| c.into_owned())
            .unwrap_or_else(|_| s.to_string())
    }

    pub fn store_roots(&self) -> Vec<String> {
        self.stores()
            .into_iter()
            .map(|store| Self::expand_path(&store))
            .collect()
    }

    pub fn store(&self) -> String {
        self.store_roots().into_iter().next().unwrap_or_default()
    }

    pub fn new_pass_file_template(&self) -> String {
        if let Some(s) = &self.settings {
            s.string("new-pass-file-template").to_string()
        } else {
            let cfg = load_file_prefs();
            cfg.new_pass_file_template
                .unwrap_or_else(|| DEFAULT_NEW_PASS_FILE_TEMPLATE.to_string())
        }
    }

    pub fn stores(&self) -> Vec<String> {
        if let Some(s) = &self.settings {
            s.strv("password-store-dirs")
                .iter()
                .map(|g| g.to_string())
                .collect()
        } else {
            let cfg = load_file_prefs();
            if let Some(dirs) = cfg.password_store_dirs {
                dirs
            } else {
                default_store_dirs()
            }
        }
    }

    pub fn paths(&self) -> Vec<PathBuf> {
        self.store_roots().into_iter().map(PathBuf::from).collect()
    }

    pub fn set_stores(&self, stores: Vec<String>) -> Result<(), BoolError> {
        if let Some(s) = &self.settings {
            s.set_strv("password-store-dirs", stores.clone())
        } else {
            let mut cfg = load_file_prefs();
            cfg.password_store_dirs = Some(stores);
            save_file_prefs(&cfg)
        }
    }

    pub fn set_new_pass_file_template(&self, template: &str) -> Result<(), BoolError> {
        if let Some(s) = &self.settings {
            s.set_string("new-pass-file-template", template)
        } else {
            let mut cfg = load_file_prefs();
            cfg.new_pass_file_template = Some(template.to_string());
            save_file_prefs(&cfg)
        }
    }

    pub fn prune_missing_stores(&self) -> Result<bool, BoolError> {
        let stores = self.stores();
        let existing = stores
            .iter()
            .filter(|store| Self::store_dir_exists(store))
            .cloned()
            .collect::<Vec<_>>();

        if existing.len() == stores.len() {
            Ok(false)
        } else {
            self.set_stores(existing)?;
            Ok(true)
        }
    }

    fn store_dir_exists(store: &str) -> bool {
        let path = PathBuf::from(Self::expand_path(store));
        path.exists() && path.is_dir()
    }
}

fn config_path() -> PathBuf {
    if let Some(dir) = dirs_next::config_dir() {
        dir.join(format!("{}.toml", env!("CARGO_PKG_NAME")))
    } else {
        PathBuf::from(format!("{}.toml", env!("CARGO_PKG_NAME")))
    }
}

fn load_file_prefs() -> PreferenceFile {
    let path = config_path();
    if let Ok(data) = fs::read_to_string(&path) {
        toml::from_str(&data).unwrap_or_default()
    } else {
        PreferenceFile::default()
    }
}

fn save_file_prefs(cfg: &PreferenceFile) -> Result<(), BoolError> {
    let path = config_path();

    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|e| bool_error!("Failed to create config dir: {e}"))?;
    }

    let toml =
        toml::to_string_pretty(cfg).map_err(|e| bool_error!("Failed to serialize config: {e}"))?;

    fs::write(&path, toml).map_err(|e| bool_error!("Failed to write config file: {e}"))?;

    Ok(())
}

#[cfg(test)]
mod tests {
    #[cfg(not(feature = "flatpak"))]
    use super::{default_backend_kind, BackendKind};
    use super::{default_store_dirs, Preferences};
    use std::path::PathBuf;
    use std::time::{SystemTime, UNIX_EPOCH};

    #[test]
    fn default_store_dirs_match_build_mode() {
        #[cfg(feature = "flatpak")]
        assert!(default_store_dirs().is_empty());

        #[cfg(not(feature = "flatpak"))]
        if let Ok(home) = std::env::var("HOME") {
            assert_eq!(default_store_dirs(), vec![format!("{home}/.password-store")]);
        } else {
            assert!(default_store_dirs().is_empty());
        }
    }

    #[cfg(not(feature = "flatpak"))]
    #[test]
    fn default_backend_matches_build_mode() {
        assert_eq!(default_backend_kind(), BackendKind::Integrated);
    }

    #[cfg(not(feature = "flatpak"))]
    #[test]
    fn backend_storage_accepts_current_and_legacy_names() {
        assert_eq!(BackendKind::Integrated.stored_value(), "integrated");
        assert_eq!(BackendKind::from_stored("integrated"), BackendKind::Integrated);
        assert_eq!(BackendKind::from_stored("ripasso"), BackendKind::Integrated);
        assert_eq!(BackendKind::from_stored("host-command"), BackendKind::HostCommand);
        assert_eq!(BackendKind::from_stored("pass"), BackendKind::HostCommand);
    }

    #[test]
    fn missing_store_paths_are_filtered_out() {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock before unix epoch")
            .as_nanos();
        let existing = std::env::temp_dir().join(format!("passwordstore-test-{nanos}"));
        std::fs::create_dir_all(&existing).expect("create temp store dir");

        assert!(Preferences::store_dir_exists(existing.to_string_lossy().as_ref()));
        assert!(!Preferences::store_dir_exists(
            PathBuf::from(existing.join("missing")).to_string_lossy().as_ref()
        ));

        std::fs::remove_dir_all(&existing).expect("remove temp store dir");
    }
}
