use super::ToolsPageState;
#[cfg(all(target_os = "linux", feature = "flatpak"))]
use crate::clipboard::set_clipboard_text;
#[cfg(all(target_os = "linux", feature = "setup"))]
use crate::logging::log_error;
use crate::preferences::Preferences;
#[cfg(all(target_os = "linux", feature = "setup"))]
use crate::setup::{
    can_install_locally, install_locally, is_installed_locally, local_menu_action_label,
    uninstall_locally,
};
use crate::store::management::schedule_store_import_row;
#[cfg(all(target_os = "linux", feature = "flatpak"))]
use crate::support::runtime::has_host_permission;
use crate::support::ui::append_action_row_with_button;
#[cfg(all(target_os = "linux", feature = "flatpak"))]
use crate::support::ui::{connect_row_action, flat_icon_button_with_tooltip};
#[cfg(debug_assertions)]
use crate::window::navigation::show_log_page;
#[cfg(all(target_os = "linux", feature = "flatpak"))]
use adw::prelude::*;
#[cfg(all(target_os = "linux", feature = "flatpak"))]
use adw::ActionRow;
#[cfg(any(
    all(target_os = "linux", feature = "flatpak"),
    all(target_os = "linux", feature = "setup")
))]
use adw::Toast;
#[cfg(all(target_os = "linux", feature = "flatpak"))]
use std::rc::Rc;

#[cfg(all(target_os = "linux", feature = "flatpak"))]
const FLATPAK_HOST_OVERRIDE_COMMAND: &str =
    "flatpak override --user --talk-name=org.freedesktop.Flatpak io.github.noobping.keycord";

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
pub(super) fn append_optional_log_row(_state: &ToolsPageState) {}

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

#[cfg(all(target_os = "linux", feature = "flatpak"))]
pub(super) fn append_optional_flatpak_override_row(state: &ToolsPageState) {
    if has_host_permission() {
        return;
    }

    let row = ActionRow::builder()
        .title("Enable Flatpak host access")
        .subtitle("Copy the override command needed for Flatpak host integration.")
        .build();
    row.set_activatable(true);

    let button = flat_icon_button_with_tooltip("edit-copy-symbolic", "Copy override command");
    row.add_suffix(&button);
    state.list.append(&row);

    let overlay = state.overlay.clone();
    let feedback_button = button.clone();
    let copy_action = Rc::new(move || {
        if set_clipboard_text(
            FLATPAK_HOST_OVERRIDE_COMMAND,
            &overlay,
            Some(&feedback_button),
        ) {
            overlay.add_toast(Toast::new("Copied."));
        }
    });

    {
        let copy_action = copy_action.clone();
        connect_row_action(&row, move || copy_action());
    }

    button.connect_clicked(move |_| copy_action());
}

#[cfg(not(all(target_os = "linux", feature = "flatpak")))]
pub(super) const fn append_optional_flatpak_override_row(_state: &ToolsPageState) {}

pub(super) fn append_optional_pass_import_row(state: &ToolsPageState) {
    let settings = Preferences::new();
    schedule_store_import_row(&state.list, &settings, &state.window, &state.overlay);
}
