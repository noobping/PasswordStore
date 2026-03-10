mod editor;
#[cfg(feature = "flatpak")]
mod flatpak;
#[cfg(not(feature = "flatpak"))]
mod standard;
mod state;

use super::file::{
    clear_box_children, new_pass_file_contents_from_template, structured_pass_contents,
    sync_username_row,
};
use super::list::load_passwords_async;
use crate::backend::{read_password_entry, save_password_entry};
use crate::logging::log_error;
use crate::password::model::OpenPassFile;
use crate::password::opened::{
    clear_opened_pass_file, get_opened_pass_file, is_opened_pass_file,
    refresh_opened_pass_file_from_contents, set_opened_pass_file,
};
use crate::preferences::Preferences;
use crate::support::background::spawn_result_task;
use crate::support::ui::{
    pop_navigation_to_root, push_navigation_page_if_needed, visible_navigation_page_is,
};
use crate::window::messages::with_logs_hint;
use crate::window::navigation::set_save_button_for_password;
use adw::gtk::Popover;
use adw::prelude::*;
use adw::Toast;

use self::editor::{current_editor_contents, structured_editor_contents, sync_editor_contents};
#[cfg(feature = "flatpak")]
use self::flatpak as platform;
use self::platform::{friendly_password_entry_error_message, handle_open_password_entry_error};
#[cfg(not(feature = "flatpak"))]
use self::standard as platform;
pub(crate) use self::state::PasswordPageState;
use self::state::{
    show_password_editor_chrome, show_password_editor_fields, show_password_loading_state,
    show_password_open_error,
};

fn save_error_toast(message: &str) -> &'static str {
    if message.contains("already exists") {
        "An item with that name already exists."
    } else {
        "Couldn't save changes."
    }
}

pub(crate) fn open_password_entry_page(
    state: &PasswordPageState,
    opened_pass_file: OpenPassFile,
    push_page: bool,
) {
    let pass_label = opened_pass_file.label();
    let store_for_thread = opened_pass_file.store_path().to_string();
    set_opened_pass_file(opened_pass_file.clone());

    show_password_loading_state(state, opened_pass_file.title(), &pass_label);
    if push_page {
        push_navigation_page_if_needed(&state.nav, &state.page);
    }

    let label_for_thread = pass_label.clone();
    let state_for_result = state.clone();
    let opened_pass_file_for_result = opened_pass_file.clone();
    let state_for_disconnect = state.clone();
    let opened_pass_file_for_disconnect = opened_pass_file.clone();
    spawn_result_task(
        move || read_password_entry(&store_for_thread, &label_for_thread),
        move |result| {
            if !is_opened_pass_file(&opened_pass_file_for_result) {
                return;
            }

            match result {
                Ok(output) => {
                    let updated_pass_file = refresh_opened_pass_file_from_contents(
                        &opened_pass_file_for_result,
                        &output,
                    );
                    show_password_editor_fields(&state_for_result);
                    sync_editor_contents(&state_for_result, &output, updated_pass_file.as_ref());
                }
                Err(msg) => {
                    log_error(format!("Failed to open password entry: {msg}"));
                    if handle_open_password_entry_error(
                        &state_for_result,
                        &opened_pass_file_for_result,
                        &msg,
                    ) {
                        return;
                    }

                    show_password_open_error(&state_for_result);
                    let toast = if let Some(message) = friendly_password_entry_error_message(&msg) {
                        Toast::new(message)
                    } else {
                        Toast::new(&with_logs_hint("Couldn't open the item."))
                    };
                    state_for_result.overlay.add_toast(toast);
                }
            }
        },
        move || {
            if !is_opened_pass_file(&opened_pass_file_for_disconnect) {
                return;
            }

            show_password_open_error(&state_for_disconnect);
            state_for_disconnect
                .overlay
                .add_toast(Toast::new(&with_logs_hint("Couldn't open the item.")));
        },
    );
}

pub(crate) fn begin_new_password_entry(
    state: &PasswordPageState,
    path: &str,
    store_root: Option<String>,
    add_popover: &Popover,
    git_popover: &Popover,
) {
    let path = path.trim();
    if path.is_empty() {
        state.overlay.add_toast(Toast::new("Enter a name."));
        return;
    }

    let settings = Preferences::new();
    let store_root = store_root.unwrap_or_else(|| settings.store());
    if store_root.trim().is_empty() {
        state
            .overlay
            .add_toast(Toast::new("Add a store folder first."));
        add_popover.popdown();
        return;
    }
    let template_contents =
        new_pass_file_contents_from_template(&settings.new_pass_file_template());
    let opened_pass_file = OpenPassFile::from_label(store_root, path);
    set_opened_pass_file(opened_pass_file.clone());
    let template_pass_file =
        refresh_opened_pass_file_from_contents(&opened_pass_file, &template_contents)
            .or_else(get_opened_pass_file);

    show_password_editor_chrome(state, "New item", path);
    show_password_editor_fields(state);
    state.otp.clear();
    push_navigation_page_if_needed(&state.nav, &state.page);

    add_popover.popdown();
    git_popover.popdown();
    sync_editor_contents(state, &template_contents, template_pass_file.as_ref());
}

pub(crate) fn show_raw_pass_file_page(state: &PasswordPageState) {
    let contents = structured_editor_contents(state);
    state.text.buffer().set_text(&contents);

    let subtitle = get_opened_pass_file()
        .map(|pass_file| pass_file.label())
        .unwrap_or_else(|| "Password Store".to_string());
    show_password_editor_chrome(state, "Raw Pass File", &subtitle);

    push_navigation_page_if_needed(&state.nav, &state.raw_page);
}

pub(crate) fn save_current_password_entry(state: &PasswordPageState) {
    let Some(pass_file) = get_opened_pass_file() else {
        state.overlay.add_toast(Toast::new("Open an item first."));
        return;
    };

    let contents = current_editor_contents(state);
    let password = contents.lines().next().unwrap_or_default().to_string();
    if password.is_empty() {
        state.overlay.add_toast(Toast::new("Enter a password."));
        return;
    }

    let otp_url = match state.otp.current_url_for_save() {
        Ok(otp_url) => otp_url,
        Err(message) => {
            state.overlay.add_toast(Toast::new(message));
            return;
        }
    };
    let contents = if visible_navigation_page_is(&state.nav, &state.raw_page) {
        contents
    } else {
        structured_pass_contents(
            &state.entry.text(),
            &state.username.text(),
            otp_url.as_deref(),
            &state.structured_templates.borrow(),
            &state.dynamic_rows.borrow(),
        )
    };
    let label = pass_file.label();
    match save_password_entry(pass_file.store_path(), &label, &contents, true) {
        Ok(()) => {
            let updated_pass_file = refresh_opened_pass_file_from_contents(&pass_file, &contents);
            show_password_editor_fields(state);
            sync_editor_contents(state, &contents, updated_pass_file.as_ref());
            state.overlay.add_toast(Toast::new("Saved."));
        }
        Err(message) => {
            log_error(format!("Failed to save password entry: {message}"));
            state
                .overlay
                .add_toast(Toast::new(save_error_toast(&message)));
        }
    }
}

pub(crate) fn show_password_list_page(state: &PasswordPageState, show_hidden: bool) {
    pop_navigation_to_root(&state.nav);

    clear_opened_pass_file();
    state.back.set_visible(false);
    state.save.set_visible(false);
    set_save_button_for_password(&state.save);
    state.add.set_visible(true);
    state.find.set_visible(true);
    state.git.set_visible(false);

    state.win.set_title("Password Store");
    state.win.set_subtitle("Manage your passwords");

    state.entry.set_text("");
    sync_username_row(&state.username, None);
    state.otp.clear();
    clear_box_children(&state.dynamic_box);
    state.dynamic_box.set_visible(false);
    state.raw_button.set_visible(false);
    state.structured_templates.borrow_mut().clear();
    state.dynamic_rows.borrow_mut().clear();
    state.text.buffer().set_text("");

    load_passwords_async(
        &state.list,
        state.git.clone(),
        state.find.clone(),
        state.save.clone(),
        state.overlay.clone(),
        true,
        show_hidden,
    );
}

pub(crate) fn retry_open_password_entry_if_needed(state: &PasswordPageState) -> bool {
    if !visible_navigation_page_is(&state.nav, &state.page)
        || !state.status.is_visible()
        || state.entry.is_visible()
    {
        return false;
    }

    let Some(pass_file) = get_opened_pass_file() else {
        return false;
    };
    open_password_entry_page(state, pass_file, false);
    true
}
