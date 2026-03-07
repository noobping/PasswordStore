use crate::backend::{
    unlock_ripasso_private_key_for_session, ManagedRipassoPrivateKey,
};
use crate::logging::log_error;
use crate::private_key_dialog::{
    build_private_key_progress_dialog, present_private_key_password_dialog,
};
use adw::{glib, prelude::*, ApplicationWindow, Toast, ToastOverlay};
use std::rc::Rc;
use std::sync::mpsc;
use std::sync::mpsc::TryRecvError;
use std::thread;
use std::time::Duration;

pub(crate) fn is_locked_private_key_error(message: &str) -> bool {
    message.contains("The selected private key is locked.")
}

fn toast_overlay_window(overlay: &ToastOverlay) -> Option<ApplicationWindow> {
    overlay
        .root()
        .and_then(|root| root.downcast::<ApplicationWindow>().ok())
}

fn start_private_key_unlock_for_action(
    window: &ApplicationWindow,
    overlay: &ToastOverlay,
    fingerprint: String,
    passphrase: String,
    after_unlock: Rc<dyn Fn()>,
) {
    let progress_dialog = build_private_key_progress_dialog(
        window,
        "Unlocking Private Key",
        "Please wait while ripasso unlocks the private key for this session.",
    );
    let (tx, rx) = mpsc::channel::<Result<ManagedRipassoPrivateKey, String>>();
    let fingerprint_for_thread = fingerprint.clone();
    thread::spawn(move || {
        let result = unlock_ripasso_private_key_for_session(&fingerprint_for_thread, &passphrase);
        let _ = tx.send(result);
    });

    let overlay = overlay.clone();
    let after_unlock = after_unlock.clone();
    glib::timeout_add_local(Duration::from_millis(50), move || match rx.try_recv() {
        Ok(Ok(_)) => {
            progress_dialog.force_close();
            after_unlock();
            overlay.add_toast(Toast::new("Private key unlocked for this session."));
            glib::ControlFlow::Break
        }
        Ok(Err(err)) => {
            progress_dialog.force_close();
            log_error(format!("Failed to unlock ripasso private key: {err}"));
            let message = if err.contains("incorrect") {
                "Couldn't unlock that private key."
            } else if err.contains("cannot decrypt password store entries") {
                "That private key cannot decrypt password entries."
            } else {
                "Couldn't unlock that private key."
            };
            overlay.add_toast(Toast::new(message));
            glib::ControlFlow::Break
        }
        Err(TryRecvError::Empty) => glib::ControlFlow::Continue,
        Err(TryRecvError::Disconnected) => {
            progress_dialog.force_close();
            log_error("Private key unlock worker disconnected unexpectedly.".to_string());
            overlay.add_toast(Toast::new("Couldn't unlock that private key."));
            glib::ControlFlow::Break
        }
    });
}

pub(crate) fn prompt_private_key_unlock_for_action(
    overlay: &ToastOverlay,
    fingerprint: String,
    after_unlock: Rc<dyn Fn()>,
) {
    let Some(window) = toast_overlay_window(overlay) else {
        log_error("Couldn't find the application window for the private key unlock dialog.".to_string());
        overlay.add_toast(Toast::new("Couldn't unlock that private key."));
        return;
    };

    let window_for_submit = window.clone();
    let overlay_for_submit = overlay.clone();
    present_private_key_password_dialog(
        &window,
        overlay,
        "Unlock Private Key",
        move |passphrase| {
            start_private_key_unlock_for_action(
                &window_for_submit,
                &overlay_for_submit,
                fingerprint.clone(),
                passphrase,
                after_unlock.clone(),
            );
        },
    );
}

#[cfg(test)]
mod tests {
    use super::is_locked_private_key_error;

    #[test]
    fn locked_private_key_errors_are_detected() {
        assert!(is_locked_private_key_error(
            "The selected private key is locked. Unlock it in Preferences and enter its password."
        ));
    }
}
