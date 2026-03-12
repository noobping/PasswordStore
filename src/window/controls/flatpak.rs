use crate::window::git::{handle_git_busy_back, GitActionState};
use adw::prelude::GtkApplicationExt;
use adw::Application;

#[derive(Clone)]
pub(crate) struct PlatformBackActionState {
    pub(crate) git_actions: GitActionState,
}

pub(crate) fn before_back_action(state: &PlatformBackActionState) -> bool {
    handle_git_busy_back(&state.git_actions)
}

pub(crate) fn configure_shortcuts(app: &Application) {
    app.set_accels_for_action("win.open-store-picker", &["<primary>i"]);
}
