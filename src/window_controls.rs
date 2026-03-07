use crate::password_list::load_passwords_async;
use crate::password_page::{show_password_list_page, PasswordPageState};
use crate::store_management::StoreRecipientsPageState;
#[cfg(not(feature = "flatpak"))]
use crate::window_git::{handle_git_busy_back, GitActionState, GitOperationControl};
use crate::window_navigation::{restore_window_for_current_page, WindowNavigationState};
use adw::gio::SimpleAction;
use adw::prelude::*;
use adw::Application;
use adw::gtk::{ListBox, Popover, SearchEntry};
use adw::ToastOverlay;

#[derive(Clone)]
pub(crate) struct BackActionState {
    pub(crate) overlay: ToastOverlay,
    pub(crate) list: ListBox,
    pub(crate) password_page: PasswordPageState,
    pub(crate) recipients_page: StoreRecipientsPageState,
    pub(crate) navigation: WindowNavigationState,
    #[cfg(not(feature = "flatpak"))]
    pub(crate) git_actions: GitActionState,
    #[cfg(not(feature = "flatpak"))]
    pub(crate) git_operation: GitOperationControl,
}

pub(crate) fn register_open_new_password_action(
    window: &adw::ApplicationWindow,
    popover: &Popover,
) {
    let popover = popover.clone();
    let action = SimpleAction::new("open-new-password", None);
    action.connect_activate(move |_, _| {
        if popover.is_visible() {
            popover.popdown();
        } else {
            popover.popup();
        }
    });
    window.add_action(&action);
}

pub(crate) fn register_toggle_find_action(
    window: &adw::ApplicationWindow,
    search_entry: &SearchEntry,
) {
    let search_entry = search_entry.clone();
    let action = SimpleAction::new("toggle-find", None);
    action.connect_activate(move |_, _| {
        let visible = search_entry.is_visible();
        search_entry.set_visible(!visible);
        if !visible {
            search_entry.grab_focus();
        }
    });
    window.add_action(&action);
}

pub(crate) fn register_back_action(
    window: &adw::ApplicationWindow,
    state: &BackActionState,
) {
    let state = state.clone();
    let action = SimpleAction::new("back", None);
    action.connect_activate(move |_, _| {
        #[cfg(not(feature = "flatpak"))]
        if handle_git_busy_back(&state.git_actions, &state.git_operation) {
            return;
        }

        state.navigation.nav.pop();
        if restore_window_for_current_page(&state.navigation, &state.recipients_page) {
            show_password_list_page(&state.password_page);
            return;
        }
        load_passwords_async(
            &state.list,
            state.navigation.git.clone(),
            state.navigation.find.clone(),
            state.navigation.save.clone(),
            state.overlay.clone(),
            state.navigation.nav.navigation_stack().n_items() <= 1,
        );
    });
    window.add_action(&action);
}

pub(crate) fn configure_window_shortcuts(app: &Application) {
    app.set_accels_for_action("win.back", &["Escape"]);
    app.set_accels_for_action("win.toggle-find", &["<primary>f"]);
    app.set_accels_for_action("win.open-new-password", &["<primary>n"]);
    #[cfg(not(feature = "flatpak"))]
    app.set_accels_for_action("win.open-log", &["F12"]);
    app.set_accels_for_action("win.open-preferences", &["<primary>p"]);
    #[cfg(not(feature = "flatpak"))]
    app.set_accels_for_action("win.synchronize", &["<primary>s"]);
    #[cfg(not(feature = "flatpak"))]
    app.set_accels_for_action("win.open-git", &["<primary>i"]);
}

pub(crate) fn apply_startup_query(
    startup_query: Option<String>,
    search_entry: &SearchEntry,
    list: &ListBox,
) {
    if let Some(query) = startup_query {
        if !query.is_empty() {
            search_entry.set_visible(true);
            search_entry.set_text(&query);
            list.invalidate_filter();
        }
    }
}
