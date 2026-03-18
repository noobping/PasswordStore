use super::ToolsPageState;
#[cfg(not(debug_assertions))]
use crate::clipboard::set_clipboard_text;
#[cfg(all(target_os = "linux", feature = "setup"))]
use crate::logging::log_error;
#[cfg(not(debug_assertions))]
use crate::logging::log_snapshot;
use crate::preferences::Preferences;
#[cfg(all(target_os = "linux", feature = "setup"))]
use crate::setup::{
    can_install_locally, install_locally, is_installed_locally, local_menu_action_label,
    uninstall_locally,
};
use crate::store::management::schedule_store_import_row;
#[cfg(any(debug_assertions, feature = "setup"))]
use crate::support::ui::append_action_row_with_button;
#[cfg(not(debug_assertions))]
use crate::support::ui::flat_icon_button_with_tooltip;
#[cfg(debug_assertions)]
use crate::window::navigation::show_log_page;
#[cfg(not(debug_assertions))]
use adw::prelude::*;
#[cfg(any(not(debug_assertions), all(target_os = "linux", feature = "setup")))]
use adw::ActionRow;
#[cfg(any(not(debug_assertions), all(target_os = "linux", feature = "setup")))]
use adw::Toast;
#[cfg(not(debug_assertions))]
use std::rc::Rc;

#[cfg(debug_assertions)]
pub(super) fn append_optional_log_row(state: &ToolsPageState) {
    let navigation = state.navigation.clone();
    append_action_row_with_button(
        &state.list,
        "Open logs",
        "Inspect recent app and command output.",
        "go-next-symbolic",
        move || show_log_page(&navigation),
    );
}

#[cfg(not(debug_assertions))]
pub(super) fn append_optional_log_row(state: &ToolsPageState) {
    let row = ActionRow::builder()
        .title("Copy logs")
        .subtitle("Copy recent app and command output to the clipboard.")
        .build();
    row.set_activatable(true);

    let button = flat_icon_button_with_tooltip("edit-copy-symbolic", "Copy logs");
    row.add_suffix(&button);
    state.list.append(&row);

    let overlay = state.overlay.clone();
    let feedback_button = button.clone();
    let copy_action = Rc::new(move || {
        let (_, _, text) = log_snapshot();
        if set_clipboard_text(&text, &overlay, Some(&feedback_button)) {
            overlay.add_toast(Toast::new("Copied."));
        }
    });

    {
        let copy_action = copy_action.clone();
        row.connect_activated(move |_| copy_action());
    }
    button.connect_clicked(move |_| copy_action());
}

#[cfg(all(target_os = "linux", feature = "setup"))]
pub(super) fn append_optional_setup_row(state: &ToolsPageState) {
    if !can_install_locally() {
        return;
    }

    let title = local_menu_action_label(is_installed_locally());
    let overlay = state.overlay.clone();
    let refresh_state = state.clone();
    append_action_row_with_button(
        &state.list,
        title,
        "Add or remove this build from the local app menu.",
        "emblem-system-symbolic",
        move || {
            let installed = is_installed_locally();
            let result = if installed {
                uninstall_locally()
            } else {
                install_locally()
            };

            match result {
                Ok(()) => refresh_state.rebuild(),
                Err(err) => {
                    log_error(format!("Failed to update local app menu entry: {err}"));
                    overlay.add_toast(Toast::new("Couldn't update the app menu."));
                }
            }
        },
    );
}

#[cfg(not(feature = "setup"))]
pub(super) const fn append_optional_setup_row(_state: &ToolsPageState) {}

pub(super) fn append_optional_pass_import_row(state: &ToolsPageState) {
    let settings = Preferences::new();
    schedule_store_import_row(&state.list, &settings, &state.window, &state.overlay);
}
