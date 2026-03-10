use crate::password::list::load_passwords_async;
use crate::password::page::{
    retry_open_password_entry_if_needed, show_password_list_page, PasswordPageState,
};
use crate::store::management::StoreRecipientsPageState;
use crate::support::actions::{activate_widget_action, register_window_action};
use crate::support::ui::{navigation_stack_is_root, visible_navigation_page_is};
use crate::window::navigation::{restore_window_for_current_page, WindowNavigationState};
use adw::gtk::{ListBox, SearchEntry};
use adw::prelude::*;
use adw::ToastOverlay;
use adw::{Application, ApplicationWindow, NavigationPage};
use std::cell::Cell;
use std::rc::Rc;

#[cfg(feature = "flatpak")]
mod flatpak;
#[cfg(feature = "flatpak")]
use self::flatpak as platform;
#[cfg(not(feature = "flatpak"))]
mod standard;
#[cfg(not(feature = "flatpak"))]
use self::standard as platform;

pub(crate) use self::platform::PlatformBackActionState;
use self::platform::{before_back_action, configure_shortcuts};

#[derive(Clone)]
pub(crate) struct BackActionState {
    pub(crate) password_page: PasswordPageState,
    pub(crate) recipients_page: StoreRecipientsPageState,
    pub(crate) navigation: WindowNavigationState,
    pub(crate) show_hidden: Rc<Cell<bool>>,
    pub(crate) platform: PlatformBackActionState,
}

#[derive(Clone)]
pub(crate) struct HiddenEntriesActionState {
    pub(crate) overlay: ToastOverlay,
    pub(crate) list: ListBox,
    pub(crate) navigation: WindowNavigationState,
    pub(crate) show_hidden: Rc<Cell<bool>>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ContextSaveTarget {
    Password,
    StoreRecipients,
    #[cfg(not(feature = "flatpak"))]
    Synchronize,
    None,
}

fn context_save_target_from_flags(
    at_root: bool,
    text_page_visible: bool,
    raw_page_visible: bool,
    recipients_page_visible: bool,
) -> ContextSaveTarget {
    if text_page_visible || raw_page_visible {
        return ContextSaveTarget::Password;
    }

    if recipients_page_visible {
        return ContextSaveTarget::StoreRecipients;
    }

    if at_root {
        #[cfg(not(feature = "flatpak"))]
        return ContextSaveTarget::Synchronize;
        #[cfg(feature = "flatpak")]
        return ContextSaveTarget::None;
    }

    ContextSaveTarget::None
}

fn context_save_target(
    navigation: &WindowNavigationState,
    recipients_page: &NavigationPage,
) -> ContextSaveTarget {
    context_save_target_from_flags(
        navigation_stack_is_root(&navigation.nav),
        visible_navigation_page_is(&navigation.nav, &navigation.text_page),
        visible_navigation_page_is(&navigation.nav, &navigation.raw_text_page),
        visible_navigation_page_is(&navigation.nav, recipients_page),
    )
}

pub(crate) fn register_context_save_action(
    window: &ApplicationWindow,
    navigation: &WindowNavigationState,
    recipients_page: &StoreRecipientsPageState,
) {
    let action_window = window.clone();
    let dispatch_window = action_window.clone();
    let navigation = navigation.clone();
    let recipients_page = recipients_page.page.clone();
    register_window_action(
        &action_window,
        "context-save",
        move || match context_save_target(&navigation, &recipients_page) {
            ContextSaveTarget::Password => {
                activate_widget_action(&dispatch_window, "win.save-password")
            }
            ContextSaveTarget::StoreRecipients => {
                activate_widget_action(&dispatch_window, "win.save-store-recipients")
            }
            #[cfg(not(feature = "flatpak"))]
            ContextSaveTarget::Synchronize => {
                activate_widget_action(&dispatch_window, "win.synchronize")
            }
            ContextSaveTarget::None => {}
        },
    );
}

pub(crate) fn register_toggle_find_action(
    window: &adw::ApplicationWindow,
    search_entry: &SearchEntry,
) {
    let search_entry = search_entry.clone();
    register_window_action(window, "toggle-find", move || {
        let visible = search_entry.is_visible();
        search_entry.set_visible(!visible);
        if !visible {
            search_entry.grab_focus();
        }
    });
}

pub(crate) fn register_back_action(window: &adw::ApplicationWindow, state: &BackActionState) {
    let state = state.clone();
    register_window_action(window, "back", move || {
        if before_back_action(&state.platform) {
            return;
        }

        state.navigation.nav.pop();
        if restore_window_for_current_page(&state.navigation, &state.recipients_page) {
            show_password_list_page(&state.password_page, state.show_hidden.get());
            return;
        }

        let _ = retry_open_password_entry_if_needed(&state.password_page);
    });
}

pub(crate) fn configure_window_shortcuts(app: &Application) {
    app.set_accels_for_action("win.back", &["Escape"]);
    app.set_accels_for_action("win.context-save", &["<primary>s"]);
    app.set_accels_for_action("win.toggle-find", &["<primary>f"]);
    app.set_accels_for_action("win.toggle-hidden", &["<primary>h"]);
    app.set_accels_for_action("win.open-new-password", &["<primary>n"]);
    app.set_accels_for_action("win.open-preferences", &["<primary>p"]);
    configure_shortcuts(app);
}

pub(crate) fn register_toggle_hidden_action(
    window: &adw::ApplicationWindow,
    state: &HiddenEntriesActionState,
) {
    let state = state.clone();
    register_window_action(window, "toggle-hidden", move || {
        let show_hidden = !state.show_hidden.get();
        let show_list_actions = navigation_stack_is_root(&state.navigation.nav);
        if !show_list_actions {
            return;
        }
        state.show_hidden.set(show_hidden);
        load_passwords_async(
            &state.list,
            &state.navigation.git,
            &state.navigation.find,
            &state.navigation.save,
            &state.overlay,
            show_list_actions,
            show_hidden,
        );
    });
}

pub(crate) fn apply_startup_query(
    startup_query: Option<String>,
    search_entry: &SearchEntry,
    list: &ListBox,
) {
    if let Some(query) = startup_query {
        if !query.is_empty() {
            search_entry.set_visible(true);
            search_entry.set_text(&query);
            list.invalidate_filter();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{context_save_target_from_flags, ContextSaveTarget};

    #[test]
    fn context_save_prefers_password_pages() {
        assert_eq!(
            context_save_target_from_flags(false, true, false, false),
            ContextSaveTarget::Password
        );
        assert_eq!(
            context_save_target_from_flags(false, false, true, true),
            ContextSaveTarget::Password
        );
    }

    #[test]
    fn context_save_uses_recipients_page_before_list_mode() {
        assert_eq!(
            context_save_target_from_flags(false, false, false, true),
            ContextSaveTarget::StoreRecipients
        );
    }

    #[test]
    fn context_save_uses_sync_on_the_root_list_page() {
        #[cfg(not(feature = "flatpak"))]
        assert_eq!(
            context_save_target_from_flags(true, false, false, false),
            ContextSaveTarget::Synchronize
        );
        #[cfg(feature = "flatpak")]
        assert_eq!(
            context_save_target_from_flags(true, false, false, false),
            ContextSaveTarget::None
        );
    }
}
