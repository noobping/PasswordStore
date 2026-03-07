use crate::background::spawn_result_task;
use crate::backend::{
    import_ripasso_private_key_bytes, is_ripasso_private_key_unlocked, list_ripasso_private_keys,
    remove_ripasso_private_key, ripasso_private_key_requires_passphrase,
    ripasso_private_key_requires_session_unlock, ManagedRipassoPrivateKey,
};
use crate::logging::log_error;
use crate::private_key_dialog::{
    build_private_key_progress_dialog, present_private_key_password_dialog,
};
use crate::preferences::Preferences;
use crate::ripasso_unlock::prompt_private_key_unlock_for_action;
use crate::ui_helpers::{clear_list_box, connect_row_and_button_action};
use adw::gio;
use adw::prelude::*;
use adw::{ActionRow, ApplicationWindow, Toast, ToastOverlay};
use adw::gtk::{Button, FileChooserAction, FileChooserNative, Image, ListBox, ResponseType};
use std::rc::Rc;

#[derive(Clone)]
pub(crate) struct RipassoPrivateKeysState {
    pub(crate) window: ApplicationWindow,
    pub(crate) list: ListBox,
    pub(crate) overlay: ToastOverlay,
}

fn sync_ripasso_private_key_selection(keys: &[ManagedRipassoPrivateKey]) -> Option<String> {
    let settings = Preferences::new();
    let configured = settings.ripasso_own_fingerprint();
    let resolved = configured
        .as_deref()
        .and_then(|fingerprint| {
            keys.iter()
                .find(|key| key.fingerprint.eq_ignore_ascii_case(fingerprint))
                .map(|key| key.fingerprint.clone())
        })
        .or_else(|| keys.first().map(|key| key.fingerprint.clone()));

    if configured.as_deref() != resolved.as_deref() {
        if let Err(err) = settings.set_ripasso_own_fingerprint(resolved.as_deref()) {
            log_error(format!(
                "Failed to store the selected ripasso private key fingerprint: {err}"
            ));
        }
    }

    resolved
}

fn open_ripasso_private_key_picker(state: &RipassoPrivateKeysState) {
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
                    Ok(true) => prompt_ripasso_private_key_passphrase(&state_for_response, bytes),
                    Ok(false) => start_ripasso_private_key_import(&state_for_response, bytes, None),
                    Err(err) => {
                        log_error(format!("Failed to inspect ripasso private key: {err}"));
                        let message = if err.contains("does not include a private key") {
                            "That file does not contain a private key."
                        } else {
                            "Couldn't read that private key."
                        };
                        state_for_response.overlay.add_toast(Toast::new(message));
                    }
                }
            }
            Err(err) => {
                log_error(format!("Failed to read the selected private key file: {err}"));
                state_for_response
                    .overlay
                    .add_toast(Toast::new("Couldn't read that private key file."));
            }
        }

        dialog.hide();
    });

    dialog.show();
}

fn finish_ripasso_private_key_import(
    state: &RipassoPrivateKeysState,
    result: Result<ManagedRipassoPrivateKey, String>,
) {
    match result {
        Ok(key) => {
            let settings = Preferences::new();
            if let Err(err) = settings.set_ripasso_own_fingerprint(Some(&key.fingerprint)) {
                log_error(format!(
                    "Failed to store the imported ripasso private key fingerprint: {err}"
                ));
                state.overlay.add_toast(Toast::new(
                    "The private key was imported, but it could not be selected.",
                ));
            } else {
                rebuild_ripasso_private_keys_list(state);
                state.overlay.add_toast(Toast::new("Private key imported."));
            }
        }
        Err(err) => {
            log_error(format!("Failed to import ripasso private key: {err}"));
            state
                .overlay
                .add_toast(Toast::new(import_private_key_error_message(&err)));
        }
    }
}

fn import_private_key_error_message(message: &str) -> &'static str {
    if message.contains("does not include a private key") {
        "That file does not contain a private key."
    } else if message.contains("must be password protected") {
        "Protect that private key with a password before importing it."
    } else if message.contains("cannot decrypt password store entries") {
        "That private key cannot decrypt password entries."
    } else if message.contains("password protected") || message.contains("incorrect") {
        "Couldn't unlock that private key."
    } else {
        "Couldn't import that private key."
    }
}

fn start_ripasso_private_key_import(
    state: &RipassoPrivateKeysState,
    bytes: Vec<u8>,
    passphrase: Option<String>,
) {
    let progress_dialog = build_private_key_progress_dialog(
        &state.window,
        "Importing Private Key",
        "Please wait while ripasso imports the private key.",
    );
    let state = state.clone();
    let progress_dialog_for_disconnect = progress_dialog.clone();
    let state_for_disconnect = state.clone();
    spawn_result_task(
        move || import_ripasso_private_key_bytes(&bytes, passphrase.as_deref()),
        move |result| {
            progress_dialog.force_close();
            finish_ripasso_private_key_import(&state, result);
        },
        move || {
            progress_dialog_for_disconnect.force_close();
            log_error("Private key import worker disconnected unexpectedly.".to_string());
            state_for_disconnect
                .overlay
                .add_toast(Toast::new("Couldn't import that private key."));
        },
    );
}

fn select_ripasso_private_key(state: &RipassoPrivateKeysState, fingerprint: &str) {
    let settings = Preferences::new();
    if let Err(err) = settings.set_ripasso_own_fingerprint(Some(fingerprint)) {
        log_error(format!(
            "Failed to store the selected ripasso private key fingerprint: {err}"
        ));
        state
            .overlay
            .add_toast(Toast::new("Couldn't select that private key."));
    } else {
        rebuild_ripasso_private_keys_list(state);
    }
}

fn prompt_ripasso_private_key_passphrase(state: &RipassoPrivateKeysState, bytes: Vec<u8>) {
    let bytes = Rc::new(bytes);
    let state = state.clone();
    let window = state.window.clone();
    let overlay = state.overlay.clone();
    present_private_key_password_dialog(
        &window,
        &overlay,
        "Unlock Private Key",
        move |passphrase| {
            start_ripasso_private_key_import(&state, bytes.as_slice().to_vec(), Some(passphrase));
        },
    );
}

fn append_ripasso_private_key_import_row(state: &RipassoPrivateKeysState) {
    let row = ActionRow::builder()
        .title("Import private key")
        .subtitle("Choose an OpenPGP private key file.")
        .build();
    row.set_activatable(true);

    let button = Button::from_icon_name("document-open-symbolic");
    button.add_css_class("flat");
    row.add_suffix(&button);
    state.list.append(&row);

    let row_state = state.clone();
    connect_row_and_button_action(&row, &button, move || {
        open_ripasso_private_key_picker(&row_state);
    });
}

fn inspect_private_key_lock_state(fingerprint: &str) -> (bool, bool) {
    let unlocked = match is_ripasso_private_key_unlocked(fingerprint) {
        Ok(unlocked) => unlocked,
        Err(err) => {
            log_error(format!(
                "Failed to inspect whether ripasso private key '{fingerprint}' is unlocked: {err}"
            ));
            false
        }
    };
    let requires_unlock = match ripasso_private_key_requires_session_unlock(fingerprint) {
        Ok(requires_unlock) => requires_unlock,
        Err(err) => {
            log_error(format!(
                "Failed to inspect whether ripasso private key '{fingerprint}' requires unlocking: {err}"
            ));
            false
        }
    };

    (unlocked, requires_unlock)
}

fn activate_private_key_row(state: &RipassoPrivateKeysState, key: &ManagedRipassoPrivateKey) {
    let settings = Preferences::new();
    let is_selected =
        settings.ripasso_own_fingerprint().as_deref() == Some(key.fingerprint.as_str());
    let requires_unlock = match ripasso_private_key_requires_session_unlock(&key.fingerprint) {
        Ok(requires_unlock) => requires_unlock,
        Err(err) => {
            log_error(format!(
                "Failed to inspect ripasso private key '{}': {err}",
                key.fingerprint
            ));
            state
                .overlay
                .add_toast(Toast::new("Couldn't open that private key."));
            return;
        }
    };

    if requires_unlock {
        let select_state = state.clone();
        let fingerprint = key.fingerprint.clone();
        let after_unlock: Rc<dyn Fn()> = if is_selected {
            Rc::new(move || rebuild_ripasso_private_keys_list(&select_state))
        } else {
            Rc::new(move || select_ripasso_private_key(&select_state, &fingerprint))
        };
        prompt_private_key_unlock_for_action(&state.overlay, key.fingerprint.clone(), after_unlock);
        return;
    }

    if !is_selected {
        select_ripasso_private_key(state, &key.fingerprint);
    }
}

pub(crate) fn rebuild_ripasso_private_keys_list(state: &RipassoPrivateKeysState) {
    clear_list_box(&state.list);

    let keys = match list_ripasso_private_keys() {
        Ok(keys) => keys,
        Err(err) => {
            log_error(format!("Failed to read ripasso private keys: {err}"));
            state
                .overlay
                .add_toast(Toast::new("Couldn't load the private keys."));
            append_ripasso_private_key_import_row(state);
            return;
        }
    };
    let selected = sync_ripasso_private_key_selection(&keys);

    if keys.is_empty() {
        let empty_row = ActionRow::builder()
            .title("No private keys imported")
            .subtitle("Import an OpenPGP private key to let ripasso decrypt and save entries.")
            .build();
        empty_row.set_activatable(false);
        state.list.append(&empty_row);
    } else {
        for key in keys {
            let (unlocked, requires_unlock) = inspect_private_key_lock_state(&key.fingerprint);
            let title = adw::glib::markup_escape_text(&key.title());
            let row = ActionRow::builder()
                .title(title.as_str())
                .subtitle(&key.fingerprint)
                .build();
            row.set_activatable(true);

            let key_icon = Image::from_icon_name("dialog-password-symbolic");
            key_icon.add_css_class("dim-label");
            row.add_prefix(&key_icon);

            if requires_unlock {
                let locked_icon = Image::from_icon_name("system-lock-screen-symbolic");
                locked_icon.add_css_class("dim-label");
                row.add_suffix(&locked_icon);
            } else if unlocked {
                let unlocked_icon = Image::from_icon_name("changes-allow-symbolic");
                unlocked_icon.add_css_class("accent");
                row.add_suffix(&unlocked_icon);
            }

            if selected.as_deref() == Some(key.fingerprint.as_str()) {
                let selected_icon = Image::from_icon_name("object-select-symbolic");
                selected_icon.add_css_class("accent");
                row.add_suffix(&selected_icon);
            }

            let delete_button = Button::from_icon_name("user-trash-symbolic");
            delete_button.add_css_class("flat");
            row.add_suffix(&delete_button);
            state.list.append(&row);

            let select_state = state.clone();
            let key_for_select = key.clone();
            row.connect_activated(move |_| {
                activate_private_key_row(&select_state, &key_for_select);
            });

            let delete_state = state.clone();
            let key_for_delete = key.clone();
            delete_button.connect_clicked(move |_| {
                if let Err(err) = remove_ripasso_private_key(&key_for_delete.fingerprint) {
                    log_error(format!(
                        "Failed to remove ripasso private key '{}': {err}",
                        key_for_delete.fingerprint
                    ));
                    delete_state
                        .overlay
                        .add_toast(Toast::new("Couldn't remove that private key."));
                    return;
                }

                let settings = Preferences::new();
                if settings.ripasso_own_fingerprint().as_deref()
                    == Some(key_for_delete.fingerprint.as_str())
                {
                    let _ = settings.set_ripasso_own_fingerprint(None);
                }
                rebuild_ripasso_private_keys_list(&delete_state);
            });
        }
    }

    append_ripasso_private_key_import_row(state);
}
