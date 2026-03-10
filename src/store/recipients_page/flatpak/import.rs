use super::list::rebuild_store_recipients_list;
use super::StoreRecipientsPageState;
use crate::backend::{
    import_ripasso_private_key_bytes, ripasso_private_key_requires_passphrase,
    ManagedRipassoPrivateKey,
};
use crate::logging::log_error;
use crate::private_key::dialog::{
    build_private_key_progress_dialog, present_private_key_password_dialog,
};
use crate::support::background::spawn_result_task;
use adw::gio;
use adw::prelude::*;
use adw::{ActionRow, Toast};
use adw::gtk::{Button, FileChooserAction, FileChooserNative, ResponseType};
use std::rc::Rc;

fn finish_private_key_import(
    state: &StoreRecipientsPageState,
    result: Result<ManagedRipassoPrivateKey, String>,
) {
    match result {
        Ok(_) => {
            rebuild_store_recipients_list(state);
            state.platform.overlay.add_toast(Toast::new("Key imported."));
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
            state.platform.overlay.add_toast(Toast::new(message));
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
                        state_for_response
                            .platform
                            .overlay
                            .add_toast(Toast::new(message));
                    }
                }
            }
            Err(err) => {
                log_error(format!("Failed to read the selected private key file: {err}"));
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

pub(super) fn append_private_key_import_row(state: &StoreRecipientsPageState) {
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
