use crate::logging::log_error;
use crate::password::generation::{PasswordGenerationControls, PasswordGenerationSettings};
use crate::preferences::{Preferences, UsernameFallbackMode};
use crate::store::management::{rebuild_store_list, StoreRecipientsPageState};
use crate::support::actions::register_window_action;
use crate::support::ui::push_navigation_page_if_needed;
use crate::window::navigation::{show_secondary_page_chrome, HasWindowChrome, APP_WINDOW_TITLE};
use adw::gtk::{Button, CheckButton, ListBox, TextView};
use adw::prelude::*;
use adw::{
    ApplicationWindow, ComboRow, EntryRow, NavigationPage, NavigationView, Toast, ToastOverlay,
    WindowTitle,
};
use std::cell::Cell;
use std::rc::Rc;

#[cfg(feature = "setup")]
mod setup;
mod standard;

#[cfg(feature = "setup")]
pub(crate) use self::setup::register_install_locally_action;
pub(crate) use self::standard::{
    connect_backend_row, connect_pass_command_row, initialize_backend_row,
};

pub(super) fn toast_preferences_save_error(
    overlay: &ToastOverlay,
    context: &str,
    err: &adw::glib::BoolError,
) {
    log_error(format!(
        "Failed to save preference ({context}): {}",
        err.message
    ));
    overlay.add_toast(Toast::new("Couldn't save that setting."));
}

#[derive(Clone)]
pub(crate) struct PreferencesActionState {
    pub(crate) window: ApplicationWindow,
    pub(crate) nav: NavigationView,
    pub(crate) page: NavigationPage,
    pub(crate) back: Button,
    pub(crate) add: Button,
    pub(crate) find: Button,
    pub(crate) git: Button,
    pub(crate) store: Button,
    pub(crate) save: Button,
    pub(crate) raw: Button,
    pub(crate) win: WindowTitle,
    pub(crate) template_view: TextView,
    pub(crate) username_folder_check: CheckButton,
    pub(crate) username_filename_check: CheckButton,
    pub(crate) generator_controls: PasswordGenerationControls,
    pub(crate) stores_list: ListBox,
    pub(crate) overlay: ToastOverlay,
    pub(crate) recipients_page: StoreRecipientsPageState,
    pub(crate) pass_row: EntryRow,
    pub(crate) backend_row: ComboRow,
}

pub(crate) fn connect_new_password_template_autosave(
    template_view: &TextView,
    overlay: &ToastOverlay,
) {
    let overlay = overlay.clone();
    let preferences = Preferences::new();
    let buffer = template_view.buffer();
    buffer.connect_changed(move |buffer| {
        let (start, end) = buffer.bounds();
        let template = buffer.text(&start, &end, false).to_string();
        if template == preferences.new_pass_file_template() {
            return;
        }
        if let Err(err) = preferences.set_new_pass_file_template(&template) {
            toast_preferences_save_error(&overlay, "new item template", &err);
        }
    });
}

fn sync_username_fallback_checks(
    folder_check: &CheckButton,
    filename_check: &CheckButton,
    mode: UsernameFallbackMode,
) {
    let (folder_active, filename_active) = username_fallback_check_state(mode);
    folder_check.set_active(folder_active);
    filename_check.set_active(filename_active);
}

fn username_fallback_check_state(mode: UsernameFallbackMode) -> (bool, bool) {
    match mode {
        UsernameFallbackMode::Folder => (true, false),
        UsernameFallbackMode::Filename => (false, true),
    }
}

pub(crate) fn connect_username_fallback_autosave(
    folder_check: &CheckButton,
    filename_check: &CheckButton,
    overlay: &ToastOverlay,
) {
    let preferences = Preferences::new();
    sync_username_fallback_checks(
        folder_check,
        filename_check,
        preferences.username_fallback_mode(),
    );

    let syncing = Rc::new(Cell::new(false));
    for (button, mode) in [
        (folder_check.clone(), UsernameFallbackMode::Folder),
        (filename_check.clone(), UsernameFallbackMode::Filename),
    ] {
        let folder_check = folder_check.clone();
        let filename_check = filename_check.clone();
        let overlay = overlay.clone();
        let preferences = preferences.clone();
        let syncing = syncing.clone();
        button.connect_toggled(move |button| {
            if syncing.get() || !button.is_active() {
                return;
            }

            let stored = preferences.username_fallback_mode();
            if stored == mode {
                return;
            }

            syncing.set(true);
            if let Err(err) = preferences.set_username_fallback_mode(mode) {
                toast_preferences_save_error(&overlay, "username fallback", &err);
                sync_username_fallback_checks(&folder_check, &filename_check, stored);
            } else {
                sync_username_fallback_checks(&folder_check, &filename_check, mode);
            }
            syncing.set(false);
        });
    }
}

pub(crate) fn connect_password_generation_autosave(
    controls: &PasswordGenerationControls,
    mirrors: &[PasswordGenerationControls],
    overlay: &ToastOverlay,
) {
    sync_password_generation_controls(controls, &Preferences::new().password_generation_settings());
    for mirror in mirrors {
        sync_password_generation_controls(
            mirror,
            &Preferences::new().password_generation_settings(),
        );
    }

    let controls = controls.clone();
    let mirrors = mirrors.to_vec();
    let overlay = overlay.clone();
    let preferences = Preferences::new();
    let syncing = Rc::new(Cell::new(false));
    let changed: Rc<dyn Fn()> = Rc::new({
        let controls = controls.clone();
        let mirrors = mirrors.clone();
        let overlay = overlay.clone();
        let preferences = preferences.clone();
        let syncing = syncing.clone();
        move || {
            if syncing.get() {
                return;
            }

            syncing.set(true);
            let stored = preferences.password_generation_settings();
            let updated = controls.settings().normalized();
            let save_result = preferences.set_password_generation_settings(&updated);
            match save_result {
                Ok(()) => {
                    sync_password_generation_controls(&controls, &updated);
                    for mirror in &mirrors {
                        sync_password_generation_controls(mirror, &updated);
                    }
                }
                Err(err) => {
                    toast_preferences_save_error(&overlay, "password generation", &err);
                    sync_password_generation_controls(&controls, &stored);
                    for mirror in &mirrors {
                        sync_password_generation_controls(mirror, &stored);
                    }
                }
            }
            syncing.set(false);
        }
    });
    controls.connect_changed(changed);
}

pub(crate) fn sync_password_generation_controls(
    controls: &PasswordGenerationControls,
    settings: &PasswordGenerationSettings,
) {
    controls.set_settings(settings);
}

pub(crate) fn register_open_preferences_action(
    window: &ApplicationWindow,
    state: &PreferencesActionState,
) {
    let state = state.clone();
    register_window_action(window, "open-preferences", move || {
        let chrome = state.window_chrome();
        show_secondary_page_chrome(&chrome, "Preferences", APP_WINDOW_TITLE, false);

        push_navigation_page_if_needed(&state.nav, &state.page);

        let settings = Preferences::new();
        self::standard::refresh_open_preferences_state(&state, &settings);
        sync_username_fallback_checks(
            &state.username_folder_check,
            &state.username_filename_check,
            settings.username_fallback_mode(),
        );
        sync_password_generation_controls(
            &state.generator_controls,
            &settings.password_generation_settings(),
        );
        state
            .template_view
            .buffer()
            .set_text(&settings.new_pass_file_template());
        rebuild_store_list(
            &state.stores_list,
            &settings,
            &state.window,
            &state.overlay,
            &state.recipients_page,
        );
    });
}

#[cfg(test)]
mod tests {
    use super::username_fallback_check_state;
    use crate::preferences::UsernameFallbackMode;

    #[test]
    fn username_fallback_sync_marks_only_the_selected_mode() {
        assert_eq!(
            username_fallback_check_state(UsernameFallbackMode::Folder),
            (true, false)
        );
        assert_eq!(
            username_fallback_check_state(UsernameFallbackMode::Filename),
            (false, true)
        );
    }
}
