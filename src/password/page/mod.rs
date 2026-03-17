mod editor;
mod linux;
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
use crate::support::actions::activate_widget_action;
use crate::support::background::spawn_result_task;
use crate::support::ui::{
    pop_navigation_to_root, push_navigation_page_if_needed, visible_navigation_page_is,
};
use crate::window::navigation::{show_primary_page_chrome, HasWindowChrome, APP_WINDOW_TITLE};
use adw::gtk::Popover;
use adw::prelude::*;
use adw::Toast;
use std::string::ToString;

use self::editor::{
    add_empty_otp_secret as add_empty_otp_secret_to_editor, current_editor_contents,
    structured_editor_contents, sync_editor_contents,
};
use self::linux as platform;
use self::platform::handle_open_password_entry_error;
pub use self::state::PasswordPageState;
use self::state::{
    reset_password_editor, show_password_editor_chrome, show_password_editor_fields,
    show_password_loading_state, sync_saved_password_state,
};

fn password_open_failure_message(error: Option<&PasswordEntryError>) -> &'static str {
    error
        .and_then(PasswordEntryError::toast_message)
        .unwrap_or("Couldn't open the item.")
}

const fn password_save_failure_message(error: &PasswordEntryWriteError) -> &'static str {
    error.save_toast_message()
}

const fn username_fallback_failure_message(error: UsernameFallbackError) -> &'static str {
    error.toast_message()
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum PasswordPageDisplay {
    Hidden,
    Loading,
    Editor,
}

struct PasswordSaveContext {
    pass_file: OpenPassFile,
    contents: String,
    previous_store: String,
    previous_label: String,
    previous_contents: String,
    previous_entry_exists: bool,
    target_label: Option<String>,
}

fn show_password_open_failure(state: &PasswordPageState, error: Option<&PasswordEntryError>) {
    activate_widget_action(&state.nav, "win.go-home");
    state
        .overlay
        .add_toast(Toast::new(password_open_failure_message(error)));
}

const fn should_retry_open_password_entry(
    page_display: PasswordPageDisplay,
    has_opened_pass_file: bool,
) -> bool {
    matches!(page_display, PasswordPageDisplay::Loading) && has_opened_pass_file
}

fn password_page_display(state: &PasswordPageState) -> PasswordPageDisplay {
    if !visible_navigation_page_is(&state.nav, &state.page) {
        return PasswordPageDisplay::Hidden;
    }
    if state.status.is_visible() && !state.entry.is_visible() {
        return PasswordPageDisplay::Loading;
    }

    PasswordPageDisplay::Editor
}

fn prepare_password_save_context(state: &PasswordPageState) -> Result<PasswordSaveContext, String> {
    let pass_file = get_opened_pass_file().ok_or_else(|| "Open an item first.".to_string())?;

    let contents = current_editor_contents(state);
    let password = contents.lines().next().unwrap_or_default().to_string();
    if password.is_empty() {
        return Err("Enter a password.".to_string());
    }

    let otp_url = state
        .otp
        .current_url_for_save()
        .map_err(ToString::to_string)?;
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
    let target_label = pass_file
        .updated_label_from_username(&state.username.text())
        .map_err(|err| username_fallback_failure_message(err).to_string())?;

    Ok(PasswordSaveContext {
        previous_store: pass_file.store_path().to_string(),
        previous_label: pass_file.label(),
        previous_contents: state.saved_contents.borrow().clone(),
        previous_entry_exists: state.saved_entry_exists.get(),
        pass_file,
        contents,
        target_label,
    })
}

fn renamed_pass_file_after_save(
    save_context: &PasswordSaveContext,
    label: &str,
) -> Result<OpenPassFile, PasswordEntryWriteError> {
    let Some(target_label) = save_context
        .target_label
        .as_ref()
        .filter(|target_label| target_label.as_str() != label)
    else {
        return Ok(save_context.pass_file.clone());
    };

    rename_password_entry(save_context.pass_file.store_path(), label, target_label)?;
    let renamed_pass_file = OpenPassFile::from_label_with_mode(
        save_context.pass_file.store_path(),
        target_label,
        save_context.pass_file.username_fallback_mode(),
    );
    set_opened_pass_file(renamed_pass_file.clone());
    Ok(renamed_pass_file)
}

fn finish_password_save(
    state: &PasswordPageState,
    save_context: &PasswordSaveContext,
    active_pass_file: &OpenPassFile,
) {
    let updated_pass_file =
        refresh_opened_pass_file_from_contents(active_pass_file, &save_context.contents)
            .or_else(|| Some(active_pass_file.clone()));
    show_password_editor_fields(state);
    sync_editor_contents(state, &save_context.contents, updated_pass_file.as_ref());
    sync_saved_password_state(state, &save_context.contents, true);
    let current_label = updated_pass_file
        .as_ref()
        .map_or_else(|| save_context.previous_label.clone(), OpenPassFile::label);
    if !save_context.previous_entry_exists
        || save_context.previous_contents != save_context.contents
        || save_context.previous_label != current_label
    {
        push_undo_action(restore_saved_entry_action(
            &save_context.previous_store,
            &save_context.previous_label,
            save_context
                .previous_entry_exists
                .then_some(save_context.previous_contents.as_str()),
            save_context.pass_file.store_path(),
            &current_label,
        ));
    }
    state.overlay.add_toast(Toast::new("Saved."));
}

pub fn open_password_entry_page(
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

    let label_for_thread = pass_label;
    let state_for_result = state.clone();
    let opened_pass_file_for_result = opened_pass_file.clone();
    let state_for_disconnect = state.clone();
    let opened_pass_file_for_disconnect = opened_pass_file;
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

pub fn begin_new_password_entry(
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

pub fn show_raw_pass_file_page(state: &PasswordPageState) {
    let contents = structured_editor_contents(state);
    state.text.buffer().set_text(&contents);

    let subtitle = get_opened_pass_file().map_or_else(
        || APP_WINDOW_TITLE.to_string(),
        |pass_file| pass_file.label(),
    );
    show_password_editor_chrome(state, "Raw Pass File", &subtitle);

    push_navigation_page_if_needed(&state.nav, &state.raw_page);
}

pub fn add_empty_otp_secret(state: &PasswordPageState) {
    add_empty_otp_secret_to_editor(state);
}

pub fn password_page_has_unsaved_changes(state: &PasswordPageState) -> bool {
    current_editor_contents(state) != *state.saved_contents.borrow()
}

pub fn revert_unsaved_password_changes(state: &PasswordPageState) -> bool {
    if !password_page_has_unsaved_changes(state) {
        return false;
    }

    let saved_contents = state.saved_contents.borrow().clone();
    let pass_file = get_opened_pass_file();
    sync_editor_contents(state, &saved_contents, pass_file.as_ref());
    state.overlay.add_toast(Toast::new("Reverted."));
    true
}

pub fn generate_password_entry(state: &PasswordPageState) {
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
    let save_context = match prepare_password_save_context(state) {
        Ok(save_context) => save_context,
        Err(message) => {
            state.overlay.add_toast(Toast::new(&message));
            return;
        }
    };

    if allow_git_unlock_prompt
        && platform::prompt_unlock_for_git_commit_if_needed(state, &save_context.pass_file)
    {
        return;
    }
    let label = save_context.pass_file.label();
    match save_password_entry(
        save_context.pass_file.store_path(),
        &label,
        &save_context.contents,
        true,
    ) {
        Ok(()) => match renamed_pass_file_after_save(&save_context, &label) {
            Ok(active_pass_file) => finish_password_save(state, &save_context, &active_pass_file),
            Err(err) => {
                log_error(format!("Failed to move password entry after save: {err}"));
                state
                    .overlay
                    .add_toast(Toast::new(err.rename_toast_message()));
            }
        },
        Err(err) => {
            log_error(format!("Failed to save password entry: {err}"));
            state
                .overlay
                .add_toast(Toast::new(password_save_failure_message(&err)));
        }
    }
}

pub fn save_current_password_entry(state: &PasswordPageState) {
    save_current_password_entry_impl(state, true);
}

pub(super) fn save_current_password_entry_without_git_unlock_prompt(state: &PasswordPageState) {
    save_current_password_entry_impl(state, false);
}

pub fn show_password_list_page(
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

pub fn retry_open_password_entry_if_needed(state: &PasswordPageState) -> bool {
    let has_opened_pass_file = get_opened_pass_file().is_some();
    if !should_retry_open_password_entry(password_page_display(state), has_opened_pass_file) {
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
        should_retry_open_password_entry, PasswordPageDisplay,
    };
    use crate::backend::{PasswordEntryError, PasswordEntryWriteError};
    use crate::password::model::{OpenPassFile, UsernameFallbackError};
    use crate::preferences::UsernameFallbackMode;

    fn expected_missing_private_key_open_failure_message() -> &'static str {
        "Add a private key in Preferences."
    }

    #[test]
    fn retry_open_requires_a_hidden_editor_on_the_password_page_with_an_open_item() {
        assert!(should_retry_open_password_entry(
            PasswordPageDisplay::Loading,
            true,
        ));
        assert!(!should_retry_open_password_entry(
            PasswordPageDisplay::Hidden,
            true,
        ));
        assert!(!should_retry_open_password_entry(
            PasswordPageDisplay::Editor,
            true,
        ));
        assert!(!should_retry_open_password_entry(
            PasswordPageDisplay::Loading,
            false,
        ));
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
        assert_eq!(
            password_open_failure_message(Some(&PasswordEntryError::missing_private_key(
                "missing"
            ))),
            expected_missing_private_key_open_failure_message()
        );
        assert_eq!(
            password_open_failure_message(Some(&PasswordEntryError::incompatible_private_key(
                "incompatible"
            ))),
            "This key can't open your items."
        );
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
