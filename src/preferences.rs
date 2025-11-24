use adw::gio::{self, prelude::*, ResourceLookupFlags, Settings};
use adw::glib::{bool_error, BoolError};
use serde::{Deserialize, Serialize};
use std::io::{Error, ErrorKind};
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::{env, fs};

const APP_ID: &str = "dev.noobping.passwordstore";
const RESOURCE_ID: &str = "/dev/noobping/passwordstore";
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
        if let Some(s) = &self.settings {
            s.string("pass-command").to_string()
        } else {
            let cfg = load_file_prefs();
            cfg.pass_command.unwrap_or_else(|| DEFAULT_CMD.to_string())
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
            } else if let Ok(home) = std::env::var("HOME") {
                vec![format!("{home}/.password-store")]
            } else {
                Vec::new()
            }
        }
    }

    pub fn paths(&self) -> Vec<PathBuf> {
        self.stores().into_iter().map(PathBuf::from).collect()
    }

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

    pub fn can_install_locally() -> bool {
        let Some(bin) = dirs::executable_dir() else {
            return false;
        };
        let Some(data) = dirs::data_dir() else {
            return false;
        };
        let apps = data.join("applications");
        bin.exists()
            && bin.is_dir()
            && is_writable(&bin)
            && apps.exists()
            && apps.is_dir()
            && is_writable(&apps)
    }

    pub fn is_installed_locally() -> bool {
        let Some(bin) = dirs::executable_dir() else {
            return false;
        };
        let Some(data) = dirs::data_dir() else {
            return false;
        };
        let bin = bin.join(env!("CARGO_PKG_NAME"));
        let desktop = data
            .join("applications")
            .join(format!("{}.desktop", APP_ID));
        bin.exists() && bin.is_file() && desktop.exists() && desktop.is_file()
    }

    pub fn install_locally() -> std::io::Result<()> {
        let project = env!("CARGO_PKG_NAME");
        let exe_path = std::env::current_exe()?;
        let Some(bin) = dirs::executable_dir() else {
            return Err(Error::new(
                ErrorKind::NotFound,
                "No executable directory found",
            ));
        };
        let Some(data) = dirs::data_dir() else {
            return Err(Error::new(ErrorKind::NotFound, "No data directory found"));
        };
        let apps = data.join("applications");
        let icons = data
            .join("icons")
            .join("hicolor")
            .join("256x256")
            .join("apps");
        let dest = bin.join(project);

        std::fs::create_dir_all(&bin)?;
        std::fs::create_dir_all(&apps)?;
        std::fs::create_dir_all(&icons)?;
        std::fs::copy(&exe_path, &dest)?; // Copy the current binary to ~/.local/bin/<appname>

        // Ensure it's executable
        let mut perms = std::fs::metadata(&dest)?.permissions();
        perms.set_mode(0o755);
        std::fs::set_permissions(&dest, perms)?;

        write_desktop_file(&dest)?;
        extract_icon(&icons)?;

        Ok(())
    }

    pub fn uninstall_locally() -> std::io::Result<()> {
        let Some(bin) = dirs::executable_dir() else {
            return Err(Error::new(
                ErrorKind::NotFound,
                "No executable directory found",
            ));
        };
        let Some(data) = dirs::data_dir() else {
            return Err(Error::new(ErrorKind::NotFound, "No data directory found"));
        };
        let bin = bin.join(env!("CARGO_PKG_NAME"));
        let icon = data
            .join("icons")
            .join("hicolor")
            .join("256x256")
            .join("apps")
            .join(format!("{}.svg", env!("CARGO_PKG_NAME")));
        let desktop = data
            .join("applications")
            .join(format!("{}.desktop", APP_ID));
        if bin.exists() {
            fs::remove_file(bin)?;
        }
        if desktop.exists() {
            fs::remove_file(desktop)?;
        }
        if icon.exists() {
            fs::remove_file(icon)?;
        }
        Ok(())
    }

    pub fn has_references(&self) -> bool {
        self.settings.is_some()
    }
}

fn config_path() -> PathBuf {
    if let Some(dir) = dirs::preference_dir() {
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

fn write_desktop_file(exe_path: &Path) -> std::io::Result<()> {
    let project = env!("CARGO_PKG_NAME");
    let Some(data) = dirs::data_dir() else {
        return Err(Error::new(ErrorKind::NotFound, "No data directory found"));
    };
    let desktop = data
        .join("applications")
        .join(format!("{}.desktop", APP_ID));

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

    fs::write(&desktop, contents)?;

    // Make sure it's readable by the user (and others) – 0644
    let mut perms = fs::metadata(&desktop)?.permissions();
    perms.set_mode(0o644);
    fs::set_permissions(&desktop, perms)?;

    Ok(())
}

fn extract_icon(data: &Path) -> std::io::Result<()> {
    let project = env!("CARGO_PKG_NAME");
    let resource_path = format!("{}/icons/{project}.svg", RESOURCE_ID);
    let bytes = gio::resources_lookup_data(&resource_path, ResourceLookupFlags::NONE)
        .map_err(|e| Error::new(ErrorKind::NotFound, format!("Resource not found: {e}")))?;
    let out_path = data.join(format!("{project}.svg"));
    std::fs::write(&out_path, bytes.as_ref())?;
    Ok(())
}
