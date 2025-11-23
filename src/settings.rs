use adw::gio::{self, prelude::*, Settings};
use adw::glib::BoolError;
use std::path::{Path, PathBuf};

const APP_ID: &str = "dev.noobping.passwordstore";
const DEFAULT_CMD: &str = "pass";

#[derive(Debug, Clone)]
pub struct AppSettings {
    settings: Option<Settings>,
}

impl AppSettings {
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
            Ok(())
        }
    }

    pub fn set_stores(&self, stores: Vec<String>) -> Result<(), BoolError> {
        if let Some(s) = &self.settings {
            s.set_strv("password-store-dirs", stores)
        } else {
            Ok(())
        }
    }

    pub fn has_gsettings(&self) -> bool {
        self.settings.is_some()
    }
}
