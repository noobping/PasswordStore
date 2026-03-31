use crate::backend::{PrivateKeyUnlockKind, PrivateKeyUnlockRequest};
use crate::i18n::gettext;
use crate::support::ui::dialog_content_shell;
use adw::gtk::{Align, Box as GtkBox, Button, Label, Orientation, Spinner};
use adw::prelude::*;
use adw::{
    ApplicationWindow, Dialog, PasswordEntryRow, PreferencesGroup, PreferencesPage, StatusPage,
    ToastOverlay,
};
use std::cell::Cell;
use std::rc::Rc;

#[derive(Clone)]
pub struct PrivateKeyDialogHandle {
    dialog: Dialog,
}

impl PrivateKeyDialogHandle {
    pub fn new(dialog: &Dialog) -> Self {
        Self {
            dialog: dialog.clone(),
        }
    }

    pub fn force_close(&self) {
        self.dialog.force_close();
    }
}

pub fn build_private_key_progress_dialog(
    window: &ApplicationWindow,
    title: &str,
    subtitle: Option<&str>,
    description: &str,
) -> Dialog {
    let status = StatusPage::builder().build();
    let translated_description = gettext(description);
    status.set_description(
        Some(translated_description.as_str()).filter(|description| !description.trim().is_empty()),
    );
    status.set_child(Some(&Spinner::builder().spinning(true).build()));

    let dialog = Dialog::builder()
        .title(&gettext(title))
        .content_width(460)
        .child(&dialog_content_shell(title, subtitle, &status))
        .build();
    dialog.set_can_close(false);
    dialog.present(Some(window));
    dialog
}

fn private_key_password_dialog_error_message(passphrase: &str) -> Option<&'static str> {
    passphrase
        .trim()
        .is_empty()
        .then_some("Enter the key password.")
}

const HARDWARE_EXTERNAL_BUTTON_LABEL: &str = "Or use a hardware key.";

fn private_key_unlock_row_title(kind: PrivateKeyUnlockKind) -> &'static str {
    match kind {
        PrivateKeyUnlockKind::Password => "Key password",
        PrivateKeyUnlockKind::HardwareOpenPgpCard => "Hardware key PIN",
        PrivateKeyUnlockKind::Fido2SecurityKey => "Security key PIN",
    }
}

fn private_key_unlock_dialog_error_message(
    kind: PrivateKeyUnlockKind,
    input: &str,
) -> Option<&'static str> {
    if !input.trim().is_empty() {
        return None;
    }

    match kind {
        PrivateKeyUnlockKind::Password => Some("Enter the key password."),
        PrivateKeyUnlockKind::HardwareOpenPgpCard => Some("Enter the hardware key PIN."),
        PrivateKeyUnlockKind::Fido2SecurityKey => Some("Enter the security key PIN."),
    }
}

pub fn present_private_key_password_dialog<F>(
    window: &ApplicationWindow,
    overlay: &ToastOverlay,
    title: &str,
    subtitle: Option<&str>,
    on_submit: F,
) where
    F: Fn(String) + 'static,
{
    present_private_key_password_dialog_with_close_handler(
        window,
        overlay,
        title,
        subtitle,
        on_submit,
        || {},
    );
}

pub fn present_private_key_password_dialog_with_close_handler<F, G>(
    window: &ApplicationWindow,
    _overlay: &ToastOverlay,
    title: &str,
    subtitle: Option<&str>,
    on_submit: F,
    on_close: G,
) where
    F: Fn(String) + 'static,
    G: Fn() + 'static,
{
    let password_row = PasswordEntryRow::new();
    password_row.set_title(&gettext("Key password"));
    password_row.set_show_apply_button(true);

    let password_group = PreferencesGroup::builder().build();
    password_group.add(&password_row);

    let page = PreferencesPage::new();
    page.add(&password_group);

    let error_label = Label::new(None);
    error_label.set_halign(Align::Start);
    error_label.set_wrap(true);
    error_label.add_css_class("error");
    error_label.add_css_class("caption");
    error_label.set_margin_top(6);
    error_label.set_margin_start(18);
    error_label.set_margin_end(18);
    error_label.set_margin_bottom(18);
    error_label.set_visible(false);

    let content = GtkBox::new(Orientation::Vertical, 0);
    content.append(&page);
    content.append(&error_label);

    let dialog = Dialog::builder()
        .title(&gettext(title))
        .content_height(280)
        .content_width(800)
        .follows_content_size(true)
        .child(&dialog_content_shell(title, subtitle, &content))
        .build();
    let submitted = Rc::new(Cell::new(false));
    let dialog_handle = PrivateKeyDialogHandle::new(&dialog);

    let submitted_for_apply = submitted.clone();
    let dialog_handle_for_apply = dialog_handle;
    let error_label_for_apply = error_label.clone();
    password_row.connect_apply(move |row| {
        let passphrase = row.text().to_string();
        if let Some(message) = private_key_password_dialog_error_message(&passphrase) {
            error_label_for_apply.set_label(&gettext(message));
            error_label_for_apply.set_visible(true);
            return;
        }
        error_label_for_apply.set_visible(false);

        submitted_for_apply.set(true);
        dialog_handle_for_apply.force_close();
        on_submit(passphrase);
    });

    {
        let error_label = error_label.clone();
        password_row.connect_changed(move |_| {
            error_label.set_visible(false);
        });
    }

    dialog.connect_closed(move |_| {
        if !submitted.get() {
            on_close();
        }
    });

    dialog.present(Some(window));
}

pub fn present_private_key_unlock_dialog_with_close_handler<F, G>(
    window: &ApplicationWindow,
    _overlay: &ToastOverlay,
    title: &str,
    subtitle: Option<&str>,
    kind: PrivateKeyUnlockKind,
    on_submit: F,
    on_close: G,
) where
    F: Fn(PrivateKeyUnlockRequest) + 'static,
    G: Fn() + 'static,
{
    let on_submit = Rc::new(on_submit);
    let password_row = PasswordEntryRow::new();
    password_row.set_title(&gettext(private_key_unlock_row_title(kind)));
    password_row.set_show_apply_button(true);

    let password_group = PreferencesGroup::builder().build();
    password_group.add(&password_row);

    let page = PreferencesPage::new();
    page.add(&password_group);

    let hardware_button = if matches!(kind, PrivateKeyUnlockKind::HardwareOpenPgpCard) {
        let button = Button::with_label(&gettext(HARDWARE_EXTERNAL_BUTTON_LABEL));
        button.add_css_class("flat");
        button.add_css_class("caption");
        button.set_halign(Align::Start);
        button.set_margin_top(6);
        button.set_margin_start(18);
        button.set_margin_end(18);
        Some(button)
    } else {
        None
    };

    let error_label = Label::new(None);
    error_label.set_halign(Align::Start);
    error_label.set_wrap(true);
    error_label.add_css_class("error");
    error_label.add_css_class("caption");
    error_label.set_margin_top(6);
    error_label.set_margin_start(18);
    error_label.set_margin_end(18);
    error_label.set_margin_bottom(18);
    error_label.set_visible(false);

    let content = GtkBox::new(Orientation::Vertical, 0);
    content.append(&page);
    if let Some(button) = hardware_button.as_ref() {
        content.append(button);
    }
    content.append(&error_label);

    let dialog = Dialog::builder()
        .title(&gettext(title))
        .content_height(280)
        .content_width(800)
        .follows_content_size(true)
        .child(&dialog_content_shell(title, subtitle, &content))
        .build();
    let submitted = Rc::new(Cell::new(false));
    let dialog_handle = PrivateKeyDialogHandle::new(&dialog);

    let submitted_for_apply = submitted.clone();
    let dialog_handle_for_apply = dialog_handle.clone();
    let error_label_for_apply = error_label.clone();
    let on_submit_for_apply = on_submit.clone();
    password_row.connect_apply(move |row| {
        let input = row.text().to_string();
        if let Some(message) = private_key_unlock_dialog_error_message(kind, &input) {
            error_label_for_apply.set_label(&gettext(message));
            error_label_for_apply.set_visible(true);
            return;
        }
        error_label_for_apply.set_visible(false);

        submitted_for_apply.set(true);
        dialog_handle_for_apply.force_close();
        let request = match kind {
            PrivateKeyUnlockKind::Password => PrivateKeyUnlockRequest::Password(input),
            PrivateKeyUnlockKind::HardwareOpenPgpCard => {
                PrivateKeyUnlockRequest::HardwarePin(input)
            }
            PrivateKeyUnlockKind::Fido2SecurityKey => PrivateKeyUnlockRequest::Fido2(Some(input)),
        };
        on_submit_for_apply(request);
    });

    {
        let error_label = error_label.clone();
        password_row.connect_changed(move |_| {
            error_label.set_visible(false);
        });
    }

    if let Some(button) = hardware_button {
        let submitted_for_button = submitted.clone();
        let dialog_handle_for_button = dialog_handle.clone();
        let on_submit_for_button = on_submit.clone();
        button.connect_clicked(move |_| {
            submitted_for_button.set(true);
            dialog_handle_for_button.force_close();
            on_submit_for_button(PrivateKeyUnlockRequest::HardwareExternal);
        });
    }

    dialog.connect_closed(move |_| {
        if !submitted.get() {
            on_close();
        }
    });

    dialog.present(Some(window));
}

#[cfg(test)]
mod tests {
    use super::{
        private_key_password_dialog_error_message, private_key_unlock_dialog_error_message,
        private_key_unlock_row_title, HARDWARE_EXTERNAL_BUTTON_LABEL,
    };
    use crate::backend::PrivateKeyUnlockKind;

    #[test]
    fn private_key_password_dialog_requires_a_non_empty_passphrase() {
        assert_eq!(
            private_key_password_dialog_error_message(""),
            Some("Enter the key password.")
        );
        assert_eq!(
            private_key_password_dialog_error_message("   "),
            Some("Enter the key password.")
        );
        assert_eq!(private_key_password_dialog_error_message("secret"), None);
    }

    #[test]
    fn private_key_unlock_dialog_matches_the_protection_mode() {
        assert_eq!(
            private_key_unlock_row_title(PrivateKeyUnlockKind::Password),
            "Key password"
        );
        assert_eq!(
            private_key_unlock_row_title(PrivateKeyUnlockKind::HardwareOpenPgpCard,),
            "Hardware key PIN"
        );
        assert_eq!(
            private_key_unlock_row_title(PrivateKeyUnlockKind::Fido2SecurityKey,),
            "Security key PIN"
        );
        assert_eq!(HARDWARE_EXTERNAL_BUTTON_LABEL, "Or use a hardware key.");
    }

    #[test]
    fn private_key_unlock_dialog_requires_the_expected_secret_input() {
        assert_eq!(
            private_key_unlock_dialog_error_message(PrivateKeyUnlockKind::Password, "   ",),
            Some("Enter the key password.")
        );
        assert_eq!(
            private_key_unlock_dialog_error_message(PrivateKeyUnlockKind::HardwareOpenPgpCard, "",),
            Some("Enter the hardware key PIN.")
        );
        assert_eq!(
            private_key_unlock_dialog_error_message(PrivateKeyUnlockKind::Fido2SecurityKey, "",),
            Some("Enter the security key PIN.")
        );
        assert_eq!(
            private_key_unlock_dialog_error_message(
                PrivateKeyUnlockKind::HardwareOpenPgpCard,
                "123456",
            ),
            None
        );
    }
}
