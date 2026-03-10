use super::{restore_window_for_current_page, WindowNavigationState};
use crate::store::management::StoreRecipientsPageState;
use crate::support::ui::{
    navigation_stack_contains_page, push_navigation_page_if_needed, visible_navigation_page_is,
};
use adw::prelude::*;
use adw::{ApplicationWindow, NavigationPage, StatusPage};

pub(crate) fn show_log_page(state: &WindowNavigationState) {
    state.add.set_visible(false);
    state.find.set_visible(false);
    state.git.set_visible(false);
    state.back.set_visible(true);
    state.save.set_visible(false);
    state.win.set_title("Logs");
    state.win.set_subtitle("Details");

    push_navigation_page_if_needed(&state.nav, &state.log_page);
}

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
    state.win.set_title("Working");
    state.win.set_subtitle(title);
    status.set_title(title);
    status.set_description(description);

    push_navigation_page_if_needed(&state.nav, page);
}

pub(crate) fn finish_git_busy_page(
    window: &ApplicationWindow,
    state: &WindowNavigationState,
    busy_page: &NavigationPage,
    recipients_page: &StoreRecipientsPageState,
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

    let _ = restore_window_for_current_page(state, recipients_page);
}
