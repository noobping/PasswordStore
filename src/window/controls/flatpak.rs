use adw::Application;

#[derive(Clone, Default)]
pub(crate) struct PlatformBackActionState;

pub(crate) fn before_back_action(_state: &PlatformBackActionState) -> bool {
    false
}

pub(crate) fn configure_shortcuts(_app: &Application) {}
