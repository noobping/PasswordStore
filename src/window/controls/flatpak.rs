use adw::prelude::GtkApplicationExt;
use adw::Application;

#[derive(Clone, Default)]
pub(crate) struct PlatformBackActionState;

pub(crate) fn before_back_action(_state: &PlatformBackActionState) -> bool {
    false
}

pub(crate) fn configure_shortcuts(app: &Application) {
    app.set_accels_for_action("win.open-store-picker", &["<primary>i"]);
}
