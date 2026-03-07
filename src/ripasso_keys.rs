use crate::backend::{
    import_ripasso_private_key_bytes, is_ripasso_private_key_unlocked, list_ripasso_private_keys,
    remove_ripasso_private_key, ripasso_private_key_requires_passphrase,
    ripasso_private_key_requires_session_unlock, unlock_ripasso_private_key_for_session,
    ManagedRipassoPrivateKey,
};
use crate::logging::log_error;
use crate::private_key_dialog::{
    build_private_key_progress_dialog, present_private_key_password_dialog,
};
use crate::preferences::Preferences;
use adw::gio;
use adw::glib;
use adw::prelude::*;
use adw::{ActionRow, ApplicationWindow, Toast, ToastOverlay};
use adw::gtk::{Button, FileChooserAction, FileChooserNative, Image, ListBox, ResponseType};
use std::rc::Rc;
use std::sync::mpsc;
use std::sync::mpsc::TryRecvError;
use std::thread;
use std::time::Duration;

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
            let message = if err.contains("does not include a private key") {
                "That file does not contain a private key."
            } else if err.contains("must be password protected") {
                "Protect that private key with a password before importing it."
            } else if err.contains("password protected") || err.contains("incorrect") {
                "Couldn't unlock that private key."
            } else if err.contains("cannot decrypt password store entries") {
                "That private key cannot decrypt password entries."
            } else {
                "Couldn't import that private key."
            };
            state.overlay.add_toast(Toast::new(message));
        }
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
    let (tx, rx) = mpsc::channel::<Result<ManagedRipassoPrivateKey, String>>();
    thread::spawn(move || {
        let result = import_ripasso_private_key_bytes(&bytes, passphrase.as_deref());
        let _ = tx.send(result);
    });

    let state = state.clone();
    glib::timeout_add_local(Duration::from_millis(50), move || match rx.try_recv() {
        Ok(result) => {
            progress_dialog.force_close();
            finish_ripasso_private_key_import(&state, result);
            glib::ControlFlow::Break
        }
        Err(TryRecvError::Empty) => glib::ControlFlow::Continue,
        Err(TryRecvError::Disconnected) => {
            progress_dialog.force_close();
            log_error("Private key import worker disconnected unexpectedly.".to_string());
            state
                .overlay
                .add_toast(Toast::new("Couldn't import that private key."));
            glib::ControlFlow::Break
        }
    });
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

fn start_ripasso_private_key_unlock(
    state: &RipassoPrivateKeysState,
    fingerprint: String,
    passphrase: String,
    select_after_unlock: bool,
    after_unlock: Option<Rc<dyn Fn()>>,
) {
    let progress_dialog = build_private_key_progress_dialog(
        &state.window,
        "Unlocking Private Key",
        "Please wait while ripasso unlocks the private key for this session.",
    );
    let (tx, rx) = mpsc::channel::<Result<ManagedRipassoPrivateKey, String>>();
    let fingerprint_for_thread = fingerprint.clone();
    thread::spawn(move || {
        let result = unlock_ripasso_private_key_for_session(&fingerprint_for_thread, &passphrase);
        let _ = tx.send(result);
    });

    let state = state.clone();
    let fingerprint_for_result = fingerprint.clone();
    let after_unlock = after_unlock.clone();
    glib::timeout_add_local(Duration::from_millis(50), move || match rx.try_recv() {
        Ok(Ok(_)) => {
            progress_dialog.force_close();
            if select_after_unlock {
                select_ripasso_private_key(&state, &fingerprint_for_result);
            } else {
                rebuild_ripasso_private_keys_list(&state);
            }
            if let Some(after_unlock) = after_unlock.as_ref() {
                after_unlock();
            }
            state
                .overlay
                .add_toast(Toast::new("Private key unlocked for this session."));
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
            state.overlay.add_toast(Toast::new(message));
            glib::ControlFlow::Break
        }
        Err(TryRecvError::Empty) => glib::ControlFlow::Continue,
        Err(TryRecvError::Disconnected) => {
            progress_dialog.force_close();
            log_error("Private key unlock worker disconnected unexpectedly.".to_string());
            state
                .overlay
                .add_toast(Toast::new("Couldn't unlock that private key."));
            glib::ControlFlow::Break
        }
    });
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

fn prompt_ripasso_private_key_unlock(
    state: &RipassoPrivateKeysState,
    fingerprint: String,
    select_after_unlock: bool,
    after_unlock: Option<Rc<dyn Fn()>>,
) {
    let state = state.clone();
    let fingerprint = Rc::new(fingerprint);
    let window = state.window.clone();
    let overlay = state.overlay.clone();
    present_private_key_password_dialog(
        &window,
        &overlay,
        "Unlock Private Key",
        move |passphrase| {
            start_ripasso_private_key_unlock(
                &state,
                fingerprint.as_str().to_string(),
                passphrase,
                select_after_unlock,
                after_unlock.clone(),
            );
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
    row.connect_activated(move |_| {
        open_ripasso_private_key_picker(&row_state);
    });

    let button_state = state.clone();
    button.connect_clicked(move |_| {
        open_ripasso_private_key_picker(&button_state);
    });
}

pub(crate) fn rebuild_ripasso_private_keys_list(state: &RipassoPrivateKeysState) {
    while let Some(child) = state.list.first_child() {
        state.list.remove(&child);
    }

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
            let unlocked = is_ripasso_private_key_unlocked(&key.fingerprint).unwrap_or(false);
            let requires_unlock =
                ripasso_private_key_requires_session_unlock(&key.fingerprint).unwrap_or(false);
            let title = glib::markup_escape_text(&key.title());
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
                let settings = Preferences::new();
                let is_selected = settings.ripasso_own_fingerprint().as_deref()
                    == Some(key_for_select.fingerprint.as_str());
                let requires_unlock =
                    match ripasso_private_key_requires_session_unlock(&key_for_select.fingerprint) {
                        Ok(requires_unlock) => requires_unlock,
                        Err(err) => {
                            log_error(format!(
                                "Failed to inspect ripasso private key '{}': {err}",
                                key_for_select.fingerprint
                            ));
                            select_state
                                .overlay
                                .add_toast(Toast::new("Couldn't open that private key."));
                            return;
                        }
                    };

                if requires_unlock {
                    prompt_ripasso_private_key_unlock(
                        &select_state,
                        key_for_select.fingerprint.clone(),
                        !is_selected,
                        None,
                    );
                    return;
                }

                if is_selected {
                    return;
                }

                select_ripasso_private_key(&select_state, &key_for_select.fingerprint);
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
