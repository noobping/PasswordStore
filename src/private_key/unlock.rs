use crate::backend::{
    list_ripasso_private_keys, ripasso_private_key_title, unlock_fido2_store_recipient_for_session,
    unlock_ripasso_private_key_for_session, ManagedRipassoPrivateKey, PrivateKeyError,
    PrivateKeyUnlockKind, PrivateKeyUnlockRequest,
};
use crate::fido2_recipient::{fido2_recipient_title, is_fido2_recipient_string};
use crate::i18n::gettext;
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
    overlay.add_toast(Toast::new(&gettext("Couldn't unlock the key.")));
}

fn private_key_unlock_kind(fingerprint: &str) -> PrivateKeyUnlockKind {
    if is_fido2_recipient_string(fingerprint) {
        return PrivateKeyUnlockKind::Fido2SecurityKey;
    }

    match list_ripasso_private_keys() {
        Ok(keys) => keys
            .into_iter()
            .find(|key| key.fingerprint.eq_ignore_ascii_case(fingerprint))
            .map(|key| key.protection.into())
            .unwrap_or(PrivateKeyUnlockKind::Password),
        Err(err) => {
            log_error(format!(
                "Failed to read private key protection for '{fingerprint}': {err}"
            ));
            PrivateKeyUnlockKind::Password
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
    if is_fido2_recipient_string(&fingerprint) {
        start_fido2_recipient_unlock_for_action(
            window,
            overlay,
            fingerprint,
            request,
            after_unlock,
            on_finish,
        );
        return;
    }

    let overlay = overlay.clone();
    let overlay_for_disconnect = overlay.clone();
    let window_for_result = window.clone();
    let after_unlock = after_unlock.clone();
    let on_finish_for_result = on_finish.clone();
    let on_finish_for_disconnect = on_finish.clone();
    let request_for_worker = request.clone();
    spawn_result_task(
        move || unlock_ripasso_private_key_for_session(&fingerprint, request_for_worker.clone()),
        move |result: Result<ManagedRipassoPrivateKey, PrivateKeyError>| match result {
            Ok(_) => {
                after_unlock();
                activate_widget_action(&window_for_result, "win.reload-store-recipients-list");
                activate_widget_action(&window_for_result, "win.reload-password-list");
                on_finish_for_result(true);
            }
            Err(err) => {
                log_error(format!("Failed to unlock ripasso private key: {err}"));
                overlay.add_toast(Toast::new(&gettext(err.unlock_message())));
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

fn start_fido2_recipient_unlock_for_action(
    window: &ApplicationWindow,
    overlay: &ToastOverlay,
    recipient: String,
    request: PrivateKeyUnlockRequest,
    after_unlock: &Rc<dyn Fn()>,
    on_finish: &Rc<dyn Fn(bool)>,
) {
    let overlay = overlay.clone();
    let overlay_for_disconnect = overlay.clone();
    let window_for_retry = window.clone();
    let after_unlock_for_result = after_unlock.clone();
    let on_finish_for_result = on_finish.clone();
    let on_finish_for_disconnect = on_finish.clone();
    let recipient_for_result = recipient.clone();
    let request_for_worker = request.clone();
    spawn_result_task(
        move || {
            let pin = match &request_for_worker {
                PrivateKeyUnlockRequest::Fido2(pin) => pin.as_deref(),
                _ => None,
            };
            unlock_fido2_store_recipient_for_session(&recipient, pin)
        },
        move |result: Result<(), PrivateKeyError>| match result {
            Ok(()) => {
                after_unlock_for_result();
                activate_widget_action(&window_for_retry, "win.reload-store-recipients-list");
                activate_widget_action(&window_for_retry, "win.reload-password-list");
                on_finish_for_result(true);
            }
            Err(PrivateKeyError::Fido2PinRequired(_))
                if matches!(request, PrivateKeyUnlockRequest::Fido2(None)) =>
            {
                let key_title = fido2_recipient_title(&recipient_for_result);
                let overlay_for_submit = overlay.clone();
                let recipient_for_submit = recipient_for_result.clone();
                let after_unlock_for_submit = after_unlock_for_result.clone();
                let on_finish_for_close = on_finish_for_result.clone();
                let window_for_dialog = window_for_retry.clone();
                let window_for_submit = window_for_retry.clone();
                present_private_key_unlock_dialog_with_close_handler(
                    &window_for_dialog,
                    &overlay,
                    "Unlock key",
                    key_title.as_deref(),
                    PrivateKeyUnlockKind::Fido2SecurityKey,
                    move |request| {
                        start_fido2_recipient_unlock_for_action(
                            &window_for_submit,
                            &overlay_for_submit,
                            recipient_for_submit.clone(),
                            request,
                            &after_unlock_for_submit,
                            &on_finish_for_result,
                        );
                    },
                    move || on_finish_for_close(false),
                );
            }
            Err(err) => {
                log_error(format!("Failed to unlock FIDO2 recipient: {err}"));
                overlay.add_toast(Toast::new(&gettext(err.unlock_message())));
                on_finish_for_result(false);
            }
        },
        move || {
            log_error("FIDO2 recipient unlock worker disconnected unexpectedly.".to_string());
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
    let key_title = if is_fido2_recipient_string(&fingerprint) {
        fido2_recipient_title(&fingerprint)
    } else {
        match ripasso_private_key_title(&fingerprint) {
            Ok(title) => Some(title),
            Err(err) => {
                log_error(format!(
                    "Failed to read private key title for '{fingerprint}': {err}"
                ));
                None
            }
        }
    };
    let kind = private_key_unlock_kind(&fingerprint);
    if matches!(kind, PrivateKeyUnlockKind::Fido2SecurityKey) {
        start_private_key_unlock_for_action(
            &window,
            overlay,
            fingerprint,
            PrivateKeyUnlockRequest::Fido2(None),
            &after_unlock,
            &on_finish,
        );
        return;
    }

    let window_for_submit = window.clone();
    let overlay_for_submit = overlay.clone();
    let on_finish_for_close = on_finish.clone();
    present_private_key_unlock_dialog_with_close_handler(
        &window,
        overlay,
        "Unlock key",
        key_title.as_deref(),
        kind,
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
