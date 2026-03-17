use crate::clipboard::connect_copy_button;
use crate::password::model::OpenPassFile;
use crate::password::new_item::{selected_new_password_store, NewPasswordPopoverState};
use crate::password::page::{
    add_empty_otp_secret, begin_new_password_entry, generate_password_entry,
    open_password_entry_page, save_current_password_entry, show_raw_pass_file_page,
    PasswordPageState,
};
use crate::support::actions::register_window_action;
use crate::support::object_data::non_null_to_string_option;
use adw::gtk::{Button, ListBox};
use adw::prelude::*;
use adw::{EntryRow, PasswordEntryRow, Toast, ToastOverlay};

pub(super) fn connect_password_list_activation(
    list: &ListBox,
    overlay: &ToastOverlay,
    page_state: &PasswordPageState,
) {
    let overlay = overlay.clone();
    let page_state = page_state.clone();
    list.connect_row_activated(move |_list, row| {
        if matches!(
            non_null_to_string_option(row, "openable").as_deref(),
            Some("false")
        ) {
            return;
        }

        let label = non_null_to_string_option(row, "label");
        let root = non_null_to_string_option(row, "root");

        let Some(label) = label else {
            overlay.add_toast(Toast::new("Couldn't open that item."));
            return;
        };
        let Some(root) = root else {
            overlay.add_toast(Toast::new("That item is missing its store."));
            return;
        };
        let opened_pass_file = OpenPassFile::from_label(root, &label);
        open_password_entry_page(&page_state, opened_pass_file, true);
    });
}

pub(super) fn connect_password_copy_buttons(
    overlay: &ToastOverlay,
    password_entry: &PasswordEntryRow,
    copy_password_button: &Button,
    username_entry: &EntryRow,
    copy_username_button: &Button,
    otp_entry: &PasswordEntryRow,
    copy_otp_button: &Button,
) {
    {
        let entry = password_entry.clone();
        let button = copy_password_button.clone();
        connect_copy_button(&button, overlay, move || entry.text().to_string());
    }
    {
        let entry = username_entry.clone();
        let button = copy_username_button.clone();
        connect_copy_button(&button, overlay, move || entry.text().to_string());
    }
    {
        let entry = otp_entry.clone();
        let button = copy_otp_button.clone();
        connect_copy_button(&button, overlay, move || entry.text().to_string());
    }
}

pub(super) fn connect_new_password_submit(
    page_state: &PasswordPageState,
    popover_state: &NewPasswordPopoverState,
) {
    let page_state_for_apply = page_state.clone();
    let popover_state_for_apply = popover_state.clone();
    let path_entry = popover_state_for_apply.path_entry.clone();
    path_entry.connect_apply(move |_| {
        begin_new_password_entry(
            &page_state_for_apply,
            &popover_state_for_apply.path_entry.text(),
            selected_new_password_store(&popover_state_for_apply),
            &popover_state_for_apply.dialog,
        );
    });
}

pub(super) fn register_password_page_actions(
    window: &adw::ApplicationWindow,
    page_state: &PasswordPageState,
) {
    {
        let page_state = page_state.clone();
        register_window_action(window, "save-password", move || {
            save_current_password_entry(&page_state);
        });
    }

    {
        let page_state = page_state.clone();
        register_window_action(window, "open-raw-pass-file", move || {
            show_raw_pass_file_page(&page_state);
        });
    }

    {
        let page_state = page_state.clone();
        register_window_action(window, "add-otp-secret", move || {
            add_empty_otp_secret(&page_state);
        });
    }

    {
        let page_state = page_state.clone();
        register_window_action(window, "generate-password", move || {
            generate_password_entry(&page_state);
        });
    }
}
