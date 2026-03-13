use super::state::show_password_status_message;
use super::{
    open_password_entry_page, save_current_password_entry_without_git_unlock_prompt, standard,
    PasswordPageState,
};
use crate::backend::{preferred_ripasso_private_key_fingerprint_for_entry, PasswordEntryError};
use crate::logging::log_error;
use crate::password::model::OpenPassFile;
use crate::preferences::Preferences;
use crate::private_key::git::prompt_private_key_unlock_for_entry_git_commit_if_needed;
use crate::private_key::unlock::prompt_private_key_unlock_for_action;
use crate::support::actions::activate_widget_action;
use std::rc::Rc;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum OpenPasswordErrorAction {
    PromptUnlock,
    OpenPreferences,
    None,
}

fn open_password_error_action(error: &PasswordEntryError) -> OpenPasswordErrorAction {
    if matches!(error, PasswordEntryError::LockedPrivateKey(_)) {
        return OpenPasswordErrorAction::PromptUnlock;
    }

    if matches!(error, PasswordEntryError::MissingPrivateKey(_)) {
        return OpenPasswordErrorAction::OpenPreferences;
    }

    OpenPasswordErrorAction::None
}

pub(super) fn handle_open_password_entry_error(
    state: &PasswordPageState,
    pass_file: &OpenPassFile,
    error: &PasswordEntryError,
) -> bool {
    if !Preferences::new().uses_integrated_backend() {
        return standard::handle_open_password_entry_error(state, pass_file, error);
    }

    if open_password_error_action(error) == OpenPasswordErrorAction::PromptUnlock {
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

    if open_password_error_action(error) == OpenPasswordErrorAction::OpenPreferences {
        activate_widget_action(&state.nav, "win.open-preferences");
    }

    false
}

pub(super) fn prompt_unlock_for_git_commit_if_needed(
    state: &PasswordPageState,
    pass_file: &OpenPassFile,
) -> bool {
    if !Preferences::new().uses_integrated_backend() {
        return false;
    }

    let retry_state = state.clone();
    prompt_private_key_unlock_for_entry_git_commit_if_needed(
        &state.overlay,
        pass_file.store_path(),
        &pass_file.label(),
        Rc::new(move || save_current_password_entry_without_git_unlock_prompt(&retry_state)),
    )
}

#[cfg(test)]
mod tests {
    use super::{open_password_error_action, OpenPasswordErrorAction};
    use crate::backend::PasswordEntryError;

    #[test]
    fn open_password_error_action_matches_supported_private_key_flows() {
        assert_eq!(
            open_password_error_action(&PasswordEntryError::locked_private_key("locked")),
            OpenPasswordErrorAction::PromptUnlock
        );
        assert_eq!(
            open_password_error_action(&PasswordEntryError::missing_private_key("missing")),
            OpenPasswordErrorAction::OpenPreferences
        );
    }

    #[test]
    fn open_password_error_action_ignores_other_failures() {
        assert_eq!(
            open_password_error_action(&PasswordEntryError::incompatible_private_key(
                "incompatible"
            )),
            OpenPasswordErrorAction::None
        );
        assert_eq!(
            open_password_error_action(&PasswordEntryError::other("other")),
            OpenPasswordErrorAction::None
        );
    }
}
