use adw::gio::{prelude::*, Settings};
use adw::glib::BoolError;

#[derive(Debug, Clone)]
pub struct AppSettings {
    settings: Settings,
}

impl AppSettings {
    /// Create from an existing `Settings` (you can also add a schema-based ctor below).
    pub fn new(settings: Settings) -> Self {
        Self { settings }
    }

    /// Optional convenience ctor if you want to construct the Settings here:
    pub fn with_schema(schema_id: &str) -> Self {
        let settings = Settings::new(schema_id);
        Self { settings }
    }

    pub fn command(&self) -> String {
        self.settings.string("pass-command").to_string()
    }

    pub fn stores(&self) -> Vec<String> {
        self.settings
            .strv("password-store-dirs")
            .iter()
            .map(|g| g.to_string())
            .collect()
    }

    pub fn set_command(&self, cmd: &str) -> Result<(), BoolError> {
        self.settings.set_string("pass-command", cmd)
    }

    pub fn set_stores(&self, stores: Vec<String>) -> Result<(), BoolError> {
        // `Vec<String>` implements `IntoStrV`
        self.settings.set_strv("password-store-dirs", stores)
    }

    /// If you ever need to access the raw gio::Settings.
    pub fn inner(&self) -> &Settings {
        &self.settings
    }
}
