use super::chrome::show_secondary_page_chrome;
use super::restore::restore_window_for_current_page;
use super::state::{HasWindowChrome, WindowNavigationState};
use crate::store::git_page::StoreGitPageState;
use crate::store::management::StoreRecipientsPageState;
use crate::support::ui::{
    navigation_stack_contains_page, push_navigation_page_if_needed, visible_navigation_page_is,
};
use adw::{ApplicationWindow, NavigationPage, StatusPage};

pub fn show_log_page(state: &WindowNavigationState) {
    let chrome = state.window_chrome();
    show_secondary_page_chrome(&chrome, "Logs", "Details", false);

    push_navigation_page_if_needed(&state.nav, &state.log_page);
}

pub fn show_git_busy_page(
    state: &WindowNavigationState,
    page: &NavigationPage,
    status: &StatusPage,
    title: &str,
) {
    let chrome = state.window_chrome();
    show_secondary_page_chrome(&chrome, "Working", title, false);
    status.set_title(title);

    push_navigation_page_if_needed(&state.nav, page);
}

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
