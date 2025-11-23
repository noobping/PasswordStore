use adw::gio::{self, prelude::*, Settings};
use adw::glib::BoolError;
use std::path::PathBuf;

const APP_ID: &str = "dev.noobping.passwordstore";
const DEFAULT_CMD: &str = "pass";

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
        let schema = source.lookup(APP_ID, true)?;
        Some(Settings::new_full(&schema, None, None))
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
                let home = std::env::var("HOME").unwrap_or(String::new());
                let mut stores: Vec<PathBuf> = Vec::new();
                stores.push(PathBuf::from(format!("{}/.password-store", home)))
            }
        }
    }

    pub fn set_command(&self, cmd: &str) -> Result<(), BoolError> {
        if let Some(s) = &self.settings {
            s.set_string("pass-command", cmd)
        } else {
            // no schema → just pretend it worked
            Ok(())
        }
    }

    pub fn set_stores(&self, stores: Vec<String>) -> Result<(), BoolError> {
        if let Some(s) = &self.settings {
            s.set_strv("password-store-dirs", stores)
        } else {
            // same: no-op in fallback mode
            Ok(())
        }
    }

    pub fn has_gsettings(&self) -> bool {
        self.settings.is_some()
    }
}
