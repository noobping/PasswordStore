use crate::logging::log_snapshot;
use crate::window::navigation::{show_log_page, WindowNavigationState};
use adw::gio::SimpleAction;
use adw::prelude::*;
use adw::{glib, ApplicationWindow};
use adw::gtk::TextView;
use std::cell::RefCell;
use std::rc::Rc;
use std::time::Duration;

pub(crate) fn register_open_log_action(
    window: &ApplicationWindow,
    navigation_state: &WindowNavigationState,
) {
    let navigation_state = navigation_state.clone();
    let action = SimpleAction::new("open-log", None);
    action.connect_activate(move |_, _| {
        show_log_page(&navigation_state);
    });
    window.add_action(&action);
}

pub(crate) fn start_log_poller(view: &TextView, navigation_state: &WindowNavigationState) {
    let navigation_state = navigation_state.clone();
    let view = view.clone();
    let seen_revision = Rc::new(RefCell::new(0usize));
    let seen_error_revision = Rc::new(RefCell::new(0usize));
    glib::timeout_add_local(Duration::from_millis(50), move || {
        let (revision, error_revision, text) = log_snapshot();
        {
            let mut seen = seen_revision.borrow_mut();
            if revision != *seen {
                view.buffer().set_text(&text);
                *seen = revision;
            }
        }

        if cfg!(debug_assertions) {
            let mut seen_error = seen_error_revision.borrow_mut();
            if error_revision > *seen_error {
                *seen_error = error_revision;
                show_log_page(&navigation_state);
            }
        }

        glib::ControlFlow::Continue
    });
}
