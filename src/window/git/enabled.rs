#[path = "operations.rs"]
mod operations;

use self::operations::{run_sync_operation, GitOperationResult};
use crate::logging::log_error;
use crate::password::list::{load_passwords_async, PasswordListActions};
use crate::preferences::Preferences;
use crate::store::git_page::StoreGitPageState;
use crate::store::management::{prompt_store_clone, StoreRecipientsPageState};
use crate::support::actions::register_window_action;
use crate::support::background::spawn_result_task;
use crate::support::ui::{navigation_stack_is_root, visible_navigation_page_is};
use crate::window::build::widgets::WindowWidgets;
use crate::window::controls::ListVisibilityState;
use crate::window::navigation::{
    finish_git_busy_page, restore_window_for_current_page, show_git_busy_page,
    WindowNavigationState,
};
use adw::gio::{prelude::*, SimpleAction};
use adw::gtk::ListBox;
use adw::{ApplicationWindow, NavigationPage, StatusPage, Toast, ToastOverlay};

#[derive(Clone)]
pub struct GitActionState {
    pub window: ApplicationWindow,
    pub overlay: ToastOverlay,
    pub list: ListBox,
    pub navigation: WindowNavigationState,
    pub recipients_page: StoreRecipientsPageState,
    pub store_git_page: StoreGitPageState,
    pub busy_page: NavigationPage,
    pub busy_status: StatusPage,
    pub visibility: ListVisibilityState,
}

impl GitActionState {
    pub fn new(
        widgets: &WindowWidgets,
        navigation: &WindowNavigationState,
        recipients_page: &StoreRecipientsPageState,
        store_git_page: &StoreGitPageState,
        visibility: &ListVisibilityState,
    ) -> Self {
        Self {
            window: widgets.window.clone(),
            overlay: widgets.toast_overlay.clone(),
            list: widgets.list.clone(),
            navigation: navigation.clone(),
            recipients_page: recipients_page.clone(),
            store_git_page: store_git_page.clone(),
            busy_page: widgets.git_busy_page.clone(),
            busy_status: widgets.git_busy_status.clone(),
            visibility: visibility.clone(),
        }
    }
}

pub fn clone_store_repository(url: &str, store_root: &str) -> Result<(), String> {
    match operations::run_clone_operation_at_root(url, store_root) {
        GitOperationResult::Success => Ok(()),
        GitOperationResult::Failed(message) => Err(message),
    }
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

pub fn set_git_action_availability(window: &ApplicationWindow, enabled: bool) {
    for action in ["git-clone", "open-git", "synchronize"] {
        set_window_action_enabled(window, action, enabled);
    }
}

fn set_git_busy_actions_enabled(window: &ApplicationWindow, enabled: bool) {
    for action in [
        "context-save",
        "context-undo",
        "open-new-password",
        "toggle-find",
        "open-git",
        "open-raw-pass-file",
        "clean-pass-file",
        "git-clone",
        "save-password",
        "save-store-recipients",
        "synchronize",
        "open-preferences",
        "open-tools",
        "toggle-hidden-and-duplicates",
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
        &state.store_git_page,
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
    );
}

fn reload_password_list(state: &GitActionState) {
    let show_list_actions = navigation_stack_is_root(&state.navigation.nav);
    let list_actions = PasswordListActions::new(
        &state.navigation.add,
        &state.navigation.git,
        &state.navigation.store,
        &state.navigation.find,
        &state.navigation.save,
    );
    load_passwords_async(
        &state.list,
        &list_actions,
        &state.overlay,
        show_list_actions,
        state.visibility.show_hidden(),
        state.visibility.show_duplicates(),
    );
}

fn register_cloned_store(
    settings: &Preferences,
    store: &str,
) -> Result<bool, adw::glib::BoolError> {
    let mut stores = settings.stores();
    if stores.iter().any(|configured| configured == store) {
        return Ok(false);
    }

    stores.push(store.to_string());
    settings.set_stores(stores)?;
    Ok(true)
}

fn start_prompted_clone(state: &GitActionState, store: String, url: String) {
    begin_git_operation(state, "Restoring store");

    let state_for_result = state.clone();
    let state_for_disconnect = state.clone();
    let settings = Preferences::new();
    let settings_for_result = settings;
    let store_for_thread = store.clone();
    let store_for_result = store;
    spawn_result_task(
        move || clone_store_repository(&url, &store_for_thread),
        move |result| match result {
            Ok(()) => match register_cloned_store(&settings_for_result, &store_for_result) {
                Ok(_) => {
                    restore_after_git_operation_and_reload(&state_for_result);
                    state_for_result
                        .overlay
                        .add_toast(Toast::new("Store restored."));
                }
                Err(err) => {
                    restore_after_git_operation(&state_for_result);
                    log_error(format!("Failed to save stores: {err}"));
                    state_for_result
                        .overlay
                        .add_toast(Toast::new("Couldn't add that folder."));
                }
            },
            Err(message) => {
                restore_after_git_operation(&state_for_result);
                state_for_result.overlay.add_toast(Toast::new(&message));
            }
        },
        move || {
            restore_after_git_operation(&state_for_disconnect);
            state_for_disconnect
                .overlay
                .add_toast(Toast::new("Restore stopped unexpectedly."));
        },
    );
}

pub fn register_open_git_action(state: &GitActionState) {
    let window = state.window.clone();
    let clone_state = state.clone();
    register_window_action(&window, "git-clone", move || {
        prompt_store_clone(&clone_state.window, &clone_state.overlay, {
            let state = clone_state.clone();
            move |store, url| start_prompted_clone(&state, store, url)
        });
    });

    let window = state.window.clone();
    let open_state = state.clone();
    register_window_action(&window, "open-git", move || {
        prompt_store_clone(&open_state.window, &open_state.overlay, {
            let state = open_state.clone();
            move |store, url| start_prompted_clone(&state, store, url)
        });
    });
}

pub fn register_synchronize_action(state: &GitActionState) {
    let window = state.window.clone();
    let state = state.clone();
    register_window_action(&window, "synchronize", move || {
        begin_git_operation(&state, "Syncing stores");

        let state = state.clone();
        let state_for_disconnect = state.clone();
        spawn_result_task(
            run_sync_operation,
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

pub fn handle_git_busy_back(state: &GitActionState) -> bool {
    if !visible_navigation_page_is(&state.navigation.nav, &state.busy_page) {
        return false;
    }

    state.navigation.nav.pop();
    let _ = restore_window_for_current_page(
        &state.navigation,
        &state.recipients_page,
        &state.store_git_page,
    );
    true
}
