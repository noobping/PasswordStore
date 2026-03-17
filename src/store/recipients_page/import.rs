use super::list::rebuild_store_recipients_list;
use super::StoreRecipientsPageState;
use crate::backend::{
    import_ripasso_private_key_bytes, ripasso_private_key_requires_passphrase,
    ManagedRipassoPrivateKey, PrivateKeyError,
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
