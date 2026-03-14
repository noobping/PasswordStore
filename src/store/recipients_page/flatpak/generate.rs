use super::list::rebuild_store_recipients_list;
use super::StoreRecipientsPageState;
use crate::backend::{generate_ripasso_private_key, ManagedRipassoPrivateKey, PrivateKeyError};
use crate::logging::log_error;
use crate::private_key::dialog::build_private_key_progress_dialog;
use crate::support::background::spawn_result_task;
use crate::support::ui::append_action_row_with_button;
use adw::glib::object::IsA;
use adw::gtk::{Box as GtkBox, Orientation};
use adw::prelude::*;
use adw::{
    Dialog, EntryRow, HeaderBar, PasswordEntryRow, PreferencesGroup, PreferencesPage, Toast,
    WindowTitle,
};

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

fn dialog_content_shell(
    title: &str,
    subtitle: Option<&str>,
    child: &impl IsA<adw::gtk::Widget>,
) -> GtkBox {
    let window_title = WindowTitle::builder().title(title).build();
    if let Some(subtitle) = subtitle.filter(|value| !value.trim().is_empty()) {
        window_title.set_subtitle(subtitle);
    }

    let header = HeaderBar::new();
    header.set_title_widget(Some(&window_title));

    let shell = GtkBox::new(Orientation::Vertical, 0);
    shell.append(&header);
    shell.append(child);
    shell
}

fn finish_private_key_generation(
    state: &StoreRecipientsPageState,
    result: Result<ManagedRipassoPrivateKey, PrivateKeyError>,
) {
    match result {
        Ok(_) => {
            rebuild_store_recipients_list(state);
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
    let progress_dialog =
        build_private_key_progress_dialog(&state.window, "Generating key", None, "Please wait.");
    let state = state.clone();
    let progress_dialog_for_disconnect = progress_dialog.clone();
    let state_for_disconnect = state.clone();
    spawn_result_task(
        move || generate_ripasso_private_key(&request.name, &request.email, &request.passphrase),
        move |result| {
            progress_dialog.force_close();
            finish_private_key_generation(&state, result);
        },
        move || {
            progress_dialog_for_disconnect.force_close();
            log_error("Private key generation worker disconnected unexpectedly.".to_string());
            state_for_disconnect
                .platform
                .overlay
                .add_toast(Toast::new("Couldn't generate the key."));
        },
    );
}

fn present_private_key_generation_dialog(state: &StoreRecipientsPageState) {
    let name_row = EntryRow::new();
    name_row.set_title("Name");

    let email_row = EntryRow::new();
    email_row.set_title("Email");

    let password_row = PasswordEntryRow::new();
    password_row.set_title("Key password");

    let confirm_row = PasswordEntryRow::new();
    confirm_row.set_title("Confirm password");
    confirm_row.set_show_apply_button(true);

    let group = PreferencesGroup::builder().build();
    group.add(&name_row);
    group.add(&email_row);
    group.add(&password_row);
    group.add(&confirm_row);

    let page = PreferencesPage::new();
    page.add(&group);

    let dialog = Dialog::builder()
        .title("Generate private key")
        .content_width(460)
        .child(&dialog_content_shell(
            "Generate private key",
            Some("Create a password-protected private key for password stores."),
            &page,
        ))
        .build();

    let dialog_for_apply = dialog.clone();
    let overlay_for_apply = state.platform.overlay.clone();
    let state_for_apply = state.clone();
    let name_row_for_apply = name_row.clone();
    let email_row_for_apply = email_row.clone();
    let password_row_for_apply = password_row.clone();
    let confirm_row_for_apply = confirm_row.clone();
    confirm_row.connect_apply(move |_| {
        let request = match validate_private_key_generation_request(
            &name_row_for_apply.text(),
            &email_row_for_apply.text(),
            &password_row_for_apply.text(),
            &confirm_row_for_apply.text(),
        ) {
            Ok(request) => request,
            Err(message) => {
                overlay_for_apply.add_toast(Toast::new(message));
                return;
            }
        };

        dialog_for_apply.close();
        start_private_key_generation(&state_for_apply, request);
    });

    dialog.present(Some(&state.window));
}

pub(super) fn append_private_key_generate_row(state: &StoreRecipientsPageState) {
    let list = state.list.clone();
    let state = state.clone();
    append_action_row_with_button(
        &list,
        "Generate private key",
        "Create a new password-protected key.",
        "document-new-symbolic",
        move || present_private_key_generation_dialog(&state),
    );
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
