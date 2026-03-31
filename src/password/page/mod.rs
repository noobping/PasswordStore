mod editor;
mod linux;
mod standard;
mod state;

use super::file::{
    apply_pass_file_template_contents, clean_pass_file_contents,
    new_pass_file_contents_from_template, pass_file_has_missing_template_fields,
    structured_pass_contents,
};
use super::generation::generate_password;
use super::list::{load_passwords_async, PasswordListActions};
use crate::backend::{
    password_entry_fido2_recipient_count, read_password_entry_with_progress, rename_password_entry,
    save_password_entry, save_password_entry_with_progress, PasswordEntryError,
    PasswordEntryReadProgress, PasswordEntryWriteError, PasswordEntryWriteProgress,
};
use crate::i18n::gettext;
use crate::logging::log_error;
use crate::password::model::{OpenPassFile, UsernameFallbackError};
use crate::password::opened::{
    clear_opened_pass_file, get_opened_pass_file, is_opened_pass_file,
    refresh_opened_pass_file_from_contents, set_opened_pass_file,
};
use crate::password::undo::{push_undo_action, restore_saved_entry_action};
use crate::preferences::Preferences;
use crate::support::actions::activate_widget_action;
use crate::support::background::spawn_progress_result_task;
use crate::support::ui::{
    pop_navigation_to_root, push_navigation_page_if_needed, visible_navigation_page_is,
};
use crate::support::validation::validate_pass_file_email_fields;
use crate::window::navigation::{show_primary_page_chrome, HasWindowChrome, APP_WINDOW_TITLE};
use crate::window::sync_tools_action_availability;
use adw::prelude::*;
use adw::{Dialog, Toast};
use std::string::ToString;

use self::editor::{
    add_empty_dynamic_field, add_empty_otp_secret as add_empty_otp_secret_to_editor,
    current_editor_contents, focus_field_add_row, structured_editor_contents, sync_editor_contents,
};
use self::linux as platform;
use self::platform::handle_open_password_entry_error;
pub use self::state::PasswordPageState;
use self::state::{
    reset_password_editor, show_password_editor_chrome, show_password_editor_fields,
    show_password_loading_state, show_password_status_message, sync_saved_password_state,
};

fn password_open_failure_message(error: Option<&PasswordEntryError>) -> &'static str {
    error
        .and_then(PasswordEntryError::toast_message)
        .unwrap_or("Couldn't open the item.")
}

fn password_save_failure_message(error: &PasswordEntryWriteError) -> &'static str {
    error.save_toast_message()
}

const fn username_fallback_failure_message(error: UsernameFallbackError) -> &'static str {
    error.toast_message()
}

const fn password_save_status_copy(fido2_recipient_count: usize) -> (&'static str, &'static str) {
    if fido2_recipient_count > 1 {
        ("Saving item", "Security keys will be checked one by one.")
    } else if fido2_recipient_count == 1 {
        (
            "Saving item",
            "Touch the security key if it starts blinking.",
        )
    } else {
        ("Saving item", "Please wait.")
    }
}

fn password_save_progress_description(progress: &PasswordEntryWriteProgress) -> String {
    password_entry_progress_description(progress)
}

fn password_open_progress_description(progress: &PasswordEntryReadProgress) -> String {
    password_entry_progress_description(progress)
}

fn password_entry_progress_description(progress: &PasswordEntryReadProgress) -> String {
    gettext("Step {current} of {total}: touch the security key if it starts blinking.")
        .replace("{current}", &progress.current_step.to_string())
        .replace("{total}", &progress.total_steps.to_string())
}

pub(super) const fn password_open_status_copy(
    fido2_recipient_count: usize,
) -> (&'static str, &'static str) {
    if fido2_recipient_count > 1 {
        (
            "Opening item",
            "Touch each security key when it starts blinking.",
        )
    } else if fido2_recipient_count == 1 {
        (
            "Opening item",
            "Touch the security key if it starts blinking.",
        )
    } else {
        ("Opening item", "Please wait.")
    }
}

pub(super) const fn password_unlock_status_copy(
    fido2_recipient_count: usize,
) -> (&'static str, &'static str) {
    if fido2_recipient_count > 1 {
        (
            "Unlock key",
            "Touch each security key when it starts blinking.",
        )
    } else if fido2_recipient_count == 1 {
        (
            "Unlock key",
            "Touch the security key if it starts blinking.",
        )
    } else {
        ("Unlock key", "Unlock your key to continue.")
    }
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
        .add_toast(Toast::new(&gettext(password_open_failure_message(error))));
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

fn validate_password_save_contents(contents: &str) -> Result<(), String> {
    validate_pass_file_email_fields(contents).map_err(ToString::to_string)
}

fn prepared_password_save_contents(
    contents: String,
    clear_empty_fields_before_save: bool,
) -> String {
    if clear_empty_fields_before_save {
        clean_pass_file_contents(&contents)
    } else {
        contents
    }
}

fn prepare_password_save_context(state: &PasswordPageState) -> Result<PasswordSaveContext, String> {
    let pass_file =
        get_opened_pass_file(&state.nav).ok_or_else(|| "Open an item first.".to_string())?;
    let preferences = Preferences::new();
    let editor_contents = current_editor_contents(state);

    let otp_url = state
        .otp
        .current_url_for_save()
        .map_err(ToString::to_string)?;
    let contents = if visible_navigation_page_is(&state.nav, &state.raw_page) {
        editor_contents
    } else {
        structured_pass_contents(
            &state.entry.text(),
            &state.username.text(),
            otp_url.as_deref(),
            &state.structured_templates.borrow(),
            &state.dynamic_rows.borrow(),
        )
    };
    let contents =
        prepared_password_save_contents(contents, preferences.clear_empty_fields_before_save());
    let target_label = pass_file
        .updated_label_from_username(&state.username.text())
        .map_err(|err| username_fallback_failure_message(err).to_string())?;
    validate_password_save_contents(&contents)?;

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
    state: &PasswordPageState,
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
    set_opened_pass_file(&state.nav, renamed_pass_file.clone());
    Ok(renamed_pass_file)
}

fn finish_password_save(
    state: &PasswordPageState,
    save_context: &PasswordSaveContext,
    active_pass_file: &OpenPassFile,
) {
    let updated_pass_file = refresh_opened_pass_file_from_contents(
        &state.nav,
        active_pass_file,
        &save_context.contents,
    )
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
        push_undo_action(
            &state.nav,
            restore_saved_entry_action(
                &save_context.previous_store,
                &save_context.previous_label,
                save_context
                    .previous_entry_exists
                    .then_some(save_context.previous_contents.as_str()),
                save_context.pass_file.store_path(),
                &current_label,
            ),
        );
    }
    state.overlay.add_toast(Toast::new(&gettext("Saved.")));
}

fn handle_password_save_result(
    state: &PasswordPageState,
    save_context: &PasswordSaveContext,
    result: Result<(), PasswordEntryWriteError>,
) {
    let label = save_context.pass_file.label();
    match result {
        Ok(()) => match renamed_pass_file_after_save(state, save_context, &label) {
            Ok(active_pass_file) => finish_password_save(state, save_context, &active_pass_file),
            Err(err) => {
                show_password_editor_fields(state);
                log_error(format!("Failed to move password entry after save: {err}"));
                state
                    .overlay
                    .add_toast(Toast::new(&gettext(err.rename_toast_message())));
            }
        },
        Err(err) => {
            show_password_editor_fields(state);
            log_error(format!("Failed to save password entry: {err}"));
            state
                .overlay
                .add_toast(Toast::new(&gettext(password_save_failure_message(&err))));
        }
    }
}

fn start_password_save_with_progress(
    state: &PasswordPageState,
    save_context: PasswordSaveContext,
    fido2_recipient_count: usize,
) {
    let (status_title, status_description) = password_save_status_copy(fido2_recipient_count);
    show_password_status_message(state, status_title, status_description);
    state.save.set_sensitive(false);

    let store_root = save_context.pass_file.store_path().to_string();
    let label = save_context.pass_file.label();
    let contents = save_context.contents.clone();
    let state_for_result = state.clone();
    let state_for_disconnect = state.clone();
    let pass_file_for_result = save_context.pass_file.clone();
    let pass_file_for_disconnect = save_context.pass_file.clone();
    let state_for_progress = state.clone();
    let pass_file_for_progress = save_context.pass_file.clone();
    spawn_progress_result_task(
        move |progress_tx| {
            let mut report_progress = move |progress: PasswordEntryWriteProgress| {
                let _ = progress_tx.send(progress);
            };
            save_password_entry_with_progress(
                &store_root,
                &label,
                &contents,
                true,
                &mut report_progress,
            )
        },
        move |progress| {
            if !is_opened_pass_file(&state_for_progress.nav, &pass_file_for_progress) {
                return;
            }
            show_password_status_message(
                &state_for_progress,
                "Saving item",
                &password_save_progress_description(&progress),
            );
        },
        move |result| {
            state_for_result.save.set_sensitive(true);
            if !is_opened_pass_file(&state_for_result.nav, &pass_file_for_result) {
                return;
            }
            handle_password_save_result(&state_for_result, &save_context, result);
        },
        move || {
            state_for_disconnect.save.set_sensitive(true);
            if is_opened_pass_file(&state_for_disconnect.nav, &pass_file_for_disconnect) {
                show_password_editor_fields(&state_for_disconnect);
            }
            log_error("Password save worker disconnected unexpectedly.".to_string());
            state_for_disconnect
                .overlay
                .add_toast(Toast::new(&gettext("Couldn't save changes.")));
        },
    );
}

pub fn open_password_entry_page(
    state: &PasswordPageState,
    opened_pass_file: OpenPassFile,
    push_page: bool,
) {
    let pass_label = opened_pass_file.label();
    let store_for_thread = opened_pass_file.store_path().to_string();
    let fido2_recipient_count =
        password_entry_fido2_recipient_count(opened_pass_file.store_path(), &pass_label);
    set_opened_pass_file(&state.nav, opened_pass_file.clone());

    show_password_loading_state(
        state,
        opened_pass_file.title(),
        &pass_label,
        fido2_recipient_count,
    );
    if push_page {
        push_navigation_page_if_needed(&state.nav, &state.page);
    }

    let label_for_thread = pass_label;
    let state_for_result = state.clone();
    let opened_pass_file_for_result = opened_pass_file.clone();
    let state_for_disconnect = state.clone();
    let opened_pass_file_for_disconnect = opened_pass_file.clone();
    let state_for_progress = state.clone();
    let opened_pass_file_for_progress = opened_pass_file;
    spawn_progress_result_task(
        move |progress_tx| {
            let mut report_progress = move |progress: PasswordEntryReadProgress| {
                let _ = progress_tx.send(progress);
            };
            read_password_entry_with_progress(
                &store_for_thread,
                &label_for_thread,
                &mut report_progress,
            )
        },
        move |progress| {
            if !is_opened_pass_file(&state_for_progress.nav, &opened_pass_file_for_progress) {
                return;
            }
            show_password_status_message(
                &state_for_progress,
                "Opening item",
                &password_open_progress_description(&progress),
            );
        },
        move |result| {
            if !is_opened_pass_file(&state_for_result.nav, &opened_pass_file_for_result) {
                return;
            }

            match result {
                Ok(output) => {
                    let updated_pass_file = refresh_opened_pass_file_from_contents(
                        &state_for_result.nav,
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
            if !is_opened_pass_file(&state_for_disconnect.nav, &opened_pass_file_for_disconnect) {
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
    add_dialog: &Dialog,
) -> Result<(), &'static str> {
    let path = path.trim();
    if path.is_empty() {
        return Err("Enter a name.");
    }

    let settings = Preferences::new();
    let store_root = store_root.unwrap_or_else(|| settings.store());
    if store_root.trim().is_empty() {
        return Err("Add a store folder first.");
    }
    let template_contents =
        new_pass_file_contents_from_template(&settings.new_pass_file_template());
    let opened_pass_file = OpenPassFile::from_label(store_root, path);
    set_opened_pass_file(&state.nav, opened_pass_file.clone());
    let template_pass_file =
        refresh_opened_pass_file_from_contents(&state.nav, &opened_pass_file, &template_contents)
            .or_else(|| get_opened_pass_file(&state.nav));

    show_password_editor_chrome(state, "New item", path);
    show_password_editor_fields(state);
    state.otp.clear();
    push_navigation_page_if_needed(&state.nav, &state.page);

    add_dialog.force_close();
    sync_editor_contents(state, &template_contents, template_pass_file.as_ref());
    sync_saved_password_state(state, &template_contents, false);
    Ok(())
}

pub fn show_raw_pass_file_page(state: &PasswordPageState) {
    let contents = structured_editor_contents(state);
    state.text.buffer().set_text(&contents);

    let subtitle = get_opened_pass_file(&state.nav).map_or_else(
        || APP_WINDOW_TITLE.to_string(),
        |pass_file| pass_file.label(),
    );
    show_password_editor_chrome(state, "Raw Pass File", &subtitle);

    push_navigation_page_if_needed(&state.nav, &state.raw_page);
}

pub fn add_empty_otp_secret(state: &PasswordPageState) {
    if !state.otp_add_button.is_visible() {
        return;
    }

    add_empty_otp_secret_to_editor(state);
}

pub fn focus_add_pass_field_input(state: &PasswordPageState) {
    if !visible_navigation_page_is(&state.nav, &state.page) || !state.entry.is_visible() {
        return;
    }

    focus_field_add_row(state);
}

pub fn add_pass_field_from_input(state: &PasswordPageState) {
    if !visible_navigation_page_is(&state.nav, &state.page) || !state.entry.is_visible() {
        return;
    }

    match add_empty_dynamic_field(state, &state.field_add_row.text(), None) {
        Ok(()) => state.field_add_row.set_text(""),
        Err(message) => state.overlay.add_toast(Toast::new(&gettext(message))),
    }
}

pub fn refresh_apply_template_button(state: &PasswordPageState) {
    sync_apply_template_button(state, &current_editor_contents(state));
}

pub fn apply_pass_file_template(state: &PasswordPageState) {
    let editing_structured = visible_navigation_page_is(&state.nav, &state.page);
    let editing_raw = visible_navigation_page_is(&state.nav, &state.raw_page);
    if (!editing_structured || !state.entry.is_visible()) && !editing_raw {
        return;
    }

    let contents = current_editor_contents(state);
    let templated_contents =
        apply_pass_file_template_contents(&contents, &Preferences::new().new_pass_file_template());
    if templated_contents == contents {
        return;
    }

    let pass_file = get_opened_pass_file(&state.nav);
    let updated_pass_file = pass_file
        .as_ref()
        .and_then(|pass_file| {
            refresh_opened_pass_file_from_contents(&state.nav, pass_file, &templated_contents)
        })
        .or(pass_file);
    sync_editor_contents(state, &templated_contents, updated_pass_file.as_ref());
    state
        .overlay
        .add_toast(Toast::new(&gettext("Added missing template fields.")));
}

fn sync_apply_template_button(state: &PasswordPageState, contents: &str) {
    state
        .template_button
        .set_visible(pass_file_has_missing_template_fields(
            contents,
            &Preferences::new().new_pass_file_template(),
        ));
}

pub fn clean_pass_file(state: &PasswordPageState) {
    let editing_structured = visible_navigation_page_is(&state.nav, &state.page);
    let editing_raw = visible_navigation_page_is(&state.nav, &state.raw_page);
    if (!editing_structured || !state.entry.is_visible()) && !editing_raw {
        return;
    }

    let contents = current_editor_contents(state);
    let cleaned_contents = clean_pass_file_contents(&contents);
    if cleaned_contents == contents {
        return;
    }

    let pass_file = get_opened_pass_file(&state.nav);
    let updated_pass_file = pass_file
        .as_ref()
        .and_then(|pass_file| {
            refresh_opened_pass_file_from_contents(&state.nav, pass_file, &cleaned_contents)
        })
        .or(pass_file);
    sync_editor_contents(state, &cleaned_contents, updated_pass_file.as_ref());
    state
        .overlay
        .add_toast(Toast::new(&gettext("Removed empty fields.")));
}

pub fn password_page_has_unsaved_changes(state: &PasswordPageState) -> bool {
    current_editor_contents(state) != *state.saved_contents.borrow()
}

pub fn revert_unsaved_password_changes(state: &PasswordPageState) -> bool {
    if !password_page_has_unsaved_changes(state) {
        return false;
    }

    let saved_contents = state.saved_contents.borrow().clone();
    let pass_file = get_opened_pass_file(&state.nav);
    sync_editor_contents(state, &saved_contents, pass_file.as_ref());
    state.overlay.add_toast(Toast::new(&gettext("Reverted.")));
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
            state.overlay.add_toast(Toast::new(&gettext(&message)));
            return;
        }
    };

    if allow_git_unlock_prompt
        && platform::prompt_unlock_for_git_commit_if_needed(state, &save_context.pass_file)
    {
        return;
    }
    let fido2_recipient_count = password_entry_fido2_recipient_count(
        save_context.pass_file.store_path(),
        &save_context.pass_file.label(),
    );
    if fido2_recipient_count > 0 {
        start_password_save_with_progress(state, save_context, fido2_recipient_count);
        return;
    }

    let result = save_password_entry(
        save_context.pass_file.store_path(),
        &save_context.pass_file.label(),
        &save_context.contents,
        true,
    );
    handle_password_save_result(state, &save_context, result);
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

    clear_opened_pass_file(&state.nav);
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
    if let Some(root) = state.list.root() {
        if let Ok(window) = root.downcast::<adw::ApplicationWindow>() {
            sync_tools_action_availability(&window);
        }
    }
}

pub fn retry_open_password_entry_if_needed(state: &PasswordPageState) -> bool {
    let pass_file = get_opened_pass_file(&state.nav);
    if !should_retry_open_password_entry(password_page_display(state), pass_file.is_some()) {
        return false;
    }

    let Some(pass_file) = pass_file else {
        log_error("Retry-open was requested without an opened pass file.");
        return false;
    };
    open_password_entry_page(state, pass_file, false);
    true
}

#[cfg(test)]
mod tests {
    use super::{
        password_open_failure_message, password_open_progress_description,
        password_open_status_copy, password_save_failure_message,
        password_save_progress_description, password_save_status_copy, password_unlock_status_copy,
        prepared_password_save_contents, should_retry_open_password_entry,
        validate_password_save_contents, PasswordPageDisplay,
    };
    use crate::backend::{
        PasswordEntryError, PasswordEntryReadProgress, PasswordEntryWriteError,
        PasswordEntryWriteProgress,
    };
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
    fn password_save_status_copy_mentions_touch_for_fido2_saves() {
        assert_eq!(
            password_save_status_copy(1),
            (
                "Saving item",
                "Touch the security key if it starts blinking.",
            )
        );
        assert_eq!(
            password_save_status_copy(2),
            ("Saving item", "Security keys will be checked one by one.",)
        );
        assert_eq!(
            password_save_status_copy(0),
            ("Saving item", "Please wait.")
        );
    }

    #[test]
    fn password_save_progress_description_shows_step_counts() {
        assert_eq!(
            password_save_progress_description(&PasswordEntryWriteProgress {
                current_step: 2,
                total_steps: 3,
            }),
            "Step 2 of 3: touch the security key if it starts blinking."
        );
    }

    #[test]
    fn password_open_progress_description_shows_step_counts() {
        assert_eq!(
            password_open_progress_description(&PasswordEntryReadProgress {
                current_step: 1,
                total_steps: 2,
            }),
            "Step 1 of 2: touch the security key if it starts blinking."
        );
    }

    #[test]
    fn password_open_status_copy_mentions_touch_for_fido2_entries() {
        assert_eq!(
            password_open_status_copy(2),
            (
                "Opening item",
                "Touch each security key when it starts blinking.",
            )
        );
        assert_eq!(
            password_open_status_copy(1),
            (
                "Opening item",
                "Touch the security key if it starts blinking.",
            )
        );
        assert_eq!(
            password_open_status_copy(0),
            ("Opening item", "Please wait.")
        );
    }

    #[test]
    fn password_unlock_status_copy_mentions_touch_for_fido2_entries() {
        assert_eq!(
            password_unlock_status_copy(2),
            (
                "Unlock key",
                "Touch each security key when it starts blinking."
            )
        );
        assert_eq!(
            password_unlock_status_copy(1),
            (
                "Unlock key",
                "Touch the security key if it starts blinking."
            )
        );
        assert_eq!(
            password_unlock_status_copy(0),
            ("Unlock key", "Unlock your key to continue.")
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

    #[test]
    fn pass_file_save_validation_rejects_invalid_email_fields() {
        assert_eq!(
            validate_password_save_contents("secret\nemail: person@example.com"),
            Ok(())
        );
        assert_eq!(
            validate_password_save_contents("secret\nemail: invalid"),
            Err("Email fields must use a valid email address.".to_string())
        );
    }

    #[test]
    fn prepared_password_save_contents_can_auto_clean_empty_fields() {
        assert_eq!(
            prepared_password_save_contents(
                "secret\nusername:\nurl: https://example.com".to_string(),
                true
            ),
            "secret\nurl: https://example.com".to_string()
        );
        assert_eq!(
            prepared_password_save_contents(
                "secret\nusername:\nurl: https://example.com".to_string(),
                false
            ),
            "secret\nusername:\nurl: https://example.com".to_string()
        );
    }
}
