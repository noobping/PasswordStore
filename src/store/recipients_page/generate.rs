use super::list::rebuild_store_recipients_list;
use super::{sync_store_recipients_page_header, StoreRecipientsPageState};
use crate::backend::{generate_ripasso_private_key, ManagedRipassoPrivateKey, PrivateKeyError};
use crate::logging::log_error;
use crate::support::actions::activate_widget_action;
use crate::support::background::spawn_result_task;
use crate::support::ui::{
    connect_row_action, push_navigation_page_if_needed, visible_navigation_page_is,
};
use crate::window::navigation::{show_secondary_page_chrome, HasWindowChrome};
use adw::prelude::*;
use adw::Toast;

const PRIVATE_KEY_GENERATION_TITLE: &str = "Generate private key";
const PRIVATE_KEY_GENERATION_SUBTITLE: &str =
    "Create a password-protected private key for password stores.";

#[derive(Clone, Debug, PartialEq, Eq)]
struct PrivateKeyGenerationRequest {
    name: String,
    email: String,
    passphrase: String,
}

fn validate_private_key_generation_request(
    name: &str,
    email: &str,
    passphrase: &str,
    confirmation: &str,
) -> Result<PrivateKeyGenerationRequest, &'static str> {
    let name = name.trim();
    if name.is_empty() {
        return Err("Enter a name.");
    }

    let email = email.trim();
    if email.is_empty() {
        return Err("Enter an email address.");
    }
    if !email.contains('@') {
        return Err("Enter a valid email address.");
    }

    if passphrase.trim().is_empty() {
        return Err("Enter a key password.");
    }
    if passphrase != confirmation {
        return Err("The passwords do not match.");
    }

    Ok(PrivateKeyGenerationRequest {
        name: name.to_string(),
        email: email.to_string(),
        passphrase: passphrase.to_string(),
    })
}

fn finish_private_key_generation(
    state: &StoreRecipientsPageState,
    result: Result<ManagedRipassoPrivateKey, PrivateKeyError>,
) {
    set_private_key_generation_loading(state, false);

    match result {
        Ok(_) => {
            clear_private_key_generation_form(state);
            pop_private_key_generation_page_if_visible(state);
            rebuild_store_recipients_list(state);
            activate_widget_action(&state.window, "win.reload-password-list");
            state
                .platform
                .overlay
                .add_toast(Toast::new("Key generated."));
        }
        Err(err) => {
            log_error(format!("Failed to generate private key: {err}"));
            state
                .platform
                .overlay
                .add_toast(Toast::new("Couldn't generate the key."));
        }
    }
}

fn start_private_key_generation(
    state: &StoreRecipientsPageState,
    request: PrivateKeyGenerationRequest,
) {
    set_private_key_generation_loading(state, true);
    let state = state.clone();
    let state_for_disconnect = state.clone();
    spawn_result_task(
        move || generate_ripasso_private_key(&request.name, &request.email, &request.passphrase),
        move |result| {
            finish_private_key_generation(&state, result);
        },
        move || {
            set_private_key_generation_loading(&state_for_disconnect, false);
            log_error("Private key generation worker disconnected unexpectedly.".to_string());
            state_for_disconnect
                .platform
                .overlay
                .add_toast(Toast::new("Couldn't generate the key."));
        },
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
                overlay_for_apply.add_toast(Toast::new(message));
                return;
            }
        };

        start_private_key_generation(&state_for_apply, request);
    });
}

pub(super) fn connect_private_key_generate_controls(state: &StoreRecipientsPageState) {
    let row = state.platform.generate_key_row.clone();
    let state = state.clone();
    connect_row_action(&row, move || {
        show_private_key_generation_page(&state);
    });
}

#[cfg(test)]
mod tests {
    use super::validate_private_key_generation_request;

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
}
