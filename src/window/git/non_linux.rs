use crate::store::management::StoreRecipientsPageState;
use crate::window::build::widgets::WindowWidgets;
use crate::window::controls::ListVisibilityState;
use crate::window::navigation::WindowNavigationState;
use adw::ApplicationWindow;

#[derive(Clone, Default)]
pub(crate) struct GitActionState;

impl GitActionState {
    pub(crate) fn new(
        _widgets: &WindowWidgets,
        _navigation: &WindowNavigationState,
        _recipients_page: &StoreRecipientsPageState,
        _visibility: &ListVisibilityState,
    ) -> Self {
        Self
    }
}

pub(crate) fn clone_store_repository(_url: &str, _store_root: &str) -> Result<(), String> {
    Err("Git operations are unavailable on non-Linux builds.".to_string())
}

pub(crate) fn set_git_action_availability(_window: &ApplicationWindow, _enabled: bool) {}

pub(crate) fn register_open_git_action(_state: &GitActionState) {}

pub(crate) fn register_synchronize_action(_state: &GitActionState) {}

pub(crate) fn handle_git_busy_back(_state: &GitActionState) -> bool {
    false
}
