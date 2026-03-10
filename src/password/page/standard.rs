use super::{open_password_entry_page, PasswordPageState};
use crate::password::model::OpenPassFile;
use crate::logging::log_error;
use crate::preferences::{BackendKind, Preferences};
use adw::Toast;

fn should_switch_to_integrated_backend(message: &str) -> bool {
    let lowered = message.to_ascii_lowercase();
    !lowered.contains("not in the password store")
        && !lowered.contains("was not found")
        && !lowered.contains("no such file or directory")
}

pub(super) fn friendly_password_entry_error_message(_message: &str) -> Option<&'static str> {
    None
}

pub(super) fn handle_open_password_entry_error(
    state: &PasswordPageState,
    pass_file: &OpenPassFile,
    message: &str,
) -> bool {
    let settings = Preferences::new();
    if settings.uses_integrated_backend() || !should_switch_to_integrated_backend(message) {
        return false;
    }

    if let Err(err) = settings.set_backend_kind(BackendKind::Integrated) {
        log_error(format!("Failed to switch to the integrated backend: {}", err.message));
        return false;
    }

    state.overlay.add_toast(Toast::new("Using Integrated instead."));
    open_password_entry_page(state, pass_file.clone(), false);
    true
}
