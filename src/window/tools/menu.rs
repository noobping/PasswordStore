use super::ToolsPageState;
#[cfg(all(target_os = "linux", feature = "setup"))]
use crate::logging::log_error;
use crate::preferences::Preferences;
#[cfg(all(target_os = "linux", feature = "setup"))]
use crate::setup::{
    can_install_locally, install_locally, is_installed_locally, local_menu_action_label,
    uninstall_locally,
};
use crate::store::management::schedule_store_import_row;
use crate::window::host_access::append_optional_host_access_list_row;
#[cfg(any(debug_assertions, feature = "setup"))]
use crate::support::ui::append_action_row_with_button;
#[cfg(debug_assertions)]
use crate::window::navigation::show_log_page;
#[cfg(any(
    all(target_os = "linux", feature = "flatpak"),
    all(target_os = "linux", feature = "setup")
))]
use adw::Toast;

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

pub(super) fn append_optional_pass_import_row(state: &ToolsPageState) {
    let settings = Preferences::new();
    schedule_store_import_row(&state.list, &settings, &state.window, &state.overlay);
    append_optional_host_access_list_row(&state.list, &state.overlay);
}
