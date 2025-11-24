use adw::gio::{self, prelude::*, Settings};
use adw::glib::{bool_error, BoolError};
use serde::{Deserialize, Serialize};
use std::os::unix::fs::PermissionsExt;
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
    pub fn can_install_locally() -> bool {
        let bin: PathBuf = local_bin_path();
        let desktop: PathBuf = local_applications_path();
        bin.exists()
            && bin.is_dir()
            && is_writable(&bin)
            && desktop.exists()
            && desktop.is_dir()
            && is_writable(&desktop)
    }

    #[cfg(not(target_os = "linux"))]
    pub fn can_install_locally() -> Bool {
        false
    }

    #[cfg(target_os = "linux")]
    pub fn is_installed_locally() -> bool {
        let bin: PathBuf = bin_file_path();
        let desktop: PathBuf = desktop_file_path();
        bin.exists() && bin.is_file() && desktop.exists() && desktop.is_file()
    }

    #[cfg(not(target_os = "linux"))]
    pub fn is_installed_locally() -> Bool {
        false
    }

    #[cfg(target_os = "linux")]
    pub fn install_locally() -> std::io::Result<()> {
        let project = env!("CARGO_PKG_NAME");
        let exe_path = std::env::current_exe()?;
        let bin_dir = local_bin_path();
        let app_dir = local_applications_path();
        let dest = bin_dir.join(project);

        std::fs::create_dir_all(&bin_dir)?; // Create ~/.local/bin if missing
        std::fs::create_dir_all(&app_dir)?;
        std::fs::copy(&exe_path, &dest)?; // Copy the current binary to ~/.local/bin/<appname>

        // Ensure it's executable
        let mut perms = std::fs::metadata(&dest)?.permissions();
        perms.set_mode(0o755);
        std::fs::set_permissions(&dest, perms)?;

        write_desktop_file(&dest)?;

        Ok(())
    }

    #[cfg(not(target_os = "linux"))]
    pub fn install_locally() -> std::io::Result<()> {
        Err(())
    }

    #[cfg(target_os = "linux")]
    pub fn uninstall_locally() -> std::io::Result<()> {
        let bin: PathBuf = bin_file_path();
        let desktop: PathBuf = desktop_file_path();
        if bin.exists() {
            fs::remove_file(bin)?;
        }
        if desktop.exists() {
            fs::remove_file(desktop)?;
        }
        Ok(())
    }

    #[cfg(not(target_os = "linux"))]
    pub fn uninstall_locally() -> std::io::Result<()> {
        Ok(())
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
    } else if let Some(home) = std::env::var_os("HOME") {
        PathBuf::from(home).join(format!(".{}.toml", env!("CARGO_PKG_NAME")))
    } else {
        PathBuf::from(format!("{}.toml", env!("CARGO_PKG_NAME")))
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
fn local_applications_path() -> PathBuf {
    let base = std::env::var_os("HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("."));
    base.join(".local/share/applications")
}

#[cfg(target_os = "linux")]
fn home_path() -> PathBuf {
    std::env::var_os("HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("."))
}

#[cfg(target_os = "linux")]
fn desktop_file_path() -> PathBuf {
    local_applications_path().join(format!("{}.desktop", APP_ID))
}

#[cfg(target_os = "linux")]
fn bin_file_path() -> PathBuf {
    local_bin_path().join(env!("CARGO_PKG_NAME"))
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

#[cfg(target_os = "linux")]
fn write_desktop_file(exe_path: &Path) -> std::io::Result<()> {
    let project = env!("CARGO_PKG_NAME");
    let desktop_path = desktop_file_path();

    // You can tweak these as you like
    let version = env!("CARGO_PKG_VERSION");
    let comment = option_env!("CARGO_PKG_DESCRIPTION").unwrap_or("Password manager");
    let exec = exe_path.display(); // absolute path to the installed binary

    let contents = format!(
        "[Desktop Entry]
Type=Application
Version={version}
Name={project}
Comment={comment}
Exec={exec} %u
Icon={project}
Terminal=false
Categories=Utility;
",
    );

    fs::write(&desktop_path, contents)?;

    // Make sure it's readable by the user (and others) – 0644
    let mut perms = fs::metadata(&desktop_path)?.permissions();
    perms.set_mode(0o644);
    fs::set_permissions(&desktop_path, perms)?;

    Ok(())
}
