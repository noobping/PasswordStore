use crate::backend::{
    git_commit_private_key_requiring_unlock_for_entry,
    git_commit_private_key_requiring_unlock_for_store_recipients, list_connected_smartcard_keys,
    list_ripasso_private_keys, ripasso_private_key_title, unlock_ripasso_private_key_for_session,
    ManagedRipassoPrivateKey, PrivateKeyError, PrivateKeyUnlockKind, PrivateKeyUnlockRequest,
    StoreRecipients, StoreRecipientsPrivateKeyRequirement,
};
use crate::i18n::gettext;
use crate::logging::{log_error, log_info};
use crate::private_key::dialog::present_private_key_unlock_dialog_with_close_handler;
use crate::support::background::spawn_result_task_with_finalizer;
use adw::{prelude::*, ApplicationWindow, Toast, ToastOverlay};
use std::rc::Rc;

fn toast_overlay_window(overlay: &ToastOverlay) -> Option<ApplicationWindow> {
    overlay
        .root()
        .and_then(|root| root.downcast::<ApplicationWindow>().ok())
}

fn continue_without_git_signature(overlay: &ToastOverlay, reason: &str, action: &Rc<dyn Fn()>) {
    log_info(reason.to_string());
    overlay.add_toast(Toast::new(&gettext("Saving without a Git signature.")));
    action();
}

fn private_key_unlock_kind(fingerprint: &str) -> PrivateKeyUnlockKind {
    match list_ripasso_private_keys() {
        Ok(keys) => {
            if let Some(kind) = keys
                .into_iter()
                .find(|key| key.fingerprint.eq_ignore_ascii_case(fingerprint))
                .map(|key| key.protection.into())
            {
                return kind;
            }
        }
        Err(err) => {
            log_error(format!(
                "Failed to read private key protection for '{fingerprint}': {err}"
            ));
        }
    }

    match list_connected_smartcard_keys() {
        Ok(keys) => keys
            .into_iter()
            .find(|key| key.fingerprint.eq_ignore_ascii_case(fingerprint))
            .map(|_| PrivateKeyUnlockKind::HardwareOpenPgpCard)
            .unwrap_or(PrivateKeyUnlockKind::Password),
        Err(err) => {
            log_error(format!(
                "Failed to inspect connected smartcards for '{fingerprint}': {err}"
            ));
            PrivateKeyUnlockKind::Password
        }
    }
}

fn start_private_key_unlock_for_git_commit(
    overlay: &ToastOverlay,
    fingerprint: String,
    request: PrivateKeyUnlockRequest,
    after_unlock_attempt: &Rc<dyn Fn()>,
) {
    let overlay = overlay.clone();
    let overlay_for_disconnect = overlay.clone();
    let fingerprint_for_worker = fingerprint.clone();
    let fingerprint_for_failure = fingerprint.clone();
    let after_unlock_attempt_for_result = after_unlock_attempt.clone();
    let after_unlock_attempt_for_disconnect = after_unlock_attempt.clone();
    spawn_result_task_with_finalizer(
        move || unlock_ripasso_private_key_for_session(&fingerprint_for_worker, request),
        || {},
        move |result: Result<ManagedRipassoPrivateKey, PrivateKeyError>| match result {
            Ok(_) => {
                after_unlock_attempt_for_result();
            }
            Err(err) => {
                log_error(format!("Failed to unlock ripasso private key: {err}"));
                continue_without_git_signature(
                    &overlay,
                    &format!(
                        "Couldn't unlock private key {fingerprint_for_failure} for Git signing. Continuing without a signature."
                    ),
                    &after_unlock_attempt_for_result,
                );
            }
        },
        move || {
            log_error("Private key unlock worker disconnected unexpectedly.".to_string());
            continue_without_git_signature(
                &overlay_for_disconnect,
                &format!(
                    "Private key unlock worker disconnected while preparing a Git signature for {fingerprint}."
                ),
                &after_unlock_attempt_for_disconnect,
            );
        },
    );
}

fn prompt_private_key_unlock_for_git_commit_if_needed(
    overlay: &ToastOverlay,
    fingerprint: Result<Option<String>, String>,
    context: &str,
    after_unlock_attempt: &Rc<dyn Fn()>,
) -> bool {
    let context = context.to_string();

    match fingerprint {
        Ok(Some(fingerprint)) => {
            let Some(window) = toast_overlay_window(overlay) else {
                log_error(
                    "Couldn't find the application window for the Git signing unlock dialog."
                        .to_string(),
                );
                continue_without_git_signature(
                    overlay,
                    "Couldn't present the Git signing unlock dialog. Continuing without a signature.",
                    after_unlock_attempt,
                );
                return true;
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
            let overlay_for_submit = overlay.clone();
            let kind = private_key_unlock_kind(&fingerprint);
            let fingerprint_for_submit = fingerprint;
            let after_unlock_attempt_for_submit = after_unlock_attempt.clone();
            let overlay_for_close = overlay.clone();
            let after_unlock_attempt_for_close = after_unlock_attempt.clone();
            let context_for_close = context;
            present_private_key_unlock_dialog_with_close_handler(
                &window,
                overlay,
                "Unlock key",
                key_title.as_deref(),
                kind,
                move |request| {
                    start_private_key_unlock_for_git_commit(
                        &overlay_for_submit,
                        fingerprint_for_submit.clone(),
                        request,
                        &after_unlock_attempt_for_submit,
                    );
                },
                move || {
                    continue_without_git_signature(
                        &overlay_for_close,
                        &format!(
                            "Dismissed the Git signing unlock prompt for {context_for_close}. Continuing without a signature."
                        ),
                        &after_unlock_attempt_for_close,
                    );
                },
            );
            true
        }
        Ok(None) => false,
        Err(err) => {
            log_error(format!(
                "Failed to resolve the private key needed to sign the Git commit for {context}: {err}"
            ));
            false
        }
    }
}

pub fn prompt_private_key_unlock_for_entry_git_commit_if_needed(
    overlay: &ToastOverlay,
    store_root: &str,
    label: &str,
    after_unlock: &Rc<dyn Fn()>,
) -> bool {
    prompt_private_key_unlock_for_git_commit_if_needed(
        overlay,
        git_commit_private_key_requiring_unlock_for_entry(store_root, label),
        label,
        after_unlock,
    )
}

pub fn prompt_private_key_unlock_for_store_git_commit_if_needed(
    overlay: &ToastOverlay,
    store_root: &str,
    recipients: &StoreRecipients,
    private_key_requirement: StoreRecipientsPrivateKeyRequirement,
    after_unlock: &Rc<dyn Fn()>,
) -> bool {
    prompt_private_key_unlock_for_git_commit_if_needed(
        overlay,
        git_commit_private_key_requiring_unlock_for_store_recipients(
            store_root,
            recipients,
            private_key_requirement,
        ),
        store_root,
        after_unlock,
    )
}
