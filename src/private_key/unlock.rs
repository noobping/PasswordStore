use crate::backend::{
    ripasso_private_key_title, unlock_ripasso_private_key_for_session, ManagedRipassoPrivateKey,
    PrivateKeyError,
};
use crate::logging::log_error;
use crate::private_key::dialog::present_private_key_password_dialog_with_close_handler;
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
    passphrase: String,
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
        move || unlock_ripasso_private_key_for_session(&fingerprint, &passphrase),
        move |result: Result<ManagedRipassoPrivateKey, PrivateKeyError>| match result {
            Ok(_) => {
                after_unlock();
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
    present_private_key_password_dialog_with_close_handler(
        &window,
        overlay,
        "Unlock key",
        key_title.as_deref(),
        move |passphrase| {
            start_private_key_unlock_for_action(
                &window_for_submit,
                &overlay_for_submit,
                fingerprint.clone(),
                passphrase,
                &after_unlock,
                &on_finish,
            );
        },
        move || on_finish_for_close(false),
    );
}
