use crate::backend::{
    ripasso_private_key_title, unlock_ripasso_private_key_for_session, ManagedRipassoPrivateKey,
    PrivateKeyError,
};
use crate::logging::log_error;
use crate::private_key::dialog::{
    build_private_key_progress_dialog, present_private_key_password_dialog,
};
use crate::support::actions::activate_widget_action;
use crate::support::background::spawn_result_task;
use adw::{prelude::*, ApplicationWindow, Toast, ToastOverlay};
use std::rc::Rc;

fn toast_overlay_window(overlay: &ToastOverlay) -> Option<ApplicationWindow> {
    overlay
        .root()
        .and_then(|root| root.downcast::<ApplicationWindow>().ok())
}

fn show_unlock_failure_toast(overlay: &ToastOverlay) {
    overlay.add_toast(Toast::new("Couldn't unlock the key."));
}

fn start_private_key_unlock_for_action(
    window: &ApplicationWindow,
    overlay: &ToastOverlay,
    fingerprint: String,
    key_title: Option<String>,
    passphrase: String,
    after_unlock: Rc<dyn Fn()>,
) {
    let progress_dialog = build_private_key_progress_dialog(
        window,
        "Unlocking key",
        key_title.as_deref(),
        "Please wait.",
    );
    let overlay = overlay.clone();
    let progress_dialog_for_disconnect = progress_dialog.clone();
    let overlay_for_disconnect = overlay.clone();
    let window_for_result = window.clone();
    let after_unlock = after_unlock.clone();
    spawn_result_task(
        move || unlock_ripasso_private_key_for_session(&fingerprint, &passphrase),
        move |result: Result<ManagedRipassoPrivateKey, PrivateKeyError>| match result {
            Ok(_) => {
                progress_dialog.force_close();
                after_unlock();
                activate_widget_action(&window_for_result, "win.reload-password-list");
                overlay.add_toast(Toast::new("Key unlocked."));
            }
            Err(err) => {
                progress_dialog.force_close();
                log_error(format!("Failed to unlock ripasso private key: {err}"));
                overlay.add_toast(Toast::new(err.unlock_message()));
            }
        },
        move || {
            progress_dialog_for_disconnect.force_close();
            log_error("Private key unlock worker disconnected unexpectedly.".to_string());
            show_unlock_failure_toast(&overlay_for_disconnect);
        },
    );
}

pub(crate) fn prompt_private_key_unlock_for_action(
    overlay: &ToastOverlay,
    fingerprint: String,
    after_unlock: Rc<dyn Fn()>,
) {
    let Some(window) = toast_overlay_window(overlay) else {
        log_error(
            "Couldn't find the application window for the private key unlock dialog.".to_string(),
        );
        show_unlock_failure_toast(overlay);
        return;
    };
    let key_title = match ripasso_private_key_title(&fingerprint) {
        Ok(title) => Some(title),
        Err(err) => {
            log_error(format!(
                "Failed to read private key title for '{fingerprint}': {err}"
            ));
            None
        }
    };

    let window_for_submit = window.clone();
    let overlay_for_submit = overlay.clone();
    let title_for_submit = key_title.clone();
    present_private_key_password_dialog(
        &window,
        overlay,
        "Unlock key",
        key_title.as_deref(),
        move |passphrase| {
            start_private_key_unlock_for_action(
                &window_for_submit,
                &overlay_for_submit,
                fingerprint.clone(),
                title_for_submit.clone(),
                passphrase,
                after_unlock.clone(),
            );
        },
    );
}
