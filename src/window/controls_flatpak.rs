use adw::Application;

#[derive(Clone, Default)]
pub(crate) struct StandardBackActionState;

pub(crate) fn before_back_action(_state: &StandardBackActionState) -> bool {
    false
}

pub(crate) fn configure_shortcuts(_app: &Application) {}
