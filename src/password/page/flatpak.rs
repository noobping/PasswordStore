use super::state::show_password_status_message;
use super::{open_password_entry_page, PasswordPageState};
use crate::backend::{preferred_ripasso_private_key_fingerprint_for_entry, PasswordEntryError};
use crate::logging::log_error;
use crate::password::model::OpenPassFile;
use crate::private_key::unlock::prompt_private_key_unlock_for_action;
use crate::support::actions::activate_widget_action;
use std::rc::Rc;

pub(super) fn handle_open_password_entry_error(
    state: &PasswordPageState,
    pass_file: &OpenPassFile,
    error: &PasswordEntryError,
) -> bool {
    if matches!(error, PasswordEntryError::LockedPrivateKey(_)) {
        show_password_status_message(state, "Unlock key", "Enter your key password to continue.");
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

    if matches!(error, PasswordEntryError::MissingPrivateKey(_)) {
        activate_widget_action(&state.nav, "win.open-preferences");
    }

    false
}
