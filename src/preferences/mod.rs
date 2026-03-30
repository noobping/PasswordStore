use crate::password::generation::PasswordGenerationSettings;
use adw::gio::{self, prelude::*, Settings};
use adw::glib::BoolError;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

mod command_backend;
#[cfg(target_os = "linux")]
mod linux;
#[cfg(not(target_os = "linux"))]
mod non_linux;
mod restricted;
mod storage;

use self::restricted::default_store_dirs;
use self::storage::{load_file_prefs, save_file_prefs, PreferenceFile};
use crate::support::runtime::supports_host_command_features;

const DEFAULT_NEW_PASS_FILE_TEMPLATE: &str = "username:\nemail:\nurl:";
const DEFAULT_WINDOW_WIDTH: i32 = 850;
const DEFAULT_WINDOW_HEIGHT: i32 = 600;
const APP_ID: &str = env!("APP_ID");

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum BackendKind {
    Integrated,
    HostCommand,
}

const fn default_backend_kind() -> BackendKind {
    BackendKind::Integrated
}

impl BackendKind {
    pub const fn stored_value(self) -> &'static str {
        match self {
            Self::Integrated => "integrated",
            Self::HostCommand => "host",
        }
    }

    pub fn from_stored(value: &str) -> Self {
        match value.trim().to_ascii_lowercase().as_str() {
            "integrated" | "ripasso" => Self::Integrated,
            "host" | "host-command" | "host command" | "pass" | "pass-command" | "pass command" => {
                Self::HostCommand
            }
            _ => default_backend_kind(),
        }
    }

    pub const fn label(self) -> &'static str {
        match self {
            Self::Integrated => "Integrated",
            Self::HostCommand => "Host",
        }
    }

    pub const fn uses_host_command(self) -> bool {
        matches!(self, Self::HostCommand)
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum UsernameFallbackMode {
    Folder,
    #[default]
    Filename,
}

impl UsernameFallbackMode {
    pub const fn stored_value(self) -> &'static str {
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

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum PasswordListSortMode {
    Filename,
    #[default]
    StorePath,
}

impl PasswordListSortMode {
    pub const fn stored_value(self) -> &'static str {
        match self {
            Self::Filename => "filename",
            Self::StorePath => "store-path",
        }
    }

    pub fn from_stored(value: &str) -> Self {
        match value.trim().to_ascii_lowercase().as_str() {
            "filename" | "file" | "name" => Self::Filename,
            "store-path" | "store" | "path" | "folder" | "folders" => Self::StorePath,
            _ => Self::default(),
        }
    }
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
        self.settings
            .as_ref()
            .map_or_else(|| read_file(&load_file_prefs()), read_settings)
    }

    fn write_preference(
        &self,
        write_settings: impl FnOnce(&Settings) -> Result<(), BoolError>,
        write_file: impl FnOnce(&mut PreferenceFile),
    ) -> Result<(), BoolError> {
        self.settings.as_ref().map_or_else(
            || {
                let mut cfg = load_file_prefs();
                write_file(&mut cfg);
                save_file_prefs(&cfg)
            },
            write_settings,
        )
    }

    fn try_settings() -> Option<Settings> {
        let source = gio::SettingsSchemaSource::default()?;
        let _schema = source.lookup(APP_ID, true)?;
        Some(Settings::new(APP_ID))
    }

    fn expand_path(s: &str) -> String {
        shellexpand::full(s).map_or_else(|_| s.to_string(), std::borrow::Cow::into_owned)
    }

    fn stored_window_dimension(value: Option<i32>, default: i32) -> i32 {
        value.filter(|value| *value > 0).unwrap_or(default)
    }

    pub fn store_roots(&self) -> Vec<String> {
        self.stores()
            .into_iter()
            .map(|store| Self::expand_path(&store))
            .collect()
    }

    pub fn window_size(&self) -> (i32, i32) {
        self.read_preference(
            |settings| {
                (
                    Self::stored_window_dimension(
                        Some(settings.int("window-width")),
                        DEFAULT_WINDOW_WIDTH,
                    ),
                    Self::stored_window_dimension(
                        Some(settings.int("window-height")),
                        DEFAULT_WINDOW_HEIGHT,
                    ),
                )
            },
            |cfg| {
                (
                    Self::stored_window_dimension(cfg.window_width, DEFAULT_WINDOW_WIDTH),
                    Self::stored_window_dimension(cfg.window_height, DEFAULT_WINDOW_HEIGHT),
                )
            },
        )
    }

    pub fn store(&self) -> String {
        self.store_roots().into_iter().next().unwrap_or_default()
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

    pub fn clear_empty_fields_before_save(&self) -> bool {
        self.read_preference(
            |settings| settings.boolean("clear-empty-fields-before-save"),
            |cfg| cfg.clear_empty_fields_before_save.unwrap_or(false),
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

    pub fn password_list_sort_mode(&self) -> PasswordListSortMode {
        self.read_preference(
            |settings| {
                PasswordListSortMode::from_stored(&settings.string("password-list-sort-mode"))
            },
            |cfg| cfg.password_list_sort_mode.unwrap_or_default(),
        )
    }

    pub fn stores(&self) -> Vec<String> {
        self.read_preference(
            |settings| {
                settings
                    .strv("password-store-dirs")
                    .iter()
                    .map(std::string::ToString::to_string)
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

    pub fn set_window_size(&self, width: i32, height: i32) -> Result<(), BoolError> {
        let width = Self::stored_window_dimension(Some(width), DEFAULT_WINDOW_WIDTH);
        let height = Self::stored_window_dimension(Some(height), DEFAULT_WINDOW_HEIGHT);
        self.write_preference(
            |settings| {
                settings.set_int("window-width", width)?;
                settings.set_int("window-height", height)?;
                Ok(())
            },
            |cfg| {
                cfg.window_width = Some(width);
                cfg.window_height = Some(height);
            },
        )
    }

    pub fn set_new_pass_file_template(&self, template: &str) -> Result<(), BoolError> {
        self.write_preference(
            |settings| settings.set_string("new-pass-file-template", template),
            |cfg| cfg.new_pass_file_template = Some(template.to_string()),
        )
    }

    pub fn set_clear_empty_fields_before_save(&self, enabled: bool) -> Result<(), BoolError> {
        self.write_preference(
            |settings| settings.set_boolean("clear-empty-fields-before-save", enabled),
            |cfg| cfg.clear_empty_fields_before_save = Some(enabled),
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

    pub fn set_password_list_sort_mode(&self, mode: PasswordListSortMode) -> Result<(), BoolError> {
        self.write_preference(
            |settings| settings.set_string("password-list-sort-mode", mode.stored_value()),
            |cfg| cfg.password_list_sort_mode = Some(mode),
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

    pub fn sync_private_keys_with_host(&self) -> bool {
        supports_host_command_features()
            && self.read_preference(
                |settings| settings.boolean("sync-private-keys-with-host"),
                |cfg| cfg.sync_private_keys_with_host.unwrap_or(false),
            )
    }

    pub fn set_sync_private_keys_with_host(&self, enabled: bool) -> Result<(), BoolError> {
        self.write_preference(
            |settings| settings.set_boolean("sync-private-keys-with-host", enabled),
            |cfg| cfg.sync_private_keys_with_host = Some(enabled),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::{
        default_backend_kind, default_store_dirs, BackendKind, PasswordListSortMode, Preferences,
        UsernameFallbackMode, DEFAULT_WINDOW_HEIGHT, DEFAULT_WINDOW_WIDTH,
    };
    use crate::password::generation::PasswordGenerationSettings;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn expected_default_store_dirs() -> Vec<String> {
        std::env::var("HOME")
            .map(|home| vec![format!("{home}/.password-store")])
            .unwrap_or_default()
    }

    #[test]
    fn default_store_dirs_match_build_mode() {
        assert_eq!(default_store_dirs(), expected_default_store_dirs());
    }

    #[test]
    fn default_backend_matches_build_mode() {
        assert_eq!(default_backend_kind(), BackendKind::Integrated);
    }

    #[test]
    fn backend_storage_accepts_current_and_legacy_names() {
        assert_eq!(BackendKind::Integrated.stored_value(), "integrated");
        assert_eq!(BackendKind::HostCommand.stored_value(), "host");
        assert_eq!(
            BackendKind::from_stored("integrated"),
            BackendKind::Integrated
        );
        assert_eq!(BackendKind::from_stored("ripasso"), BackendKind::Integrated);
        assert_eq!(BackendKind::from_stored("host"), BackendKind::HostCommand);
        assert_eq!(
            BackendKind::from_stored("host-command"),
            BackendKind::HostCommand
        );
        assert_eq!(BackendKind::from_stored("pass"), BackendKind::HostCommand);
    }

    #[test]
    fn backend_kind_capabilities_match_expected_backends() {
        assert!(!BackendKind::Integrated.uses_host_command());
        assert!(BackendKind::HostCommand.uses_host_command());
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
    fn username_fallback_mode_defaults_to_filename() {
        assert_eq!(
            UsernameFallbackMode::default(),
            UsernameFallbackMode::Filename
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

    #[test]
    fn password_list_sort_mode_defaults_to_store_path() {
        assert_eq!(
            PasswordListSortMode::default(),
            PasswordListSortMode::StorePath
        );
    }

    #[test]
    fn password_list_sort_mode_storage_accepts_current_names() {
        assert_eq!(PasswordListSortMode::Filename.stored_value(), "filename");
        assert_eq!(PasswordListSortMode::StorePath.stored_value(), "store-path");
        assert_eq!(
            PasswordListSortMode::from_stored("filename"),
            PasswordListSortMode::Filename
        );
        assert_eq!(
            PasswordListSortMode::from_stored("store-path"),
            PasswordListSortMode::StorePath
        );
    }

    #[test]
    fn password_list_sort_mode_invalid_values_fall_back_to_store_path() {
        assert_eq!(
            PasswordListSortMode::from_stored("unexpected"),
            PasswordListSortMode::StorePath
        );
    }

    #[test]
    fn private_key_sync_defaults_to_disabled() {
        assert!(!Preferences::new().sync_private_keys_with_host());
    }

    #[test]
    fn clear_empty_fields_before_save_defaults_to_disabled() {
        assert!(!Preferences::new().clear_empty_fields_before_save());
    }

    #[test]
    fn invalid_window_dimensions_fall_back_to_the_default_size() {
        assert_eq!(
            Preferences::stored_window_dimension(Some(0), DEFAULT_WINDOW_WIDTH),
            DEFAULT_WINDOW_WIDTH
        );
        assert_eq!(
            Preferences::stored_window_dimension(Some(-1), DEFAULT_WINDOW_HEIGHT),
            DEFAULT_WINDOW_HEIGHT
        );
        assert_eq!(
            Preferences::stored_window_dimension(Some(900), DEFAULT_WINDOW_WIDTH),
            900
        );
    }

    #[test]
    fn window_size_defaults_match_the_ui_defaults() {
        assert_eq!(
            Preferences::new().window_size(),
            (DEFAULT_WINDOW_WIDTH, DEFAULT_WINDOW_HEIGHT)
        );
    }
}
