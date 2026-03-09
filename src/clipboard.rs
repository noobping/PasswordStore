use crate::background::spawn_result_task;
use crate::item::PassEntry;
use crate::logging::log_error;
#[cfg(not(feature = "flatpak"))]
use crate::logging::{run_command_status, CommandLogOptions};
use crate::backend::read_password_line;
#[cfg(feature = "flatpak")]
use crate::backend::resolved_ripasso_own_fingerprint;
#[cfg(not(feature = "flatpak"))]
use crate::preferences::Preferences;
#[cfg(feature = "flatpak")]
use crate::ripasso_unlock::{
    is_locked_private_key_error, prompt_private_key_unlock_for_action,
};
use adw::{glib, prelude::*, EntryRow, PasswordEntryRow, Toast, ToastOverlay};
use adw::gtk::{Button, Widget, gdk::Display};
#[cfg(feature = "flatpak")]
use std::rc::Rc;
#[cfg(not(feature = "flatpak"))]
use std::thread;
use std::time::Duration;

const COPY_BUTTON_ICON_NAME: &str = "edit-copy-symbolic";
const COPIED_BUTTON_ICON_NAME: &str = "object-select-symbolic";
const COPY_BUTTON_FEEDBACK_MS: u64 = 1200;

fn show_clipboard_unavailable_toast(overlay: &ToastOverlay) {
    overlay.add_toast(Toast::new("Clipboard unavailable."));
}

fn show_copy_feedback(button: &Button) {
    button.set_icon_name(COPIED_BUTTON_ICON_NAME);

    let button = button.clone();
    glib::timeout_add_local_once(Duration::from_millis(COPY_BUTTON_FEEDBACK_MS), move || {
        button.set_icon_name(COPY_BUTTON_ICON_NAME);
    });
}

pub(crate) fn set_clipboard_text(text: &str, overlay: &ToastOverlay, button: Option<&Button>) {
    if let Some(display) = Display::default() {
        let clipboard = display.clipboard();
        clipboard.set_text(text);
        if let Some(button) = button {
            show_copy_feedback(button);
        }
    } else {
        show_clipboard_unavailable_toast(overlay);
    }
}

pub(crate) fn connect_copy_button<F>(button: &Button, overlay: &ToastOverlay, text: F)
where
    F: Fn() -> String + 'static,
{
    let overlay = overlay.clone();
    let feedback_button = button.clone();
    button.connect_clicked(move |_| {
        let text = text();
        set_clipboard_text(&text, &overlay, Some(&feedback_button));
    });
}

pub(crate) fn add_copy_suffix<W>(
    widget: &W,
    text: impl Fn() -> String + 'static,
    overlay: &ToastOverlay,
) where
    W: IsA<Widget> + Clone,
{
    let button = Button::from_icon_name(COPY_BUTTON_ICON_NAME);
    button.set_tooltip_text(Some("Copy value"));
    button.add_css_class("flat");
    connect_copy_button(&button, overlay, text);

    if let Some(row) = widget.dynamic_cast_ref::<EntryRow>() {
        row.add_suffix(&button);
    } else if let Some(row) = widget.dynamic_cast_ref::<PasswordEntryRow>() {
        row.add_suffix(&button);
    }
}

#[cfg(not(feature = "flatpak"))]
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

fn copy_password_entry_to_clipboard_via_read(
    item: PassEntry,
    overlay: ToastOverlay,
    button: Option<Button>,
) {
    #[cfg(feature = "flatpak")]
    let retry_item = item.clone();
    let overlay_for_disconnect = overlay.clone();
    spawn_result_task(
        move || {
            let label = item.label();
            read_password_line(&item.store_path, &label)
        },
        move |result| match result {
            Ok(password) => {
                set_clipboard_text(&password, &overlay, button.as_ref());
            }
            Err(err) => {
                log_error(format!("Failed to copy password entry: {err}"));
                #[cfg(feature = "flatpak")]
                if is_locked_private_key_error(&err) {
                    match resolved_ripasso_own_fingerprint() {
                        Ok(fingerprint) => {
                            let retry_overlay = overlay.clone();
                            let retry_item_for_unlock = retry_item.clone();
                            let retry_button = button.clone();
                            prompt_private_key_unlock_for_action(
                                &overlay,
                                fingerprint,
                                Rc::new(move || {
                                    copy_password_entry_to_clipboard_via_read(
                                        retry_item_for_unlock.clone(),
                                        retry_overlay.clone(),
                                        retry_button.clone(),
                                    );
                                }),
                            );
                            return;
                        }
                        Err(resolve_err) => {
                            log_error(format!(
                                "Failed to resolve the selected ripasso private key for copy retry: {resolve_err}"
                            ));
                        }
                    }
                }
                overlay.add_toast(Toast::new("Couldn't copy the password."));
            }
        },
        move || {
            overlay_for_disconnect.add_toast(Toast::new("Couldn't copy the password."));
        },
    );
}

pub(crate) fn copy_password_entry_to_clipboard(
    item: PassEntry,
    overlay: ToastOverlay,
    button: Option<Button>,
) {
    #[cfg(not(feature = "flatpak"))]
    {
        let settings = Preferences::new();
        if settings.uses_integrated_backend() {
            copy_password_entry_to_clipboard_via_read(item, overlay, button);
        } else {
            copy_password_entry_to_clipboard_via_pass_command(item, button.as_ref());
        }
        return;
    }

    #[cfg(feature = "flatpak")]
    {
        copy_password_entry_to_clipboard_via_read(item, overlay, button);
    }
}
