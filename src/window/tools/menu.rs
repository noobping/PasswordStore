use super::ToolsPageState;
use crate::clipboard::set_clipboard_text;
use crate::i18n::gettext;
#[cfg(all(target_os = "linux", feature = "setup"))]
use crate::logging::log_error;
use crate::logging::log_snapshot;
use crate::preferences::Preferences;
#[cfg(all(target_os = "linux", feature = "setup"))]
use crate::setup::{
    can_install_locally, install_locally, is_installed_locally, local_menu_action_label,
    uninstall_locally,
};
use crate::store::management::schedule_store_import_row;
use crate::support::actions::activate_widget_action;
use crate::support::runtime::{
    supports_docs_features, supports_host_command_features, supports_logging_features,
};
use crate::support::ui::{append_action_row_with_button, flat_icon_button_with_tooltip};
use crate::window::navigation::show_log_page;
use adw::prelude::*;
use adw::{ActionRow, Toast};
use std::rc::Rc;

pub(super) fn append_optional_doc_row(state: &ToolsPageState) {
    if !supports_docs_features() {
        return;
    }

    let window = state.window.clone();
    append_action_row_with_button(
        &state.logs_list,
        "Documentation",
        "Open guides and reference.",
        "go-next-symbolic",
        move || activate_widget_action(&window, "win.open-docs"),
    );
}

pub(super) fn append_optional_log_rows(state: &ToolsPageState) {
    if !supports_logging_features() {
        return;
    }

    let navigation = state.navigation.clone();
    append_action_row_with_button(
        &state.logs_list,
        "Open logs",
        "Inspect recent app and command output.",
        "go-next-symbolic",
        move || show_log_page(&navigation),
    );

    let title = gettext("Copy logs");
    let subtitle = gettext("Copy recent app and command output to the clipboard.");
    let row = ActionRow::builder()
        .title(&title)
        .subtitle(&subtitle)
        .build();
    row.set_activatable(true);

    let button = flat_icon_button_with_tooltip("edit-copy-symbolic", "Copy logs");
    row.add_suffix(&button);
    state.logs_list.append(&row);

    let overlay = state.overlay.clone();
    let feedback_button = button.clone();
    let copy_action = Rc::new(move || {
        let (_, _, text) = log_snapshot();
        if set_clipboard_text(&text, &overlay, Some(&feedback_button)) {
            overlay.add_toast(Toast::new(&gettext("Copied.")));
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
                    overlay.add_toast(Toast::new(&gettext("Couldn't update the app menu.")));
                }
            }
        },
    );
}

#[cfg(not(feature = "setup"))]
pub(super) const fn append_optional_setup_row(_state: &ToolsPageState) {}

pub(super) fn append_optional_pass_import_row(state: &ToolsPageState) {
    if !supports_host_command_features() {
        return;
    }

    let settings = Preferences::new();
    schedule_store_import_row(&state.list, &settings, &state.window, &state.overlay);
}
