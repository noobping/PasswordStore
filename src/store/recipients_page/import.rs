use super::generate::connect_generation_autofill_rows;
use super::guide::{present_additional_fido2_save_guidance_dialog, saved_fido2_recipient_exists};
use super::list::rebuild_store_recipients_list;
use super::mode::{
    ensure_fido2_recipient_actions_allowed, ensure_standard_recipient_actions_allowed,
};
use super::sync::sync_private_keys_to_host_if_enabled;
use super::{
    queue_store_recipients_autosave, sync_store_recipients_page_header, StoreRecipientsPageState,
};
use crate::backend::{
    create_fido2_store_recipient, discover_ripasso_hardware_keys,
    import_ripasso_hardware_key_bytes, import_ripasso_private_key_bytes,
    ripasso_private_key_requires_passphrase, set_fido2_security_key_pin,
    supports_first_time_fido2_pin_setup, DiscoveredHardwareToken, ManagedRipassoHardwareKey,
    ManagedRipassoPrivateKey, PrivateKeyError, PrivateKeyUnlockKind,
};
use crate::i18n::gettext;
use crate::logging::log_error;
use crate::preferences::Preferences;
use crate::private_key::dialog::{
    build_private_key_progress_dialog, present_fido2_pin_setup_dialog_with_close_handler,
    present_private_key_password_dialog, present_private_key_unlock_dialog_with_close_handler,
    PrivateKeyDialogHandle,
};
use crate::support::actions::activate_widget_action;
use crate::support::background::spawn_result_task_with_finalizer;
use crate::support::file_picker::choose_file_bytes;
use crate::support::ui::{
    connect_row_action, push_navigation_page_if_needed, visible_navigation_page_is,
};
use crate::support::validation::validate_email_address;
use crate::window::navigation::{show_secondary_page_chrome, HasWindowChrome};
use adw::gio;
use adw::gtk::gdk::Display;
use adw::prelude::*;
use adw::Toast;
use secrecy::{ExposeSecret, SecretString};
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
                .add_toast(Toast::new(&gettext("Key imported.")));
        }
        Err(err) => {
            log_error(format!("Failed to import private key: {err}"));
            state
                .platform
                .overlay
                .add_toast(Toast::new(&gettext(err.import_message())));
        }
    }
}

fn finish_fido2_recipient_add(
    state: &StoreRecipientsPageState,
    result: Result<String, PrivateKeyError>,
) {
    match result {
        Ok(recipient) => {
            let requires_manual_save =
                saved_fido2_recipient_exists(&state.saved_recipients.borrow());
            let mut recipients = state.recipients.borrow_mut();
            let mut added = false;
            if !recipients.iter().any(|existing| existing == &recipient) {
                recipients.push(recipient);
                added = true;
            }
            drop(recipients);
            if !added {
                return;
            }
            rebuild_store_recipients_list(state);
            if requires_manual_save {
                present_additional_fido2_save_guidance_dialog(state);
            } else {
                queue_store_recipients_autosave(state);
            }
        }
        Err(err) => {
            log_error(format!("Failed to add FIDO2 security key: {err}"));
            state
                .platform
                .overlay
                .add_toast(Toast::new(&gettext(err.import_message())));
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
                .add_toast(Toast::new(&gettext("Hardware key added.")));
        }
        Err(err) => {
            log_error(format!("Failed to import hardware key: {err}"));
            state
                .platform
                .overlay
                .add_toast(Toast::new(&gettext(err.import_message())));
        }
    }
}

const HARDWARE_KEY_GENERATION_TITLE: &str = "Set up new hardware key";
const HARDWARE_KEY_GENERATION_SUBTITLE: &str =
    "Create a new OpenPGP key on the connected hardware token.";

#[derive(Clone, Debug)]
struct HardwareKeyGenerationPageRequest {
    name: String,
    email: String,
    admin_pin: SecretString,
    user_pin: SecretString,
}

fn validate_hardware_key_generation_request(
    name: &str,
    email: &str,
) -> Result<(String, String), &'static str> {
    let name = name.trim();
    if name.is_empty() {
        return Err("Enter a name.");
    }

    let email = validate_email_address(email.trim())?;
    Ok((name.to_string(), email))
}

fn build_hardware_key_generation_request(
    name: &str,
    email: &str,
    admin_pin: &str,
    user_pin: &str,
) -> Result<HardwareKeyGenerationPageRequest, &'static str> {
    let (name, email) = validate_hardware_key_generation_request(name, email)?;
    if admin_pin.trim().is_empty() {
        return Err("Enter the hardware key admin PIN.");
    }
    if user_pin.trim().is_empty() {
        return Err("Enter the new hardware key PIN.");
    }

    Ok(HardwareKeyGenerationPageRequest {
        name,
        email,
        admin_pin: SecretString::from(admin_pin),
        user_pin: SecretString::from(user_pin),
    })
}

fn hardware_key_generation_apply_enabled(
    name: &str,
    email: &str,
    admin_pin: &str,
    user_pin: &str,
) -> bool {
    !name.trim().is_empty()
        && !email.trim().is_empty()
        && !admin_pin.trim().is_empty()
        && !user_pin.trim().is_empty()
}

fn sync_hardware_key_generation_apply_button(
    name_row: &adw::EntryRow,
    email_row: &adw::EntryRow,
    admin_pin_row: &adw::PasswordEntryRow,
    user_pin_row: &adw::PasswordEntryRow,
) {
    user_pin_row.set_show_apply_button(hardware_key_generation_apply_enabled(
        &name_row.text(),
        &email_row.text(),
        &admin_pin_row.text(),
        &user_pin_row.text(),
    ));
}

fn start_private_key_import(
    state: &StoreRecipientsPageState,
    bytes: Vec<u8>,
    passphrase: Option<SecretString>,
) {
    let state = state.clone();
    let progress_dialog = PrivateKeyDialogHandle::new(&build_private_key_progress_dialog(
        &state.window,
        "Importing key",
        None,
        "Wait a moment.",
    ));
    let state_for_disconnect = state.clone();
    spawn_result_task_with_finalizer(
        move || {
            import_ripasso_private_key_bytes(
                &bytes,
                passphrase
                    .as_ref()
                    .map(|passphrase| passphrase.expose_secret()),
            )
        },
        move || progress_dialog.force_close(),
        move |result| {
            finish_private_key_import(&state, result);
        },
        move || {
            log_error("Private key import worker disconnected unexpectedly.".to_string());
            state_for_disconnect
                .platform
                .overlay
                .add_toast(Toast::new(&gettext("Couldn't import the key.")));
        },
    );
}

fn start_fido2_recipient_add(state: &StoreRecipientsPageState, pin: Option<SecretString>) {
    if !ensure_fido2_recipient_actions_allowed(state) {
        return;
    }

    if !Preferences::new().uses_integrated_backend() {
        state.platform.overlay.add_toast(Toast::new(&gettext(
            "Switch to the Integrated backend to add a FIDO2 security key.",
        )));
        return;
    }

    let state = state.clone();
    let progress_dialog = PrivateKeyDialogHandle::new(&build_private_key_progress_dialog(
        &state.window,
        "Adding FIDO2 security key",
        None,
        "Touch your key if it blinks.",
    ));
    let state_for_disconnect = state.clone();
    let pin_was_supplied = pin.is_some();
    spawn_result_task_with_finalizer(
        move || create_fido2_store_recipient(pin.as_ref().map(|pin| pin.expose_secret())),
        move || progress_dialog.force_close(),
        move |result| match result {
            Err(err)
                if err.is_fido2_pin_not_set()
                    && !pin_was_supplied
                    && supports_first_time_fido2_pin_setup() =>
            {
                prompt_fido2_recipient_pin_setup(&state);
            }
            Err(err) if err.is_fido2_pin_required() && !pin_was_supplied => {
                prompt_fido2_recipient_pin(&state);
            }
            other => finish_fido2_recipient_add(&state, other),
        },
        move || {
            log_error("FIDO2 recipient worker disconnected unexpectedly.".to_string());
            state_for_disconnect
                .platform
                .overlay
                .add_toast(Toast::new(&gettext("Couldn't add the FIDO2 security key.")));
        },
    );
}

fn start_fido2_recipient_pin_setup(state: &StoreRecipientsPageState, pin: SecretString) {
    if !ensure_fido2_recipient_actions_allowed(state) {
        return;
    }

    let state = state.clone();
    let progress_dialog = PrivateKeyDialogHandle::new(&build_private_key_progress_dialog(
        &state.window,
        "Set security key PIN",
        None,
        "Touch your key if it blinks.",
    ));
    let state_for_disconnect = state.clone();
    spawn_result_task_with_finalizer(
        move || {
            set_fido2_security_key_pin(pin.expose_secret())?;
            create_fido2_store_recipient(Some(pin.expose_secret()))
        },
        move || progress_dialog.force_close(),
        move |result| finish_fido2_recipient_add(&state, result),
        move || {
            log_error("FIDO2 PIN setup worker disconnected unexpectedly.".to_string());
            state_for_disconnect
                .platform
                .overlay
                .add_toast(Toast::new(&gettext("Couldn't add the FIDO2 security key.")));
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
                    .add_toast(Toast::new(&gettext("Connect a hardware key first.")));
                None
            }
            1 => tokens.pop(),
            _ => {
                state.platform.overlay.add_toast(Toast::new(&gettext(
                    "Connect only one hardware key before adding it.",
                )));
                None
            }
        },
        Err(err) => {
            log_error(format!("Failed to discover hardware keys: {err}"));
            state
                .platform
                .overlay
                .add_toast(Toast::new(&gettext("Couldn't inspect the hardware key.")));
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
        "Wait a moment.",
    ));
    let state_for_disconnect = state.clone();
    spawn_result_task_with_finalizer(
        move || import_ripasso_hardware_key_bytes(&bytes, hardware.clone()),
        move || progress_dialog.force_close(),
        move |result| {
            finish_hardware_key_import(&state, result);
        },
        move || {
            log_error("Hardware key import worker disconnected unexpectedly.".to_string());
            state_for_disconnect
                .platform
                .overlay
                .add_toast(Toast::new(&gettext("Couldn't add the hardware key.")));
        },
    );
}

fn start_hardware_key_generation(
    state: &StoreRecipientsPageState,
    request: HardwareKeyGenerationPageRequest,
) {
    let Some(token) = state
        .platform
        .hardware_key_generation_token
        .borrow()
        .clone()
    else {
        state
            .platform
            .overlay
            .add_toast(Toast::new(&gettext("Connect a hardware key first.")));
        return;
    };

    set_hardware_key_generation_loading(state, true);
    let state = state.clone();
    let state_for_finalize = state.clone();
    let state_for_disconnect = state.clone();
    spawn_result_task_with_finalizer(
        move || {
            let HardwareKeyGenerationPageRequest {
                name,
                email,
                admin_pin,
                user_pin,
            } = request;
            crate::backend::generate_ripasso_hardware_key(
                &token.ident,
                token.reader_hint.as_deref(),
                &name,
                &email,
                admin_pin,
                user_pin,
                true,
            )
        },
        move || set_hardware_key_generation_loading(&state_for_finalize, false),
        move |result| {
            finish_hardware_key_generation(&state, result);
        },
        move || {
            log_error("Hardware key generation worker disconnected unexpectedly.".to_string());
            state_for_disconnect
                .platform
                .overlay
                .add_toast(Toast::new(&gettext("Couldn't add the hardware key.")));
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
                .add_toast(Toast::new(&gettext(err.inspection_message())));
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
    let state_for_response = state.clone();
    choose_file_bytes(
        &state.window,
        title,
        "Import",
        &state.platform.overlay,
        "Failed to read the selected hardware public key file",
        "Couldn't read that file.",
        move |bytes| {
            import_hardware_key_bytes(&state_for_response, bytes, hardware.clone());
        },
    );
}

fn clear_hardware_key_generation_form(state: &StoreRecipientsPageState) {
    state.platform.hardware_key_generation_name_row.set_text("");
    state
        .platform
        .hardware_key_generation_email_row
        .set_text("");
    state
        .platform
        .hardware_key_generation_admin_pin_row
        .set_text("");
    state
        .platform
        .hardware_key_generation_user_pin_row
        .set_text("");
}

fn set_hardware_key_generation_loading(state: &StoreRecipientsPageState, loading: bool) {
    state
        .platform
        .hardware_key_generation_in_flight
        .set(loading);
    let visible_child: &adw::gtk::Widget = if loading {
        state.platform.hardware_key_generation_loading.upcast_ref()
    } else {
        state.platform.hardware_key_generation_form.upcast_ref()
    };
    state
        .platform
        .hardware_key_generation_stack
        .set_visible_child(visible_child);
}

fn pop_hardware_key_generation_page_if_visible(state: &StoreRecipientsPageState) {
    if !visible_navigation_page_is(&state.nav, &state.platform.hardware_key_generation_page) {
        return;
    }

    state.nav.pop();
    sync_store_recipients_page_header(state);
}

fn finish_hardware_key_generation(
    state: &StoreRecipientsPageState,
    result: Result<ManagedRipassoPrivateKey, PrivateKeyError>,
) {
    match result {
        Ok(key) => {
            clear_hardware_key_generation_form(state);
            state
                .platform
                .hardware_key_generation_token
                .borrow_mut()
                .take();
            pop_hardware_key_generation_page_if_visible(state);
            finish_hardware_key_import(state, Ok(key));
        }
        Err(err) => {
            log_error(format!("Failed to set up hardware key: {err}"));
            state
                .platform
                .overlay
                .add_toast(Toast::new(&gettext(err.import_message())));
        }
    }
}

fn show_hardware_key_generation_page(
    state: &StoreRecipientsPageState,
    token: DiscoveredHardwareToken,
) {
    let chrome = state.window_chrome();
    show_secondary_page_chrome(
        &chrome,
        HARDWARE_KEY_GENERATION_TITLE,
        HARDWARE_KEY_GENERATION_SUBTITLE,
        false,
    );
    push_navigation_page_if_needed(&state.nav, &state.platform.hardware_key_generation_page);
    state
        .platform
        .hardware_key_generation_token
        .borrow_mut()
        .replace(token);

    if state.platform.hardware_key_generation_in_flight.get() {
        set_hardware_key_generation_loading(state, true);
        return;
    }

    clear_hardware_key_generation_form(state);
    set_hardware_key_generation_loading(state, false);
    state.platform.hardware_key_generation_name_row.grab_focus();
}

fn add_connected_hardware_key(state: &StoreRecipientsPageState) {
    if !ensure_standard_recipient_actions_allowed(state) {
        return;
    }

    let Some(token) = selected_hardware_token(state) else {
        return;
    };
    let hardware = hardware_key_from_token(&token);
    if let Some(bytes) = token.cardholder_certificate {
        import_hardware_key_bytes(state, bytes, hardware);
        return;
    }
    if token.signing_fingerprint.is_some() || token.decryption_fingerprint.is_some() {
        state.platform.overlay.add_toast(Toast::new(&gettext(
            "This hardware key already has OpenPGP keys. Import the matching public key file instead.",
        )));
        return;
    }

    state.platform.overlay.add_toast(Toast::new(&gettext(
        "This hardware key has no OpenPGP key yet. Use Set up new hardware key instead.",
    )));
}

fn setup_connected_hardware_key(state: &StoreRecipientsPageState) {
    if !ensure_standard_recipient_actions_allowed(state) {
        return;
    }

    let Some(token) = selected_hardware_token(state) else {
        return;
    };
    if token.cardholder_certificate.is_some()
        || token.signing_fingerprint.is_some()
        || token.decryption_fingerprint.is_some()
    {
        state.platform.overlay.add_toast(Toast::new(&gettext(
            "This hardware key already has OpenPGP keys. Use Add hardware key or import the matching public key file instead.",
        )));
        return;
    }

    show_hardware_key_generation_page(state, token);
}

fn import_hardware_key_from_file(state: &StoreRecipientsPageState) {
    if !ensure_standard_recipient_actions_allowed(state) {
        return;
    }

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
    let state_for_response = state.clone();
    choose_file_bytes(
        &state.window,
        "Import private key",
        "Import",
        &state.platform.overlay,
        "Failed to read the selected private key file",
        "Couldn't read that file.",
        move |bytes| {
            import_private_key_bytes(&state_for_response, bytes);
        },
    );
}

fn import_private_key_from_clipboard(state: &StoreRecipientsPageState) {
    let Some(display) = Display::default() else {
        state
            .platform
            .overlay
            .add_toast(Toast::new(&gettext("Clipboard unavailable.")));
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
                .add_toast(Toast::new(&gettext("Clipboard does not contain a key.")));
        }
        Err(err) => {
            log_error(format!("Failed to read private key from clipboard: {err}"));
            state_for_response
                .platform
                .overlay
                .add_toast(Toast::new(&gettext("Couldn't read the clipboard.")));
        }
    });
}

fn prompt_fido2_recipient_pin(state: &StoreRecipientsPageState) {
    let window = state.window.clone();
    let overlay = state.platform.overlay.clone();
    let state = state.clone();
    present_private_key_unlock_dialog_with_close_handler(
        &window,
        &overlay,
        "Add FIDO2 security key",
        None,
        PrivateKeyUnlockKind::Fido2SecurityKey,
        move |request| {
            let pin = match request {
                crate::backend::PrivateKeyUnlockRequest::Fido2(pin) => pin,
                _ => None,
            };
            start_fido2_recipient_add(&state, pin);
        },
        || {},
    );
}

fn prompt_fido2_recipient_pin_setup(state: &StoreRecipientsPageState) {
    let window = state.window.clone();
    let overlay = state.platform.overlay.clone();
    let state = state.clone();
    present_fido2_pin_setup_dialog_with_close_handler(
        &window,
        &overlay,
        "Set security key PIN",
        None,
        move |pin| {
            start_fido2_recipient_pin_setup(&state, pin);
        },
        || {},
    );
}

pub(super) fn connect_hardware_key_generation_submit(state: &StoreRecipientsPageState) {
    let overlay_for_apply = state.platform.overlay.clone();
    let state_for_apply = state.clone();
    let name_row = state.platform.hardware_key_generation_name_row.clone();
    let email_row = state.platform.hardware_key_generation_email_row.clone();
    let admin_pin_row = state.platform.hardware_key_generation_admin_pin_row.clone();
    let user_pin_row = state.platform.hardware_key_generation_user_pin_row.clone();
    let user_pin_row_for_apply = user_pin_row.clone();

    sync_hardware_key_generation_apply_button(&name_row, &email_row, &admin_pin_row, &user_pin_row);
    {
        let name_row_for_signal = name_row.clone();
        let name_row_for_sync = name_row.clone();
        let email_row_for_sync = email_row.clone();
        let admin_pin_row_for_sync = admin_pin_row.clone();
        let user_pin_row_for_sync = user_pin_row.clone();
        name_row_for_signal.connect_changed(move |_| {
            sync_hardware_key_generation_apply_button(
                &name_row_for_sync,
                &email_row_for_sync,
                &admin_pin_row_for_sync,
                &user_pin_row_for_sync,
            );
        });
    }
    {
        let email_row_for_signal = email_row.clone();
        let name_row_for_sync = name_row.clone();
        let email_row_for_sync = email_row.clone();
        let admin_pin_row_for_sync = admin_pin_row.clone();
        let user_pin_row_for_sync = user_pin_row.clone();
        email_row_for_signal.connect_changed(move |_| {
            sync_hardware_key_generation_apply_button(
                &name_row_for_sync,
                &email_row_for_sync,
                &admin_pin_row_for_sync,
                &user_pin_row_for_sync,
            );
        });
    }
    {
        let admin_pin_row_for_signal = admin_pin_row.clone();
        let name_row_for_sync = name_row.clone();
        let email_row_for_sync = email_row.clone();
        let admin_pin_row_for_sync = admin_pin_row.clone();
        let user_pin_row_for_sync = user_pin_row.clone();
        admin_pin_row_for_signal.connect_changed(move |_| {
            sync_hardware_key_generation_apply_button(
                &name_row_for_sync,
                &email_row_for_sync,
                &admin_pin_row_for_sync,
                &user_pin_row_for_sync,
            );
        });
    }
    {
        let user_pin_row_for_signal = user_pin_row.clone();
        let name_row_for_sync = name_row.clone();
        let email_row_for_sync = email_row.clone();
        let admin_pin_row_for_sync = admin_pin_row.clone();
        let user_pin_row_for_sync = user_pin_row.clone();
        user_pin_row_for_signal.connect_changed(move |_| {
            sync_hardware_key_generation_apply_button(
                &name_row_for_sync,
                &email_row_for_sync,
                &admin_pin_row_for_sync,
                &user_pin_row_for_sync,
            );
        });
    }

    user_pin_row.connect_apply(move |_| {
        let request = match build_hardware_key_generation_request(
            &name_row.text(),
            &email_row.text(),
            &admin_pin_row.text(),
            &user_pin_row_for_apply.text(),
        ) {
            Ok(request) => request,
            Err(message) => {
                overlay_for_apply.add_toast(Toast::new(&gettext(message)));
                return;
            }
        };

        start_hardware_key_generation(&state_for_apply, request);
    });
}

pub(super) fn connect_hardware_key_generation_autofill(state: &StoreRecipientsPageState) {
    connect_generation_autofill_rows(
        &state.platform.hardware_key_generation_name_row,
        &state.platform.hardware_key_generation_email_row,
    );
}

pub(super) fn connect_private_key_import_controls(state: &StoreRecipientsPageState) {
    let setup_hardware_row = state.platform.setup_hardware_key_row.clone();
    let setup_hardware_state = state.clone();
    connect_row_action(&setup_hardware_row, move || {
        setup_connected_hardware_key(&setup_hardware_state);
    });

    let hardware_row = state.platform.add_hardware_key_row.clone();
    let hardware_state = state.clone();
    connect_row_action(&hardware_row, move || {
        add_connected_hardware_key(&hardware_state);
    });

    let fido2_row = state.platform.add_fido2_key_row.clone();
    let fido2_state = state.clone();
    connect_row_action(&fido2_row, move || {
        start_fido2_recipient_add(&fido2_state, None);
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

#[cfg(test)]
mod tests {
    use super::hardware_key_generation_apply_enabled;

    #[test]
    fn hardware_key_generation_apply_requires_all_nonempty_fields() {
        assert!(!hardware_key_generation_apply_enabled(
            "",
            "user@example.com",
            "12345678",
            "123456"
        ));
        assert!(!hardware_key_generation_apply_enabled(
            "User", "", "12345678", "123456"
        ));
        assert!(!hardware_key_generation_apply_enabled(
            "User",
            "user@example.com",
            "",
            "123456"
        ));
        assert!(!hardware_key_generation_apply_enabled(
            "User",
            "user@example.com",
            "12345678",
            ""
        ));
        assert!(hardware_key_generation_apply_enabled(
            "User",
            "user@example.com",
            "12345678",
            "123456"
        ));
    }
}
