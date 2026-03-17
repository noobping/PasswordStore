use adw::gio::SimpleAction;
use adw::gtk::Widget;
use adw::prelude::*;
use adw::ApplicationWindow;

pub fn register_window_action(
    window: &ApplicationWindow,
    name: &str,
    activate: impl Fn() + 'static,
) {
    let action = SimpleAction::new(name, None);
    action.connect_activate(move |_, _| activate());
    window.add_action(&action);
}

pub fn activate_widget_action(widget: &impl IsA<Widget>, action_name: &str) {
    let _ = widget.activate_action(action_name, None);
}
