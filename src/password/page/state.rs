use super::super::file::{
    clear_box_children, sync_username_row, DynamicFieldRow, StructuredPassLine,
};
use super::super::generation::PasswordGenerationControls;
use super::super::otp::PasswordOtpState;
use crate::window::navigation::{show_secondary_page_chrome, HasWindowChrome};
use adw::gtk::{Box as GtkBox, Button, ListBox, Revealer, TextView, ToggleButton};
use adw::prelude::*;
use adw::{EntryRow, NavigationPage, PasswordEntryRow, StatusPage, ToastOverlay, WindowTitle};
use std::cell::{Cell, RefCell};
use std::rc::Rc;

#[derive(Clone)]
pub struct PasswordPageState {
    pub nav: adw::NavigationView,
    pub page: NavigationPage,
    pub raw_page: NavigationPage,
    pub list: ListBox,
    pub back: Button,
    pub add: Button,
    pub find: Button,
    pub git: Button,
    pub store: Button,
    pub save: Button,
    pub raw: Button,
    pub win: WindowTitle,
    pub status: StatusPage,
    pub entry: PasswordEntryRow,
    pub username: EntryRow,
    pub otp: PasswordOtpState,
    pub field_add_row: EntryRow,
    pub otp_add_button: Button,
    pub generator_settings_button: ToggleButton,
    pub generator_settings_revealer: Revealer,
    pub generator_controls: PasswordGenerationControls,
    pub dynamic_box: GtkBox,
    pub structured_templates: Rc<RefCell<Vec<StructuredPassLine>>>,
    pub dynamic_rows: Rc<RefCell<Vec<DynamicFieldRow>>>,
    pub text: TextView,
    pub overlay: ToastOverlay,
    pub saved_contents: Rc<RefCell<String>>,
    pub saved_entry_exists: Rc<Cell<bool>>,
}

pub(super) fn show_password_editor_chrome(state: &PasswordPageState, title: &str, subtitle: &str) {
    let chrome = state.window_chrome();
    show_secondary_page_chrome(&chrome, title, subtitle, true);
}

fn hide_password_editor_fields(state: &PasswordPageState) {
    state.entry.set_visible(false);
    state.username.set_visible(false);
    state.otp.clear();
    state.field_add_row.set_visible(false);
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
    state.field_add_row.set_visible(true);
    state.otp_add_button.set_visible(false);
    hide_password_generator_settings(state);
}

pub(super) fn reset_password_editor(state: &PasswordPageState) {
    state.entry.set_text("");
    sync_username_row(&state.username, None);
    state.otp.clear();
    state.field_add_row.set_text("");
    state.field_add_row.set_visible(false);
    state.otp_add_button.set_visible(false);
    hide_password_generator_settings(state);
    clear_box_children(&state.dynamic_box);
    state.dynamic_box.set_visible(false);
    state.raw.set_visible(false);
    state.structured_templates.borrow_mut().clear();
    state.dynamic_rows.borrow_mut().clear();
    state.text.buffer().set_text("");
    state.saved_contents.borrow_mut().clear();
    state.saved_entry_exists.set(false);
}

fn hide_password_generator_settings(state: &PasswordPageState) {
    state.generator_settings_button.set_active(false);
    state.generator_settings_revealer.set_reveal_child(false);
}

pub(super) fn sync_saved_password_state(
    state: &PasswordPageState,
    contents: &str,
    entry_exists: bool,
) {
    *state.saved_contents.borrow_mut() = contents.to_string();
    state.saved_entry_exists.set(entry_exists);
}
