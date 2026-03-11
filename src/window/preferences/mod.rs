use crate::logging::log_error;
use crate::password::generation::{PasswordGenerationControls, PasswordGenerationSettings};
use crate::preferences::Preferences;
use crate::store::management::{rebuild_store_list, StoreRecipientsPageState};
use crate::support::actions::register_window_action;
use crate::support::ui::push_navigation_page_if_needed;
use crate::window::navigation::{show_secondary_page_chrome, HasWindowChrome, APP_WINDOW_TITLE};
use adw::gtk::{Button, ListBox, TextView};
use adw::prelude::*;
use adw::{ApplicationWindow, NavigationPage, NavigationView, Toast, ToastOverlay, WindowTitle};
#[cfg(not(feature = "flatpak"))]
use adw::{ComboRow, EntryRow};
use std::cell::Cell;
use std::rc::Rc;

#[cfg(feature = "setup")]
mod setup;
#[cfg(not(feature = "flatpak"))]
mod standard;

#[cfg(feature = "setup")]
pub(crate) use self::setup::register_install_locally_action;
#[cfg(not(feature = "flatpak"))]
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
    pub(crate) generator_controls: PasswordGenerationControls,
    pub(crate) stores_list: ListBox,
    pub(crate) overlay: ToastOverlay,
    pub(crate) recipients_page: StoreRecipientsPageState,
    #[cfg(not(feature = "flatpak"))]
    pub(crate) pass_row: EntryRow,
    #[cfg(not(feature = "flatpak"))]
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
        #[cfg(not(feature = "flatpak"))]
        self::standard::refresh_open_preferences_state(&state, &settings);
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
