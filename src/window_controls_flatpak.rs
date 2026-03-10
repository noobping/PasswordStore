use adw::Application;

#[derive(Clone, Default)]
pub(crate) struct DesktopBackActionState;

pub(crate) fn before_back_action(_state: &DesktopBackActionState) -> bool {
    false
}

pub(crate) fn configure_shortcuts(_app: &Application) {}
