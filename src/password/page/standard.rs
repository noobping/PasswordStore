use super::{open_password_entry_page, PasswordPageState};
use crate::backend::PasswordEntryError;
use crate::logging::log_error;
use crate::password::model::OpenPassFile;
use crate::preferences::{BackendKind, Preferences};
use adw::Toast;

const fn should_switch_to_integrated_backend(
    uses_integrated_backend: bool,
    error: &PasswordEntryError,
) -> bool {
    !uses_integrated_backend && !matches!(error, PasswordEntryError::EntryNotFound(_))
}

pub(super) fn handle_open_password_entry_error(
    state: &PasswordPageState,
    pass_file: &OpenPassFile,
    error: &PasswordEntryError,
) -> bool {
    let settings = Preferences::new();
    if !should_switch_to_integrated_backend(settings.uses_integrated_backend(), error) {
        return false;
    }

    if let Err(err) = settings.set_backend_kind(BackendKind::Integrated) {
        log_error(format!(
            "Failed to switch to the integrated backend: {}",
            err.message
        ));
        return false;
    }

    state
        .overlay
        .add_toast(Toast::new("Using Integrated instead."));
    open_password_entry_page(state, pass_file.clone(), false);
    true
}

#[cfg(test)]
mod tests {
    use super::should_switch_to_integrated_backend;
    use crate::backend::PasswordEntryError;

    #[test]
    fn only_non_integrated_backends_retry_with_integrated_mode() {
        assert!(should_switch_to_integrated_backend(
            false,
            &PasswordEntryError::other("failure")
        ));
        assert!(!should_switch_to_integrated_backend(
            true,
            &PasswordEntryError::other("failure")
        ));
    }

    #[test]
    fn missing_entries_do_not_trigger_a_backend_switch() {
        assert!(!should_switch_to_integrated_backend(
            false,
            &PasswordEntryError::from_store_message("item was not found")
        ));
    }
}
