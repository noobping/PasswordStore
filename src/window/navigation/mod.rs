use crate::password::file::sync_username_row;
use crate::password::opened::get_opened_pass_file;
use crate::store::management::{sync_store_recipients_page_header, StoreRecipientsPageState};
use crate::support::ui::{navigation_stack_is_root, visible_navigation_page_is};
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

pub(crate) struct WindowChrome<'a> {
    pub(crate) back: &'a Button,
    pub(crate) add: &'a Button,
    pub(crate) find: &'a Button,
    pub(crate) git: &'a Button,
    pub(crate) save: &'a Button,
    pub(crate) win: &'a WindowTitle,
}

pub(crate) const APP_WINDOW_TITLE: &str = "Password Store";
pub(crate) const APP_WINDOW_SUBTITLE: &str = "Manage your passwords";

pub(crate) fn window_chrome<'a>(
    back: &'a Button,
    add: &'a Button,
    find: &'a Button,
    git: &'a Button,
    save: &'a Button,
    win: &'a WindowTitle,
) -> WindowChrome<'a> {
    WindowChrome {
        back,
        add,
        find,
        git,
        save,
        win,
    }
}

pub(crate) fn set_save_button_for_password(save: &Button) {
    save.set_action_name(Some("win.save-password"));
    save.set_tooltip_text(Some("Save changes"));
}

pub(crate) fn show_primary_page_chrome(chrome: &WindowChrome<'_>) {
    chrome.back.set_visible(false);
    chrome.save.set_visible(false);
    set_save_button_for_password(chrome.save);
    chrome.add.set_visible(true);
    chrome.find.set_visible(true);
    chrome.git.set_visible(false);
    chrome.win.set_title(APP_WINDOW_TITLE);
    chrome.win.set_subtitle(APP_WINDOW_SUBTITLE);
}

pub(crate) fn show_secondary_page_chrome(
    chrome: &WindowChrome<'_>,
    title: &str,
    subtitle: &str,
    save_visible: bool,
) {
    chrome.back.set_visible(true);
    chrome.add.set_visible(false);
    chrome.find.set_visible(false);
    chrome.git.set_visible(false);
    chrome.save.set_visible(save_visible);
    set_save_button_for_password(chrome.save);
    chrome.win.set_title(title);
    chrome.win.set_subtitle(subtitle);
}

pub(crate) fn restore_window_for_current_page(
    state: &WindowNavigationState,
    recipients_page: &StoreRecipientsPageState,
) -> bool {
    let chrome = window_chrome(
        &state.back,
        &state.add,
        &state.find,
        &state.git,
        &state.save,
        &state.win,
    );
    if navigation_stack_is_root(&state.nav) {
        show_primary_page_chrome(&chrome);
        return true;
    }

    let is_text_page = visible_navigation_page_is(&state.nav, &state.text_page);
    let is_raw_page = visible_navigation_page_is(&state.nav, &state.raw_text_page);
    let is_settings_page = visible_navigation_page_is(&state.nav, &state.settings_page);
    let is_recipients_page = visible_navigation_page_is(&state.nav, &recipients_page.page);
    let is_log_page = visible_navigation_page_is(&state.nav, &state.log_page);

    state.save.set_visible(is_text_page || is_raw_page);
    if is_text_page {
        if let Some(pass_file) = get_opened_pass_file() {
            let label = pass_file.label();
            show_secondary_page_chrome(&chrome, pass_file.title(), &label, true);
            sync_username_row(&state.username, Some(&pass_file));
        } else {
            show_secondary_page_chrome(&chrome, APP_WINDOW_TITLE, APP_WINDOW_SUBTITLE, true);
            sync_username_row(&state.username, None);
        }
    } else if is_raw_page {
        let subtitle = get_opened_pass_file()
            .map(|pass_file| pass_file.label())
            .unwrap_or_else(|| APP_WINDOW_TITLE.to_string());
        show_secondary_page_chrome(&chrome, "Raw Pass File", &subtitle, true);
    } else if is_settings_page {
        show_secondary_page_chrome(&chrome, "Preferences", APP_WINDOW_TITLE, false);
    } else if is_recipients_page {
        set_save_button_for_password(&state.save);
        sync_store_recipients_page_header(recipients_page);
    } else if is_log_page {
        show_secondary_page_chrome(&chrome, "Logs", "Details", false);
    }

    false
}
