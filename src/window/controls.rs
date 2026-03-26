use crate::i18n::gettext;
use crate::password::list::{load_passwords_async, PasswordListActions};
use crate::password::model::OpenPassFile;
use crate::password::page::{
    open_password_entry_page, password_page_has_unsaved_changes,
    retry_open_password_entry_if_needed, revert_unsaved_password_changes, show_password_list_page,
    PasswordPageState,
};
use crate::password::undo::{
    execute_undo_action, pop_undo_action, push_undo_action, unavailable_undo_message,
    undo_action_restored_entry,
};
use crate::store::git_page::StoreGitPageState;
use crate::store::management::StoreRecipientsPageState;
use crate::support::actions::{activate_widget_action, register_window_action};
use crate::support::background::spawn_result_task;
use crate::support::runtime::{has_host_permission, supports_logging_features};
use crate::support::ui::{navigation_stack_is_root, visible_navigation_page_is};
use crate::window::git::{handle_git_busy_back, GitActionState};
use crate::window::navigation::{restore_window_for_current_page, WindowNavigationState};
use adw::gtk::{Button, ListBox, SearchEntry};
use adw::prelude::*;
use adw::ToastOverlay;
use adw::{Application, ApplicationWindow, NavigationPage};
use std::cell::Cell;
use std::rc::Rc;

#[derive(Clone)]
pub struct PlatformBackActionState {
    pub git_actions: GitActionState,
}

fn before_back_action(state: &PlatformBackActionState) -> bool {
    handle_git_busy_back(&state.git_actions)
}

#[derive(Clone)]
pub struct ListVisibilityState {
    show_hidden: Rc<Cell<bool>>,
    show_duplicates: Rc<Cell<bool>>,
}

impl ListVisibilityState {
    pub fn new(show_hidden: bool, show_duplicates: bool) -> Self {
        Self {
            show_hidden: Rc::new(Cell::new(show_hidden)),
            show_duplicates: Rc::new(Cell::new(show_duplicates)),
        }
    }

    pub fn show_hidden(&self) -> bool {
        self.show_hidden.get()
    }

    pub fn show_duplicates(&self) -> bool {
        self.show_duplicates.get()
    }

    pub fn toggle_all(&self) -> (bool, bool) {
        let (show_hidden, show_duplicates) =
            toggled_list_visibility(self.show_hidden(), self.show_duplicates());
        self.show_hidden.set(show_hidden);
        self.show_duplicates.set(show_duplicates);
        (show_hidden, show_duplicates)
    }
}

const fn toggled_list_visibility(show_hidden: bool, show_duplicates: bool) -> (bool, bool) {
    let show_all = !(show_hidden && show_duplicates);
    (show_all, show_all)
}

#[derive(Clone)]
pub struct BackActionState {
    pub password_page: PasswordPageState,
    pub recipients_page: StoreRecipientsPageState,
    pub store_git_page: StoreGitPageState,
    pub navigation: WindowNavigationState,
    pub visibility: ListVisibilityState,
    pub platform: PlatformBackActionState,
}

#[derive(Clone)]
pub struct ListVisibilityActionState {
    pub overlay: ToastOverlay,
    pub list: ListBox,
    pub navigation: WindowNavigationState,
    pub visibility: ListVisibilityState,
}

#[derive(Clone)]
pub struct ContextUndoActionState {
    pub password_page: PasswordPageState,
    pub recipients_page: StoreRecipientsPageState,
    pub store_git_page: StoreGitPageState,
    pub navigation: WindowNavigationState,
    pub visibility: ListVisibilityState,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ContextSaveTarget {
    Password,
    StoreRecipients,
    Synchronize,
    None,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ContextReloadTarget {
    PasswordList,
    StoreRecipients,
    None,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum VisibleContextPage {
    Root,
    Password,
    StoreRecipients,
    Other,
}

const fn context_save_target_from_page(
    page: VisibleContextPage,
    has_host_permission: bool,
) -> ContextSaveTarget {
    match page {
        VisibleContextPage::Password => ContextSaveTarget::Password,
        VisibleContextPage::StoreRecipients => ContextSaveTarget::StoreRecipients,
        VisibleContextPage::Root if has_host_permission => ContextSaveTarget::Synchronize,
        VisibleContextPage::Root | VisibleContextPage::Other => ContextSaveTarget::None,
    }
}

fn context_save_target(
    navigation: &WindowNavigationState,
    recipients_page: &NavigationPage,
) -> ContextSaveTarget {
    let page = if visible_navigation_page_is(&navigation.nav, &navigation.text_page)
        || visible_navigation_page_is(&navigation.nav, &navigation.raw_text_page)
    {
        VisibleContextPage::Password
    } else if visible_navigation_page_is(&navigation.nav, recipients_page) {
        VisibleContextPage::StoreRecipients
    } else if navigation_stack_is_root(&navigation.nav) {
        VisibleContextPage::Root
    } else {
        VisibleContextPage::Other
    };

    context_save_target_from_page(page, has_host_permission())
}

const fn context_reload_target_from_page(page: VisibleContextPage) -> ContextReloadTarget {
    match page {
        VisibleContextPage::Root => ContextReloadTarget::PasswordList,
        VisibleContextPage::StoreRecipients => ContextReloadTarget::StoreRecipients,
        VisibleContextPage::Password | VisibleContextPage::Other => ContextReloadTarget::None,
    }
}

fn context_reload_target(
    navigation: &WindowNavigationState,
    recipients_page: &NavigationPage,
) -> ContextReloadTarget {
    let page = if visible_navigation_page_is(&navigation.nav, recipients_page) {
        VisibleContextPage::StoreRecipients
    } else if navigation_stack_is_root(&navigation.nav) {
        VisibleContextPage::Root
    } else if visible_navigation_page_is(&navigation.nav, &navigation.text_page)
        || visible_navigation_page_is(&navigation.nav, &navigation.raw_text_page)
    {
        VisibleContextPage::Password
    } else {
        VisibleContextPage::Other
    };

    context_reload_target_from_page(page)
}

fn configure_platform_shortcuts(app: &Application) {
    if supports_logging_features() {
        app.set_accels_for_action("win.open-log", &["F12"]);
    }
}

pub fn register_context_save_action(
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
                activate_widget_action(&dispatch_window, "win.save-password");
            }
            ContextSaveTarget::StoreRecipients => {
                activate_widget_action(&dispatch_window, "win.save-store-recipients");
            }
            ContextSaveTarget::Synchronize => {
                activate_widget_action(&dispatch_window, "win.synchronize");
            }
            ContextSaveTarget::None => {}
        },
    );
}

pub fn register_context_reload_action(
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
        "context-reload",
        move || match context_reload_target(&navigation, &recipients_page) {
            ContextReloadTarget::PasswordList => {
                activate_widget_action(&dispatch_window, "win.reload-password-list");
            }
            ContextReloadTarget::StoreRecipients => {
                activate_widget_action(&dispatch_window, "win.reload-store-recipients-list");
            }
            ContextReloadTarget::None => {}
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

pub fn register_context_undo_action(window: &ApplicationWindow, state: &ContextUndoActionState) {
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

        let Some(action) = pop_undo_action(&state.navigation.nav) else {
            return;
        };
        if let Some(message) = unavailable_undo_message(&action) {
            state
                .password_page
                .overlay
                .add_toast(adw::Toast::new(&gettext(message)));
            return;
        }

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
                            &state_for_result.store_git_page,
                        );
                    }
                    overlay.add_toast(adw::Toast::new(&gettext("Undone.")));
                }
                Err(err) => {
                    push_undo_action(&state_for_result.navigation.nav, action_for_result);
                    overlay.add_toast(adw::Toast::new(&gettext(err.toast_message())));
                }
            },
            move || {
                push_undo_action(&state_for_disconnect.navigation.nav, action_for_disconnect);
                state_for_disconnect
                    .password_page
                    .overlay
                    .add_toast(adw::Toast::new(&gettext("Couldn't undo the last change.")));
            },
        );
    });
}

pub fn register_toggle_find_action(
    window: &adw::ApplicationWindow,
    find_button: &Button,
    search_entry: &SearchEntry,
    list: &ListBox,
) {
    let find_button = find_button.clone();
    let search_entry = search_entry.clone();
    let list = list.clone();
    register_window_action(window, "toggle-find", move || {
        if !find_button.is_visible() {
            hide_search_entry(&search_entry);
            return;
        }

        if search_entry.is_visible() {
            hide_and_clear_search_entry(&search_entry, &list);
            return;
        }

        search_entry.set_visible(true);
        search_entry.grab_focus();
    });
}

pub fn connect_search_visibility(find_button: &Button, search_entry: &SearchEntry, list: &ListBox) {
    let search_entry = search_entry.clone();
    let list = list.clone();
    find_button.connect_visible_notify(move |button| {
        if button.is_visible() {
            if !search_entry.text().is_empty() {
                search_entry.set_visible(true);
                list.invalidate_filter();
            }
            return;
        }

        hide_search_entry(&search_entry);
    });
}

fn hide_and_clear_search_entry(search_entry: &SearchEntry, list: &ListBox) {
    hide_search_entry(search_entry);
    if !search_entry.text().is_empty() {
        search_entry.set_text("");
    }
    list.invalidate_filter();
}

fn hide_search_entry(search_entry: &SearchEntry) {
    search_entry.set_visible(false);
}

pub fn register_back_action(window: &adw::ApplicationWindow, state: &BackActionState) {
    let state = state.clone();
    register_window_action(window, "back", move || {
        if before_back_action(&state.platform) {
            return;
        }

        state.navigation.nav.pop();
        if restore_window_for_current_page(
            &state.navigation,
            &state.recipients_page,
            &state.store_git_page,
        ) {
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

pub fn register_go_home_action(window: &adw::ApplicationWindow, state: &BackActionState) {
    let state = state.clone();
    register_window_action(window, "go-home", move || {
        show_password_list_page(
            &state.password_page,
            state.visibility.show_hidden(),
            state.visibility.show_duplicates(),
        );
    });
}

pub fn configure_window_shortcuts(app: &Application) {
    app.set_accels_for_action("win.back", &["Escape"]);
    app.set_accels_for_action("win.go-home", &["Home"]);
    app.set_accels_for_action("win.context-save", &["<primary>s"]);
    app.set_accels_for_action("win.context-reload", &["F5"]);
    app.set_accels_for_action("win.synchronize", &["<primary><shift>s"]);
    app.set_accels_for_action("win.context-undo", &["<primary>z"]);
    app.set_accels_for_action("win.toggle-find", &["<primary>f"]);
    app.set_accels_for_action("win.toggle-hidden-and-duplicates", &["<primary>h"]);
    app.set_accels_for_action("win.open-new-password", &["<primary>n"]);
    app.set_accels_for_action("win.open-store-picker", &["<primary><shift>n"]);
    app.set_accels_for_action("win.open-raw-pass-file", &["<primary><shift>r"]);
    app.set_accels_for_action("win.add-otp-secret", &["<primary><shift>o"]);
    app.set_accels_for_action("win.clean-pass-file", &["<primary><shift>k"]);
    app.set_accels_for_action("win.generate-password", &["<primary><shift>g"]);
    app.set_accels_for_action("win.open-git", &["<primary>g"]);
    app.set_accels_for_action("win.open-preferences", &["<primary>comma"]);
    app.set_accels_for_action("win.open-tools", &["<primary>t"]);
    app.set_accels_for_action("win.open-docs", &["<primary><shift>d"]);
    app.set_accels_for_action("app.shortcuts", &["<primary>question"]);
    configure_platform_shortcuts(app);
}

pub fn register_list_visibility_action(
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

pub fn register_reload_password_list_action(
    window: &adw::ApplicationWindow,
    state: &ListVisibilityActionState,
) {
    let state = state.clone();
    register_window_action(window, "reload-password-list", move || {
        reload_password_list(
            &state.list,
            &state.overlay,
            &state.navigation,
            &state.visibility,
        );
    });
}

pub fn apply_startup_query(
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
    use super::{
        context_reload_target_from_page, context_save_target_from_page, toggled_list_visibility,
        ContextReloadTarget, ContextSaveTarget, VisibleContextPage,
    };

    #[test]
    fn context_save_prefers_password_pages() {
        assert_eq!(
            context_save_target_from_page(VisibleContextPage::Password, true),
            ContextSaveTarget::Password
        );
        assert_eq!(
            context_save_target_from_page(VisibleContextPage::Password, true),
            ContextSaveTarget::Password
        );
    }

    #[test]
    fn context_save_uses_recipients_page_before_list_mode() {
        assert_eq!(
            context_save_target_from_page(VisibleContextPage::StoreRecipients, true),
            ContextSaveTarget::StoreRecipients
        );
    }

    #[test]
    fn context_save_uses_sync_on_the_root_list_page() {
        assert_eq!(
            context_save_target_from_page(VisibleContextPage::Root, true),
            ContextSaveTarget::Synchronize
        );
    }

    #[test]
    fn context_save_skips_sync_when_git_is_unavailable() {
        assert_eq!(
            context_save_target_from_page(VisibleContextPage::Root, false),
            ContextSaveTarget::None
        );
    }

    #[test]
    fn context_save_is_disabled_on_other_secondary_pages() {
        assert_eq!(
            context_save_target_from_page(VisibleContextPage::Other, true),
            ContextSaveTarget::None
        );
    }

    #[test]
    fn context_reload_uses_the_root_password_list() {
        assert_eq!(
            context_reload_target_from_page(VisibleContextPage::Root),
            ContextReloadTarget::PasswordList
        );
    }

    #[test]
    fn context_reload_uses_the_recipients_page_list() {
        assert_eq!(
            context_reload_target_from_page(VisibleContextPage::StoreRecipients),
            ContextReloadTarget::StoreRecipients
        );
    }

    #[test]
    fn context_reload_is_disabled_on_editor_and_other_pages() {
        assert_eq!(
            context_reload_target_from_page(VisibleContextPage::Password),
            ContextReloadTarget::None
        );
        assert_eq!(
            context_reload_target_from_page(VisibleContextPage::Other),
            ContextReloadTarget::None
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
