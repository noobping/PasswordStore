mod operations;

use self::operations::{run_clone_operation, run_sync_operation, GitOperationResult};
use crate::password::list::{load_passwords_async, PasswordListActions};
use crate::store::management::StoreRecipientsPageState;
use crate::support::actions::{activate_widget_action, register_window_action};
use crate::support::background::spawn_result_task;
use crate::support::ui::{navigation_stack_is_root, toggle_popover, visible_navigation_page_is};
use crate::window::navigation::{
    finish_git_busy_page, restore_window_for_current_page, show_git_busy_page,
    WindowNavigationState,
};
use adw::gio::{prelude::*, SimpleAction};
use adw::gtk::{ListBox, Popover};
use adw::prelude::*;
use adw::{ApplicationWindow, EntryRow, NavigationPage, StatusPage, Toast, ToastOverlay};
use std::cell::Cell;
use std::rc::Rc;

#[derive(Clone)]
pub(crate) struct GitActionState {
    pub(crate) window: ApplicationWindow,
    pub(crate) overlay: ToastOverlay,
    pub(crate) list: ListBox,
    pub(crate) navigation: WindowNavigationState,
    pub(crate) recipients_page: StoreRecipientsPageState,
    pub(crate) busy_page: NavigationPage,
    pub(crate) busy_status: StatusPage,
    pub(crate) show_hidden: Rc<Cell<bool>>,
}

fn set_window_action_enabled(window: &ApplicationWindow, name: &str, enabled: bool) {
    let Some(action) = window.lookup_action(name) else {
        return;
    };
    let Ok(action) = action.downcast::<SimpleAction>() else {
        return;
    };
    action.set_enabled(enabled);
}

fn set_git_busy_actions_enabled(window: &ApplicationWindow, enabled: bool) {
    for action in [
        "context-save",
        "open-new-password",
        "toggle-find",
        "open-git",
        "open-raw-pass-file",
        "git-clone",
        "save-password",
        "save-store-recipients",
        "synchronize",
        "open-preferences",
        "toggle-hidden",
    ] {
        set_window_action_enabled(window, action, enabled);
    }
}

fn restore_after_git_operation(state: &GitActionState) {
    finish_git_busy_page(
        &state.window,
        &state.navigation,
        &state.busy_page,
        &state.recipients_page,
        set_git_busy_actions_enabled,
    );
}

fn restore_after_git_operation_and_reload(state: &GitActionState) {
    restore_after_git_operation(state);
    reload_password_list(state);
}

fn begin_git_operation(state: &GitActionState, title: &str) {
    set_git_busy_actions_enabled(&state.window, false);
    show_git_busy_page(
        &state.navigation,
        &state.busy_page,
        &state.busy_status,
        title,
        Some("Please wait."),
    );
}

fn reload_password_list(state: &GitActionState) {
    let show_list_actions = navigation_stack_is_root(&state.navigation.nav);
    let list_actions = PasswordListActions::new(
        &state.navigation.add,
        &state.navigation.git,
        &state.navigation.find,
        &state.navigation.save,
    );
    load_passwords_async(
        &state.list,
        &list_actions,
        &state.overlay,
        show_list_actions,
        state.show_hidden.get(),
    );
}

pub(crate) fn register_open_git_action(window: &ApplicationWindow, popover: &Popover) {
    let popover = popover.clone();
    register_window_action(window, "open-git", move || {
        toggle_popover(&popover);
    });
}

pub(crate) fn connect_git_clone_apply(window: &ApplicationWindow, entry: &EntryRow) {
    let window = window.clone();
    entry.connect_apply(move |_| {
        activate_widget_action(&window, "win.git-clone");
    });
}

pub(crate) fn register_git_clone_action(
    state: &GitActionState,
    popover: &Popover,
    entry: &EntryRow,
) {
    let window = state.window.clone();
    let state = state.clone();
    let popover = popover.clone();
    let entry = entry.clone();
    register_window_action(&window, "git-clone", move || {
        let url = entry.text().trim().to_string();
        if url.is_empty() {
            state
                .overlay
                .add_toast(Toast::new("Enter a repository URL."));
            return;
        }

        popover.popdown();
        begin_git_operation(&state, "Restoring store");

        let url_for_thread = url.clone();
        let state = state.clone();
        let entry = entry.clone();
        let state_for_disconnect = state.clone();
        spawn_result_task(
            move || run_clone_operation(&url_for_thread),
            move |result| match result {
                GitOperationResult::Success => {
                    entry.set_text("");
                    restore_after_git_operation_and_reload(&state);
                    state.overlay.add_toast(Toast::new("Store restored."));
                }
                GitOperationResult::Failed(message) => {
                    restore_after_git_operation(&state);
                    state.overlay.add_toast(Toast::new(&message));
                }
            },
            move || {
                restore_after_git_operation(&state_for_disconnect);
                state_for_disconnect
                    .overlay
                    .add_toast(Toast::new("Restore stopped unexpectedly."));
            },
        );
    });
}

pub(crate) fn register_synchronize_action(state: &GitActionState) {
    let window = state.window.clone();
    let state = state.clone();
    register_window_action(&window, "synchronize", move || {
        begin_git_operation(&state, "Syncing stores");

        let state = state.clone();
        let state_for_disconnect = state.clone();
        spawn_result_task(
            move || run_sync_operation(),
            move |result| match result {
                GitOperationResult::Success => {
                    restore_after_git_operation_and_reload(&state);
                }
                GitOperationResult::Failed(message) => {
                    restore_after_git_operation_and_reload(&state);
                    state.overlay.add_toast(Toast::new(&message));
                }
            },
            move || {
                restore_after_git_operation_and_reload(&state_for_disconnect);
            },
        );
    });
}

pub(crate) fn handle_git_busy_back(state: &GitActionState) -> bool {
    if !visible_navigation_page_is(&state.navigation.nav, &state.busy_page) {
        return false;
    }

    state.navigation.nav.pop();
    let _ = restore_window_for_current_page(&state.navigation, &state.recipients_page);
    true
}
