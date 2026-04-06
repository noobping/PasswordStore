use crate::clipboard::connect_copy_button;
use crate::i18n::gettext;
use crate::password::list::{
    clear_password_search, password_list_row_action_kind, toggle_password_list_folder_row,
    PasswordListActionRowKind,
};
use crate::password::model::OpenPassFile;
use crate::password::new_item::{
    clear_new_password_dialog_error, selected_new_password_store, show_new_password_dialog_error,
    NewPasswordDialogState,
};
use crate::password::page::{
    add_empty_otp_secret, add_pass_field_from_input, apply_pass_file_template,
    begin_new_password_entry, clean_pass_file, copy_current_otp, copy_current_password,
    copy_current_username, focus_add_pass_field_input, generate_password_entry,
    open_password_entry_page, refresh_apply_template_button, save_current_password_entry,
    show_raw_pass_file_page, toggle_password_options, PasswordPageState,
};
use crate::support::actions::{activate_widget_action, register_window_action};
use crate::support::object_data::non_null_to_string_option;
use crate::support::ui::connect_entry_row_apply_button_to_nonempty_text;
use adw::glib::Propagation;
use adw::gtk::{gdk, Button, DirectionType, EventControllerKey, ListBox, PropagationPhase, Widget};
use adw::prelude::*;
use adw::{EntryRow, PasswordEntryRow, Toast, ToastOverlay};

pub(super) fn connect_password_list_activation(
    list: &ListBox,
    search_entry: &adw::gtk::SearchEntry,
    overlay: &ToastOverlay,
    page_state: &PasswordPageState,
) {
    let search_entry = search_entry.clone();
    let overlay = overlay.clone();
    let page_state = page_state.clone();
    list.connect_row_activated(move |list, row| {
        if toggle_password_list_folder_row(list, row) {
            return;
        }

        match password_list_row_action_kind(row) {
            Some(PasswordListActionRowKind::NewPassword) => {
                activate_widget_action(row, "win.open-new-password");
                return;
            }
            Some(PasswordListActionRowKind::ClearSearch) => {
                clear_password_search(&search_entry, list);
                return;
            }
            None => {}
        }

        if matches!(
            non_null_to_string_option(row, "openable").as_deref(),
            Some("false")
        ) {
            return;
        }

        let label = non_null_to_string_option(row, "label");
        let root = non_null_to_string_option(row, "root");

        let Some(label) = label else {
            overlay.add_toast(Toast::new(&gettext("Couldn't open that item.")));
            return;
        };
        let Some(root) = root else {
            overlay.add_toast(Toast::new(&gettext("That item is missing its store.")));
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
    dialog_state: &NewPasswordDialogState,
) {
    let page_state_for_apply = page_state.clone();
    let dialog_state_for_apply = dialog_state.clone();
    let path_entry = dialog_state_for_apply.path_entry.clone();
    path_entry.connect_apply(move |_| {
        clear_new_password_dialog_error(&dialog_state_for_apply);
        if let Err(message) = begin_new_password_entry(
            &page_state_for_apply,
            &dialog_state_for_apply.path_entry.text(),
            selected_new_password_store(&dialog_state_for_apply),
            &dialog_state_for_apply.dialog,
        ) {
            show_new_password_dialog_error(&dialog_state_for_apply, message);
        }
    });
}

pub(super) fn register_password_page_actions(
    window: &adw::ApplicationWindow,
    page_state: &PasswordPageState,
) {
    {
        let page_state = page_state.clone();
        let page = page_state.page.clone();
        let page_for_keys = page.clone();
        let template_button: Widget = page_state.template_button.clone().upcast();
        let clean_button: Widget = page_state.clean_button.clone().upcast();
        let otp_add_button: Widget = page_state.otp_add_button.clone().upcast();
        let editor_save_button: Widget = page_state.editor_save_button.clone().upcast();
        let controller = EventControllerKey::new();
        controller.set_propagation_phase(PropagationPhase::Capture);
        controller.connect_key_pressed(move |_, key, _, _| {
            let direction = match key {
                gdk::Key::Up | gdk::Key::KP_Up => DirectionType::Up,
                gdk::Key::Down | gdk::Key::KP_Down => DirectionType::Down,
                _ => return Propagation::Proceed,
            };

            let Some(root) = page_for_keys.root() else {
                return Propagation::Proceed;
            };
            let Some(focus) = adw::gtk::prelude::RootExt::focus(&root) else {
                return Propagation::Proceed;
            };
            if !focus.is::<Button>() || !focus.is_ancestor(&page_for_keys) {
                return Propagation::Proceed;
            }

            if matches!(direction, DirectionType::Up)
                && (focus == template_button
                    || focus == clean_button
                    || focus == otp_add_button
                    || focus == editor_save_button)
            {
                focus_add_pass_field_input(&page_state);
                return Propagation::Stop;
            }

            if page_for_keys.child_focus(direction) {
                Propagation::Stop
            } else {
                Propagation::Proceed
            }
        });
        page.add_controller(controller);
    }

    {
        let page_state = page_state.clone();
        let buffer = page_state.text.buffer();
        buffer.connect_changed(move |_| {
            refresh_apply_template_button(&page_state);
        });
    }

    {
        let page_state = page_state.clone();
        let add_field_row = page_state.field_add_row.clone();
        connect_entry_row_apply_button_to_nonempty_text(&add_field_row);
        add_field_row.connect_apply(move |_| {
            add_pass_field_from_input(&page_state);
        });
    }

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
        register_window_action(window, "add-pass-field", move || {
            focus_add_pass_field_input(&page_state);
        });
    }

    {
        let page_state = page_state.clone();
        register_window_action(window, "clean-pass-file", move || {
            clean_pass_file(&page_state);
        });
    }

    {
        let page_state = page_state.clone();
        register_window_action(window, "apply-pass-template", move || {
            apply_pass_file_template(&page_state);
        });
    }

    {
        let page_state = page_state.clone();
        register_window_action(window, "generate-password", move || {
            generate_password_entry(&page_state);
        });
    }

    {
        let page_state = page_state.clone();
        register_window_action(window, "copy-password", move || {
            copy_current_password(&page_state);
        });
    }

    {
        let page_state = page_state.clone();
        register_window_action(window, "copy-username", move || {
            copy_current_username(&page_state);
        });
    }

    {
        let page_state = page_state.clone();
        register_window_action(window, "copy-otp", move || {
            copy_current_otp(&page_state);
        });
    }

    {
        let page_state = page_state.clone();
        register_window_action(window, "toggle-password-options", move || {
            toggle_password_options(&page_state);
        });
    }
}
