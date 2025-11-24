use adw::gio::{self, prelude::*, Settings};
use adw::glib::BoolError;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::{env, fs};

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

    #[cfg(target_os = "linux")]
    pub fn command(&self) -> String {
        if let Some(s) = &self.settings {
            s.string("pass-command").to_string()
        } else {
            let cfg = load_file_prefs();
            cfg.pass_command.unwrap_or_else(|| DEFAULT_CMD.to_string())
        }
    }

    #[cfg(not(target_os = "linux"))]
    pub fn command(&self) -> String {
        if let Some(s) = &self.settings {
            s.string("pass-command").to_string()
        } else {
            DEFAULT_CMD.to_string()
        }
    }

    #[cfg(target_os = "linux")]
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
            } else if let Ok(home) = std::env::var("HOME") {
                vec![format!("{home}/.password-store")]
            } else {
                Vec::new()
            }
        }
    }

    #[cfg(not(target_os = "linux"))]
    pub fn stores(&self) -> Vec<String> {
        if let Some(s) = &self.settings {
            s.strv("password-store-dirs")
                .iter()
                .map(|g| g.to_string())
                .collect()
        } else if let Ok(home) = std::env::var("HOME") {
            vec![format!("{home}/.password-store")]
        } else {
            Vec::new()
        }
    }

    pub fn paths(&self) -> Vec<PathBuf> {
        self.stores().into_iter().map(PathBuf::from).collect()
    }

    #[cfg(target_os = "linux")]
    pub fn set_command(&self, cmd: &str) -> Result<(), BoolError> {
        if let Some(s) = &self.settings {
            s.set_string("pass-command", cmd)
        } else {
            let mut cfg = load_file_prefs();
            cfg.pass_command = Some(cmd.to_string());
            save_file_prefs(&cfg)
        }
    }

    #[cfg(not(target_os = "linux"))]
    pub fn set_command(&self, cmd: &str) -> Result<(), BoolError> {
        if let Some(s) = &self.settings {
            s.set_string("pass-command", cmd)
        } else {
            Err(false)
        }
    }

    #[cfg(target_os = "linux")]
    pub fn set_stores(&self, stores: Vec<String>) -> Result<(), BoolError> {
        if let Some(s) = &self.settings {
            s.set_strv("password-store-dirs", stores.clone())
        } else {
            let mut cfg = load_file_prefs();
            cfg.password_store_dirs = Some(stores);
            save_file_prefs(&cfg)
        }
    }

    #[cfg(not(target_os = "linux"))]
    pub fn set_stores(&self, stores: Vec<String>) -> Result<(), BoolError> {
        if let Some(s) = &self.settings {
            s.set_strv("password-store-dirs", stores.clone())
        } else {
            Err(false)
        }
    }

    #[cfg(target_os = "linux")]
    pub fn can_install_locally(&self, stores: Vec<String>) -> bool {
        let bin: PathBuf = local_bin_path();
        let desktop: PathBuf = local_desktop_file_path();
        !bin.exists() && !bin.is_dir() && is_writable(&bin) &&
        !desktop.exists() && !desktop.is_dir() && is_writable(&desktop)
    }

    #[cfg(not(target_os = "linux"))]
    pub fn can_install_locally(&self, stores: Vec<String>) -> Bool {
        false
    }

    pub fn has_references(&self) -> bool {
        self.settings.is_some()
    }
}

#[cfg(target_os = "linux")]
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

#[cfg(target_os = "linux")]
fn local_bin_path() -> PathBuf {
    let base = std::env::var_os("HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("."));
    base.join(".local/bin")
}

#[cfg(target_os = "linux")]
fn local_desktop_file_path() -> PathBuf {
    let base = std::env::var_os("HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("."));
    base.join(".local/share/applications")
}

#[cfg(target_os = "linux")]
fn load_file_prefs() -> PreferenceFile {
    let path = config_path();

    if let Ok(data) = fs::read_to_string(&path) {
        toml::from_str(&data).unwrap_or_default()
    } else {
        PreferenceFile::default()
    }
}

#[cfg(target_os = "linux")]
fn save_file_prefs(cfg: &PreferenceFile) -> Result<(), BoolError> {
    use adw::glib::bool_error; // macro to construct BoolError

    let path = config_path();

    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|e| bool_error!("Failed to create config dir: {e}"))?;
    }

    let toml =
        toml::to_string_pretty(cfg).map_err(|e| bool_error!("Failed to serialize config: {e}"))?;

    fs::write(&path, toml).map_err(|e| bool_error!("Failed to write config file: {e}"))?;

    Ok(())
}

#[cfg(target_os = "linux")]
fn rename_application() -> std::io::Result<()> {
    let project = env!("CARGO_PKG_NAME");
    let exe = env::current_exe()?;
    let new = exe.with_file_name(project);
    fs::rename(&exe, &new)?;

    Ok(())
}

#[cfg(target_os = "linux")]
fn is_writable(dir: &Path) -> bool {
    // Try to open a temp file for writing
    let test_path = dir.join(".perm_test");
    match std::fs::OpenOptions::new()
        .create(true)
        .write(true)
        .open(&test_path)
    {
        Ok(_) => {
            let _ = std::fs::remove_file(test_path);
            true
        }
        Err(_) => false,
    }
}
