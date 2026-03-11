use super::super::file::{
    clear_box_children, sync_username_row, DynamicFieldRow, StructuredPassLine,
};
use super::super::generation::PasswordGenerationControls;
use super::super::otp::PasswordOtpState;
use crate::window::navigation::{show_secondary_page_chrome, HasWindowChrome};
use adw::gtk::{Box as GtkBox, Button, ListBox, Revealer, TextView, ToggleButton};
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
    pub(crate) store: Button,
    pub(crate) save: Button,
    pub(crate) raw: Button,
    pub(crate) win: WindowTitle,
    pub(crate) status: StatusPage,
    pub(crate) entry: PasswordEntryRow,
    pub(crate) username: EntryRow,
    pub(crate) otp: PasswordOtpState,
    pub(crate) otp_add_button: Button,
    pub(crate) generator_settings_button: ToggleButton,
    pub(crate) generator_settings_revealer: Revealer,
    pub(crate) generator_controls: PasswordGenerationControls,
    pub(crate) dynamic_box: GtkBox,
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
    state.otp_add_button.set_visible(false);
    hide_password_generator_settings(state);
    state.dynamic_box.set_visible(false);
    state.raw.set_visible(false);
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
    state.raw.set_visible(true);
    state.otp_add_button.set_visible(false);
    hide_password_generator_settings(state);
}

pub(super) fn reset_password_editor(state: &PasswordPageState) {
    state.entry.set_text("");
    sync_username_row(&state.username, None);
    state.otp.clear();
    state.otp_add_button.set_visible(false);
    hide_password_generator_settings(state);
    clear_box_children(&state.dynamic_box);
    state.dynamic_box.set_visible(false);
    state.raw.set_visible(false);
    state.structured_templates.borrow_mut().clear();
    state.dynamic_rows.borrow_mut().clear();
    state.text.buffer().set_text("");
}

pub(super) fn show_password_open_error(state: &PasswordPageState) {
    show_password_status_message(state, "Item unavailable", "Try again.");
}

fn hide_password_generator_settings(state: &PasswordPageState) {
    state.generator_settings_button.set_active(false);
    state.generator_settings_revealer.set_reveal_child(false);
}
