use crate::password::generation::PasswordGenerationSettings;
use adw::gio::{self, prelude::*, Settings};
use adw::glib::{bool_error, BoolError};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;
use std::process::Command;

#[cfg(feature = "flatpak")]
mod flatpak;
#[cfg(not(feature = "flatpak"))]
mod standard;

#[cfg(feature = "flatpak")]
use self::flatpak as platform_defaults;
use self::platform_defaults::default_store_dirs;
#[cfg(not(feature = "flatpak"))]
use self::standard as platform_defaults;

const DEFAULT_NEW_PASS_FILE_TEMPLATE: &str = "username:\nurl:";
const APP_ID: &str = env!("APP_ID");

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

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum UsernameFallbackMode {
    #[default]
    Folder,
    Filename,
}

impl UsernameFallbackMode {
    pub fn stored_value(self) -> &'static str {
        match self {
            Self::Folder => "folder",
            Self::Filename => "filename",
        }
    }

    pub fn from_stored(value: &str) -> Self {
        match value.trim().to_ascii_lowercase().as_str() {
            "filename" | "file" | "name" => Self::Filename,
            _ => Self::Folder,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
struct PreferenceFile {
    backend: Option<String>,
    pass_command: Option<String>,
    password_store_dirs: Option<Vec<String>>,
    new_pass_file_template: Option<String>,
    password_generation: Option<PasswordGenerationSettings>,
    username_fallback_mode: Option<UsernameFallbackMode>,
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

    fn read_preference<T>(
        &self,
        read_settings: impl FnOnce(&Settings) -> T,
        read_file: impl FnOnce(&PreferenceFile) -> T,
    ) -> T {
        if let Some(settings) = &self.settings {
            read_settings(settings)
        } else {
            read_file(&load_file_prefs())
        }
    }

    fn write_preference(
        &self,
        write_settings: impl FnOnce(&Settings) -> Result<(), BoolError>,
        write_file: impl FnOnce(&mut PreferenceFile),
    ) -> Result<(), BoolError> {
        if let Some(settings) = &self.settings {
            write_settings(settings)
        } else {
            let mut cfg = load_file_prefs();
            write_file(&mut cfg);
            save_file_prefs(&cfg)
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

    pub fn git_command(&self) -> Command {
        Command::new("git")
    }

    pub fn new_pass_file_template(&self) -> String {
        self.read_preference(
            |settings| settings.string("new-pass-file-template").to_string(),
            |cfg| {
                cfg.new_pass_file_template
                    .clone()
                    .unwrap_or_else(|| DEFAULT_NEW_PASS_FILE_TEMPLATE.to_string())
            },
        )
    }

    pub fn password_generation_settings(&self) -> PasswordGenerationSettings {
        self.read_preference(
            |settings| {
                PasswordGenerationSettings {
                    length: settings.uint("password-generator-length"),
                    min_lowercase: settings.uint("password-generator-min-lowercase"),
                    min_uppercase: settings.uint("password-generator-min-uppercase"),
                    min_numbers: settings.uint("password-generator-min-numbers"),
                    min_symbols: settings.uint("password-generator-min-symbols"),
                }
                .normalized()
            },
            |cfg| {
                cfg.password_generation
                    .clone()
                    .unwrap_or_default()
                    .normalized()
            },
        )
    }

    pub fn username_fallback_mode(&self) -> UsernameFallbackMode {
        self.read_preference(
            |settings| {
                UsernameFallbackMode::from_stored(&settings.string("username-fallback-mode"))
            },
            |cfg| cfg.username_fallback_mode.unwrap_or_default(),
        )
    }

    pub fn stores(&self) -> Vec<String> {
        self.read_preference(
            |settings| {
                settings
                    .strv("password-store-dirs")
                    .iter()
                    .map(|path| path.to_string())
                    .collect()
            },
            |cfg| {
                cfg.password_store_dirs
                    .clone()
                    .unwrap_or_else(default_store_dirs)
            },
        )
    }

    pub fn paths(&self) -> Vec<PathBuf> {
        self.store_roots().into_iter().map(PathBuf::from).collect()
    }

    pub fn set_stores(&self, stores: Vec<String>) -> Result<(), BoolError> {
        let settings_stores = stores.clone();
        self.write_preference(
            |settings| settings.set_strv("password-store-dirs", settings_stores.clone()),
            |cfg| cfg.password_store_dirs = Some(stores),
        )
    }

    pub fn set_new_pass_file_template(&self, template: &str) -> Result<(), BoolError> {
        self.write_preference(
            |settings| settings.set_string("new-pass-file-template", template),
            |cfg| cfg.new_pass_file_template = Some(template.to_string()),
        )
    }

    pub fn set_password_generation_settings(
        &self,
        settings: &PasswordGenerationSettings,
    ) -> Result<(), BoolError> {
        let settings = settings.normalized();
        let file_settings = settings.clone();
        self.write_preference(
            |gio_settings| {
                gio_settings.set_uint("password-generator-length", settings.length)?;
                gio_settings
                    .set_uint("password-generator-min-lowercase", settings.min_lowercase)?;
                gio_settings
                    .set_uint("password-generator-min-uppercase", settings.min_uppercase)?;
                gio_settings.set_uint("password-generator-min-numbers", settings.min_numbers)?;
                gio_settings.set_uint("password-generator-min-symbols", settings.min_symbols)?;
                Ok(())
            },
            |cfg| cfg.password_generation = Some(file_settings),
        )
    }

    pub fn set_username_fallback_mode(&self, mode: UsernameFallbackMode) -> Result<(), BoolError> {
        self.write_preference(
            |settings| settings.set_string("username-fallback-mode", mode.stored_value()),
            |cfg| cfg.username_fallback_mode = Some(mode),
        )
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
    use super::{default_store_dirs, Preferences, UsernameFallbackMode};
    use crate::password::generation::PasswordGenerationSettings;
    use std::time::{SystemTime, UNIX_EPOCH};

    #[test]
    fn default_store_dirs_match_build_mode() {
        #[cfg(feature = "flatpak")]
        assert!(default_store_dirs().is_empty());

        #[cfg(not(feature = "flatpak"))]
        if let Ok(home) = std::env::var("HOME") {
            assert_eq!(
                default_store_dirs(),
                vec![format!("{home}/.password-store")]
            );
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
        assert_eq!(
            BackendKind::from_stored("integrated"),
            BackendKind::Integrated
        );
        assert_eq!(BackendKind::from_stored("ripasso"), BackendKind::Integrated);
        assert_eq!(
            BackendKind::from_stored("host-command"),
            BackendKind::HostCommand
        );
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

        assert!(Preferences::store_dir_exists(
            existing.to_string_lossy().as_ref()
        ));
        assert!(!Preferences::store_dir_exists(
            existing.join("missing").to_string_lossy().as_ref()
        ));

        std::fs::remove_dir_all(&existing).expect("remove temp store dir");
    }

    #[test]
    fn password_generation_settings_default_to_a_usable_configuration() {
        let settings = Preferences::new().password_generation_settings();

        assert!(settings.length >= settings.minimum_length());
        assert!(settings.minimum_length() > 0);
    }

    #[test]
    fn password_generation_settings_normalize_disabled_classes_with_minimums() {
        let settings = PasswordGenerationSettings {
            length: 6,
            min_lowercase: 2,
            min_uppercase: 1,
            min_numbers: 5,
            min_symbols: 0,
        }
        .normalized();

        assert_eq!(settings.length, 8);
    }

    #[test]
    fn username_fallback_mode_defaults_to_folder() {
        assert_eq!(
            UsernameFallbackMode::default(),
            UsernameFallbackMode::Folder
        );
    }

    #[test]
    fn username_fallback_mode_storage_accepts_current_names() {
        assert_eq!(UsernameFallbackMode::Folder.stored_value(), "folder");
        assert_eq!(UsernameFallbackMode::Filename.stored_value(), "filename");
        assert_eq!(
            UsernameFallbackMode::from_stored("folder"),
            UsernameFallbackMode::Folder
        );
        assert_eq!(
            UsernameFallbackMode::from_stored("filename"),
            UsernameFallbackMode::Filename
        );
    }
}
