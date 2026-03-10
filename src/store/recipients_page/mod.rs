use crate::support::background::spawn_result_task;
use crate::logging::log_error;
use crate::preferences::Preferences;
#[cfg(not(feature = "flatpak"))]
use super::recipients::append_gpg_recipients;
use super::recipients::{
    apply_password_store_recipients, read_store_gpg_recipients, stores_with_preferred_first,
};
use super::management::rebuild_store_list;
#[cfg(not(feature = "flatpak"))]
use crate::support::ui::clear_list_box;
use crate::support::ui::navigation_stack_contains_page;
use crate::window::messages::with_logs_hint;
use crate::window::navigation::set_save_button_for_password;
use adw::gio::SimpleAction;
use adw::prelude::*;
#[cfg(not(feature = "flatpak"))]
use adw::ActionRow;
use adw::{ApplicationWindow, NavigationPage, NavigationView, Toast, ToastOverlay, WindowTitle};
#[cfg(not(feature = "flatpak"))]
use adw::EntryRow;
use adw::gtk::{Button, ListBox};
#[cfg(not(feature = "flatpak"))]
use adw::gtk::Image;
use std::cell::{Cell, RefCell};
use std::rc::Rc;

#[cfg(feature = "flatpak")]
mod flatpak;

#[derive(Clone, Debug, PartialEq, Eq)]
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

    #[cfg_attr(feature = "flatpak", allow(dead_code))]
    fn empty_state_subtitle(&self) -> &'static str {
        match self {
            Self::Create => "Add at least one recipient to create this store.",
            Self::Edit => "Add at least one recipient to keep saving changes.",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct StoreRecipientsRequest {
    pub(crate) store: String,
    pub(crate) mode: StoreRecipientsMode,
}

#[derive(Clone)]
pub(crate) struct StoreRecipientsPageState {
    pub(crate) window: ApplicationWindow,
    pub(crate) nav: NavigationView,
    pub(crate) page: NavigationPage,
    pub(crate) list: ListBox,
    #[cfg(feature = "flatpak")]
    pub(crate) overlay: ToastOverlay,
    #[cfg(not(feature = "flatpak"))]
    pub(crate) entry: EntryRow,
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

fn current_store_recipients_request(
    state: &StoreRecipientsPageState,
) -> Option<StoreRecipientsRequest> {
    state.request.borrow().clone()
}

fn store_recipients_are_dirty(state: &StoreRecipientsPageState) -> bool {
    *state.recipients.borrow() != *state.saved_recipients.borrow()
}

fn can_autosave_store_recipients(state: &StoreRecipientsPageState) -> bool {
    current_store_recipients_request(state).is_some()
        && !state.recipients.borrow().is_empty()
        && store_recipients_are_dirty(state)
}

fn finish_store_recipients_save(state: &StoreRecipientsPageState, include_dirty: bool) {
    state.save_in_flight.set(false);
    if state.save_queued.get() || (include_dirty && store_recipients_are_dirty(state)) {
        state.save_queued.set(false);
        queue_store_recipients_autosave(state);
    }
}

fn save_store_recipients_async(
    overlay: &ToastOverlay,
    stores_list: &ListBox,
    state: &StoreRecipientsPageState,
) {
    let Some(request) = current_store_recipients_request(state) else {
        return;
    };

    let recipients = state.recipients.borrow().clone();
    if recipients.is_empty() {
        return;
    }
    if !store_recipients_are_dirty(state) {
        state.save_queued.set(false);
        return;
    }
    if state.save_in_flight.replace(true) {
        state.save_queued.set(true);
        return;
    }
    state.save_queued.set(false);

    let store_for_thread = request.store.clone();
    let recipients_for_save = recipients.clone();
    let overlay = overlay.clone();
    let stores_list = stores_list.clone();
    let state = state.clone();
    let request = request.clone();
    let overlay_for_disconnect = overlay.clone();
    let state_for_disconnect = state.clone();
    let request_for_disconnect = request.clone();
    spawn_result_task(
        move || apply_password_store_recipients(&store_for_thread, &recipients_for_save),
        move |result| match result {
            Ok(()) => {
                let settings = Preferences::new();
                *state.saved_recipients.borrow_mut() = recipients.clone();
                match request.mode {
                    StoreRecipientsMode::Create => {
                        let stores =
                            stores_with_preferred_first(&settings.stores(), &request.store);
                        if let Err(err) = settings.set_stores(stores) {
                            log_error(format!("Failed to save stores: {err}"));
                            overlay.add_toast(Toast::new(
                                "Store created, but it wasn't added.",
                            ));
                        } else {
                            rebuild_store_list(
                                &stores_list,
                                &settings,
                                &state.window,
                                &overlay,
                                &state,
                            );
                            *state.request.borrow_mut() = Some(StoreRecipientsRequest {
                                store: request.store.clone(),
                                mode: StoreRecipientsMode::Edit,
                            });
                            sync_store_recipients_page_header(&state);
                        }
                    }
                    StoreRecipientsMode::Edit => {
                        rebuild_store_list(&stores_list, &settings, &state.window, &overlay, &state);
                    }
                }
                finish_store_recipients_save(&state, true);
            }
            Err(message) => {
                log_error(format!(
                    "Failed to save store recipients for '{}': {message}",
                    request.store
                ));
                let message = if request.mode == StoreRecipientsMode::Create {
                    with_logs_hint("Couldn't create the store.")
                } else {
                    with_logs_hint("Couldn't save recipients.")
                };
                finish_store_recipients_save(&state, false);
                overlay.add_toast(Toast::new(&message));
            }
        },
        move || {
            let message = if request_for_disconnect.mode == StoreRecipientsMode::Create {
                with_logs_hint("Couldn't create the store.")
            } else {
                with_logs_hint("Couldn't save recipients.")
            };
            finish_store_recipients_save(&state_for_disconnect, false);
            overlay_for_disconnect.add_toast(Toast::new(&message));
        },
    );
}

pub(crate) fn connect_store_recipients_entry(state: &StoreRecipientsPageState) {
    #[cfg(feature = "flatpak")]
    {
        let _ = state;
        return;
    }

    #[cfg(not(feature = "flatpak"))]
    let page_state = state.clone();
    #[cfg(not(feature = "flatpak"))]
    state.entry.connect_apply(move |entry| {
        if append_gpg_recipients(&page_state.recipients, entry.text().as_str()) {
            entry.set_text("");
            rebuild_store_recipients_list(&page_state);
            queue_store_recipients_autosave(&page_state);
        }
    });
}

pub(crate) fn queue_store_recipients_autosave(state: &StoreRecipientsPageState) {
    if !can_autosave_store_recipients(state) {
        return;
    }
    if state.save_in_flight.get() {
        state.save_queued.set(true);
        return;
    }

    let _ =
        adw::prelude::WidgetExt::activate_action(&state.window, "win.save-store-recipients", None);
}

#[cfg(not(feature = "flatpak"))]
pub(crate) fn rebuild_store_recipients_list(state: &StoreRecipientsPageState) {
    clear_list_box(&state.list);
    state.list.append(&state.entry);

    let empty_subtitle = current_store_recipients_request(state)
        .map(|request| request.mode.empty_state_subtitle())
        .unwrap_or("Add at least one recipient before saving.");

    if state.recipients.borrow().is_empty() {
        let empty_row = ActionRow::builder()
            .title("No recipients yet")
            .subtitle(empty_subtitle)
            .build();
        empty_row.set_activatable(false);
        state.list.append(&empty_row);
        return;
    }

    for recipient in state.recipients.borrow().iter().cloned() {
        let row = ActionRow::builder().title(&recipient).build();
        row.set_activatable(false);
        let row_icon = Image::from_icon_name("dialog-password-symbolic");
        row_icon.add_css_class("dim-label");
        row.add_prefix(&row_icon);

        let delete_button = Button::from_icon_name("user-trash-symbolic");
        delete_button.add_css_class("flat");
        row.add_suffix(&delete_button);
        state.list.append(&row);

        let page_state = state.clone();
        delete_button.connect_clicked(move |_| {
            page_state
                .recipients
                .borrow_mut()
                .retain(|value| value != &recipient);
            rebuild_store_recipients_list(&page_state);
            queue_store_recipients_autosave(&page_state);
        });
    }
}

#[cfg(feature = "flatpak")]
pub(crate) fn rebuild_store_recipients_list(state: &StoreRecipientsPageState) {
    flatpak::rebuild_store_recipients_list(state);
}

pub(crate) fn register_store_recipients_save_action(
    window: &ApplicationWindow,
    overlay: &ToastOverlay,
    stores_list: &ListBox,
    state: &StoreRecipientsPageState,
) {
    let overlay = overlay.clone();
    let stores_list = stores_list.clone();
    let state = state.clone();
    let action = SimpleAction::new("save-store-recipients", None);
    action.connect_activate(move |_, _| {
        save_store_recipients_async(&overlay, &stores_list, &state);
    });
    window.add_action(&action);
}

pub(crate) fn sync_store_recipients_page_header(state: &StoreRecipientsPageState) {
    let Some(request) = current_store_recipients_request(state) else {
        state.save.set_visible(false);
        set_save_button_for_password(&state.save);
        state.win.set_title("Recipients");
        state.win.set_subtitle("Password Store");
        return;
    };

    state.add.set_visible(false);
    state.find.set_visible(false);
    state.git.set_visible(false);
    state.back.set_visible(true);
    state.save.set_visible(false);
    set_save_button_for_password(&state.save);
    state.page.set_title(request.mode.page_title());
    state.win.set_title(request.mode.page_title());
    state.win.set_subtitle(&request.store);
}

pub(crate) fn show_store_recipients_page(
    state: &StoreRecipientsPageState,
    request: StoreRecipientsRequest,
    initial_recipients: Vec<String>,
) {
    let saved_recipients = read_store_gpg_recipients(&request.store);
    *state.request.borrow_mut() = Some(request);
    *state.recipients.borrow_mut() = initial_recipients;
    *state.saved_recipients.borrow_mut() = saved_recipients;
    state.save_in_flight.set(false);
    state.save_queued.set(false);
    #[cfg(not(feature = "flatpak"))]
    state.entry.set_text("");
    rebuild_store_recipients_list(state);
    sync_store_recipients_page_header(state);

    let already_visible = state
        .nav
        .visible_page()
        .as_ref()
        .map(|visible| visible == &state.page)
        .unwrap_or(false);
    if already_visible {
        return;
    }

    if navigation_stack_contains_page(&state.nav, &state.page) {
        let _ = state.nav.pop_to_page(&state.page);
    } else {
        state.nav.push(&state.page);
    }

    if current_store_recipients_request(state)
        .map(|request| request.mode == StoreRecipientsMode::Create)
        .unwrap_or(false)
    {
        queue_store_recipients_autosave(state);
    }
}

#[cfg(test)]
mod tests {
    use super::StoreRecipientsMode;

    #[test]
    fn create_mode_has_create_title() {
        assert_eq!(StoreRecipientsMode::Create.page_title(), "New Store");
    }

    #[test]
    fn edit_mode_has_edit_empty_state_copy() {
        assert_eq!(
            StoreRecipientsMode::Edit.empty_state_subtitle(),
            "Add at least one recipient to keep saving changes."
        );
    }
}
