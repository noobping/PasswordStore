use crate::logging::log_snapshot;
use crate::support::actions::register_window_action;
use crate::window::navigation::{show_log_page, WindowNavigationState};
use adw::gio::Menu;
use adw::gtk::TextView;
use adw::prelude::*;
use adw::{glib, ApplicationWindow};
use std::cell::RefCell;
use std::rc::Rc;
use std::time::Duration;

#[cfg(debug_assertions)]
pub(crate) fn append_debug_log_menu_item(menu: &Menu) {
    menu.append(Some("_Logs"), Some("win.open-log"));
}

#[cfg(not(debug_assertions))]
pub(crate) fn append_debug_log_menu_item(_menu: &Menu) {}

pub(crate) fn register_open_log_action(
    window: &ApplicationWindow,
    navigation_state: &WindowNavigationState,
) {
    let navigation_state = navigation_state.clone();
    register_window_action(window, "open-log", move || {
        show_log_page(&navigation_state);
    });
}

pub(crate) fn start_log_poller(view: &TextView) {
    let view = view.clone();
    let seen_revision = Rc::new(RefCell::new(0usize));
    glib::timeout_add_local(Duration::from_millis(50), move || {
        let (revision, _error_revision, text) = log_snapshot();
        {
            let mut seen = seen_revision.borrow_mut();
            if revision != *seen {
                view.buffer().set_text(&text);
                *seen = revision;
            }
        }

        glib::ControlFlow::Continue
    });
}
