use crate::backend::{
    list_ripasso_private_keys, ripasso_private_key_title, unlock_ripasso_private_key_for_session,
    ManagedRipassoPrivateKey, ManagedRipassoPrivateKeyProtection, PrivateKeyError,
    PrivateKeyUnlockRequest,
};
use crate::logging::log_error;
use crate::private_key::dialog::present_private_key_unlock_dialog_with_close_handler;
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

fn private_key_unlock_protection(fingerprint: &str) -> ManagedRipassoPrivateKeyProtection {
    match list_ripasso_private_keys() {
        Ok(keys) => keys
            .into_iter()
            .find(|key| key.fingerprint.eq_ignore_ascii_case(fingerprint))
            .map(|key| key.protection)
            .unwrap_or(ManagedRipassoPrivateKeyProtection::Password),
        Err(err) => {
            log_error(format!(
                "Failed to read private key protection for '{fingerprint}': {err}"
            ));
            ManagedRipassoPrivateKeyProtection::Password
        }
    }
}

fn start_private_key_unlock_for_action(
    window: &ApplicationWindow,
    overlay: &ToastOverlay,
    fingerprint: String,
    request: PrivateKeyUnlockRequest,
    after_unlock: &Rc<dyn Fn()>,
    on_finish: &Rc<dyn Fn(bool)>,
) {
    let overlay = overlay.clone();
    let overlay_for_disconnect = overlay.clone();
    let window_for_result = window.clone();
    let after_unlock = after_unlock.clone();
    let on_finish_for_result = on_finish.clone();
    let on_finish_for_disconnect = on_finish.clone();
    spawn_result_task(
        move || unlock_ripasso_private_key_for_session(&fingerprint, request.clone()),
        move |result: Result<ManagedRipassoPrivateKey, PrivateKeyError>| match result {
            Ok(_) => {
                after_unlock();
                activate_widget_action(&window_for_result, "win.reload-store-recipients-list");
                activate_widget_action(&window_for_result, "win.reload-password-list");
                on_finish_for_result(true);
            }
            Err(err) => {
                log_error(format!("Failed to unlock ripasso private key: {err}"));
                overlay.add_toast(Toast::new(err.unlock_message()));
                on_finish_for_result(false);
            }
        },
        move || {
            log_error("Private key unlock worker disconnected unexpectedly.".to_string());
            show_unlock_failure_toast(&overlay_for_disconnect);
            on_finish_for_disconnect(false);
        },
    );
}

pub fn prompt_private_key_unlock_for_action(
    overlay: &ToastOverlay,
    fingerprint: String,
    after_unlock: Rc<dyn Fn()>,
    on_finish: Rc<dyn Fn(bool)>,
) {
    let Some(window) = toast_overlay_window(overlay) else {
        log_error(
            "Couldn't find the application window for the private key unlock dialog.".to_string(),
        );
        show_unlock_failure_toast(overlay);
        on_finish(false);
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
    let on_finish_for_close = on_finish.clone();
    present_private_key_unlock_dialog_with_close_handler(
        &window,
        overlay,
        "Unlock key",
        key_title.as_deref(),
        private_key_unlock_protection(&fingerprint),
        move |request| {
            start_private_key_unlock_for_action(
                &window_for_submit,
                &overlay_for_submit,
                fingerprint.clone(),
                request,
                &after_unlock,
                &on_finish,
            );
        },
        move || on_finish_for_close(false),
    );
}
