use super::super::file::{DynamicFieldRow, StructuredPassLine};
use super::super::otp::PasswordOtpState;
use crate::window::navigation::{show_secondary_page_chrome, HasWindowChrome};
use adw::gtk::{Box as GtkBox, Button, ListBox, TextView};
use adw::prelude::*;
use adw::{EntryRow, NavigationPage, PasswordEntryRow, StatusPage, ToastOverlay, WindowTitle};
use std::cell::RefCell;
use std::rc::Rc;

#[derive(Clone)]
pub(crate) struct PasswordPageState {
    pub(crate) nav: adw::NavigationView,
    pub(crate) page: NavigationPage,
    pub(crate) raw_page: NavigationPage,
    pub(crate) list: ListBox,
    pub(crate) back: Button,
    pub(crate) add: Button,
    pub(crate) find: Button,
    pub(crate) git: Button,
    pub(crate) save: Button,
    pub(crate) win: WindowTitle,
    pub(crate) status: StatusPage,
    pub(crate) entry: PasswordEntryRow,
    pub(crate) username: EntryRow,
    pub(crate) otp: PasswordOtpState,
    pub(crate) dynamic_box: GtkBox,
    pub(crate) raw_button: Button,
    pub(crate) structured_templates: Rc<RefCell<Vec<StructuredPassLine>>>,
    pub(crate) dynamic_rows: Rc<RefCell<Vec<DynamicFieldRow>>>,
    pub(crate) text: TextView,
    pub(crate) overlay: ToastOverlay,
}

pub(super) fn show_password_editor_chrome(state: &PasswordPageState, title: &str, subtitle: &str) {
    let chrome = state.window_chrome();
    show_secondary_page_chrome(&chrome, title, subtitle, true);
}

fn hide_password_editor_fields(state: &PasswordPageState) {
    state.entry.set_visible(false);
    state.username.set_visible(false);
    state.otp.clear();
    state.dynamic_box.set_visible(false);
    state.raw_button.set_visible(false);
}

pub(super) fn show_password_status_message(
    state: &PasswordPageState,
    status_title: &str,
    status_description: &str,
) {
    hide_password_editor_fields(state);
    state.status.set_visible(true);
    state.status.set_title(status_title);
    state.status.set_description(Some(status_description));
}

pub(super) fn show_password_loading_state(state: &PasswordPageState, title: &str, subtitle: &str) {
    state.username.set_text("");
    show_password_editor_chrome(state, title, subtitle);
    show_password_status_message(state, "Opening item", "Please wait.");
}

pub(super) fn show_password_editor_fields(state: &PasswordPageState) {
    state.status.set_visible(false);
    state.entry.set_visible(true);
    state.raw_button.set_visible(true);
}

pub(super) fn show_password_open_error(state: &PasswordPageState) {
    show_password_status_message(state, "Item unavailable", "Try again.");
}
