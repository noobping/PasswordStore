use crate::password::file::{DynamicFieldRow, StructuredPassLine};
use crate::password::new_item::NewPasswordPopoverState;
use crate::password::otp::PasswordOtpState;
use crate::password::page::PasswordPageState;
use crate::store::management::{
    StoreRecipientsPageState, StoreRecipientsPlatformState, StoreRecipientsRequest,
};
use crate::window::controls::{
    BackActionState, HiddenEntriesActionState, PlatformBackActionState,
};
use crate::window::navigation::WindowNavigationState;
use crate::window::preferences::PreferencesActionState;
use adw::{EntryRow, NavigationPage, NavigationView, PasswordEntryRow, StatusPage, ToastOverlay, WindowTitle};
use adw::gtk::{Box as GtkBox, Button, ListBox, Popover, TextView};
use std::cell::{Cell, RefCell};
use std::rc::Rc;

#[cfg(not(feature = "flatpak"))]
use crate::window::git::GitActionState;
#[cfg(not(feature = "flatpak"))]
use crate::window::standard::StandardWindowParts;
use adw::ApplicationWindow;

pub(super) fn new_password_popover_state(
    popover: &Popover,
    path_entry: &EntryRow,
    store_box: &GtkBox,
    store_list: &GtkBox,
) -> NewPasswordPopoverState {
    NewPasswordPopoverState {
        popover: popover.clone(),
        path_entry: path_entry.clone(),
        store_box: store_box.clone(),
        store_list: store_list.clone(),
        store_roots: Rc::new(RefCell::new(Vec::new())),
        selected_store: Rc::new(RefCell::new(None)),
    }
}

pub(super) fn password_page_state(
    nav: &NavigationView,
    page: &NavigationPage,
    raw_page: &NavigationPage,
    list: &ListBox,
    back: &Button,
    add: &Button,
    find: &Button,
    git: &Button,
    save: &Button,
    win: &WindowTitle,
    status: &StatusPage,
    entry: &PasswordEntryRow,
    username: &EntryRow,
    otp: &PasswordOtpState,
    dynamic_box: &GtkBox,
    raw_button: &Button,
    text: &TextView,
    overlay: &ToastOverlay,
) -> PasswordPageState {
    PasswordPageState {
        nav: nav.clone(),
        page: page.clone(),
        raw_page: raw_page.clone(),
        list: list.clone(),
        back: back.clone(),
        add: add.clone(),
        find: find.clone(),
        git: git.clone(),
        save: save.clone(),
        win: win.clone(),
        status: status.clone(),
        entry: entry.clone(),
        username: username.clone(),
        otp: otp.clone(),
        dynamic_box: dynamic_box.clone(),
        raw_button: raw_button.clone(),
        structured_templates: Rc::new(RefCell::new(Vec::<StructuredPassLine>::new())),
        dynamic_rows: Rc::new(RefCell::new(Vec::<DynamicFieldRow>::new())),
        text: text.clone(),
        overlay: overlay.clone(),
    }
}

#[cfg(feature = "flatpak")]
fn build_store_recipients_platform_state(overlay: &ToastOverlay) -> StoreRecipientsPlatformState {
    StoreRecipientsPlatformState {
        overlay: overlay.clone(),
    }
}

#[cfg(not(feature = "flatpak"))]
fn build_store_recipients_platform_state(
    standard_parts: &StandardWindowParts,
) -> StoreRecipientsPlatformState {
    StoreRecipientsPlatformState {
        entry: standard_parts.store_recipients_entry.clone(),
    }
}

pub(super) fn store_recipients_page_state(
    window: &ApplicationWindow,
    nav: &NavigationView,
    page: &NavigationPage,
    list: &ListBox,
    back: &Button,
    add: &Button,
    find: &Button,
    git: &Button,
    save: &Button,
    win: &WindowTitle,
    _overlay: &ToastOverlay,
    #[cfg(not(feature = "flatpak"))] standard_parts: &StandardWindowParts,
) -> StoreRecipientsPageState {
    let request = Rc::new(RefCell::new(None::<StoreRecipientsRequest>));
    let recipients = Rc::new(RefCell::new(Vec::<String>::new()));
    let saved_recipients = Rc::new(RefCell::new(Vec::<String>::new()));
    let save_in_flight = Rc::new(Cell::new(false));
    let save_queued = Rc::new(Cell::new(false));
    #[cfg(feature = "flatpak")]
    let platform = build_store_recipients_platform_state(_overlay);
    #[cfg(not(feature = "flatpak"))]
    let platform = build_store_recipients_platform_state(standard_parts);

    StoreRecipientsPageState {
        window: window.clone(),
        nav: nav.clone(),
        page: page.clone(),
        list: list.clone(),
        platform,
        back: back.clone(),
        add: add.clone(),
        find: find.clone(),
        git: git.clone(),
        save: save.clone(),
        win: win.clone(),
        request,
        recipients,
        saved_recipients,
        save_in_flight,
        save_queued,
    }
}

pub(super) fn window_navigation_state(
    nav: &NavigationView,
    text_page: &NavigationPage,
    raw_text_page: &NavigationPage,
    settings_page: &NavigationPage,
    log_page: &NavigationPage,
    back: &Button,
    add: &Button,
    find: &Button,
    git: &Button,
    save: &Button,
    win: &WindowTitle,
    username: &EntryRow,
) -> WindowNavigationState {
    WindowNavigationState {
        nav: nav.clone(),
        text_page: text_page.clone(),
        raw_text_page: raw_text_page.clone(),
        settings_page: settings_page.clone(),
        log_page: log_page.clone(),
        back: back.clone(),
        add: add.clone(),
        find: find.clone(),
        git: git.clone(),
        save: save.clone(),
        win: win.clone(),
        username: username.clone(),
    }
}

pub(super) fn preferences_action_state(
    window: &ApplicationWindow,
    nav: &NavigationView,
    page: &NavigationPage,
    back: &Button,
    add: &Button,
    find: &Button,
    git: &Button,
    save: &Button,
    win: &WindowTitle,
    template_view: &TextView,
    stores_list: &ListBox,
    overlay: &ToastOverlay,
    recipients_page: &StoreRecipientsPageState,
    #[cfg(not(feature = "flatpak"))] standard_parts: &StandardWindowParts,
) -> PreferencesActionState {
    PreferencesActionState {
        window: window.clone(),
        nav: nav.clone(),
        page: page.clone(),
        back: back.clone(),
        add: add.clone(),
        find: find.clone(),
        git: git.clone(),
        save: save.clone(),
        win: win.clone(),
        template_view: template_view.clone(),
        stores_list: stores_list.clone(),
        overlay: overlay.clone(),
        recipients_page: recipients_page.clone(),
        #[cfg(not(feature = "flatpak"))]
        pass_row: standard_parts.pass_row.clone(),
        #[cfg(not(feature = "flatpak"))]
        backend_row: standard_parts.backend_row.clone(),
    }
}

#[cfg(feature = "flatpak")]
fn build_back_action_platform_state() -> PlatformBackActionState {
    PlatformBackActionState
}

#[cfg(not(feature = "flatpak"))]
fn build_back_action_platform_state(git_action_state: &GitActionState) -> PlatformBackActionState {
    PlatformBackActionState {
        git_actions: git_action_state.clone(),
    }
}

pub(super) fn back_action_state(
    password_page: &PasswordPageState,
    recipients_page: &StoreRecipientsPageState,
    navigation: &WindowNavigationState,
    show_hidden: &Rc<Cell<bool>>,
    #[cfg(not(feature = "flatpak"))] git_action_state: &GitActionState,
) -> BackActionState {
    #[cfg(feature = "flatpak")]
    let platform = build_back_action_platform_state();
    #[cfg(not(feature = "flatpak"))]
    let platform = build_back_action_platform_state(git_action_state);

    BackActionState {
        password_page: password_page.clone(),
        recipients_page: recipients_page.clone(),
        navigation: navigation.clone(),
        show_hidden: show_hidden.clone(),
        platform,
    }
}

pub(super) fn hidden_entries_action_state(
    overlay: &ToastOverlay,
    list: &ListBox,
    navigation: &WindowNavigationState,
    show_hidden: &Rc<Cell<bool>>,
) -> HiddenEntriesActionState {
    HiddenEntriesActionState {
        overlay: overlay.clone(),
        list: list.clone(),
        navigation: navigation.clone(),
        show_hidden: show_hidden.clone(),
    }
}
