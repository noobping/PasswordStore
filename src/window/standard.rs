use super::build::widgets::WindowWidgets;
use super::preferences::{connect_backend_row, connect_pass_command_row, initialize_backend_row};
use crate::preferences::Preferences;
use adw::prelude::*;
use adw::{EntryRow, ToastOverlay};
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

pub(crate) fn register_standard_window_actions(
    state: &StandardWindowState,
    widgets: &WindowWidgets,
    overlay: &ToastOverlay,
) {
    connect_pass_command_row(&widgets.pass_command_row, overlay, &state.settings);
    connect_backend_row(
        &widgets.backend_row,
        &widgets.pass_command_row,
        overlay,
        &state.settings,
    );
}
