use super::build::widgets::WindowWidgets;
use super::controls::ListVisibilityState;
use super::git::{register_open_git_action, register_synchronize_action, GitActionState};
use super::logs::{register_open_log_action, start_log_poller};
use super::navigation::WindowNavigationState;
use super::preferences::{connect_backend_row, connect_pass_command_row, initialize_backend_row};
use crate::preferences::Preferences;
use crate::store::management::StoreRecipientsPageState;
use adw::gtk::ListBox;
use adw::prelude::*;
use adw::{ApplicationWindow, EntryRow, ToastOverlay};
#[derive(Clone)]
pub(crate) struct StandardWindowState {
    pub(crate) settings: Preferences,
    pub(crate) store_recipients_entry: EntryRow,
}

pub(crate) fn configure_standard_window(widgets: &WindowWidgets) -> StandardWindowState {
    widgets.backend_preferences.set_visible(true);
    let settings = Preferences::new();
    initialize_backend_row(&widgets.backend_row, &widgets.pass_command_row, &settings);

    let store_recipients_entry = EntryRow::new();
    store_recipients_entry.set_title("Add recipient");
    store_recipients_entry.set_show_apply_button(true);

    StandardWindowState {
        settings,
        store_recipients_entry,
    }
}

pub(crate) fn create_git_action_state(
    widgets: &WindowWidgets,
    window: &ApplicationWindow,
    overlay: &ToastOverlay,
    list: &ListBox,
    navigation: &WindowNavigationState,
    recipients_page: &StoreRecipientsPageState,
    visibility: &ListVisibilityState,
) -> GitActionState {
    GitActionState {
        window: window.clone(),
        overlay: overlay.clone(),
        list: list.clone(),
        navigation: navigation.clone(),
        recipients_page: recipients_page.clone(),
        busy_page: widgets.git_busy_page.clone(),
        busy_status: widgets.git_busy_status.clone(),
        visibility: visibility.clone(),
    }
}

pub(crate) fn register_standard_window_actions(
    state: &StandardWindowState,
    widgets: &WindowWidgets,
    window: &ApplicationWindow,
    overlay: &ToastOverlay,
    navigation_state: &WindowNavigationState,
    git_action_state: &GitActionState,
) {
    connect_pass_command_row(&widgets.pass_command_row, overlay, &state.settings);
    connect_backend_row(
        &widgets.backend_row,
        &widgets.pass_command_row,
        overlay,
        &state.settings,
    );
    register_open_log_action(window, navigation_state);
    register_open_git_action(git_action_state);
    register_synchronize_action(git_action_state);
    start_log_poller(&widgets.log_view, navigation_state);
}
