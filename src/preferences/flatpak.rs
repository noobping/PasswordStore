use super::Preferences;
use adw::gio::prelude::*;
use adw::glib::BoolError;

impl Preferences {
    pub fn ripasso_own_fingerprint(&self) -> Option<String> {
        let value = self.read_preference(
            |settings| settings.string("ripasso-own-fingerprint").to_string(),
            |cfg| cfg.ripasso_own_fingerprint.clone().unwrap_or_default(),
        );

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
        let settings_value = value.clone();
        self.write_preference(
            |settings| settings.set_string("ripasso-own-fingerprint", &settings_value),
            |cfg| {
                cfg.ripasso_own_fingerprint = if value.is_empty() { None } else { Some(value) };
            },
        )
    }
}

pub(super) fn default_store_dirs() -> Vec<String> {
    Vec::new()
}
