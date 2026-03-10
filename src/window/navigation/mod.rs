use crate::password::file::sync_username_row;
use crate::password::opened::get_opened_pass_file;
use crate::store::management::{sync_store_recipients_page_header, StoreRecipientsPageState};
use adw::gtk::Button;
use adw::prelude::*;
use adw::{EntryRow, NavigationPage, NavigationView, WindowTitle};

#[cfg(not(feature = "flatpak"))]
mod standard;
#[cfg(not(feature = "flatpak"))]
pub(crate) use self::standard::{finish_git_busy_page, show_git_busy_page, show_log_page};

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
    save.set_tooltip_text(Some("Save changes"));
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
        state.win.set_subtitle("Details");
    }

    false
}
