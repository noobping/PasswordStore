use super::{open_password_entry_page, PasswordPageState};
use crate::backend::PasswordEntryError;
use crate::logging::log_error;
use crate::password::model::OpenPassFile;
use crate::preferences::{BackendKind, Preferences};
use adw::Toast;

pub(super) fn handle_open_password_entry_error(
    state: &PasswordPageState,
    pass_file: &OpenPassFile,
    error: &PasswordEntryError,
) -> bool {
    let settings = Preferences::new();
    if settings.uses_integrated_backend() || matches!(error, PasswordEntryError::EntryNotFound(_)) {
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
