use adw::gio::{self, prelude::*, Settings};
use adw::glib::{bool_error, BoolError};
use serde::{Deserialize, Serialize};
use shlex;
use std::path::PathBuf;
use std::process::Command;
use std::{env, fs};

use crate::config::APP_ID;

#[cfg(not(feature = "flatpak"))]
const DEFAULT_CMD: &str = "pass";
#[cfg(feature = "flatpak")]
const DEFAULT_CMD: &str = "flatpak-spawn --host pass";

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

    #[cfg(not(feature = "flatpak"))]
    pub fn command_value(&self) -> String {
        if let Some(s) = &self.settings {
            s.string("pass-command").to_string()
        } else {
            let cfg = load_file_prefs();
            cfg.pass_command.unwrap_or_else(|| DEFAULT_CMD.to_string())
        }
    }

    #[cfg(feature = "flatpak")]
    pub fn command_value(&self) -> String { DEFAULT_CMD.into() }

    pub fn command(&self) -> Command {
        let (program, args) = self.command_parts();
        let mut cmd = Command::new(program);
        cmd.args(args);

        if let Ok(appdir) = env::var("APPDIR") {
            cmd.env(
                "PATH",
                format!("{appdir}/usr/bin:{}", env::var("PATH").unwrap_or_default()),
            );
            cmd.env(
                "LD_LIBRARY_PATH",
                format!("{appdir}/usr/lib/x86_64-linux-gnu:{appdir}/usr/lib"),
            );
            cmd.env("PASSWORD_STORE_ENABLE_EXTENSIONS", "true");
            cmd.env(
                "PASSWORD_STORE_EXTENSIONS_DIR",
                format!("{appdir}/usr/lib/password-store/extensions"),
            );
        }

        cmd
    }

    #[cfg(not(feature = "flatpak"))]
    pub fn git_command(&self) -> Command {
        Command::new("git")
    }

    fn command_parts(&self) -> (String, Vec<String>) {
        let cmdline = self.command_value();
        // Try to split like a shell would
        if let Some(mut parts) = shlex::split(&cmdline) {
            if parts.is_empty() {
                return ("pass".to_string(), Vec::new()); // sane fallback
            }
            let program = parts.remove(0);
            (program, parts)
        } else {
            // If shlex fails, fallback to using whole string as program
            (cmdline, Vec::new())
        }
    }

    fn expand_path(s: &str) -> String {
        shellexpand::full(s)
            .map(|c| c.into_owned())
            .unwrap_or_else(|_| s.to_string())
    }

    pub fn store(&self) -> String {
        Self::expand_path(&self.stores().into_iter().next().unwrap_or_default())
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
        self.stores().into_iter().map(PathBuf::from).collect()
    }

    #[cfg(not(feature = "flatpak"))]
    pub fn set_command(&self, cmd: &str) -> Result<(), BoolError> {
        if let Some(s) = &self.settings {
            s.set_string("pass-command", cmd)
        } else {
            let mut cfg = load_file_prefs();
            cfg.pass_command = Some(cmd.to_string());
            save_file_prefs(&cfg)
        }
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
    if let Some(dir) = dirs::preference_dir() {
        dir.join(format!("{}.toml", env!("CARGO_PKG_NAME")))
    } else {
        PathBuf::from(format!("{}.toml", env!("CARGO_PKG_NAME")))
    }
}

#[cfg(not(feature = "flatpak"))]
fn default_store_dirs() -> Vec<String> {
    if let Ok(home) = std::env::var("HOME") {
        vec![format!("{home}/.password-store")]
    } else {
        Vec::new()
    }
}

#[cfg(feature = "flatpak")]
fn default_store_dirs() -> Vec<String> {
    Vec::new()
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
