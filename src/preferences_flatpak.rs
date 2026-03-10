use super::{Preferences, load_file_prefs, save_file_prefs};
use adw::gio::prelude::*;
use adw::glib::BoolError;

impl Preferences {
    pub fn ripasso_own_fingerprint(&self) -> Option<String> {
        let value = if let Some(s) = &self.settings {
            s.string("ripasso-own-fingerprint").to_string()
        } else {
            let cfg = load_file_prefs();
            cfg.ripasso_own_fingerprint.unwrap_or_default()
        };

        let trimmed = value.trim();
        if trimmed.is_empty() {
            None
        } else {
            Some(trimmed.to_string())
        }
    }

    pub fn set_ripasso_own_fingerprint(
        &self,
        fingerprint: Option<&str>,
    ) -> Result<(), BoolError> {
        let value = fingerprint.unwrap_or("").trim().to_string();
        if let Some(s) = &self.settings {
            s.set_string("ripasso-own-fingerprint", &value)
        } else {
            let mut cfg = load_file_prefs();
            cfg.ripasso_own_fingerprint = if value.is_empty() { None } else { Some(value) };
            save_file_prefs(&cfg)
        }
    }
}

pub(super) fn default_store_dirs() -> Vec<String> {
    Vec::new()
}
