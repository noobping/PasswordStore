use super::recipients::{
    read_store_private_key_requirement, read_store_private_key_requirement_for_scope,
    read_store_recipients, read_store_recipients_for_scope, store_is_supported_in_current_build,
    ROOT_STORE_RECIPIENTS_SCOPE, UNSUPPORTED_FIDOSTORE_MESSAGE,
};
use crate::backend::DiscoveredHardwareToken;
use crate::backend::StoreRecipientsPrivateKeyRequirement;
use crate::i18n::gettext;
use crate::store::git_page::StoreGitPageState;
use crate::support::actions::register_window_action;
use crate::support::ui::{
    focus_first_preferences_group_child_in_order, reveal_navigation_page,
    visible_navigation_page_is,
};
use crate::window::navigation::{show_secondary_page_chrome, HasWindowChrome, APP_WINDOW_TITLE};
use crate::window::preferences_search::PreferencesPageSearchState;
use adw::gtk::{Button, CheckButton, ScrolledWindow, Stack, Widget};
use adw::prelude::*;
use adw::{
    ActionRow, ApplicationWindow, ComboRow, Dialog, EntryRow, NavigationPage, NavigationView,
    PasswordEntryRow, PreferencesGroup, StatusPage, Toast, ToastOverlay, WindowTitle,
};
use std::cell::{Cell, RefCell};
use std::rc::Rc;

mod export;
mod generate;
mod guide;
mod import;
mod list;
mod mode;
mod progress;
mod save;
mod sync;
use self::progress::StoreRecipientsSaveProgressDialogHandle;
pub use self::save::{queue_store_recipients_autosave, register_store_recipients_save_action};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StoreRecipientsMode {
    Create,
    Edit,
}

impl StoreRecipientsMode {
    pub const fn page_title(self) -> &'static str {
        match self {
            Self::Create => "New Store",
            Self::Edit => "Store keys",
        }
    }

    #[cfg(test)]
    pub const fn empty_state_subtitle(self) -> &'static str {
        match self {
            Self::Create => "Add at least one recipient to create this store.",
            Self::Edit => "Add at least one recipient to keep saving changes.",
        }
    }

    pub const fn save_failure_message(self) -> &'static str {
        match self {
            Self::Create => "Couldn't create the store.",
            Self::Edit => "Couldn't save store keys.",
        }
    }

    pub const fn creates_store(self) -> bool {
        matches!(self, Self::Create)
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StoreRecipientsRequest {
    pub store: String,
    pub mode: StoreRecipientsMode,
}

impl StoreRecipientsRequest {
    pub fn create(store: impl Into<String>) -> Self {
        Self {
            store: store.into(),
            mode: StoreRecipientsMode::Create,
        }
    }

    pub fn edit(store: impl Into<String>) -> Self {
        Self {
            store: store.into(),
            mode: StoreRecipientsMode::Edit,
        }
    }
}

#[derive(Clone)]
pub struct StoreRecipientsPageState {
    pub window: ApplicationWindow,
    pub nav: NavigationView,
    pub page: NavigationPage,
    pub back_row: ActionRow,
    pub search: PreferencesPageSearchState,
    pub list: PreferencesGroup,
    pub platform: StoreRecipientsPlatformState,
    pub back: Button,
    pub add: Button,
    pub find: Button,
    pub git: Button,
    pub store: Button,
    pub save: Button,
    pub raw: Button,
    pub win: WindowTitle,
    pub request: Rc<RefCell<Option<StoreRecipientsRequest>>>,
    pub recipients: Rc<RefCell<Vec<String>>>,
    pub saved_recipients: Rc<RefCell<Vec<String>>>,
    pub recipient_scope_dirs: Rc<RefCell<Vec<String>>>,
    pub selected_recipient_scope: Rc<RefCell<String>>,
    pub private_key_requirement: Rc<Cell<StoreRecipientsPrivateKeyRequirement>>,
    pub saved_private_key_requirement: Rc<Cell<StoreRecipientsPrivateKeyRequirement>>,
    pub save_in_flight: Rc<Cell<bool>>,
    pub save_queued: Rc<Cell<bool>>,
    pub additional_fido2_save_guide_dialog: Rc<RefCell<Option<Dialog>>>,
    pub(crate) fido2_save_progress_dialog:
        Rc<RefCell<Option<StoreRecipientsSaveProgressDialogHandle>>>,
    pub(crate) reopen_after_subpage: Rc<Cell<bool>>,
    pub(crate) key_rows: Rc<RefCell<Vec<Widget>>>,
    pub(crate) git_rows: Rc<RefCell<Vec<Widget>>>,
}

#[derive(Clone)]
pub struct StoreRecipientsPlatformState {
    pub overlay: ToastOverlay,
    pub host_gpg_warning_group: PreferencesGroup,
    pub host_gpg_warning_list: PreferencesGroup,
    pub host_gpg_warning_row: ActionRow,
    pub fido2_info_group: PreferencesGroup,
    pub fido2_info_list: PreferencesGroup,
    pub scope_group: PreferencesGroup,
    pub saving_group: PreferencesGroup,
    pub keys_group: PreferencesGroup,
    pub scope_list: PreferencesGroup,
    pub add_group: PreferencesGroup,
    pub add_list: PreferencesGroup,
    pub create_group: PreferencesGroup,
    pub create_list: PreferencesGroup,
    pub options_group: PreferencesGroup,
    pub options_list: PreferencesGroup,
    pub scope_row: ComboRow,
    pub git_group: PreferencesGroup,
    pub git_list: PreferencesGroup,
    pub setup_hardware_key_row: ActionRow,
    pub add_hardware_key_row: ActionRow,
    pub add_fido2_key_row: ActionRow,
    pub store_git_page: StoreGitPageState,
    pub import_hardware_key_row: ActionRow,
    pub import_clipboard_row: ActionRow,
    pub import_file_row: ActionRow,
    pub generate_key_row: ActionRow,
    pub generate_fido2_key_row: ActionRow,
    pub require_all_row: ActionRow,
    pub all_fido2_keys_required_row: ActionRow,
    pub require_all_check: CheckButton,
    pub private_key_generation_page: NavigationPage,
    pub private_key_generation_stack: Stack,
    pub private_key_generation_form: ScrolledWindow,
    pub private_key_generation_loading: StatusPage,
    pub private_key_generation_name_row: EntryRow,
    pub private_key_generation_email_row: EntryRow,
    pub private_key_generation_password_row: PasswordEntryRow,
    pub private_key_generation_confirm_row: PasswordEntryRow,
    pub private_key_generation_in_flight: Rc<Cell<bool>>,
    pub hardware_key_generation_page: NavigationPage,
    pub hardware_key_generation_stack: Stack,
    pub hardware_key_generation_form: ScrolledWindow,
    pub hardware_key_generation_loading: StatusPage,
    pub hardware_key_generation_name_row: EntryRow,
    pub hardware_key_generation_email_row: EntryRow,
    pub hardware_key_generation_admin_pin_row: PasswordEntryRow,
    pub hardware_key_generation_user_pin_row: PasswordEntryRow,
    pub hardware_key_generation_token: Rc<RefCell<Option<DiscoveredHardwareToken>>>,
    pub hardware_key_generation_in_flight: Rc<Cell<bool>>,
}

impl StoreRecipientsPageState {
    pub fn current_request(&self) -> Option<StoreRecipientsRequest> {
        self.request.borrow().clone()
    }

    pub fn current_recipient_scope(&self) -> String {
        self.selected_recipient_scope.borrow().clone()
    }

    pub fn recipients_are_dirty(&self) -> bool {
        *self.recipients.borrow() != *self.saved_recipients.borrow()
            || self.private_key_requirement.get() != self.saved_private_key_requirement.get()
    }
}

fn ordered_store_recipients_lists(state: &StoreRecipientsPageState) -> [PreferencesGroup; 8] {
    [
        state.platform.host_gpg_warning_list.clone(),
        state.platform.fido2_info_list.clone(),
        state.platform.scope_list.clone(),
        state.list.clone(),
        state.platform.create_list.clone(),
        state.platform.add_list.clone(),
        state.platform.options_list.clone(),
        state.platform.git_list.clone(),
    ]
}

pub fn present_store_recipients_dialog(state: &StoreRecipientsPageState) {
    sync_store_recipients_page_header(state);
    reveal_navigation_page(&state.nav, &state.page);
    let _ = focus_first_preferences_group_child_in_order(&ordered_store_recipients_lists(state));
}

pub fn handle_store_recipients_subpage_back(state: &StoreRecipientsPageState) -> bool {
    if !visible_navigation_page_is(&state.nav, &state.platform.private_key_generation_page)
        && !visible_navigation_page_is(&state.nav, &state.platform.hardware_key_generation_page)
    {
        return false;
    }

    state.nav.pop();
    if state.reopen_after_subpage.replace(false) {
        present_store_recipients_dialog(state);
    } else {
        sync_store_recipients_page_header(state);
    }
    true
}

pub(super) fn load_store_recipients_scope(
    state: &StoreRecipientsPageState,
    store_root: &str,
    scope: &str,
) {
    let normalized_scope = if scope.trim().is_empty() {
        ROOT_STORE_RECIPIENTS_SCOPE
    } else {
        scope
    };
    let recipients = read_store_recipients_for_scope(store_root, normalized_scope);
    let private_key_requirement =
        read_store_private_key_requirement_for_scope(store_root, normalized_scope);
    *state.selected_recipient_scope.borrow_mut() = normalized_scope.to_string();
    *state.recipients.borrow_mut() = recipients.clone();
    *state.saved_recipients.borrow_mut() = recipients;
    state.private_key_requirement.set(private_key_requirement);
    state
        .saved_private_key_requirement
        .set(private_key_requirement);
}

pub fn connect_store_recipients_controls(state: &StoreRecipientsPageState) {
    state.back_row.set_visible(false);

    import::connect_private_key_import_controls(state);
    import::connect_hardware_key_generation_autofill(state);
    import::connect_hardware_key_generation_submit(state);
    generate::connect_private_key_generate_controls(state);
    list::connect_recipient_scope_control(state);
    list::connect_private_key_requirement_control(state);
    list::connect_dismissible_notice_controls(state);
    generate::connect_private_key_generation_autofill(state);
    generate::connect_private_key_generation_submit(state);
}

pub fn rebuild_store_recipients_list(state: &StoreRecipientsPageState) {
    list::rebuild_store_recipients_list(state);
    state.search.sync();
}

pub fn register_store_recipients_reload_action(
    window: &ApplicationWindow,
    state: &StoreRecipientsPageState,
) {
    let state = state.clone();
    register_window_action(window, "reload-store-recipients-list", move || {
        if state.current_request().is_none() {
            return;
        }

        rebuild_store_recipients_list(&state);
    });
}

pub fn sync_store_recipients_page_header(state: &StoreRecipientsPageState) {
    let chrome = state.window_chrome();
    let Some(request) = state.current_request() else {
        state.page.set_title(&gettext("Store keys"));
        show_secondary_page_chrome(&chrome, "Store keys", APP_WINDOW_TITLE, false);
        chrome.find.set_visible(true);
        return;
    };

    state.page.set_title(&gettext(request.mode.page_title()));
    show_secondary_page_chrome(&chrome, request.mode.page_title(), &request.store, false);
    chrome.find.set_visible(true);
}

fn show_store_recipients_page(
    state: &StoreRecipientsPageState,
    request: StoreRecipientsRequest,
    initial_recipients: Vec<String>,
    private_key_requirement: StoreRecipientsPrivateKeyRequirement,
) {
    let mode = request.mode;
    *state.request.borrow_mut() = Some(request);
    *state.recipient_scope_dirs.borrow_mut() = Vec::new();
    *state.selected_recipient_scope.borrow_mut() = ROOT_STORE_RECIPIENTS_SCOPE.to_string();
    *state.recipients.borrow_mut() = initial_recipients.clone();
    *state.saved_recipients.borrow_mut() = initial_recipients;
    state.private_key_requirement.set(private_key_requirement);
    state
        .saved_private_key_requirement
        .set(private_key_requirement);
    state.save_in_flight.set(false);
    state.save_queued.set(false);
    state.reopen_after_subpage.set(false);
    state.platform.add_group.set_visible(true);
    state.platform.create_group.set_visible(true);
    state.platform.options_group.set_visible(true);
    rebuild_store_recipients_list(state);
    state.search.sync();
    present_store_recipients_dialog(state);

    if mode.creates_store() {
        queue_store_recipients_autosave(state);
    }
}

pub fn show_store_recipients_create_page(
    state: &StoreRecipientsPageState,
    store: impl Into<String>,
    initial_recipients: Vec<String>,
) {
    show_store_recipients_page(
        state,
        StoreRecipientsRequest::create(store),
        initial_recipients,
        StoreRecipientsPrivateKeyRequirement::AnyManagedKey,
    );
}

pub fn show_store_recipients_edit_page(state: &StoreRecipientsPageState, store: impl Into<String>) {
    let store = store.into();
    if !store_is_supported_in_current_build(&store) {
        state
            .platform
            .overlay
            .add_toast(Toast::new(&gettext(UNSUPPORTED_FIDOSTORE_MESSAGE)));
        return;
    }

    show_store_recipients_page(
        state,
        StoreRecipientsRequest::edit(store.clone()),
        read_store_recipients(&store),
        read_store_private_key_requirement(&store),
    );
}

#[cfg(test)]
mod tests {
    use super::StoreRecipientsMode;

    #[test]
    fn mode_titles_match_their_behavior() {
        assert_eq!(StoreRecipientsMode::Create.page_title(), "New Store");
        assert_eq!(StoreRecipientsMode::Edit.page_title(), "Store keys");
    }

    #[test]
    fn mode_messages_match_their_behavior() {
        assert_eq!(
            StoreRecipientsMode::Create.empty_state_subtitle(),
            "Add at least one recipient to create this store."
        );
        assert_eq!(
            StoreRecipientsMode::Edit.save_failure_message(),
            "Couldn't save store keys."
        );
    }
}
