use crate::store::git_page::StoreGitPageState;
use crate::store::management::StoreRecipientsPageState;
use crate::support::actions::register_window_action;
use crate::window::build::widgets::WindowWidgets;
use crate::window::controls::ListVisibilityState;
use crate::window::navigation::WindowNavigationState;
use adw::gio::{prelude::*, SimpleAction};
use adw::gtk::ListBox;
use adw::{ApplicationWindow, NavigationPage, StatusPage, ToastOverlay};

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

pub fn clone_store_repository(_url: &str, _store_root: &str) -> Result<(), String> {
    Err("Host command features are only available on Linux.".to_string())
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

pub fn register_open_git_action(state: &GitActionState) {
    let window = state.window.clone();
    register_window_action(&window, "git-clone", || {});
    register_window_action(&window, "open-git", || {});
}

pub fn register_synchronize_action(state: &GitActionState) {
    let window = state.window.clone();
    register_window_action(&window, "synchronize", || {});
}

pub fn handle_git_busy_back(_state: &GitActionState) -> bool {
    false
}
