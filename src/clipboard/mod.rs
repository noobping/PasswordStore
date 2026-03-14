use crate::backend::read_password_line;
use crate::logging::log_error;
use crate::password::model::PassEntry;
use crate::support::background::spawn_result_task;
use crate::support::ui::flat_icon_button_with_tooltip;
use adw::gtk::{gdk::Display, Button, Widget};
use adw::{glib, prelude::*, EntryRow, PasswordEntryRow, Toast, ToastOverlay};
use std::time::Duration;

#[cfg(keycord_restricted)]
mod flatpak;
#[cfg(keycord_standard_linux)]
mod standard;

#[cfg(keycord_restricted)]
use self::flatpak as platform;
#[cfg(keycord_standard_linux)]
use self::standard as platform;

const COPY_BUTTON_ICON_NAME: &str = "edit-copy-symbolic";
const COPIED_BUTTON_ICON_NAME: &str = "object-select-symbolic";
const COPY_BUTTON_FEEDBACK_MS: u64 = 1200;

fn show_clipboard_unavailable_toast(overlay: &ToastOverlay) {
    overlay.add_toast(Toast::new("Clipboard unavailable."));
}

pub(super) fn show_copy_feedback(button: &Button) {
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
    let button = flat_icon_button_with_tooltip(COPY_BUTTON_ICON_NAME, "Copy value");
    connect_copy_button(&button, overlay, text);

    if let Some(row) = widget.dynamic_cast_ref::<EntryRow>() {
        row.add_suffix(&button);
    } else if let Some(row) = widget.dynamic_cast_ref::<PasswordEntryRow>() {
        row.add_suffix(&button);
    }
}

pub(super) fn copy_password_entry_to_clipboard_via_read(
    item: PassEntry,
    overlay: ToastOverlay,
    button: Option<Button>,
) {
    let overlay_for_disconnect = overlay.clone();
    let task_item = item.clone();
    spawn_result_task(
        move || {
            let label = task_item.label();
            read_password_line(&task_item.store_path, &label)
        },
        move |result| match result {
            Ok(password) => {
                set_clipboard_text(&password, &overlay, button.as_ref());
            }
            Err(err) => {
                log_error(format!("Failed to copy password entry: {err}"));
                if platform::handle_copy_password_error(&item, &overlay, &button, &err) {
                    return;
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
    platform::copy_password_entry_to_clipboard(item, overlay, button);
}
