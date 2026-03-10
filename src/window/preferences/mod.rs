use crate::preferences::Preferences;
use crate::store::management::{rebuild_store_list, StoreRecipientsPageState};
use crate::support::ui::push_navigation_page_if_needed;
use crate::window::navigation::set_save_button_for_password;
use crate::logging::log_error;
use adw::gio::SimpleAction;
use adw::prelude::*;
use adw::{ApplicationWindow, NavigationPage, NavigationView, Toast, ToastOverlay, WindowTitle};
use adw::gtk::{Button, ListBox, TextView};
#[cfg(not(feature = "flatpak"))]
use adw::{ComboRow, EntryRow};

#[cfg(not(feature = "flatpak"))]
mod standard;
#[cfg(feature = "setup")]
mod setup;

#[cfg(not(feature = "flatpak"))]
pub(crate) use self::standard::{
    connect_backend_row, connect_pass_command_row, initialize_backend_row,
};
#[cfg(feature = "setup")]
pub(crate) use self::setup::register_install_locally_action;

pub(super) fn toast_preferences_save_error(
    overlay: &ToastOverlay,
    context: &str,
    err: &adw::glib::BoolError,
) {
    log_error(format!("Failed to save preference ({context}): {}", err.message));
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
    pub(crate) save: Button,
    pub(crate) win: WindowTitle,
    pub(crate) template_view: TextView,
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

pub(crate) fn register_open_preferences_action(
    window: &ApplicationWindow,
    state: &PreferencesActionState,
) {
    let state = state.clone();
    let action = SimpleAction::new("open-preferences", None);
    action.connect_activate(move |_, _| {
        state.add.set_visible(false);
        state.find.set_visible(false);
        state.git.set_visible(false);
        state.back.set_visible(true);
        state.save.set_visible(false);
        set_save_button_for_password(&state.save);
        state.win.set_title("Preferences");
        state.win.set_subtitle("Password Store");

        push_navigation_page_if_needed(&state.nav, &state.page);

        let settings = Preferences::new();
        #[cfg(not(feature = "flatpak"))]
        self::standard::refresh_open_preferences_state(&state, &settings);
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
    window.add_action(&action);
}
