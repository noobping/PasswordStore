use adw::gio::{prelude::*, Settings};
use adw::glib::BoolError;

pub fn get_command(settings: &Settings) -> String {
    settings.string("pass-command").to_string()
}

pub fn get_stores(settings: &Settings) -> Vec<String> {
    settings
        .strv("password-store-dirs")
        .iter()
        .map(|g| g.to_string())
        .collect()
}

pub fn set_command(settings: &Settings, cmd: String) -> Result<(), BoolError> {
    settings.set_string("pass-command", &cmd)
}

pub fn set_stores(settings: &Settings, stores: Vec<String>) -> Result<(), BoolError> {
    settings.set_strv("password-store-dirs", stores)
}
