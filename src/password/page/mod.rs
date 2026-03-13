mod editor;
#[cfg(feature = "flatpak")]
mod flatpak;
mod standard;
mod state;

use super::file::{new_pass_file_contents_from_template, structured_pass_contents};
use super::generation::generate_password;
use super::list::{load_passwords_async, PasswordListActions};
use crate::backend::{
    read_password_entry, rename_password_entry, save_password_entry, PasswordEntryError,
    PasswordEntryWriteError,
};
use crate::logging::log_error;
use crate::password::model::{OpenPassFile, UsernameFallbackError};
use crate::password::opened::{
    clear_opened_pass_file, get_opened_pass_file, is_opened_pass_file,
    refresh_opened_pass_file_from_contents, set_opened_pass_file,
};
use crate::password::undo::{push_undo_action, restore_saved_entry_action};
use crate::preferences::Preferences;
use crate::support::background::spawn_result_task;
use crate::support::ui::{
    pop_navigation_to_root, push_navigation_page_if_needed, visible_navigation_page_is,
};
use crate::window::navigation::{show_primary_page_chrome, HasWindowChrome, APP_WINDOW_TITLE};
use adw::gtk::Popover;
use adw::prelude::*;
use adw::Toast;

use self::editor::{
    add_empty_otp_secret as add_empty_otp_secret_to_editor, current_editor_contents,
    structured_editor_contents, sync_editor_contents,
};
#[cfg(feature = "flatpak")]
use self::flatpak as platform;
use self::platform::handle_open_password_entry_error;
#[cfg(not(feature = "flatpak"))]
use self::standard as platform;
pub(crate) use self::state::PasswordPageState;
use self::state::{
    reset_password_editor, show_password_editor_chrome, show_password_editor_fields,
    show_password_loading_state, show_password_open_error, sync_saved_password_state,
};

fn password_open_failure_message(error: Option<&PasswordEntryError>) -> &'static str {
    error
        .and_then(PasswordEntryError::toast_message)
        .unwrap_or("Couldn't open the item.")
}

fn password_save_failure_message(error: &PasswordEntryWriteError) -> &'static str {
    error.save_toast_message()
}

fn username_fallback_failure_message(error: UsernameFallbackError) -> &'static str {
    error.toast_message()
}

fn show_password_open_failure(state: &PasswordPageState, error: Option<&PasswordEntryError>) {
    show_password_open_error(state);
    state
        .overlay
        .add_toast(Toast::new(password_open_failure_message(error)));
}

fn should_retry_open_password_entry(
    page_visible: bool,
    status_visible: bool,
    entry_visible: bool,
    has_opened_pass_file: bool,
) -> bool {
    page_visible && status_visible && !entry_visible && has_opened_pass_file
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
                    sync_saved_password_state(&state_for_result, &output, true);
                }
                Err(err) => {
                    log_error(format!("Failed to open password entry: {err}"));
                    if handle_open_password_entry_error(
                        &state_for_result,
                        &opened_pass_file_for_result,
                        &err,
                    ) {
                        return;
                    }

                    show_password_open_failure(&state_for_result, Some(&err));
                }
            }
        },
        move || {
            if !is_opened_pass_file(&opened_pass_file_for_disconnect) {
                return;
            }
            show_password_open_failure(&state_for_disconnect, None);
        },
    );
}

pub(crate) fn begin_new_password_entry(
    state: &PasswordPageState,
    path: &str,
    store_root: Option<String>,
    add_popover: &Popover,
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
    sync_editor_contents(state, &template_contents, template_pass_file.as_ref());
    sync_saved_password_state(state, &template_contents, false);
}

pub(crate) fn show_raw_pass_file_page(state: &PasswordPageState) {
    let contents = structured_editor_contents(state);
    state.text.buffer().set_text(&contents);

    let subtitle = get_opened_pass_file()
        .map(|pass_file| pass_file.label())
        .unwrap_or_else(|| APP_WINDOW_TITLE.to_string());
    show_password_editor_chrome(state, "Raw Pass File", &subtitle);

    push_navigation_page_if_needed(&state.nav, &state.raw_page);
}

pub(crate) fn add_empty_otp_secret(state: &PasswordPageState) {
    add_empty_otp_secret_to_editor(state);
}

pub(crate) fn password_page_has_unsaved_changes(state: &PasswordPageState) -> bool {
    current_editor_contents(state) != *state.saved_contents.borrow()
}

pub(crate) fn revert_unsaved_password_changes(state: &PasswordPageState) -> bool {
    if !password_page_has_unsaved_changes(state) {
        return false;
    }

    let saved_contents = state.saved_contents.borrow().clone();
    let pass_file = get_opened_pass_file();
    sync_editor_contents(state, &saved_contents, pass_file.as_ref());
    state.overlay.add_toast(Toast::new("Reverted."));
    true
}

pub(crate) fn generate_password_entry(state: &PasswordPageState) {
    if !state.entry.is_visible() {
        return;
    }

    let password = generate_password(&state.generator_controls.settings());
    state.entry.set_text(&password);
    if !visible_navigation_page_is(&state.nav, &state.raw_page) {
        state
            .text
            .buffer()
            .set_text(&structured_editor_contents(state));
    }
}

fn save_current_password_entry_impl(state: &PasswordPageState, allow_git_unlock_prompt: bool) {
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
    let previous_store = pass_file.store_path().to_string();
    let previous_label = pass_file.label();
    let previous_contents = state.saved_contents.borrow().clone();
    let previous_entry_exists = state.saved_entry_exists.get();
    let target_label = match pass_file.updated_label_from_username(&state.username.text()) {
        Ok(target_label) => target_label,
        Err(err) => {
            state
                .overlay
                .add_toast(Toast::new(username_fallback_failure_message(err)));
            return;
        }
    };
    if allow_git_unlock_prompt
        && platform::prompt_unlock_for_git_commit_if_needed(state, &pass_file)
    {
        return;
    }
    let label = pass_file.label();
    match save_password_entry(pass_file.store_path(), &label, &contents, true) {
        Ok(()) => {
            let active_pass_file = if let Some(target_label) =
                target_label.filter(|target_label| target_label != &label)
            {
                match rename_password_entry(pass_file.store_path(), &label, &target_label) {
                    Ok(()) => {
                        let renamed_pass_file = OpenPassFile::from_label_with_mode(
                            pass_file.store_path(),
                            &target_label,
                            pass_file.username_fallback_mode(),
                        );
                        set_opened_pass_file(renamed_pass_file.clone());
                        renamed_pass_file
                    }
                    Err(err) => {
                        log_error(format!("Failed to move password entry after save: {err}"));
                        state
                            .overlay
                            .add_toast(Toast::new(err.rename_toast_message()));
                        return;
                    }
                }
            } else {
                pass_file.clone()
            };
            let updated_pass_file =
                refresh_opened_pass_file_from_contents(&active_pass_file, &contents)
                    .or(Some(active_pass_file));
            show_password_editor_fields(state);
            sync_editor_contents(state, &contents, updated_pass_file.as_ref());
            sync_saved_password_state(state, &contents, true);
            let current_label = updated_pass_file
                .as_ref()
                .map(OpenPassFile::label)
                .unwrap_or_else(|| previous_label.clone());
            if !previous_entry_exists
                || previous_contents != contents
                || previous_label != current_label
            {
                push_undo_action(restore_saved_entry_action(
                    &previous_store,
                    &previous_label,
                    previous_entry_exists.then_some(previous_contents.as_str()),
                    pass_file.store_path(),
                    &current_label,
                ));
            }
            state.overlay.add_toast(Toast::new("Saved."));
        }
        Err(err) => {
            log_error(format!("Failed to save password entry: {err}"));
            state
                .overlay
                .add_toast(Toast::new(password_save_failure_message(&err)));
        }
    }
}

pub(crate) fn save_current_password_entry(state: &PasswordPageState) {
    save_current_password_entry_impl(state, true);
}

#[cfg(feature = "flatpak")]
pub(super) fn save_current_password_entry_without_git_unlock_prompt(state: &PasswordPageState) {
    save_current_password_entry_impl(state, false);
}

pub(crate) fn show_password_list_page(
    state: &PasswordPageState,
    show_hidden: bool,
    show_duplicates: bool,
) {
    pop_navigation_to_root(&state.nav);

    clear_opened_pass_file();
    let chrome = state.window_chrome();
    show_primary_page_chrome(&chrome, !Preferences::new().stores().is_empty());

    reset_password_editor(state);

    let list_actions = PasswordListActions::new(
        &state.add,
        &state.git,
        &state.store,
        &state.find,
        &state.save,
    );
    load_passwords_async(
        &state.list,
        &list_actions,
        &state.overlay,
        true,
        show_hidden,
        show_duplicates,
    );
}

pub(crate) fn retry_open_password_entry_if_needed(state: &PasswordPageState) -> bool {
    let has_opened_pass_file = get_opened_pass_file().is_some();
    if !should_retry_open_password_entry(
        visible_navigation_page_is(&state.nav, &state.page),
        state.status.is_visible(),
        state.entry.is_visible(),
        has_opened_pass_file,
    ) {
        return false;
    }

    let pass_file =
        get_opened_pass_file().expect("opened pass file should exist when retry is needed");
    open_password_entry_page(state, pass_file, false);
    true
}

#[cfg(test)]
mod tests {
    use super::{
        password_open_failure_message, password_save_failure_message,
        should_retry_open_password_entry,
    };
    use crate::backend::{PasswordEntryError, PasswordEntryWriteError};
    use crate::password::model::{OpenPassFile, UsernameFallbackError};
    use crate::preferences::UsernameFallbackMode;

    #[test]
    fn retry_open_requires_a_hidden_editor_on_the_password_page_with_an_open_item() {
        assert!(should_retry_open_password_entry(true, true, false, true));
        assert!(!should_retry_open_password_entry(false, true, false, true));
        assert!(!should_retry_open_password_entry(true, false, false, true));
        assert!(!should_retry_open_password_entry(true, true, true, true));
        assert!(!should_retry_open_password_entry(true, true, false, false));
    }

    #[test]
    fn password_open_failure_message_falls_back_without_a_specific_error() {
        assert_eq!(
            password_open_failure_message(None),
            "Couldn't open the item."
        );
        assert_eq!(
            password_open_failure_message(Some(&PasswordEntryError::other("boom"))),
            "Couldn't open the item."
        );
    }

    #[test]
    fn password_open_failure_message_uses_specific_private_key_toasts_when_available() {
        #[cfg(feature = "flatpak")]
        {
            assert_eq!(
                password_open_failure_message(Some(&PasswordEntryError::missing_private_key(
                    "missing"
                ))),
                "Add a private key in Preferences."
            );
            assert_eq!(
                password_open_failure_message(Some(&PasswordEntryError::incompatible_private_key(
                    "incompatible"
                ))),
                "This key can't open your items."
            );
        }

        #[cfg(not(feature = "flatpak"))]
        {
            assert_eq!(
                password_open_failure_message(Some(&PasswordEntryError::other("missing"))),
                "Couldn't open the item."
            );
        }
    }

    #[test]
    fn password_save_failure_message_uses_typed_write_error_mapping() {
        assert_eq!(
            password_save_failure_message(&PasswordEntryWriteError::already_exists("duplicate")),
            "An item with that name already exists."
        );
        assert_eq!(
            password_save_failure_message(&PasswordEntryWriteError::LockedPrivateKey(
                "locked".to_string(),
            )),
            "Unlock the key in Preferences."
        );
    }

    #[test]
    fn folder_derived_usernames_update_the_pass_file_path_on_save() {
        let pass_file = OpenPassFile::from_label_with_mode(
            "/tmp/store",
            "work/alice/github",
            UsernameFallbackMode::Folder,
        );
        assert_eq!(
            pass_file.updated_label_from_username("bob"),
            Ok(Some("work/bob/github".to_string()))
        );
    }

    #[test]
    fn explicit_usernames_do_not_move_the_pass_file_path_on_save() {
        let mut pass_file = OpenPassFile::from_label_with_mode(
            "/tmp/store",
            "work/alice/github",
            UsernameFallbackMode::Folder,
        );
        pass_file.refresh_from_contents("secret\nusername: bob");
        assert_eq!(pass_file.updated_label_from_username("carol"), Ok(None));
    }

    #[test]
    fn filename_derived_usernames_update_only_the_file_name_on_save() {
        let pass_file = OpenPassFile::from_label_with_mode(
            "/tmp/store",
            "work/alice/github",
            UsernameFallbackMode::Filename,
        );
        assert_eq!(
            pass_file.updated_label_from_username("gitlab"),
            Ok(Some("work/alice/gitlab".to_string()))
        );
    }

    #[test]
    fn filename_derived_usernames_reject_invalid_names_on_save() {
        let pass_file = OpenPassFile::from_label_with_mode(
            "/tmp/store",
            "work/alice/github",
            UsernameFallbackMode::Filename,
        );
        assert_eq!(
            pass_file.updated_label_from_username(""),
            Err(UsernameFallbackError::EmptyFilename)
        );
    }
}
