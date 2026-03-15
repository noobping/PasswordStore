use super::state::WindowChrome;
use crate::support::runtime::git_network_operations_available;
use adw::gtk::Button;
use adw::prelude::*;

pub const APP_WINDOW_TITLE: &str = "Keycord";
pub const APP_WINDOW_SUBTITLE: &str = "Browse and edit password stores";

pub fn set_save_button_for_password(save: &Button) {
    save.set_action_name(Some("win.save-password"));
    save.set_tooltip_text(Some("Save changes"));
}

const fn root_store_button_visible(has_store_dirs: bool) -> bool {
    !has_store_dirs
}

const fn root_git_button_visible(has_store_dirs: bool) -> bool {
    !has_store_dirs && git_network_operations_available()
}

pub fn show_primary_page_chrome(chrome: &WindowChrome<'_>, has_store_dirs: bool) {
    chrome.back.set_visible(false);
    chrome.save.set_visible(false);
    set_save_button_for_password(chrome.save);
    chrome.add.set_visible(has_store_dirs);
    chrome.find.set_visible(true);
    chrome
        .git
        .set_visible(root_git_button_visible(has_store_dirs));
    chrome
        .store
        .set_visible(root_store_button_visible(has_store_dirs));
    chrome.win.set_title(APP_WINDOW_TITLE);
    chrome.win.set_subtitle(APP_WINDOW_SUBTITLE);
    chrome.raw.set_visible(false);
}

pub fn show_secondary_page_chrome(
    chrome: &WindowChrome<'_>,
    title: &str,
    subtitle: &str,
    save_visible: bool,
) {
    chrome.back.set_visible(true);
    chrome.add.set_visible(false);
    chrome.find.set_visible(false);
    chrome.git.set_visible(false);
    chrome.store.set_visible(false);
    chrome.save.set_visible(save_visible);
    chrome.raw.set_visible(false);
    set_save_button_for_password(chrome.save);
    chrome.win.set_title(title);
    chrome.win.set_subtitle(subtitle);
}
