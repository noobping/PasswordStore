use super::chrome::show_secondary_page_chrome;
#[cfg(target_os = "linux")]
use super::restore::restore_window_for_current_page;
use super::state::{HasWindowChrome, WindowNavigationState};
#[cfg(target_os = "linux")]
use crate::i18n::gettext;
#[cfg(target_os = "linux")]
use crate::store::git_page::StoreGitPageState;
#[cfg(target_os = "linux")]
use crate::store::management::StoreRecipientsPageState;
#[cfg(feature = "docs")]
use crate::support::runtime::supports_docs_features;
use crate::support::runtime::supports_logging_features;
#[cfg(not(target_os = "linux"))]
use crate::support::ui::push_navigation_page_if_needed;
#[cfg(target_os = "linux")]
use crate::support::ui::{
    navigation_stack_contains_page, push_navigation_page_if_needed, visible_navigation_page_is,
};
#[cfg(target_os = "linux")]
use adw::{ApplicationWindow, NavigationPage, StatusPage};

pub fn show_log_page(state: &WindowNavigationState) {
    if !supports_logging_features() {
        return;
    }

    let chrome = state.window_chrome();
    show_secondary_page_chrome(&chrome, "Logs", "Details", false);

    push_navigation_page_if_needed(&state.nav, &state.log_page);
}

#[cfg(feature = "docs")]
pub fn show_docs_page(state: &WindowNavigationState) {
    if !supports_docs_features() {
        return;
    }

    let chrome = state.window_chrome();
    show_secondary_page_chrome(&chrome, "Documentation", "Guides and reference", false);

    push_navigation_page_if_needed(&state.nav, &state.docs_page);
}

#[cfg(target_os = "linux")]
pub fn show_git_busy_page(
    state: &WindowNavigationState,
    page: &NavigationPage,
    status: &StatusPage,
    title: &str,
) {
    let chrome = state.window_chrome();
    show_secondary_page_chrome(&chrome, "Working", title, false);
    status.set_title(&gettext(title));

    push_navigation_page_if_needed(&state.nav, page);
}

#[cfg(target_os = "linux")]
pub fn finish_git_busy_page(
    window: &ApplicationWindow,
    state: &WindowNavigationState,
    busy_page: &NavigationPage,
    recipients_page: &StoreRecipientsPageState,
    store_git_page: &StoreGitPageState,
    set_actions_enabled: fn(&ApplicationWindow, bool),
) {
    set_actions_enabled(window, true);

    let current_page = state.nav.visible_page();
    let busy_visible = visible_navigation_page_is(&state.nav, busy_page);
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

    let _ = restore_window_for_current_page(state, recipients_page, store_git_page);
}
