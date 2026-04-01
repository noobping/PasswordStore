use super::recipients::{read_store_private_key_requirement, read_store_recipients};
use crate::backend::StoreRecipientsPrivateKeyRequirement;
use crate::i18n::gettext;
use crate::store::git_page::StoreGitPageState;
use crate::support::actions::register_window_action;
use crate::support::ui::reveal_navigation_page;
use crate::window::navigation::{
    set_save_button_for_password, show_secondary_page_chrome, HasWindowChrome, APP_WINDOW_TITLE,
};
use adw::gtk::{Button, CheckButton, ListBox, ScrolledWindow, Stack};
use adw::prelude::*;
use adw::{
    ActionRow, ApplicationWindow, Dialog, EntryRow, NavigationPage, NavigationView,
    PasswordEntryRow, PreferencesGroup, StatusPage, ToastOverlay, WindowTitle,
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
    pub list: ListBox,
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
    pub private_key_requirement: Rc<Cell<StoreRecipientsPrivateKeyRequirement>>,
    pub saved_private_key_requirement: Rc<Cell<StoreRecipientsPrivateKeyRequirement>>,
    pub save_in_flight: Rc<Cell<bool>>,
    pub save_queued: Rc<Cell<bool>>,
    pub additional_fido2_save_guide_dialog: Rc<RefCell<Option<Dialog>>>,
    pub(crate) fido2_save_progress_dialog:
        Rc<RefCell<Option<StoreRecipientsSaveProgressDialogHandle>>>,
}

#[derive(Clone)]
pub struct StoreRecipientsPlatformState {
    pub overlay: ToastOverlay,
    pub host_gpg_warning_group: PreferencesGroup,
    pub host_gpg_warning_row: ActionRow,
    pub fido2_info_group: PreferencesGroup,
    pub add_group: PreferencesGroup,
    pub add_list: ListBox,
    pub create_group: PreferencesGroup,
    pub options_group: PreferencesGroup,
    pub git_group: PreferencesGroup,
    pub git_list: ListBox,
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
}

impl StoreRecipientsPageState {
    pub fn current_request(&self) -> Option<StoreRecipientsRequest> {
        self.request.borrow().clone()
    }

    pub fn recipients_are_dirty(&self) -> bool {
        *self.recipients.borrow() != *self.saved_recipients.borrow()
            || self.private_key_requirement.get() != self.saved_private_key_requirement.get()
    }
}

pub fn connect_store_recipients_controls(state: &StoreRecipientsPageState) {
    import::connect_private_key_import_controls(state);
    generate::connect_private_key_generate_controls(state);
    list::connect_private_key_requirement_control(state);
    generate::connect_private_key_generation_autofill(state);
    generate::connect_private_key_generation_submit(state);
}

pub fn rebuild_store_recipients_list(state: &StoreRecipientsPageState) {
    list::rebuild_store_recipients_list(state);
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
    let Some(request) = state.current_request() else {
        state.save.set_visible(false);
        set_save_button_for_password(&state.save);
        state.win.set_title(&gettext("Store keys"));
        state.win.set_subtitle(&gettext(APP_WINDOW_TITLE));
        return;
    };

    let chrome = state.window_chrome();
    show_secondary_page_chrome(&chrome, request.mode.page_title(), &request.store, false);
    state.page.set_title(&gettext(request.mode.page_title()));
}

fn show_store_recipients_page(
    state: &StoreRecipientsPageState,
    request: StoreRecipientsRequest,
    initial_recipients: Vec<String>,
    private_key_requirement: StoreRecipientsPrivateKeyRequirement,
) {
    let saved_recipients = read_store_recipients(&request.store);
    let mode = request.mode;
    *state.request.borrow_mut() = Some(request);
    *state.recipients.borrow_mut() = initial_recipients;
    *state.saved_recipients.borrow_mut() = saved_recipients;
    state.private_key_requirement.set(private_key_requirement);
    state
        .saved_private_key_requirement
        .set(private_key_requirement);
    state.save_in_flight.set(false);
    state.save_queued.set(false);
    state.platform.add_group.set_visible(true);
    state.platform.create_group.set_visible(true);
    state.platform.options_group.set_visible(true);
    rebuild_store_recipients_list(state);
    sync_store_recipients_page_header(state);

    if !reveal_navigation_page(&state.nav, &state.page) {
        return;
    }

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
