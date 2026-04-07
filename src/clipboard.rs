use crate::backend::{
    preferred_ripasso_private_key_fingerprint_for_entry, read_password_line, PasswordEntryError,
};
use crate::i18n::gettext;
use crate::logging::{log_error, run_command_status, CommandLogOptions};
use crate::password::model::PassEntry;
use crate::preferences::Preferences;
use crate::private_key::unlock::prompt_private_key_unlock_for_action;
use crate::support::background::{spawn_result_task, spawn_worker};
use crate::support::ui::flat_icon_button_with_tooltip;
use adw::gtk::{gdk::Display, Button, Widget};
use adw::{glib, prelude::*, EntryRow, PasswordEntryRow, Toast, ToastOverlay};
use std::rc::Rc;
use std::time::Duration;

const COPY_BUTTON_ICON_NAME: &str = "edit-copy-symbolic";
const COPIED_BUTTON_ICON_NAME: &str = "object-select-symbolic";
const COPY_BUTTON_FEEDBACK_MS: u64 = 1200;

fn show_clipboard_unavailable_toast(overlay: &ToastOverlay) {
    overlay.add_toast(Toast::new(&gettext("Clipboard unavailable.")));
}

pub fn set_copy_button_loading(button: Option<&Button>, loading: bool) {
    let Some(button) = button else {
        return;
    };

    button.set_sensitive(!loading);
}

pub fn show_copy_feedback(button: &Button) {
    button.set_icon_name(COPIED_BUTTON_ICON_NAME);

    let button = button.clone();
    glib::timeout_add_local_once(Duration::from_millis(COPY_BUTTON_FEEDBACK_MS), move || {
        button.set_icon_name(COPY_BUTTON_ICON_NAME);
    });
}

pub fn set_clipboard_text(text: &str, overlay: &ToastOverlay, button: Option<&Button>) -> bool {
    Display::default().map_or_else(
        || {
            show_clipboard_unavailable_toast(overlay);
            false
        },
        |display| {
            let clipboard = display.clipboard();
            clipboard.set_text(text);
            if let Some(button) = button {
                show_copy_feedback(button);
            }
            true
        },
    )
}

pub fn connect_copy_button<F>(button: &Button, overlay: &ToastOverlay, text: F)
where
    F: Fn() -> String + 'static,
{
    let overlay = overlay.clone();
    let feedback_button = button.clone();
    button.connect_clicked(move |_| {
        let text = text();
        let _ = set_clipboard_text(&text, &overlay, Some(&feedback_button));
    });
}

pub fn add_copy_suffix<W>(widget: &W, text: impl Fn() -> String + 'static, overlay: &ToastOverlay)
where
    W: IsA<Widget> + Clone,
{
    let button = flat_icon_button_with_tooltip(COPY_BUTTON_ICON_NAME, "Copy value");
    connect_copy_button(&button, overlay, text);

    if let Some(row) = widget.dynamic_cast_ref::<EntryRow>() {
        row.add_suffix(&button);
    } else if let Some(row) = widget.dynamic_cast_ref::<PasswordEntryRow>() {
        row.add_suffix(&button);
    }
}

fn copy_password_entry_to_clipboard_via_pass_command(item: PassEntry, button: Option<&Button>) {
    if let Some(button) = button {
        show_copy_feedback(button);
    }

    if let Err(err) = spawn_worker("clipboard-pass-copy", move || {
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
    }) {
        log_error(format!("Failed to spawn clipboard copy worker: {err}"));
    }
}

fn handle_copy_password_error(
    item: &PassEntry,
    overlay: &ToastOverlay,
    button: Option<&Button>,
    error: &PasswordEntryError,
) -> bool {
    if !matches!(error, PasswordEntryError::LockedPrivateKey(_)) {
        return false;
    }

    match preferred_ripasso_private_key_fingerprint_for_entry(&item.store_path, &item.label()) {
        Ok(fingerprint) => {
            let retry_overlay = overlay.clone();
            let retry_item = item.clone();
            let retry_button = button.cloned();
            let finish_button = button.cloned();
            prompt_private_key_unlock_for_action(
                overlay,
                fingerprint,
                Rc::new(move || {
                    copy_password_entry_to_clipboard_via_read(
                        retry_item.clone(),
                        retry_overlay.clone(),
                        retry_button.clone(),
                    );
                }),
                Rc::new(move |success| {
                    if !success {
                        set_copy_button_loading(finish_button.as_ref(), false);
                    }
                }),
            );
            true
        }
        Err(resolve_err) => {
            log_error(format!(
                "Failed to resolve the private key for copy retry: {resolve_err}"
            ));
            false
        }
    }
}

pub fn copy_password_entry_to_clipboard_via_read(
    item: PassEntry,
    overlay: ToastOverlay,
    button: Option<Button>,
) {
    set_copy_button_loading(button.as_ref(), true);
    let overlay_for_disconnect = overlay.clone();
    let button_for_disconnect = button.clone();
    let task_item = item.clone();
    spawn_result_task(
        move || {
            let label = task_item.label();
            read_password_line(&task_item.store_path, &label)
        },
        move |result| match result {
            Ok(password) => {
                if set_clipboard_text(&password, &overlay, button.as_ref()) {
                    overlay.add_toast(Toast::new(&gettext("Copied.")));
                }
                set_copy_button_loading(button.as_ref(), false);
            }
            Err(err) => {
                log_error(format!("Failed to copy password entry: {err}"));
                if handle_copy_password_error(&item, &overlay, button.as_ref(), &err) {
                    return;
                }
                set_copy_button_loading(button.as_ref(), false);
                overlay.add_toast(Toast::new(&gettext("Couldn't copy the password.")));
            }
        },
        move || {
            set_copy_button_loading(button_for_disconnect.as_ref(), false);
            overlay_for_disconnect.add_toast(Toast::new(&gettext("Couldn't copy the password.")));
        },
    );
}

pub fn copy_password_entry_to_clipboard(
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
