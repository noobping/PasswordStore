use crate::methods::get_opened_pass_file;
use crate::pass_file::sync_username_row;
use crate::store_management::{sync_store_recipients_page_header, StoreRecipientsPageState};
#[cfg(not(feature = "flatpak"))]
use crate::ui_helpers::navigation_stack_contains_page;
use adw::prelude::*;
#[cfg(not(feature = "flatpak"))]
use adw::{
    ApplicationWindow, EntryRow, NavigationPage, NavigationView, StatusPage, WindowTitle,
};
#[cfg(feature = "flatpak")]
use adw::{EntryRow, NavigationPage, NavigationView, WindowTitle};
use adw::gtk::Button;

#[derive(Clone)]
pub(crate) struct WindowNavigationState {
    pub(crate) nav: NavigationView,
    pub(crate) text_page: NavigationPage,
    pub(crate) raw_text_page: NavigationPage,
    pub(crate) settings_page: NavigationPage,
    pub(crate) log_page: NavigationPage,
    pub(crate) back: Button,
    pub(crate) add: Button,
    pub(crate) find: Button,
    pub(crate) git: Button,
    pub(crate) save: Button,
    pub(crate) win: WindowTitle,
    pub(crate) username: EntryRow,
}

pub(crate) fn set_save_button_for_password(save: &Button) {
    save.set_action_name(Some("win.save-password"));
    save.set_tooltip_text(Some("Save password"));
}

pub(crate) fn restore_window_for_current_page(
    state: &WindowNavigationState,
    recipients_page: &StoreRecipientsPageState,
) -> bool {
    let stack = state.nav.navigation_stack();
    if stack.n_items() <= 1 {
        state.back.set_visible(false);
        state.save.set_visible(false);
        set_save_button_for_password(&state.save);
        state.add.set_visible(true);
        state.find.set_visible(true);
        state.git.set_visible(false);
        state.win.set_title("Password Store");
        state.win.set_subtitle("Manage your passwords");
        return true;
    }

    state.back.set_visible(true);
    state.add.set_visible(false);
    state.find.set_visible(false);
    state.git.set_visible(false);

    let visible_page = state.nav.visible_page();
    let is_text_page = visible_page
        .as_ref()
        .map(|page| page == &state.text_page)
        .unwrap_or(false);
    let is_raw_page = visible_page
        .as_ref()
        .map(|page| page == &state.raw_text_page)
        .unwrap_or(false);
    let is_settings_page = visible_page
        .as_ref()
        .map(|page| page == &state.settings_page)
        .unwrap_or(false);
    let is_recipients_page = visible_page
        .as_ref()
        .map(|page| page == &recipients_page.page)
        .unwrap_or(false);
    let is_log_page = visible_page
        .as_ref()
        .map(|page| page == &state.log_page)
        .unwrap_or(false);

    state.save.set_visible(is_text_page || is_raw_page);
    if is_text_page {
        set_save_button_for_password(&state.save);
        if let Some(pass_file) = get_opened_pass_file() {
            let label = pass_file.label();
            state.win.set_title(pass_file.title());
            state.win.set_subtitle(&label);
            sync_username_row(&state.username, Some(&pass_file));
        } else {
            state.win.set_title("Password Store");
            state.win.set_subtitle("Manage your passwords");
            sync_username_row(&state.username, None);
        }
    } else if is_raw_page {
        set_save_button_for_password(&state.save);
        state.win.set_title("Raw Pass File");
        if let Some(pass_file) = get_opened_pass_file() {
            let label = pass_file.label();
            state.win.set_subtitle(&label);
        } else {
            state.win.set_subtitle("Password Store");
        }
    } else if is_settings_page {
        set_save_button_for_password(&state.save);
        state.win.set_title("Preferences");
        state.win.set_subtitle("Password Store");
    } else if is_recipients_page {
        set_save_button_for_password(&state.save);
        sync_store_recipients_page_header(recipients_page);
    } else if is_log_page {
        set_save_button_for_password(&state.save);
        state.win.set_title("Logs");
        state.win.set_subtitle("Command output");
    }

    false
}

#[cfg(not(feature = "flatpak"))]
pub(crate) fn show_log_page(state: &WindowNavigationState) {
    state.add.set_visible(false);
    state.find.set_visible(false);
    state.git.set_visible(false);
    state.back.set_visible(true);
    state.save.set_visible(false);
    state.win.set_title("Logs");
    state.win.set_subtitle("Command output");

    let already_visible = state
        .nav
        .visible_page()
        .as_ref()
        .map(|visible| visible == &state.log_page)
        .unwrap_or(false);
    if !already_visible {
        state.nav.push(&state.log_page);
    }
}

#[cfg(not(feature = "flatpak"))]
pub(crate) fn show_git_busy_page(
    state: &WindowNavigationState,
    page: &NavigationPage,
    status: &StatusPage,
    title: &str,
    description: Option<&str>,
) {
    state.add.set_visible(false);
    state.find.set_visible(false);
    state.git.set_visible(false);
    state.back.set_visible(true);
    state.save.set_visible(false);
    state.win.set_title("Git");
    state.win.set_subtitle(title);
    status.set_title(title);
    status.set_description(description);

    let already_visible = state
        .nav
        .visible_page()
        .as_ref()
        .map(|visible| visible == page)
        .unwrap_or(false);
    if !already_visible {
        state.nav.push(page);
    }
}

#[cfg(not(feature = "flatpak"))]
pub(crate) fn finish_git_busy_page(
    window: &ApplicationWindow,
    state: &WindowNavigationState,
    busy_page: &NavigationPage,
    recipients_page: &StoreRecipientsPageState,
    set_actions_enabled: fn(&ApplicationWindow, bool),
) {
    set_actions_enabled(window, true);

    let current_page = state.nav.visible_page();
    let busy_visible = current_page
        .as_ref()
        .map(|visible| visible == busy_page)
        .unwrap_or(false);
    let busy_in_stack = navigation_stack_contains_page(&state.nav, busy_page);

    if busy_visible {
        state.nav.pop();
    } else if busy_in_stack {
        if let Some(current_page) = current_page.filter(|page| page != busy_page) {
            let _ = state.nav.pop_to_page(busy_page);
            let _ = state.nav.pop();
            state.nav.push(&current_page);
        }
    }

    let _ = restore_window_for_current_page(state, recipients_page);
}
