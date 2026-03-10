use adw::gio::SimpleAction;
use adw::prelude::*;
use adw::ApplicationWindow;

pub(crate) fn register_window_action(
    window: &ApplicationWindow,
    name: &str,
    activate: impl Fn() + 'static,
) {
    let action = SimpleAction::new(name, None);
    action.connect_activate(move |_, _| activate());
    window.add_action(&action);
}
