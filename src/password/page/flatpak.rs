use super::{open_password_entry_page, PasswordPageState};
use crate::backend::preferred_ripasso_private_key_fingerprint_for_entry;
use crate::password::model::OpenPassFile;
use crate::logging::log_error;
use crate::private_key::unlock::{is_locked_private_key_error, prompt_private_key_unlock_for_action};
use std::rc::Rc;

pub(super) fn friendly_password_entry_error_message(message: &str) -> Option<&'static str> {
    if message.contains("cannot decrypt password store entries")
        || message.contains("available private keys cannot decrypt")
    {
        Some("This key can't open your items.")
    } else if message.contains("Import a private key in Preferences") {
        Some("Add a private key in Preferences.")
    } else {
        None
    }
}

pub(super) fn handle_open_password_entry_error(
    state: &PasswordPageState,
    pass_file: &OpenPassFile,
    message: &str,
) -> bool {
    if is_locked_private_key_error(message) {
        state.status.set_title("Unlock key");
        state
            .status
            .set_description(Some("Enter your key password to continue."));
        match preferred_ripasso_private_key_fingerprint_for_entry(
            pass_file.store_path(),
            &pass_file.label(),
        ) {
            Ok(fingerprint) => {
                let retry_pass_file = pass_file.clone();
                let retry_page_state = state.clone();
                prompt_private_key_unlock_for_action(
                    &state.overlay,
                    fingerprint,
                    Rc::new(move || {
                        open_password_entry_page(&retry_page_state, retry_pass_file.clone(), false);
                    }),
                );
                return true;
            }
            Err(err) => {
                log_error(format!(
                    "Failed to resolve the private key for this item: {err}"
                ));
            }
        }
    }

    if message.contains("Import a private key in Preferences") {
        let _ =
            adw::prelude::WidgetExt::activate_action(&state.nav, "win.open-preferences", None);
    }

    false
}
