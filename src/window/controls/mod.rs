use crate::password::list::load_passwords_async;
use crate::password::page::{
    retry_open_password_entry_if_needed, show_password_list_page, PasswordPageState,
};
use crate::store::management::StoreRecipientsPageState;
use crate::support::actions::register_window_action;
use crate::support::ui::navigation_stack_is_root;
use crate::window::navigation::{restore_window_for_current_page, WindowNavigationState};
use adw::gtk::{ListBox, SearchEntry};
use adw::prelude::*;
use adw::Application;
use adw::ToastOverlay;
use std::cell::Cell;
use std::rc::Rc;

#[cfg(feature = "flatpak")]
mod flatpak;
#[cfg(feature = "flatpak")]
use self::flatpak as platform;
#[cfg(not(feature = "flatpak"))]
mod standard;
#[cfg(not(feature = "flatpak"))]
use self::standard as platform;

pub(crate) use self::platform::PlatformBackActionState;
use self::platform::{before_back_action, configure_shortcuts};

#[derive(Clone)]
pub(crate) struct BackActionState {
    pub(crate) password_page: PasswordPageState,
    pub(crate) recipients_page: StoreRecipientsPageState,
    pub(crate) navigation: WindowNavigationState,
    pub(crate) show_hidden: Rc<Cell<bool>>,
    pub(crate) platform: PlatformBackActionState,
}

#[derive(Clone)]
pub(crate) struct HiddenEntriesActionState {
    pub(crate) overlay: ToastOverlay,
    pub(crate) list: ListBox,
    pub(crate) navigation: WindowNavigationState,
    pub(crate) show_hidden: Rc<Cell<bool>>,
}

pub(crate) fn register_toggle_find_action(
    window: &adw::ApplicationWindow,
    search_entry: &SearchEntry,
) {
    let search_entry = search_entry.clone();
    register_window_action(window, "toggle-find", move || {
        let visible = search_entry.is_visible();
        search_entry.set_visible(!visible);
        if !visible {
            search_entry.grab_focus();
        }
    });
}

pub(crate) fn register_back_action(window: &adw::ApplicationWindow, state: &BackActionState) {
    let state = state.clone();
    register_window_action(window, "back", move || {
        if before_back_action(&state.platform) {
            return;
        }

        state.navigation.nav.pop();
        if restore_window_for_current_page(&state.navigation, &state.recipients_page) {
            show_password_list_page(&state.password_page, state.show_hidden.get());
            return;
        }

        let _ = retry_open_password_entry_if_needed(&state.password_page);
    });
}

pub(crate) fn configure_window_shortcuts(app: &Application) {
    app.set_accels_for_action("win.back", &["Escape"]);
    app.set_accels_for_action("win.toggle-find", &["<primary>f"]);
    app.set_accels_for_action("win.toggle-hidden", &["<primary>h"]);
    app.set_accels_for_action("win.open-new-password", &["<primary>n"]);
    app.set_accels_for_action("win.open-preferences", &["<primary>p"]);
    configure_shortcuts(app);
}

pub(crate) fn register_toggle_hidden_action(
    window: &adw::ApplicationWindow,
    state: &HiddenEntriesActionState,
) {
    let state = state.clone();
    register_window_action(window, "toggle-hidden", move || {
        let show_hidden = !state.show_hidden.get();
        let show_list_actions = navigation_stack_is_root(&state.navigation.nav);
        if !show_list_actions {
            return;
        }
        state.show_hidden.set(show_hidden);
        load_passwords_async(
            &state.list,
            &state.navigation.git,
            &state.navigation.find,
            &state.navigation.save,
            &state.overlay,
            show_list_actions,
            show_hidden,
        );
    });
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
