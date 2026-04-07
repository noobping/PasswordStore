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
use crate::store::management::{schedule_store_import_row, StoreImportToolRowState};
use crate::support::actions::activate_widget_action;
use crate::support::runtime::{
    supports_docs_features, supports_host_command_features, supports_logging_features,
};
#[cfg(all(target_os = "linux", feature = "setup"))]
use crate::support::ui::append_action_row_with_button;
use crate::window::navigation::show_log_page;
use adw::prelude::*;
use adw::{ActionRow, Toast};
use std::rc::Rc;

const fn information_group_visible(docs_supported: bool, logging_supported: bool) -> bool {
    docs_supported || logging_supported
}

fn sync_optional_information_group(
    state: &ToolsPageState,
    docs_supported: bool,
    logging_supported: bool,
) {
    state
        .select_page
        .information_group
        .set_visible(information_group_visible(docs_supported, logging_supported));
}

pub(super) fn configure_optional_doc_row(state: &ToolsPageState) {
    let docs_supported = supports_docs_features();
    state.select_page.docs_row.set_visible(docs_supported);
    sync_optional_information_group(state, docs_supported, supports_logging_features());
    let window = state.window.clone();
    let state_for_open = state.clone();
    state.select_page.docs_row.connect_activated(move |_| {
        state_for_open.close_select_dialog();
        activate_widget_action(&window, "win.open-docs");
    });
}

pub(super) fn configure_optional_log_rows(state: &ToolsPageState) {
    let logging_supported = supports_logging_features();
    sync_optional_information_group(state, supports_docs_features(), logging_supported);
    state.select_page.logs_row.set_visible(logging_supported);
    state
        .select_page
        .copy_logs_row
        .set_visible(logging_supported);

    let navigation = state.navigation.clone();
    let state_for_logs = state.clone();
    state.select_page.logs_row.connect_activated(move |_| {
        state_for_logs.close_select_dialog();
        show_log_page(&navigation);
    });

    let overlay = state.overlay.clone();
    let feedback_button = state.select_page.copy_logs_button.clone();
    let copy_action = Rc::new(move || {
        let (_, _, text) = log_snapshot();
        if set_clipboard_text(&text, &overlay, Some(&feedback_button)) {
            overlay.add_toast(Toast::new(&gettext("Copied.")));
        }
    });

    {
        let copy_action = copy_action.clone();
        state
            .select_page
            .copy_logs_row
            .connect_activated(move |_| copy_action());
    }
    state
        .select_page
        .copy_logs_button
        .connect_clicked(move |_| copy_action());
}

#[cfg(test)]
mod tests {
    use super::information_group_visible;

    #[test]
    fn information_group_requires_docs_or_logs() {
        assert!(!information_group_visible(false, false));
        assert!(information_group_visible(true, false));
        assert!(information_group_visible(false, true));
        assert!(information_group_visible(true, true));
    }
}

#[cfg(all(target_os = "linux", feature = "setup"))]
pub(super) fn append_optional_setup_row(state: &ToolsPageState) -> Option<ActionRow> {
    if !can_install_locally() {
        return None;
    }

    let overlay = state.overlay.clone();
    let refresh_state = state.clone();
    let row = append_action_row_with_button(
        &state.select_page.list,
        local_menu_action_label(is_installed_locally()),
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
                Ok(()) => refresh_state.refresh_select_page(),
                Err(err) => {
                    log_error(format!("Failed to update local app menu entry: {err}"));
                    overlay.add_toast(Toast::new(&gettext("Couldn't update the app menu.")));
                }
            }
        },
    );
    Some(row)
}

#[cfg(not(all(target_os = "linux", feature = "setup")))]
pub(super) const fn append_optional_setup_row(_state: &ToolsPageState) -> Option<ActionRow> {
    None
}

#[cfg(all(target_os = "linux", feature = "setup"))]
pub(super) fn sync_optional_setup_row(row: Option<&ActionRow>) {
    let Some(row) = row else {
        return;
    };

    row.set_title(&gettext(local_menu_action_label(is_installed_locally())));
}

#[cfg(not(all(target_os = "linux", feature = "setup")))]
pub(super) const fn sync_optional_setup_row(_row: Option<&ActionRow>) {}

pub(super) fn append_optional_pass_import_row(
    state: &ToolsPageState,
) -> Option<StoreImportToolRowState> {
    if !supports_host_command_features() {
        return None;
    }

    let settings = Preferences::new();
    schedule_store_import_row(
        &state.select_page.list,
        &settings,
        &state.window,
        &state.overlay,
        Some(Rc::new({
            let state = state.clone();
            move || {
                state.close_select_dialog();
            }
        })),
    )
}
