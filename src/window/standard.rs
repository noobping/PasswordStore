use crate::preferences::Preferences;
use crate::store::management::StoreRecipientsPageState;
use super::git::{
    connect_git_clone_apply, register_git_clone_action, register_open_git_action,
    register_synchronize_action, GitActionState, GitOperationControl,
};
use super::logs::{register_open_log_action, start_log_poller};
use super::navigation::WindowNavigationState;
use super::preferences::{
    connect_backend_row, connect_pass_command_row, initialize_backend_row,
};
use adw::prelude::*;
use adw::{ApplicationWindow, ComboRow, EntryRow, NavigationPage, StatusPage, ToastOverlay};
use adw::gtk::{Builder, ListBox, Popover, TextView};
use std::cell::Cell;
use std::rc::Rc;

#[derive(Clone)]
pub(crate) struct StandardWindowParts {
    pub(crate) settings: Preferences,
    pub(crate) backend_row: ComboRow,
    pub(crate) pass_row: EntryRow,
    pub(crate) git_url_entry: EntryRow,
    pub(crate) git_busy_page: NavigationPage,
    pub(crate) git_busy_status: StatusPage,
    pub(crate) log_view: TextView,
    pub(crate) git_operation: GitOperationControl,
    pub(crate) store_recipients_entry: EntryRow,
}

pub(crate) fn load_standard_window_parts(builder: &Builder) -> StandardWindowParts {
    let backend_preferences: adw::PreferencesGroup = builder
        .object("backend_preferences")
        .expect("Failed to get backend_preferences");
    let backend_row: ComboRow = builder
        .object("backend_row")
        .expect("Failed to get backend_row");
    backend_preferences.set_visible(true);

    let settings = Preferences::new();
    let pass_row: EntryRow = builder
        .object("pass_command_row")
        .expect("Failed to get pass row");
    initialize_backend_row(&backend_row, &pass_row, &settings);

    let store_recipients_entry = EntryRow::new();
    store_recipients_entry.set_title("Add recipient");
    store_recipients_entry.set_show_apply_button(true);

    StandardWindowParts {
        settings,
        backend_row,
        pass_row,
        git_url_entry: builder
            .object("git_url_entry")
            .expect("Failed to get git_url_entry"),
        git_busy_page: builder
            .object("git_busy_page")
            .expect("Failed to get git busy page"),
        git_busy_status: builder
            .object("git_busy_status")
            .expect("Failed to get git busy status"),
        log_view: builder
            .object("log_view")
            .expect("Failed to get log_view"),
        git_operation: GitOperationControl::default(),
        store_recipients_entry,
    }
}

pub(crate) fn create_git_action_state(
    parts: &StandardWindowParts,
    window: &ApplicationWindow,
    overlay: &ToastOverlay,
    list: &ListBox,
    navigation: &WindowNavigationState,
    recipients_page: &StoreRecipientsPageState,
    show_hidden: &Rc<Cell<bool>>,
) -> GitActionState {
    GitActionState {
        window: window.clone(),
        overlay: overlay.clone(),
        list: list.clone(),
        navigation: navigation.clone(),
        recipients_page: recipients_page.clone(),
        busy_page: parts.git_busy_page.clone(),
        busy_status: parts.git_busy_status.clone(),
        show_hidden: show_hidden.clone(),
    }
}

pub(crate) fn register_standard_window_actions(
    parts: &StandardWindowParts,
    window: &ApplicationWindow,
    overlay: &ToastOverlay,
    navigation_state: &WindowNavigationState,
    git_action_state: &GitActionState,
    git_popover: &Popover,
) {
    connect_pass_command_row(&parts.pass_row, overlay, &parts.settings);
    connect_backend_row(
        &parts.backend_row,
        &parts.pass_row,
        overlay,
        &parts.settings,
    );
    register_open_log_action(window, navigation_state);
    register_open_git_action(window, git_popover, &parts.git_url_entry);
    connect_git_clone_apply(window, &parts.git_url_entry);
    register_git_clone_action(
        git_action_state,
        git_popover,
        &parts.git_url_entry,
        &parts.git_operation,
    );
    register_synchronize_action(git_action_state, &parts.git_operation);
    start_log_poller(&parts.log_view, navigation_state);
}
