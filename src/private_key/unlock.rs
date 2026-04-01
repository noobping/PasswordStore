use crate::backend::{
    list_ripasso_private_keys, ripasso_private_key_title, unlock_fido2_store_recipient_for_session,
    unlock_ripasso_private_key_for_session, ManagedRipassoPrivateKey, PrivateKeyError,
    PrivateKeyUnlockKind, PrivateKeyUnlockRequest,
};
use crate::fido2_recipient::{fido2_recipient_title, is_fido2_recipient_string};
use crate::i18n::gettext;
use crate::logging::log_error;
use crate::private_key::dialog::{
    build_private_key_progress_dialog, present_private_key_unlock_dialog_with_close_handler,
    PrivateKeyDialogHandle,
};
use crate::support::actions::activate_widget_action;
use crate::support::background::spawn_result_task;
use adw::{glib, prelude::*, ApplicationWindow, Toast, ToastOverlay};
use std::rc::Rc;

fn toast_overlay_window(overlay: &ToastOverlay) -> Option<ApplicationWindow> {
    overlay
        .root()
        .and_then(|root| root.downcast::<ApplicationWindow>().ok())
}

fn show_unlock_failure_toast(overlay: &ToastOverlay) {
    overlay.add_toast(Toast::new(&gettext("Couldn't unlock the key.")));
}

fn finish_unlock_success(
    window: &ApplicationWindow,
    after_unlock: &Rc<dyn Fn()>,
    on_finish: &Rc<dyn Fn(bool)>,
) {
    after_unlock();
    activate_widget_action(window, "win.reload-store-recipients-list");
    activate_widget_action(window, "win.reload-password-list");
    on_finish(true);
}

fn present_fido2_unlock_progress_dialog(
    window: &ApplicationWindow,
    subtitle: Option<&str>,
) -> PrivateKeyDialogHandle {
    PrivateKeyDialogHandle::new(&build_private_key_progress_dialog(
        window,
        "Unlock key",
        subtitle,
        "Touch the security key if it starts blinking.",
    ))
}

#[cfg(feature = "fidokey")]
fn managed_fido2_unlock_enabled(kind: PrivateKeyUnlockKind) -> bool {
    matches!(kind, PrivateKeyUnlockKind::Fido2SecurityKey)
}

#[cfg(not(feature = "fidokey"))]
const fn managed_fido2_unlock_enabled(_kind: PrivateKeyUnlockKind) -> bool {
    false
}

#[cfg(feature = "fidokey")]
fn handle_managed_fido2_unlock_retry(
    window: &ApplicationWindow,
    overlay: &ToastOverlay,
    fingerprint: &str,
    request: &PrivateKeyUnlockRequest,
    after_unlock: &Rc<dyn Fn()>,
    on_finish: &Rc<dyn Fn(bool)>,
) -> bool {
    if !matches!(request, PrivateKeyUnlockRequest::Fido2(None)) {
        return false;
    }

    let key_title = match ripasso_private_key_title(fingerprint) {
        Ok(title) => Some(title),
        Err(err) => {
            log_error(format!(
                "Failed to read private key title for '{fingerprint}': {err}"
            ));
            None
        }
    };
    let overlay_for_submit = overlay.clone();
    let fingerprint_for_submit = fingerprint.to_string();
    let after_unlock_for_submit = after_unlock.clone();
    let on_finish_for_submit = on_finish.clone();
    let on_finish_for_close = on_finish.clone();
    let window_for_dialog = window.clone();
    let window_for_submit = window.clone();
    present_private_key_unlock_dialog_with_close_handler(
        &window_for_dialog,
        overlay,
        "Unlock key",
        key_title.as_deref(),
        PrivateKeyUnlockKind::Fido2SecurityKey,
        move |request| {
            start_private_key_unlock_for_action(
                &window_for_submit,
                &overlay_for_submit,
                fingerprint_for_submit.clone(),
                request,
                &after_unlock_for_submit,
                &on_finish_for_submit,
            );
        },
        move || on_finish_for_close(false),
    );
    true
}

#[cfg(not(feature = "fidokey"))]
fn handle_managed_fido2_unlock_retry(
    _window: &ApplicationWindow,
    _overlay: &ToastOverlay,
    _fingerprint: &str,
    _request: &PrivateKeyUnlockRequest,
    _after_unlock: &Rc<dyn Fn()>,
    _on_finish: &Rc<dyn Fn(bool)>,
) -> bool {
    false
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
    let key_title = match ripasso_private_key_title(&fingerprint) {
        Ok(title) => Some(title),
        Err(err) => {
            log_error(format!(
                "Failed to read private key title for '{fingerprint}': {err}"
            ));
            None
        }
    };
    let overlay = overlay.clone();
    let overlay_for_disconnect = overlay.clone();
    let window_for_result = window.clone();
    let after_unlock = after_unlock.clone();
    let on_finish_for_result = on_finish.clone();
    let on_finish_for_disconnect = on_finish.clone();
    let request_for_worker = request.clone();
    let fingerprint_for_worker = fingerprint.clone();
    let progress_dialog = present_fido2_unlock_progress_dialog(window, key_title.as_deref());
    let progress_dialog_for_result = progress_dialog.clone();
    let progress_dialog_for_disconnect = progress_dialog.clone();
    // Let GTK show the dialog before the hardware unlock flow starts.
    glib::idle_add_local_once(move || {
        spawn_result_task(
            move || {
                unlock_ripasso_private_key_for_session(
                    &fingerprint_for_worker,
                    request_for_worker.clone(),
                )
            },
            move |result: Result<ManagedRipassoPrivateKey, PrivateKeyError>| match result {
                Ok(_) => {
                    progress_dialog_for_result.force_close();
                    finish_unlock_success(&window_for_result, &after_unlock, &on_finish_for_result);
                }
                Err(
                    PrivateKeyError::Fido2PinRequired(_) | PrivateKeyError::Fido2TokenNotPresent(_),
                ) if handle_managed_fido2_unlock_retry(
                    &window_for_result,
                    &overlay,
                    &fingerprint,
                    &request,
                    &after_unlock,
                    &on_finish_for_result,
                ) =>
                {
                    progress_dialog_for_result.force_close();
                }
                Err(err) => {
                    progress_dialog_for_result.force_close();
                    log_error(format!("Failed to unlock ripasso private key: {err}"));
                    overlay.add_toast(Toast::new(&gettext(err.unlock_message())));
                    on_finish_for_result(false);
                }
            },
            move || {
                progress_dialog_for_disconnect.force_close();
                log_error("Private key unlock worker disconnected unexpectedly.".to_string());
                show_unlock_failure_toast(&overlay_for_disconnect);
                on_finish_for_disconnect(false);
            },
        );
    });
}

fn start_fido2_recipient_unlock_for_action(
    window: &ApplicationWindow,
    overlay: &ToastOverlay,
    recipient: String,
    request: PrivateKeyUnlockRequest,
    after_unlock: &Rc<dyn Fn()>,
    on_finish: &Rc<dyn Fn(bool)>,
) {
    let key_title = fido2_recipient_title(&recipient);
    let overlay = overlay.clone();
    let overlay_for_disconnect = overlay.clone();
    let window_for_retry = window.clone();
    let after_unlock_for_result = after_unlock.clone();
    let on_finish_for_result = on_finish.clone();
    let on_finish_for_disconnect = on_finish.clone();
    let recipient_for_result = recipient.clone();
    let request_for_worker = request.clone();
    let progress_dialog = present_fido2_unlock_progress_dialog(window, key_title.as_deref());
    let progress_dialog_for_result = progress_dialog.clone();
    let progress_dialog_for_disconnect = progress_dialog.clone();
    // Let GTK show the dialog before the hardware unlock flow starts.
    glib::idle_add_local_once(move || {
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
                    progress_dialog_for_result.force_close();
                    finish_unlock_success(
                        &window_for_retry,
                        &after_unlock_for_result,
                        &on_finish_for_result,
                    );
                }
                Err(PrivateKeyError::Fido2PinRequired(_))
                    if matches!(request, PrivateKeyUnlockRequest::Fido2(None)) =>
                {
                    progress_dialog_for_result.force_close();
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
                    progress_dialog_for_result.force_close();
                    log_error(format!("Failed to unlock FIDO2 recipient: {err}"));
                    overlay.add_toast(Toast::new(&gettext(err.unlock_message())));
                    on_finish_for_result(false);
                }
            },
            move || {
                progress_dialog_for_disconnect.force_close();
                log_error("FIDO2 recipient unlock worker disconnected unexpectedly.".to_string());
                show_unlock_failure_toast(&overlay_for_disconnect);
                on_finish_for_disconnect(false);
            },
        );
    });
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
    if is_fido2_recipient_string(&fingerprint) {
        start_fido2_recipient_unlock_for_action(
            &window,
            overlay,
            fingerprint,
            PrivateKeyUnlockRequest::Fido2(None),
            &after_unlock,
            &on_finish,
        );
        return;
    }

    if managed_fido2_unlock_enabled(kind) {
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
