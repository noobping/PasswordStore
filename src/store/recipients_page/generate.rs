use super::list::rebuild_store_recipients_list;
use super::mode::ensure_standard_recipient_actions_allowed;
use super::sync::sync_private_keys_to_host_if_enabled;
use super::{sync_store_recipients_page_header, StoreRecipientsPageState};
use crate::backend::{
    generate_fido2_private_key, generate_ripasso_private_key, ManagedRipassoPrivateKey,
    PrivateKeyError, PrivateKeyUnlockKind, PrivateKeyUnlockRequest,
};
use crate::i18n::gettext;
use crate::logging::log_error;
use crate::private_key::dialog::{
    build_private_key_progress_dialog, present_private_key_unlock_dialog_with_close_handler,
    PrivateKeyDialogHandle,
};
use crate::support::actions::activate_widget_action;
use crate::support::background::spawn_result_task_with_finalizer;
use crate::support::ui::{
    connect_row_action, push_navigation_page_if_needed, visible_navigation_page_is,
};
use crate::support::validation::validate_email_address;
use crate::window::navigation::{show_secondary_page_chrome, HasWindowChrome};
use adw::prelude::*;
use adw::Toast;
use secrecy::{ExposeSecret, SecretString};
use std::cell::{Cell, RefCell};
use std::rc::Rc;

const PRIVATE_KEY_GENERATION_TITLE: &str = "Generate private key";
const PRIVATE_KEY_GENERATION_SUBTITLE: &str =
    "Create a password-protected private key for password stores.";

#[derive(Clone, Debug)]
struct PrivateKeyGenerationRequest {
    name: String,
    email: String,
    passphrase: SecretString,
}

impl PartialEq for PrivateKeyGenerationRequest {
    fn eq(&self, other: &Self) -> bool {
        self.name == other.name
            && self.email == other.email
            && self.passphrase.expose_secret() == other.passphrase.expose_secret()
    }
}

impl Eq for PrivateKeyGenerationRequest {}

fn validate_name_and_email(name: &str, email: &str) -> Result<(String, String), &'static str> {
    let name = name.trim();
    if name.is_empty() {
        return Err("Enter a name.");
    }

    let email = email.trim();
    let email = validate_email_address(email)?;

    Ok((name.to_string(), email))
}

fn validate_private_key_generation_request(
    name: &str,
    email: &str,
    passphrase: &str,
    confirmation: &str,
) -> Result<PrivateKeyGenerationRequest, &'static str> {
    let (name, email) = validate_name_and_email(name, email)?;
    if passphrase.trim().is_empty() {
        return Err("Enter a key password.");
    }
    if passphrase != confirmation {
        return Err("The passwords do not match.");
    }

    Ok(PrivateKeyGenerationRequest {
        name,
        email,
        passphrase: SecretString::from(passphrase),
    })
}

fn suggested_name_from_email(email: &str) -> Option<String> {
    let suggested = email
        .trim()
        .split_once('@')
        .map(|(local, _)| local.trim())
        .unwrap_or_default();
    (!suggested.is_empty()).then(|| suggested.to_string())
}

fn suggested_email_from_name(name: &str) -> Option<String> {
    let suggested = name.trim();
    (!suggested.is_empty()).then(|| format!("{suggested}@pass.store"))
}

fn next_autofilled_value(
    current_value: &str,
    previous_autofill: Option<&str>,
    suggestion: Option<String>,
) -> Option<String> {
    let current_value = current_value.trim();
    if !(current_value.is_empty() || previous_autofill == Some(current_value)) {
        return None;
    }

    Some(suggestion.unwrap_or_default())
}

fn finish_private_key_generation(
    state: &StoreRecipientsPageState,
    result: Result<ManagedRipassoPrivateKey, PrivateKeyError>,
) {
    match result {
        Ok(_) => {
            clear_private_key_generation_form(state);
            pop_private_key_generation_page_if_visible(state);
            finish_generated_key(state);
        }
        Err(err) => {
            log_error(format!("Failed to generate private key: {err}"));
            state
                .platform
                .overlay
                .add_toast(Toast::new(&gettext("Couldn't generate the key.")));
        }
    }
}

fn finish_generated_key(state: &StoreRecipientsPageState) {
    let _ = sync_private_keys_to_host_if_enabled(state);
    rebuild_store_recipients_list(state);
    activate_widget_action(&state.window, "win.reload-password-list");
    state
        .platform
        .overlay
        .add_toast(Toast::new(&gettext("Key generated.")));
}

fn start_private_key_generation(
    state: &StoreRecipientsPageState,
    request: PrivateKeyGenerationRequest,
) {
    set_private_key_generation_loading(state, true);
    let state = state.clone();
    let state_for_finalize = state.clone();
    let state_for_disconnect = state.clone();
    spawn_result_task_with_finalizer(
        move || {
            generate_ripasso_private_key(
                &request.name,
                &request.email,
                request.passphrase.expose_secret(),
            )
        },
        move || set_private_key_generation_loading(&state_for_finalize, false),
        move |result| {
            finish_private_key_generation(&state, result);
        },
        move || {
            log_error("Private key generation worker disconnected unexpectedly.".to_string());
            state_for_disconnect
                .platform
                .overlay
                .add_toast(Toast::new(&gettext("Couldn't generate the key.")));
        },
    );
}

fn finish_fido2_private_key_generation(
    state: &StoreRecipientsPageState,
    result: Result<ManagedRipassoPrivateKey, PrivateKeyError>,
) {
    match result {
        Ok(_) => finish_generated_key(state),
        Err(err) => {
            log_error(format!("Failed to generate FIDO2-protected key: {err}"));
            state
                .platform
                .overlay
                .add_toast(Toast::new(&gettext(err.import_message())));
        }
    }
}

fn start_fido2_private_key_generation(state: &StoreRecipientsPageState, pin: Option<SecretString>) {
    if !ensure_standard_recipient_actions_allowed(state) {
        return;
    }

    let state = state.clone();
    let progress_dialog = PrivateKeyDialogHandle::new(&build_private_key_progress_dialog(
        &state.window,
        "Generating FIDO2-protected key",
        None,
        "Touch it if it starts blinking.",
    ));
    let state_for_disconnect = state.clone();
    let pin_was_supplied = pin.is_some();
    spawn_result_task_with_finalizer(
        move || generate_fido2_private_key(pin.as_ref().map(|pin| pin.expose_secret())),
        move || progress_dialog.force_close(),
        move |result| match result {
            Err(err) if err.is_fido2_pin_required() && !pin_was_supplied => {
                prompt_fido2_private_key_pin(&state);
            }
            other => finish_fido2_private_key_generation(&state, other),
        },
        move || {
            log_error(
                "FIDO2-protected key generation worker disconnected unexpectedly.".to_string(),
            );
            state_for_disconnect
                .platform
                .overlay
                .add_toast(Toast::new(&gettext("Couldn't generate the key.")));
        },
    );
}

fn prompt_fido2_private_key_pin(state: &StoreRecipientsPageState) {
    let window = state.window.clone();
    let overlay = state.platform.overlay.clone();
    let state = state.clone();
    present_private_key_unlock_dialog_with_close_handler(
        &window,
        &overlay,
        "Generate FIDO2-protected key",
        None,
        PrivateKeyUnlockKind::Fido2SecurityKey,
        move |request| {
            let pin = match request {
                PrivateKeyUnlockRequest::Fido2(pin) => pin,
                _ => None,
            };
            start_fido2_private_key_generation(&state, pin);
        },
        || {},
    );
}

fn clear_private_key_generation_form(state: &StoreRecipientsPageState) {
    state.platform.private_key_generation_name_row.set_text("");
    state.platform.private_key_generation_email_row.set_text("");
    state
        .platform
        .private_key_generation_password_row
        .set_text("");
    state
        .platform
        .private_key_generation_confirm_row
        .set_text("");
}

fn set_private_key_generation_loading(state: &StoreRecipientsPageState, loading: bool) {
    state.platform.private_key_generation_in_flight.set(loading);
    let visible_child: &adw::gtk::Widget = if loading {
        state.platform.private_key_generation_loading.upcast_ref()
    } else {
        state.platform.private_key_generation_form.upcast_ref()
    };
    state
        .platform
        .private_key_generation_stack
        .set_visible_child(visible_child);
}

fn pop_private_key_generation_page_if_visible(state: &StoreRecipientsPageState) {
    if !visible_navigation_page_is(&state.nav, &state.platform.private_key_generation_page) {
        return;
    }

    state.nav.pop();
    sync_store_recipients_page_header(state);
}

fn show_private_key_generation_page(state: &StoreRecipientsPageState) {
    if !ensure_standard_recipient_actions_allowed(state) {
        return;
    }

    let chrome = state.window_chrome();
    show_secondary_page_chrome(
        &chrome,
        PRIVATE_KEY_GENERATION_TITLE,
        PRIVATE_KEY_GENERATION_SUBTITLE,
        false,
    );
    push_navigation_page_if_needed(&state.nav, &state.platform.private_key_generation_page);

    if state.platform.private_key_generation_in_flight.get() {
        set_private_key_generation_loading(state, true);
        return;
    }

    clear_private_key_generation_form(state);
    set_private_key_generation_loading(state, false);
    state.platform.private_key_generation_name_row.grab_focus();
}

pub(super) fn connect_private_key_generation_submit(state: &StoreRecipientsPageState) {
    let overlay_for_apply = state.platform.overlay.clone();
    let state_for_apply = state.clone();
    let name_row = state.platform.private_key_generation_name_row.clone();
    let email_row = state.platform.private_key_generation_email_row.clone();
    let password_row = state.platform.private_key_generation_password_row.clone();
    let confirm_row = state.platform.private_key_generation_confirm_row.clone();
    let confirm_row_for_apply = confirm_row.clone();

    confirm_row.connect_apply(move |_| {
        let request = match validate_private_key_generation_request(
            &name_row.text(),
            &email_row.text(),
            &password_row.text(),
            &confirm_row_for_apply.text(),
        ) {
            Ok(request) => request,
            Err(message) => {
                overlay_for_apply.add_toast(Toast::new(&gettext(message)));
                return;
            }
        };

        start_private_key_generation(&state_for_apply, request);
    });
}

pub(super) fn connect_generation_autofill_rows(
    name_row: &adw::EntryRow,
    email_row: &adw::EntryRow,
) {
    let name_row = name_row.clone();
    let email_row = email_row.clone();
    let syncing = Rc::new(Cell::new(false));
    let last_autofilled_name = Rc::new(RefCell::new(None::<String>));
    let last_autofilled_email = Rc::new(RefCell::new(None::<String>));

    {
        let name_row = name_row.clone();
        let syncing = syncing.clone();
        let last_autofilled_name = last_autofilled_name.clone();
        email_row.connect_changed(move |row| {
            if syncing.get() {
                return;
            }

            let next_name = next_autofilled_value(
                &name_row.text(),
                last_autofilled_name.borrow().as_deref(),
                suggested_name_from_email(&row.text()),
            );
            let Some(name) = next_name else {
                last_autofilled_name.borrow_mut().take();
                return;
            };

            let tracked_name = (!name.is_empty()).then_some(name.clone());
            syncing.set(true);
            name_row.set_text(&name);
            syncing.set(false);
            last_autofilled_name.replace(tracked_name);
        });
    }

    {
        let email_row = email_row.clone();
        let last_autofilled_email = last_autofilled_email.clone();
        name_row.connect_changed(move |row| {
            if syncing.get() {
                return;
            }

            let next_email = next_autofilled_value(
                &email_row.text(),
                last_autofilled_email.borrow().as_deref(),
                suggested_email_from_name(&row.text()),
            );
            let Some(email) = next_email else {
                last_autofilled_email.borrow_mut().take();
                return;
            };

            let tracked_email = (!email.is_empty()).then_some(email.clone());
            syncing.set(true);
            email_row.set_text(&email);
            syncing.set(false);
            last_autofilled_email.replace(tracked_email);
        });
    }
}

pub(super) fn connect_private_key_generation_autofill(state: &StoreRecipientsPageState) {
    connect_generation_autofill_rows(
        &state.platform.private_key_generation_name_row,
        &state.platform.private_key_generation_email_row,
    );
}

pub(super) fn connect_private_key_generate_controls(state: &StoreRecipientsPageState) {
    let row = state.platform.generate_key_row.clone();
    let state_for_password = state.clone();
    connect_row_action(&row, move || {
        show_private_key_generation_page(&state_for_password);
    });

    let fido2_row = state.platform.generate_fido2_key_row.clone();
    let state_for_fido2 = state.clone();
    connect_row_action(&fido2_row, move || {
        start_fido2_private_key_generation(&state_for_fido2, None);
    });
}

#[cfg(test)]
mod tests {
    use super::{
        next_autofilled_value, suggested_email_from_name, suggested_name_from_email,
        validate_private_key_generation_request,
    };

    #[test]
    fn generation_request_requires_name_email_and_matching_passwords() {
        assert_eq!(
            validate_private_key_generation_request("", "user@example.com", "hunter2", "hunter2"),
            Err("Enter a name.")
        );
        assert_eq!(
            validate_private_key_generation_request("User", "", "hunter2", "hunter2"),
            Err("Enter an email address.")
        );
        assert_eq!(
            validate_private_key_generation_request("User", "invalid", "hunter2", "hunter2"),
            Err("Enter a valid email address.")
        );
        assert_eq!(
            validate_private_key_generation_request("User", "user@example.com", "hunter2", "other"),
            Err("The passwords do not match.")
        );
    }

    #[test]
    fn autofill_helpers_only_fill_empty_fields() {
        assert_eq!(
            next_autofilled_value("", None, Some("Alice".to_string())),
            Some("Alice".to_string())
        );
        assert_eq!(
            next_autofilled_value("custom", None, Some("Alice".to_string())),
            None
        );
        assert_eq!(
            next_autofilled_value("Alice", Some("Alice"), Some("Bob".to_string())),
            Some("Bob".to_string())
        );
    }

    #[test]
    fn autofill_suggestions_match_expected_patterns() {
        assert_eq!(
            suggested_name_from_email("alice@example.com").as_deref(),
            Some("alice")
        );
        assert_eq!(
            suggested_email_from_name("Alice Example").as_deref(),
            Some("Alice Example@pass.store")
        );
    }
}
