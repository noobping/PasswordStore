use crate::password::list::{load_passwords_async, PasswordListActions};
use crate::password::model::OpenPassFile;
use crate::password::page::{
    open_password_entry_page, password_page_has_unsaved_changes,
    retry_open_password_entry_if_needed, revert_unsaved_password_changes, show_password_list_page,
    PasswordPageState,
};
use crate::password::undo::{
    execute_undo_action, pop_undo_action, push_undo_action, undo_action_restored_entry,
};
use crate::store::management::StoreRecipientsPageState;
use crate::support::actions::{activate_widget_action, register_window_action};
use crate::support::background::spawn_result_task;
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
pub(crate) struct ListVisibilityState {
    show_hidden: Rc<Cell<bool>>,
    show_duplicates: Rc<Cell<bool>>,
}

impl ListVisibilityState {
    pub(crate) fn new(show_hidden: bool, show_duplicates: bool) -> Self {
        Self {
            show_hidden: Rc::new(Cell::new(show_hidden)),
            show_duplicates: Rc::new(Cell::new(show_duplicates)),
        }
    }

    pub(crate) fn show_hidden(&self) -> bool {
        self.show_hidden.get()
    }

    pub(crate) fn show_duplicates(&self) -> bool {
        self.show_duplicates.get()
    }

    pub(crate) fn toggle_all(&self) -> (bool, bool) {
        let (show_hidden, show_duplicates) =
            toggled_list_visibility(self.show_hidden(), self.show_duplicates());
        self.show_hidden.set(show_hidden);
        self.show_duplicates.set(show_duplicates);
        (show_hidden, show_duplicates)
    }
}

fn toggled_list_visibility(show_hidden: bool, show_duplicates: bool) -> (bool, bool) {
    let show_all = !(show_hidden && show_duplicates);
    (show_all, show_all)
}

#[derive(Clone)]
pub(crate) struct BackActionState {
    pub(crate) password_page: PasswordPageState,
    pub(crate) recipients_page: StoreRecipientsPageState,
    pub(crate) navigation: WindowNavigationState,
    pub(crate) visibility: ListVisibilityState,
    pub(crate) platform: PlatformBackActionState,
}

#[derive(Clone)]
pub(crate) struct ListVisibilityActionState {
    pub(crate) overlay: ToastOverlay,
    pub(crate) list: ListBox,
    pub(crate) navigation: WindowNavigationState,
    pub(crate) visibility: ListVisibilityState,
}

#[derive(Clone)]
pub(crate) struct ContextUndoActionState {
    pub(crate) password_page: PasswordPageState,
    pub(crate) recipients_page: StoreRecipientsPageState,
    pub(crate) navigation: WindowNavigationState,
    pub(crate) visibility: ListVisibilityState,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ContextSaveTarget {
    Password,
    StoreRecipients,
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
        return ContextSaveTarget::Synchronize;
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
            ContextSaveTarget::Synchronize => {
                activate_widget_action(&dispatch_window, "win.synchronize")
            }
            ContextSaveTarget::None => {}
        },
    );
}

fn reload_password_list(
    list: &ListBox,
    overlay: &ToastOverlay,
    navigation: &WindowNavigationState,
    visibility: &ListVisibilityState,
) {
    let show_list_actions = navigation_stack_is_root(&navigation.nav);
    let list_actions = PasswordListActions::new(
        &navigation.add,
        &navigation.git,
        &navigation.store,
        &navigation.find,
        &navigation.save,
    );
    load_passwords_async(
        list,
        &list_actions,
        overlay,
        show_list_actions,
        visibility.show_hidden(),
        visibility.show_duplicates(),
    );
}

pub(crate) fn register_context_undo_action(
    window: &ApplicationWindow,
    state: &ContextUndoActionState,
) {
    let state = state.clone();
    register_window_action(window, "context-undo", move || {
        let editing_password =
            visible_navigation_page_is(&state.navigation.nav, &state.navigation.text_page)
                || visible_navigation_page_is(
                    &state.navigation.nav,
                    &state.navigation.raw_text_page,
                );
        if editing_password && password_page_has_unsaved_changes(&state.password_page) {
            let _ = revert_unsaved_password_changes(&state.password_page);
            return;
        }

        let Some(action) = pop_undo_action() else {
            return;
        };

        let overlay = state.password_page.overlay.clone();
        let state_for_result = state.clone();
        let state_for_disconnect = state.clone();
        let action_for_result = action.clone();
        let action_for_disconnect = action.clone();
        spawn_result_task(
            move || execute_undo_action(&action),
            move |result| match result {
                Ok(()) => {
                    if editing_password {
                        if let Some((store, label)) = undo_action_restored_entry(&action_for_result)
                        {
                            open_password_entry_page(
                                &state_for_result.password_page,
                                OpenPassFile::from_label(store, &label),
                                false,
                            );
                        } else {
                            show_password_list_page(
                                &state_for_result.password_page,
                                state_for_result.visibility.show_hidden(),
                                state_for_result.visibility.show_duplicates(),
                            );
                        }
                    } else {
                        reload_password_list(
                            &state_for_result.password_page.list,
                            &state_for_result.password_page.overlay,
                            &state_for_result.navigation,
                            &state_for_result.visibility,
                        );
                        let _ = restore_window_for_current_page(
                            &state_for_result.navigation,
                            &state_for_result.recipients_page,
                        );
                    }
                    overlay.add_toast(adw::Toast::new("Undone."));
                }
                Err(err) => {
                    push_undo_action(action_for_result);
                    overlay.add_toast(adw::Toast::new(err.toast_message()));
                }
            },
            move || {
                push_undo_action(action_for_disconnect);
                state_for_disconnect
                    .password_page
                    .overlay
                    .add_toast(adw::Toast::new("Couldn't undo the last change."));
            },
        );
    });
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
            show_password_list_page(
                &state.password_page,
                state.visibility.show_hidden(),
                state.visibility.show_duplicates(),
            );
            return;
        }

        let _ = retry_open_password_entry_if_needed(&state.password_page);
    });
}

pub(crate) fn configure_window_shortcuts(app: &Application) {
    app.set_accels_for_action("win.back", &["Escape"]);
    app.set_accels_for_action("win.context-save", &["<primary>s"]);
    app.set_accels_for_action("win.context-undo", &["<primary>z"]);
    app.set_accels_for_action("win.toggle-find", &["<primary>f"]);
    app.set_accels_for_action("win.toggle-hidden-and-duplicates", &["<primary>h"]);
    app.set_accels_for_action("win.open-new-password", &["<primary>n"]);
    app.set_accels_for_action("win.open-preferences", &["<primary>p"]);
    app.set_accels_for_action("app.shortcuts", &["<primary>question"]);
    configure_shortcuts(app);
}

pub(crate) fn register_list_visibility_action(
    window: &adw::ApplicationWindow,
    state: &ListVisibilityActionState,
) {
    let state = state.clone();
    register_window_action(window, "toggle-hidden-and-duplicates", move || {
        let show_list_actions = navigation_stack_is_root(&state.navigation.nav);
        if !show_list_actions {
            return;
        }
        let _ = state.visibility.toggle_all();
        reload_password_list(
            &state.list,
            &state.overlay,
            &state.navigation,
            &state.visibility,
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
    use super::{context_save_target_from_flags, toggled_list_visibility, ContextSaveTarget};

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
        assert_eq!(
            context_save_target_from_flags(true, false, false, false),
            ContextSaveTarget::Synchronize
        );
    }

    #[test]
    fn context_save_is_disabled_on_other_secondary_pages() {
        assert_eq!(
            context_save_target_from_flags(false, false, false, false),
            ContextSaveTarget::None
        );
    }

    #[test]
    fn combined_visibility_action_shows_everything_until_both_flags_are_enabled() {
        assert_eq!(toggled_list_visibility(false, false), (true, true));
        assert_eq!(toggled_list_visibility(true, false), (true, true));
        assert_eq!(toggled_list_visibility(false, true), (true, true));
        assert_eq!(toggled_list_visibility(true, true), (false, false));
    }
}
