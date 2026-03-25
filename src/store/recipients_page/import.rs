use super::list::rebuild_store_recipients_list;
use super::sync::sync_private_keys_to_host_if_enabled;
use super::StoreRecipientsPageState;
use crate::backend::{
    discover_ripasso_hardware_keys, import_ripasso_hardware_key_bytes,
    import_ripasso_private_key_bytes, ripasso_private_key_requires_passphrase,
    DiscoveredHardwareToken, ManagedRipassoHardwareKey, ManagedRipassoPrivateKey, PrivateKeyError,
};
use crate::logging::log_error;
use crate::private_key::dialog::{
    build_private_key_progress_dialog, present_private_key_password_dialog, PrivateKeyDialogHandle,
};
use crate::support::actions::activate_widget_action;
use crate::support::background::spawn_result_task;
use crate::support::ui::connect_row_action;
use adw::gio;
use adw::gtk::{gdk::Display, FileChooserAction, FileChooserNative, ResponseType};
use adw::prelude::*;
use adw::Toast;
use std::rc::Rc;

fn finish_private_key_import(
    state: &StoreRecipientsPageState,
    result: Result<ManagedRipassoPrivateKey, PrivateKeyError>,
) {
    match result {
        Ok(_) => {
            let _ = sync_private_keys_to_host_if_enabled(state);
            rebuild_store_recipients_list(state);
            activate_widget_action(&state.window, "win.reload-password-list");
            state
                .platform
                .overlay
                .add_toast(Toast::new("Key imported."));
        }
        Err(err) => {
            log_error(format!("Failed to import private key: {err}"));
            state
                .platform
                .overlay
                .add_toast(Toast::new(err.import_message()));
        }
    }
}

fn finish_hardware_key_import(
    state: &StoreRecipientsPageState,
    result: Result<ManagedRipassoPrivateKey, PrivateKeyError>,
) {
    match result {
        Ok(_) => {
            let _ = sync_private_keys_to_host_if_enabled(state);
            rebuild_store_recipients_list(state);
            activate_widget_action(&state.window, "win.reload-password-list");
            state
                .platform
                .overlay
                .add_toast(Toast::new("Hardware key added."));
        }
        Err(err) => {
            log_error(format!("Failed to import hardware key: {err}"));
            state
                .platform
                .overlay
                .add_toast(Toast::new(err.import_message()));
        }
    }
}

fn start_private_key_import(
    state: &StoreRecipientsPageState,
    bytes: Vec<u8>,
    passphrase: Option<String>,
) {
    let state = state.clone();
    let progress_dialog = PrivateKeyDialogHandle::new(&build_private_key_progress_dialog(
        &state.window,
        "Importing key",
        None,
        "Please wait.",
    ));
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
                .platform
                .overlay
                .add_toast(Toast::new("Couldn't import the key."));
        },
    );
}

fn hardware_key_from_token(token: &DiscoveredHardwareToken) -> ManagedRipassoHardwareKey {
    ManagedRipassoHardwareKey {
        ident: token.ident.clone(),
        signing_fingerprint: token.signing_fingerprint.clone(),
        decryption_fingerprint: token.decryption_fingerprint.clone(),
        reader_hint: token.reader_hint.clone(),
    }
}

fn selected_hardware_token(state: &StoreRecipientsPageState) -> Option<DiscoveredHardwareToken> {
    match discover_ripasso_hardware_keys() {
        Ok(mut tokens) => match tokens.len() {
            0 => {
                state
                    .platform
                    .overlay
                    .add_toast(Toast::new("Connect a hardware key first."));
                None
            }
            1 => tokens.pop(),
            _ => {
                state.platform.overlay.add_toast(Toast::new(
                    "Connect only one hardware key before adding it.",
                ));
                None
            }
        },
        Err(err) => {
            log_error(format!("Failed to discover hardware keys: {err}"));
            state
                .platform
                .overlay
                .add_toast(Toast::new("Couldn't inspect the hardware key."));
            None
        }
    }
}

fn start_hardware_key_import(
    state: &StoreRecipientsPageState,
    bytes: Vec<u8>,
    hardware: ManagedRipassoHardwareKey,
) {
    let state = state.clone();
    let progress_dialog = PrivateKeyDialogHandle::new(&build_private_key_progress_dialog(
        &state.window,
        "Adding hardware key",
        None,
        "Please wait.",
    ));
    let progress_dialog_for_disconnect = progress_dialog.clone();
    let state_for_disconnect = state.clone();
    spawn_result_task(
        move || import_ripasso_hardware_key_bytes(&bytes, hardware.clone()),
        move |result| {
            progress_dialog.force_close();
            finish_hardware_key_import(&state, result);
        },
        move || {
            progress_dialog_for_disconnect.force_close();
            log_error("Hardware key import worker disconnected unexpectedly.".to_string());
            state_for_disconnect
                .platform
                .overlay
                .add_toast(Toast::new("Couldn't add the hardware key."));
        },
    );
}

fn prompt_private_key_passphrase(state: &StoreRecipientsPageState, bytes: Vec<u8>) {
    let bytes = Rc::new(bytes);
    let window = state.window.clone();
    let overlay = state.platform.overlay.clone();
    let state = state.clone();
    present_private_key_password_dialog(&window, &overlay, "Unlock key", None, move |passphrase| {
        start_private_key_import(&state, bytes.as_slice().to_vec(), Some(passphrase));
    });
}

fn import_private_key_bytes(state: &StoreRecipientsPageState, bytes: Vec<u8>) {
    match ripasso_private_key_requires_passphrase(&bytes) {
        Ok(true) => prompt_private_key_passphrase(state, bytes),
        Ok(false) => start_private_key_import(state, bytes, None),
        Err(err) => {
            log_error(format!("Failed to inspect private key: {err}"));
            state
                .platform
                .overlay
                .add_toast(Toast::new(err.inspection_message()));
        }
    }
}

fn import_hardware_key_bytes(
    state: &StoreRecipientsPageState,
    bytes: Vec<u8>,
    hardware: ManagedRipassoHardwareKey,
) {
    start_hardware_key_import(state, bytes, hardware);
}

fn open_hardware_public_key_picker(
    state: &StoreRecipientsPageState,
    hardware: ManagedRipassoHardwareKey,
    title: &str,
) {
    let dialog = FileChooserNative::new(
        Some(title),
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
                import_hardware_key_bytes(
                    &state_for_response,
                    bytes.as_ref().to_vec(),
                    hardware.clone(),
                );
            }
            Err(err) => {
                log_error(format!(
                    "Failed to read the selected hardware public key file: {err}"
                ));
                state_for_response
                    .platform
                    .overlay
                    .add_toast(Toast::new("Couldn't read that file."));
            }
        }

        dialog.hide();
    });

    dialog.show();
}

fn add_connected_hardware_key(state: &StoreRecipientsPageState) {
    let Some(token) = selected_hardware_token(state) else {
        return;
    };
    let hardware = hardware_key_from_token(&token);
    if let Some(bytes) = token.cardholder_certificate {
        import_hardware_key_bytes(state, bytes, hardware);
        return;
    }

    state.platform.overlay.add_toast(Toast::new(
        "Choose the matching hardware public key file to finish setup.",
    ));
    open_hardware_public_key_picker(state, hardware, "Import hardware public key");
}

fn import_hardware_key_from_file(state: &StoreRecipientsPageState) {
    let Some(token) = selected_hardware_token(state) else {
        return;
    };
    open_hardware_public_key_picker(
        state,
        hardware_key_from_token(&token),
        "Import hardware public key",
    );
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
                import_private_key_bytes(&state_for_response, bytes.as_ref().to_vec());
            }
            Err(err) => {
                log_error(format!(
                    "Failed to read the selected private key file: {err}"
                ));
                state_for_response
                    .platform
                    .overlay
                    .add_toast(Toast::new("Couldn't read that file."));
            }
        }

        dialog.hide();
    });

    dialog.show();
}

fn import_private_key_from_clipboard(state: &StoreRecipientsPageState) {
    let Some(display) = Display::default() else {
        state
            .platform
            .overlay
            .add_toast(Toast::new("Clipboard unavailable."));
        return;
    };

    let clipboard = display.clipboard();
    let state_for_response = state.clone();
    clipboard.read_text_async(None::<&gio::Cancellable>, move |result| match result {
        Ok(Some(text)) if !text.trim().is_empty() => {
            import_private_key_bytes(&state_for_response, text.as_bytes().to_vec());
        }
        Ok(_) => {
            state_for_response
                .platform
                .overlay
                .add_toast(Toast::new("Clipboard does not contain a key."));
        }
        Err(err) => {
            log_error(format!("Failed to read private key from clipboard: {err}"));
            state_for_response
                .platform
                .overlay
                .add_toast(Toast::new("Couldn't read the clipboard."));
        }
    });
}

pub(super) fn connect_private_key_import_controls(state: &StoreRecipientsPageState) {
    let hardware_row = state.platform.add_hardware_key_row.clone();
    let hardware_state = state.clone();
    connect_row_action(&hardware_row, move || {
        add_connected_hardware_key(&hardware_state);
    });

    let import_hardware_row = state.platform.import_hardware_key_row.clone();
    let import_hardware_state = state.clone();
    connect_row_action(&import_hardware_row, move || {
        import_hardware_key_from_file(&import_hardware_state);
    });

    let clipboard_row = state.platform.import_clipboard_row.clone();
    let clipboard_state = state.clone();
    connect_row_action(&clipboard_row, move || {
        import_private_key_from_clipboard(&clipboard_state);
    });

    let file_row = state.platform.import_file_row.clone();
    let file_state = state.clone();
    connect_row_action(&file_row, move || {
        open_private_key_picker(&file_state);
    });
}
