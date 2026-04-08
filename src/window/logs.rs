use crate::logging::log_snapshot;
use crate::support::actions::register_window_action;
use crate::support::runtime::supports_logging_features;
use crate::window::navigation::{show_log_page, WindowNavigationState};
use adw::gtk::{ScrolledWindow, TextView};
use adw::prelude::*;
use adw::{glib, ApplicationWindow};
use std::cell::RefCell;
use std::rc::Rc;
use std::time::Duration;

const LOG_SCROLL_BOTTOM_EPSILON: f64 = 1.0;

pub fn register_open_log_action(
    window: &ApplicationWindow,
    navigation_state: &WindowNavigationState,
) {
    if !supports_logging_features() {
        return;
    }

    let navigation_state = navigation_state.clone();
    register_window_action(window, "open-log", move || {
        show_log_page(&navigation_state);
    });
}

pub fn start_log_poller(view: &TextView) {
    if !supports_logging_features() {
        return;
    }

    let view = view.clone();
    let scrolled = enclosing_scrolled_window(&view);
    let seen_revision = Rc::new(RefCell::new(0usize));
    glib::timeout_add_local(Duration::from_millis(50), move || {
        let (revision, _error_revision, text) = log_snapshot();
        {
            let mut seen = seen_revision.borrow_mut();
            if revision != *seen {
                let keep_bottom = scrolled.as_ref().is_some_and(scrolled_window_is_at_bottom);
                view.buffer().set_text(&gtk_safe_log_text(&text));
                if keep_bottom {
                    if let Some(scrolled) = scrolled.as_ref() {
                        queue_scroll_to_bottom(scrolled);
                    }
                }
                *seen = revision;
            }
        }

        glib::ControlFlow::Continue
    });
}

fn enclosing_scrolled_window(view: &TextView) -> Option<ScrolledWindow> {
    let mut parent = view.parent();
    while let Some(widget) = parent {
        if let Ok(scrolled) = widget.clone().downcast::<ScrolledWindow>() {
            return Some(scrolled);
        }
        parent = widget.parent();
    }

    None
}

fn scrolled_window_is_at_bottom(scrolled: &ScrolledWindow) -> bool {
    let adjustment = scrolled.vadjustment();
    scroll_position_is_at_bottom(
        adjustment.value(),
        adjustment.upper(),
        adjustment.page_size(),
    )
}

fn queue_scroll_to_bottom(scrolled: &ScrolledWindow) {
    let scrolled = scrolled.clone();
    glib::idle_add_local_once(move || {
        let adjustment = scrolled.vadjustment();
        adjustment.set_value(scroll_bottom_value(
            adjustment.upper(),
            adjustment.page_size(),
        ));
    });
}

fn scroll_bottom_value(upper: f64, page_size: f64) -> f64 {
    (upper - page_size).max(0.0)
}

fn scroll_position_is_at_bottom(value: f64, upper: f64, page_size: f64) -> bool {
    scroll_bottom_value(upper, page_size) - value <= LOG_SCROLL_BOTTOM_EPSILON
}

fn gtk_safe_log_text(text: &str) -> String {
    text.replace('\0', "\u{FFFD}")
}

#[cfg(test)]
mod tests {
    use super::{gtk_safe_log_text, scroll_bottom_value, scroll_position_is_at_bottom};

    #[test]
    fn gtk_safe_log_text_replaces_embedded_nuls() {
        assert_eq!(gtk_safe_log_text("left\0right"), "left\u{FFFD}right");
    }

    #[test]
    fn scroll_bottom_value_clamps_to_zero() {
        assert_eq!(scroll_bottom_value(40.0, 80.0), 0.0);
    }

    #[test]
    fn scroll_position_counts_small_gap_as_bottom() {
        assert!(scroll_position_is_at_bottom(99.5, 200.0, 100.0));
    }

    #[test]
    fn scroll_position_detects_when_user_scrolled_up() {
        assert!(!scroll_position_is_at_bottom(60.0, 200.0, 100.0));
    }
}
