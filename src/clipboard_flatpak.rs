use super::copy_password_entry_to_clipboard_via_read;
use crate::backend::preferred_ripasso_private_key_fingerprint_for_entry;
use crate::item::PassEntry;
use crate::logging::log_error;
use crate::ripasso_unlock::{is_locked_private_key_error, prompt_private_key_unlock_for_action};
use adw::gtk::Button;
use adw::ToastOverlay;
use std::rc::Rc;

pub(super) fn handle_copy_password_error(
    item: &PassEntry,
    overlay: &ToastOverlay,
    button: &Option<Button>,
    message: &str,
) -> bool {
    if !is_locked_private_key_error(message) {
        return false;
    }

    match preferred_ripasso_private_key_fingerprint_for_entry(&item.store_path, &item.label()) {
        Ok(fingerprint) => {
            let retry_overlay = overlay.clone();
            let retry_item = item.clone();
            let retry_button = button.clone();
            prompt_private_key_unlock_for_action(
                overlay,
                fingerprint,
                Rc::new(move || {
                    copy_password_entry_to_clipboard_via_read(
                        retry_item.clone(),
                        retry_overlay.clone(),
                        retry_button.clone(),
                    );
                }),
            );
            true
        }
        Err(resolve_err) => {
            log_error(format!(
                "Failed to resolve the private key for copy retry: {resolve_err}"
            ));
            false
        }
    }
}

pub(super) fn copy_password_entry_to_clipboard(
    item: PassEntry,
    overlay: ToastOverlay,
    button: Option<Button>,
) {
    copy_password_entry_to_clipboard_via_read(item, overlay, button);
}
