use super::recipients::read_store_gpg_recipients;
use crate::support::ui::reveal_navigation_page;
use crate::window::navigation::{
    set_save_button_for_password, show_secondary_page_chrome, window_chrome, APP_WINDOW_TITLE,
};
use adw::gtk::{Button, ListBox};
use adw::prelude::*;
use adw::{ApplicationWindow, NavigationPage, NavigationView, WindowTitle};
use std::cell::{Cell, RefCell};
use std::rc::Rc;

#[cfg(feature = "flatpak")]
mod flatpak;
mod save;
#[cfg(not(feature = "flatpak"))]
mod standard;

#[cfg(feature = "flatpak")]
use self::flatpak as platform;
#[cfg(not(feature = "flatpak"))]
use self::standard as platform;

pub(crate) use self::platform::StoreRecipientsPlatformState;
pub(crate) use self::save::{
    queue_store_recipients_autosave, register_store_recipients_save_action,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum StoreRecipientsMode {
    Create,
    Edit,
}

impl StoreRecipientsMode {
    pub(crate) fn page_title(&self) -> &'static str {
        match self {
            Self::Create => "New Store",
            Self::Edit => "Recipients",
        }
    }

    #[cfg(not(feature = "flatpak"))]
    pub(crate) fn empty_state_subtitle(&self) -> &'static str {
        match self {
            Self::Create => "Add at least one recipient to create this store.",
            Self::Edit => "Add at least one recipient to keep saving changes.",
        }
    }

    pub(crate) fn save_failure_message(&self) -> &'static str {
        match self {
            Self::Create => "Couldn't create the store.",
            Self::Edit => "Couldn't save recipients.",
        }
    }

    pub(crate) fn creates_store(&self) -> bool {
        matches!(self, Self::Create)
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct StoreRecipientsRequest {
    pub(crate) store: String,
    pub(crate) mode: StoreRecipientsMode,
}

impl StoreRecipientsRequest {
    pub(crate) fn create(store: impl Into<String>) -> Self {
        Self {
            store: store.into(),
            mode: StoreRecipientsMode::Create,
        }
    }

    pub(crate) fn edit(store: impl Into<String>) -> Self {
        Self {
            store: store.into(),
            mode: StoreRecipientsMode::Edit,
        }
    }
}

#[derive(Clone)]
pub(crate) struct StoreRecipientsPageState {
    pub(crate) window: ApplicationWindow,
    pub(crate) nav: NavigationView,
    pub(crate) page: NavigationPage,
    pub(crate) list: ListBox,
    pub(crate) platform: StoreRecipientsPlatformState,
    pub(crate) back: Button,
    pub(crate) add: Button,
    pub(crate) find: Button,
    pub(crate) git: Button,
    pub(crate) save: Button,
    pub(crate) win: WindowTitle,
    pub(crate) request: Rc<RefCell<Option<StoreRecipientsRequest>>>,
    pub(crate) recipients: Rc<RefCell<Vec<String>>>,
    pub(crate) saved_recipients: Rc<RefCell<Vec<String>>>,
    pub(crate) save_in_flight: Rc<Cell<bool>>,
    pub(crate) save_queued: Rc<Cell<bool>>,
}

impl StoreRecipientsPageState {
    pub(crate) fn current_request(&self) -> Option<StoreRecipientsRequest> {
        self.request.borrow().clone()
    }

    pub(crate) fn recipients_are_dirty(&self) -> bool {
        *self.recipients.borrow() != *self.saved_recipients.borrow()
    }
}

pub(crate) fn connect_store_recipients_entry(state: &StoreRecipientsPageState) {
    platform::connect_store_recipients_entry(state);
}

pub(crate) fn rebuild_store_recipients_list(state: &StoreRecipientsPageState) {
    platform::rebuild_store_recipients_list(state);
}

pub(crate) fn sync_store_recipients_page_header(state: &StoreRecipientsPageState) {
    let Some(request) = state.current_request() else {
        state.save.set_visible(false);
        set_save_button_for_password(&state.save);
        state.win.set_title("Recipients");
        state.win.set_subtitle(APP_WINDOW_TITLE);
        return;
    };

    let chrome = window_chrome(
        &state.back,
        &state.add,
        &state.find,
        &state.git,
        &state.save,
        &state.win,
    );
    show_secondary_page_chrome(&chrome, request.mode.page_title(), &request.store, false);
    state.page.set_title(request.mode.page_title());
}

fn show_store_recipients_page(
    state: &StoreRecipientsPageState,
    request: StoreRecipientsRequest,
    initial_recipients: Vec<String>,
) {
    let saved_recipients = read_store_gpg_recipients(&request.store);
    let mode = request.mode;
    *state.request.borrow_mut() = Some(request);
    *state.recipients.borrow_mut() = initial_recipients;
    *state.saved_recipients.borrow_mut() = saved_recipients;
    state.save_in_flight.set(false);
    state.save_queued.set(false);
    platform::prepare_store_recipients_page(state);
    rebuild_store_recipients_list(state);
    sync_store_recipients_page_header(state);

    if !reveal_navigation_page(&state.nav, &state.page) {
        return;
    }

    if mode.creates_store() {
        queue_store_recipients_autosave(state);
    }
}

pub(crate) fn show_store_recipients_create_page(
    state: &StoreRecipientsPageState,
    store: impl Into<String>,
    initial_recipients: Vec<String>,
) {
    show_store_recipients_page(
        state,
        StoreRecipientsRequest::create(store),
        initial_recipients,
    );
}

pub(crate) fn show_store_recipients_edit_page(
    state: &StoreRecipientsPageState,
    store: impl Into<String>,
) {
    let store = store.into();
    show_store_recipients_page(
        state,
        StoreRecipientsRequest::edit(store.clone()),
        read_store_gpg_recipients(&store),
    );
}

#[cfg(test)]
mod tests {
    use super::StoreRecipientsMode;

    #[test]
    fn create_mode_has_create_title() {
        assert_eq!(StoreRecipientsMode::Create.page_title(), "New Store");
    }

    #[test]
    fn mode_messages_match_their_behavior() {
        #[cfg(not(feature = "flatpak"))]
        assert_eq!(
            StoreRecipientsMode::Create.empty_state_subtitle(),
            "Add at least one recipient to create this store."
        );
        assert_eq!(
            StoreRecipientsMode::Edit.save_failure_message(),
            "Couldn't save recipients."
        );
    }
}
