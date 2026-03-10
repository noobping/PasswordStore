use crate::window_git::{handle_git_busy_back, GitActionState};
use adw::Application;
use adw::prelude::GtkApplicationExt;

#[derive(Clone)]
pub(crate) struct DesktopBackActionState {
    pub(crate) git_actions: GitActionState,
}

pub(crate) fn before_back_action(state: &DesktopBackActionState) -> bool {
    handle_git_busy_back(&state.git_actions)
}

pub(crate) fn configure_shortcuts(app: &Application) {
    app.set_accels_for_action("win.open-log", &["F12"]);
    app.set_accels_for_action("win.synchronize", &["<primary>s"]);
    app.set_accels_for_action("win.open-git", &["<primary>i"]);
}
