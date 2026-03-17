use super::StoreRecipientsPageState;
use crate::backend::armored_ripasso_private_key;
use crate::clipboard::set_clipboard_text;
use crate::logging::log_error;
use adw::gtk::Button;
use adw::{Toast, ToastOverlay};

fn copy_private_key_to_clipboard(
    overlay: &ToastOverlay,
    fingerprint: &str,
    button: Option<&Button>,
) -> Result<(), String> {
    let armored = armored_ripasso_private_key(fingerprint)?;
    set_clipboard_text(&armored, overlay, button);
    Ok(())
}

pub(super) fn copy_armored_private_key(
    state: &StoreRecipientsPageState,
    fingerprint: &str,
    button: Option<&Button>,
) {
    if let Err(err) = copy_private_key_to_clipboard(&state.platform.overlay, fingerprint, button) {
        log_error(format!(
            "Failed to copy armored private key '{fingerprint}': {err}"
        ));
        state
            .platform
            .overlay
            .add_toast(Toast::new("Couldn't copy that key."));
    }
}
