use super::widgets::WindowWidgets;
use crate::password::file::{DynamicFieldRow, StructuredPassLine};
use crate::password::new_item::NewPasswordPopoverState;
use crate::password::otp::PasswordOtpState;
use crate::password::page::PasswordPageState;
use crate::store::management::{
    StoreRecipientsPageState, StoreRecipientsPlatformState, StoreRecipientsRequest,
};
use crate::window::controls::{BackActionState, HiddenEntriesActionState, PlatformBackActionState};
use crate::window::navigation::WindowNavigationState;
use crate::window::preferences::PreferencesActionState;
use std::cell::{Cell, RefCell};
use std::rc::Rc;

#[cfg(not(feature = "flatpak"))]
use crate::window::git::GitActionState;
#[cfg(not(feature = "flatpak"))]
use adw::EntryRow;
#[cfg(feature = "flatpak")]
use adw::ToastOverlay;

pub(super) fn new_password_popover_state(widgets: &WindowWidgets) -> NewPasswordPopoverState {
    NewPasswordPopoverState {
        popover: widgets.add_button_popover.clone(),
        store_dropdown: widgets.new_password_store_dropdown.clone(),
        store_roots: Rc::new(RefCell::new(Vec::new())),
    }
}

pub(super) fn password_page_state(
    widgets: &WindowWidgets,
    otp: &PasswordOtpState,
) -> PasswordPageState {
    PasswordPageState {
        nav: widgets.navigation_view.clone(),
        page: widgets.text_page.clone(),
        raw_page: widgets.raw_text_page.clone(),
        list: widgets.list.clone(),
        back: widgets.back_button.clone(),
        add: widgets.add_button.clone(),
        find: widgets.find_button.clone(),
        git: widgets.git_button.clone(),
        save: widgets.save_button.clone(),
        win: widgets.window_title.clone(),
        status: widgets.password_status.clone(),
        entry: widgets.password_entry.clone(),
        username: widgets.username_entry.clone(),
        otp: otp.clone(),
        otp_add_button: widgets.add_otp_button.clone(),
        dynamic_box: widgets.dynamic_fields_box.clone(),
        raw_button: widgets.open_raw_button.clone(),
        structured_templates: Rc::new(RefCell::new(Vec::<StructuredPassLine>::new())),
        dynamic_rows: Rc::new(RefCell::new(Vec::<DynamicFieldRow>::new())),
        text: widgets.text_view.clone(),
        overlay: widgets.toast_overlay.clone(),
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
    store_recipients_entry: &EntryRow,
) -> StoreRecipientsPlatformState {
    StoreRecipientsPlatformState {
        entry: store_recipients_entry.clone(),
    }
}

pub(super) fn store_recipients_page_state(
    widgets: &WindowWidgets,
    #[cfg(not(feature = "flatpak"))] store_recipients_entry: &EntryRow,
) -> StoreRecipientsPageState {
    let request = Rc::new(RefCell::new(None::<StoreRecipientsRequest>));
    let recipients = Rc::new(RefCell::new(Vec::<String>::new()));
    let saved_recipients = Rc::new(RefCell::new(Vec::<String>::new()));
    let save_in_flight = Rc::new(Cell::new(false));
    let save_queued = Rc::new(Cell::new(false));
    #[cfg(feature = "flatpak")]
    let platform = build_store_recipients_platform_state(&widgets.toast_overlay);
    #[cfg(not(feature = "flatpak"))]
    let platform = build_store_recipients_platform_state(store_recipients_entry);

    StoreRecipientsPageState {
        window: widgets.window.clone(),
        nav: widgets.navigation_view.clone(),
        page: widgets.store_recipients_page.clone(),
        list: widgets.store_recipients_list.clone(),
        platform,
        back: widgets.back_button.clone(),
        add: widgets.add_button.clone(),
        find: widgets.find_button.clone(),
        git: widgets.git_button.clone(),
        save: widgets.save_button.clone(),
        win: widgets.window_title.clone(),
        request,
        recipients,
        saved_recipients,
        save_in_flight,
        save_queued,
    }
}

pub(super) fn window_navigation_state(widgets: &WindowWidgets) -> WindowNavigationState {
    WindowNavigationState {
        nav: widgets.navigation_view.clone(),
        text_page: widgets.text_page.clone(),
        raw_text_page: widgets.raw_text_page.clone(),
        settings_page: widgets.settings_page.clone(),
        log_page: widgets.log_page.clone(),
        back: widgets.back_button.clone(),
        add: widgets.add_button.clone(),
        find: widgets.find_button.clone(),
        git: widgets.git_button.clone(),
        save: widgets.save_button.clone(),
        win: widgets.window_title.clone(),
        username: widgets.username_entry.clone(),
    }
}

pub(super) fn preferences_action_state(
    widgets: &WindowWidgets,
    recipients_page: &StoreRecipientsPageState,
) -> PreferencesActionState {
    PreferencesActionState {
        window: widgets.window.clone(),
        nav: widgets.navigation_view.clone(),
        page: widgets.settings_page.clone(),
        back: widgets.back_button.clone(),
        add: widgets.add_button.clone(),
        find: widgets.find_button.clone(),
        git: widgets.git_button.clone(),
        save: widgets.save_button.clone(),
        win: widgets.window_title.clone(),
        template_view: widgets.new_pass_file_template_view.clone(),
        stores_list: widgets.password_stores.clone(),
        overlay: widgets.toast_overlay.clone(),
        recipients_page: recipients_page.clone(),
        #[cfg(not(feature = "flatpak"))]
        pass_row: widgets.pass_command_row.clone(),
        #[cfg(not(feature = "flatpak"))]
        backend_row: widgets.backend_row.clone(),
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
    widgets: &WindowWidgets,
    navigation: &WindowNavigationState,
    show_hidden: &Rc<Cell<bool>>,
) -> HiddenEntriesActionState {
    HiddenEntriesActionState {
        overlay: widgets.toast_overlay.clone(),
        list: widgets.list.clone(),
        navigation: navigation.clone(),
        show_hidden: show_hidden.clone(),
    }
}
