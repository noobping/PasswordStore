use super::{queue_store_recipients_autosave, StoreRecipientsPageState};
use crate::backend::{
    import_ripasso_private_key_bytes, is_ripasso_private_key_unlocked, list_ripasso_private_keys,
    remove_ripasso_private_key, ripasso_private_key_requires_passphrase,
    ripasso_private_key_requires_session_unlock, ManagedRipassoPrivateKey,
};
use crate::support::background::spawn_result_task;
use crate::logging::log_error;
use crate::preferences::Preferences;
use crate::private_key_dialog::{
    build_private_key_progress_dialog, present_private_key_password_dialog,
};
use crate::ripasso_unlock::prompt_private_key_unlock_for_action;
use crate::support::ui::clear_list_box;
use adw::gio;
use adw::prelude::*;
use adw::{ActionRow, Toast};
use adw::gtk::{
    Button, CheckButton, FileChooserAction, FileChooserNative, Image, ResponseType,
};
use std::rc::Rc;

fn inspect_private_key_lock_state(fingerprint: &str) -> (bool, bool) {
    let unlocked = match is_ripasso_private_key_unlocked(fingerprint) {
        Ok(unlocked) => unlocked,
        Err(err) => {
            log_error(format!(
                "Failed to inspect whether private key '{fingerprint}' is unlocked: {err}"
            ));
            false
        }
    };
    let requires_unlock = match ripasso_private_key_requires_session_unlock(fingerprint) {
        Ok(requires_unlock) => requires_unlock,
        Err(err) => {
            log_error(format!(
                "Failed to inspect whether private key '{fingerprint}' requires unlocking: {err}"
            ));
            false
        }
    };

    (unlocked, requires_unlock)
}

fn finish_private_key_import(
    state: &StoreRecipientsPageState,
    result: Result<ManagedRipassoPrivateKey, String>,
) {
    match result {
        Ok(_) => {
            rebuild_store_recipients_list(state);
            state.overlay.add_toast(Toast::new("Key imported."));
        }
        Err(err) => {
            log_error(format!("Failed to import private key: {err}"));
            let message = if err.contains("does not include a private key") {
                "That file does not contain a private key."
            } else if err.contains("must be password protected") {
                "Add a password to that key first."
            } else if err.contains("cannot decrypt password store entries") {
                "This key can't open your items."
            } else if err.contains("password protected") || err.contains("incorrect") {
                "Couldn't unlock the key."
            } else {
                "Couldn't import the key."
            };
            state.overlay.add_toast(Toast::new(message));
        }
    }
}

fn start_private_key_import(
    state: &StoreRecipientsPageState,
    bytes: Vec<u8>,
    passphrase: Option<String>,
) {
    let progress_dialog =
        build_private_key_progress_dialog(&state.window, "Importing key", None, "Please wait.");
    let state = state.clone();
    let progress_dialog_for_disconnect = progress_dialog.clone();
    let state_for_disconnect = state.clone();
    spawn_result_task(
        move || import_ripasso_private_key_bytes(&bytes, passphrase.as_deref()),
        move |result| {
            progress_dialog.force_close();
            finish_private_key_import(&state, result);
        },
        move || {
            progress_dialog_for_disconnect.force_close();
            log_error("Private key import worker disconnected unexpectedly.".to_string());
            state_for_disconnect
                .overlay
                .add_toast(Toast::new("Couldn't import the key."));
        },
    );
}

fn prompt_private_key_passphrase(state: &StoreRecipientsPageState, bytes: Vec<u8>) {
    let bytes = Rc::new(bytes);
    let window = state.window.clone();
    let overlay = state.overlay.clone();
    let state = state.clone();
    present_private_key_password_dialog(&window, &overlay, "Unlock key", None, move |passphrase| {
        start_private_key_import(&state, bytes.as_slice().to_vec(), Some(passphrase));
    });
}

fn open_private_key_picker(state: &StoreRecipientsPageState) {
    let dialog = FileChooserNative::new(
        Some("Import private key"),
        Some(&state.window),
        FileChooserAction::Open,
        Some("Import"),
        Some("Cancel"),
    );
    let state_for_response = state.clone();
    dialog.connect_response(move |dialog, response| {
        if response != ResponseType::Accept {
            dialog.hide();
            return;
        }

        let Some(file) = dialog.file() else {
            dialog.hide();
            return;
        };

        match file.load_bytes(None::<&gio::Cancellable>) {
            Ok((bytes, _)) => {
                let bytes = bytes.as_ref().to_vec();
                match ripasso_private_key_requires_passphrase(&bytes) {
                    Ok(true) => prompt_private_key_passphrase(&state_for_response, bytes),
                    Ok(false) => start_private_key_import(&state_for_response, bytes, None),
                    Err(err) => {
                        log_error(format!("Failed to inspect private key: {err}"));
                        let message = if err.contains("does not include a private key") {
                            "That file does not contain a private key."
                        } else {
                            "Couldn't read that key."
                        };
                        state_for_response.overlay.add_toast(Toast::new(message));
                    }
                }
            }
            Err(err) => {
                log_error(format!("Failed to read the selected private key file: {err}"));
                state_for_response
                    .overlay
                    .add_toast(Toast::new("Couldn't read that file."));
            }
        }

        dialog.hide();
    });

    dialog.show();
}

fn append_private_key_import_row(state: &StoreRecipientsPageState) {
    let row = ActionRow::builder()
        .title("Import private key")
        .subtitle("Choose a private key file.")
        .build();
    row.set_activatable(true);

    let button = Button::from_icon_name("document-open-symbolic");
    button.add_css_class("flat");
    row.add_suffix(&button);
    state.list.append(&row);

    let row_state = state.clone();
    row.connect_activated(move |_| open_private_key_picker(&row_state));

    let button_state = state.clone();
    button.connect_clicked(move |_| open_private_key_picker(&button_state));
}

pub(super) fn recipient_matches_private_key(
    recipient: &str,
    key: &ManagedRipassoPrivateKey,
) -> bool {
    let recipient = recipient.trim();
    recipient.eq_ignore_ascii_case(&key.fingerprint)
        || key
            .user_ids
            .iter()
            .any(|user_id| user_id.eq_ignore_ascii_case(recipient))
}

fn set_private_key_recipient_enabled(
    state: &StoreRecipientsPageState,
    key: &ManagedRipassoPrivateKey,
    enabled: bool,
) -> bool {
    let mut recipients = state.recipients.borrow_mut();
    let before = recipients.clone();
    recipients.retain(|value| !recipient_matches_private_key(value, key));
    if enabled {
        recipients.push(key.fingerprint.clone());
    }
    *recipients != before
}

pub(super) fn rebuild_store_recipients_list(state: &StoreRecipientsPageState) {
    clear_list_box(&state.list);

    let keys = match list_ripasso_private_keys() {
        Ok(keys) => keys,
        Err(err) => {
            log_error(format!("Failed to load private keys for recipients: {err}"));
            let row = ActionRow::builder()
                .title("Couldn't load private keys")
                .subtitle("Try again from Preferences.")
                .build();
            row.set_activatable(false);
            state.list.append(&row);
            append_private_key_import_row(state);
            return;
        }
    };

    if keys.is_empty() {
        let row = ActionRow::builder()
            .title("No private keys yet")
            .subtitle("Import a private key first.")
            .build();
        row.set_activatable(false);
        state.list.append(&row);
        append_private_key_import_row(state);
        return;
    }

    for key in keys {
        let active = state
            .recipients
            .borrow()
            .iter()
            .any(|recipient| recipient_matches_private_key(recipient, &key));
        let title = adw::glib::markup_escape_text(&key.title());
        let row = ActionRow::builder()
            .title(title.as_str())
            .subtitle(&key.fingerprint)
            .build();
        row.set_activatable(true);

        let key_icon = Image::from_icon_name("dialog-password-symbolic");
        key_icon.add_css_class("dim-label");
        row.add_prefix(&key_icon);

        let (unlocked, requires_unlock) = inspect_private_key_lock_state(&key.fingerprint);
        let toggle = CheckButton::new();
        toggle.set_active(active);
        row.add_suffix(&toggle);

        if requires_unlock {
            let unlock_button = Button::from_icon_name("system-lock-screen-symbolic");
            unlock_button.add_css_class("flat");
            unlock_button.set_tooltip_text(Some("Unlock key"));
            row.add_suffix(&unlock_button);

            let unlock_state = state.clone();
            let fingerprint = key.fingerprint.clone();
            unlock_button.connect_clicked(move |_| {
                let refresh_state = unlock_state.clone();
                prompt_private_key_unlock_for_action(
                    &unlock_state.overlay,
                    fingerprint.clone(),
                    Rc::new(move || super::rebuild_store_recipients_list(&refresh_state)),
                );
            });
        } else if unlocked {
            let unlocked_icon = Image::from_icon_name("changes-allow-symbolic");
            unlocked_icon.add_css_class("accent");
            row.add_suffix(&unlocked_icon);
        }

        let delete_button = Button::from_icon_name("user-trash-symbolic");
        delete_button.add_css_class("flat");
        delete_button.set_tooltip_text(Some("Remove key"));
        row.add_suffix(&delete_button);
        state.list.append(&row);

        let toggle_for_row = toggle.clone();
        row.connect_activated(move |_| {
            toggle_for_row.set_active(!toggle_for_row.is_active());
        });

        let page_state = state.clone();
        let key_for_toggle = key.clone();
        toggle.connect_toggled(move |button| {
            if set_private_key_recipient_enabled(&page_state, &key_for_toggle, button.is_active()) {
                queue_store_recipients_autosave(&page_state);
            }
        });

        let page_state = state.clone();
        let key_for_delete = key.clone();
        delete_button.connect_clicked(move |_| {
            if let Err(err) = remove_ripasso_private_key(&key_for_delete.fingerprint) {
                log_error(format!(
                    "Failed to remove private key '{}': {err}",
                    key_for_delete.fingerprint
                ));
                page_state
                    .overlay
                    .add_toast(Toast::new("Couldn't remove that key."));
                return;
            }

            if Preferences::new().ripasso_own_fingerprint().as_deref()
                == Some(key_for_delete.fingerprint.as_str())
            {
                let _ = Preferences::new().set_ripasso_own_fingerprint(None);
            }
            let recipients_changed =
                set_private_key_recipient_enabled(&page_state, &key_for_delete, false);
            super::rebuild_store_recipients_list(&page_state);
            if recipients_changed {
                queue_store_recipients_autosave(&page_state);
            }
        });
    }

    append_private_key_import_row(state);
}

#[cfg(test)]
mod tests {
    use super::recipient_matches_private_key;
    use crate::backend::ManagedRipassoPrivateKey;

    #[test]
    fn imported_private_keys_match_existing_user_id_recipients() {
        let key = ManagedRipassoPrivateKey {
            fingerprint: "10F4487A3768155709168A8E3D00743E10EA9232".to_string(),
            user_ids: vec!["pass@store.local".to_string()],
        };

        assert!(recipient_matches_private_key("pass@store.local", &key));
        assert!(recipient_matches_private_key(
            "10F4487A3768155709168A8E3D00743E10EA9232",
            &key
        ));
        assert!(!recipient_matches_private_key("other@example.com", &key));
    }
}
