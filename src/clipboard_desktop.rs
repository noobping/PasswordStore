use super::{copy_password_entry_to_clipboard_via_read, show_copy_feedback};
use crate::item::PassEntry;
use crate::logging::{run_command_status, CommandLogOptions};
use crate::preferences::Preferences;
use adw::gtk::Button;
use adw::ToastOverlay;
use std::thread;

fn copy_password_entry_to_clipboard_via_pass_command(item: PassEntry, button: Option<&Button>) {
    if let Some(button) = button {
        show_copy_feedback(button);
    }

    thread::spawn(move || {
        let settings = Preferences::new();
        let mut cmd = settings.command();
        cmd.env("PASSWORD_STORE_DIR", &item.store_path)
            .arg("-c")
            .arg(item.label());
        let _ = run_command_status(
            &mut cmd,
            "Copy password to clipboard",
            CommandLogOptions::SENSITIVE,
        );
    });
}

pub(super) fn handle_copy_password_error(
    _item: &PassEntry,
    _overlay: &ToastOverlay,
    _button: &Option<Button>,
    _message: &str,
) -> bool {
    false
}

pub(super) fn copy_password_entry_to_clipboard(
    item: PassEntry,
    overlay: ToastOverlay,
    button: Option<Button>,
) {
    let settings = Preferences::new();
    if settings.uses_integrated_backend() {
        copy_password_entry_to_clipboard_via_read(item, overlay, button);
    } else {
        copy_password_entry_to_clipboard_via_pass_command(item, button.as_ref());
    }
}
