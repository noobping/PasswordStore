mod operations;

use self::operations::{run_clone_operation, run_sync_operation, GitOperationResult};
use crate::password::list::load_passwords_async;
use crate::store::management::StoreRecipientsPageState;
use crate::support::background::spawn_result_task;
use crate::support::ui::{navigation_stack_is_root, visible_navigation_page_is};
use crate::window::messages::with_logs_hint;
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

fn reload_password_list(state: &GitActionState) {
    let show_list_actions = navigation_stack_is_root(&state.navigation.nav);
    load_passwords_async(
        &state.list,
        state.navigation.git.clone(),
        state.navigation.find.clone(),
        state.navigation.save.clone(),
        state.overlay.clone(),
        show_list_actions,
        state.show_hidden.get(),
    );
}

pub(crate) fn register_open_git_action(
    window: &ApplicationWindow,
    popover: &Popover,
    entry: &EntryRow,
) {
    let popover = popover.clone();
    let entry = entry.clone();
    let action = SimpleAction::new("open-git", None);
    action.connect_activate(move |_, _| {
        if popover.is_visible() {
            popover.popdown();
        } else {
            popover.popup();
            entry.grab_focus();
        }
    });
    window.add_action(&action);
}

pub(crate) fn connect_git_clone_apply(window: &ApplicationWindow, entry: &EntryRow) {
    let window = window.clone();
    entry.connect_apply(move |_| {
        let _ = adw::prelude::WidgetExt::activate_action(&window, "win.git-clone", None);
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
    let action = SimpleAction::new("git-clone", None);
    action.connect_activate(move |_, _| {
        let url = entry.text().trim().to_string();
        if url.is_empty() {
            state
                .overlay
                .add_toast(Toast::new("Enter a repository URL."));
            return;
        }

        popover.popdown();
        set_git_busy_actions_enabled(&state.window, false);
        show_git_busy_page(
            &state.navigation,
            &state.busy_page,
            &state.busy_status,
            "Restoring store",
            Some("Please wait."),
        );

        let url_for_thread = url.clone();
        let state = state.clone();
        let entry = entry.clone();
        let state_for_disconnect = state.clone();
        spawn_result_task(
            move || run_clone_operation(&url_for_thread),
            move |result| match result {
                GitOperationResult::Success => {
                    entry.set_text("");
                    restore_after_git_operation(&state);
                    state.overlay.add_toast(Toast::new("Store restored."));
                    reload_password_list(&state);
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
                    .add_toast(Toast::new(&with_logs_hint("Restore stopped unexpectedly.")));
            },
        );
    });
    window.add_action(&action);
}

pub(crate) fn register_synchronize_action(state: &GitActionState) {
    let window = state.window.clone();
    let state = state.clone();
    let action = SimpleAction::new("synchronize", None);
    action.connect_activate(move |_, _| {
        set_git_busy_actions_enabled(&state.window, false);
        show_git_busy_page(
            &state.navigation,
            &state.busy_page,
            &state.busy_status,
            "Syncing stores",
            Some("Please wait."),
        );

        let state = state.clone();
        let state_for_disconnect = state.clone();
        spawn_result_task(
            move || run_sync_operation(),
            move |result| match result {
                GitOperationResult::Success => {
                    restore_after_git_operation(&state);
                    reload_password_list(&state);
                }
                GitOperationResult::Failed(message) => {
                    restore_after_git_operation(&state);
                    state.overlay.add_toast(Toast::new(&message));
                    reload_password_list(&state);
                }
            },
            move || {
                restore_after_git_operation(&state_for_disconnect);
                reload_password_list(&state_for_disconnect);
            },
        );
    });
    window.add_action(&action);
}

pub(crate) fn handle_git_busy_back(state: &GitActionState) -> bool {
    if !visible_navigation_page_is(&state.navigation.nav, &state.busy_page) {
        return false;
    }

    state.navigation.nav.pop();
    let _ = restore_window_for_current_page(&state.navigation, &state.recipients_page);
    true
}
