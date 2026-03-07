use crate::background::spawn_result_task;
use crate::logging::log_error;
use crate::preferences::Preferences;
use crate::stores::{
    apply_password_store_recipients, read_store_gpg_recipients,
    store_gpg_recipients_subtitle, stores_with_preferred_first, suggested_gpg_recipients,
};
use crate::window_messages::with_logs_hint;
use crate::window_navigation::set_save_button_for_password;
use adw::gio::SimpleAction;
use adw::prelude::*;
use adw::{
    ActionRow, ApplicationWindow, EntryRow, NavigationPage, NavigationView, Toast, ToastOverlay,
    WindowTitle,
};
use adw::gtk::{Button, FileChooserAction, FileChooserNative, Image, ListBox, ResponseType};
use std::cell::{Cell, RefCell};
use std::rc::Rc;

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum StoreRecipientsMode {
    Create,
    Edit,
}

impl StoreRecipientsMode {
    pub(crate) fn page_title(&self) -> &'static str {
        match self {
            Self::Create => "Create Password Store",
            Self::Edit => "GPG Recipients",
        }
    }

    fn empty_state_subtitle(&self) -> &'static str {
        match self {
            Self::Create => "Add at least one recipient to initialize the password store.",
            Self::Edit => "Add at least one recipient before saving your changes.",
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

fn navigation_stack_contains_page(nav: &NavigationView, page: &NavigationPage) -> bool {
    let stack = nav.navigation_stack();
    let mut index = 0;
    let len = stack.n_items();
    while index < len {
        if let Some(item) = stack.item(index) {
            if let Ok(stack_page) = item.downcast::<NavigationPage>() {
                if stack_page == *page {
                    return true;
                }
            }
        }
        index += 1;
    }
    false
}

pub(crate) fn current_store_recipients_request(
    state: &StoreRecipientsPageState,
) -> Option<StoreRecipientsRequest> {
    state.request.borrow().clone()
}

pub(crate) fn store_recipients_are_dirty(state: &StoreRecipientsPageState) -> bool {
    *state.recipients.borrow() != *state.saved_recipients.borrow()
}

fn clear_list(list: &ListBox) {
    while let Some(child) = list.first_child() {
        list.remove(&child);
    }
}

fn can_autosave_store_recipients(state: &StoreRecipientsPageState) -> bool {
    current_store_recipients_request(state).is_some()
        && !state.recipients.borrow().is_empty()
        && store_recipients_are_dirty(state)
}

fn connect_row_and_button_action(row: &ActionRow, button: &Button, action: impl Fn() + 'static) {
    let action = Rc::new(action);

    let row_action = action.clone();
    row.connect_activated(move |_| row_action());

    let button_action = action.clone();
    button.connect_clicked(move |_| button_action());
}

fn selected_local_folder(dialog: &FileChooserNative, overlay: &ToastOverlay) -> Option<String> {
    let file = dialog.file()?;
    let path = file.path().or_else(|| {
        log_error(
            "The selected folder is not available as a local path. Choose a local folder."
                .to_string(),
        );
        overlay.add_toast(Toast::new("Choose a local password store folder."));
        None
    })?;

    Some(path.to_string_lossy().to_string())
}

fn open_store_folder_picker(
    window: &ApplicationWindow,
    title: &str,
    accept_label: &str,
    create_folders: bool,
    overlay: &ToastOverlay,
    on_selected: impl Fn(String) + 'static,
) {
    let dialog = FileChooserNative::new(
        Some(title),
        Some(window),
        FileChooserAction::SelectFolder,
        Some(accept_label),
        Some("Cancel"),
    );
    dialog.set_create_folders(create_folders);

    let overlay = overlay.clone();
    let on_selected = Rc::new(on_selected);
    dialog.connect_response(move |dialog, response| {
        if response == ResponseType::Accept {
            if let Some(store) = selected_local_folder(dialog, &overlay) {
                on_selected(store);
            }
        }

        dialog.hide();
    });

    dialog.show();
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

fn finish_store_recipients_save(state: &StoreRecipientsPageState, include_dirty: bool) {
    state.save_in_flight.set(false);
    if state.save_queued.get() || (include_dirty && store_recipients_are_dirty(state)) {
        state.save_queued.set(false);
        queue_store_recipients_autosave(state);
    }
}

pub(crate) fn save_store_recipients_async(
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
                                "Password store created, but it couldn't be added to Preferences.",
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
                let message = if request.mode == StoreRecipientsMode::Create {
                    with_logs_hint("Couldn't create the password store.")
                } else {
                    message
                };
                finish_store_recipients_save(&state, false);
                overlay.add_toast(Toast::new(&message));
            }
        },
        move || {
            let message = if request_for_disconnect.mode == StoreRecipientsMode::Create {
                with_logs_hint("Couldn't create the password store.")
            } else {
                with_logs_hint("Couldn't save the password store recipients.")
            };
            finish_store_recipients_save(&state_for_disconnect, false);
            overlay_for_disconnect.add_toast(Toast::new(&message));
        },
    );
}

pub(crate) fn rebuild_store_recipients_list(state: &StoreRecipientsPageState) {
    clear_list(&state.list);

    state.list.append(&state.entry);

    let empty_subtitle = current_store_recipients_request(state)
        .map(|request| request.mode.empty_state_subtitle())
        .unwrap_or("Add at least one recipient before saving.");

    if state.recipients.borrow().is_empty() {
        let empty_row = ActionRow::builder()
            .title("No GPG recipients added")
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
        state.win.set_title("GPG Recipients");
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

pub(crate) fn rebuild_store_list(
    list: &ListBox,
    settings: &Preferences,
    window: &ApplicationWindow,
    overlay: &ToastOverlay,
    recipients_page: &StoreRecipientsPageState,
) {
    clear_list(list);

    if let Err(err) = settings.prune_missing_stores() {
        log_error(format!("Failed to remove missing password stores: {err}"));
    }

    for store in settings.stores() {
        append_store_row(list, settings, &store, recipients_page);
    }

    append_store_picker_row(list, settings, window, overlay, recipients_page);
    append_store_creator_row(list, settings, window, overlay, recipients_page);
}

fn append_store_row(
    list: &ListBox,
    settings: &Preferences,
    store: &str,
    recipients_page: &StoreRecipientsPageState,
) {
    let row = ActionRow::builder()
        .title(store)
        .subtitle(store_gpg_recipients_subtitle(store))
        .build();
    row.set_activatable(true);

    let delete_btn = Button::from_icon_name("user-trash-symbolic");
    delete_btn.add_css_class("flat");
    row.add_suffix(&delete_btn);

    list.append(&row);

    let settings = settings.clone();
    let list = list.clone();
    let row_clone = row.clone();
    let store = store.to_string();
    let recipients_page = recipients_page.clone();
    let edit_store = store.clone();

    row.connect_activated(move |_| {
        show_store_recipients_page(
            &recipients_page,
            StoreRecipientsRequest {
                store: edit_store.clone(),
                mode: StoreRecipientsMode::Edit,
            },
            read_store_gpg_recipients(&edit_store),
        );
    });

    delete_btn.connect_clicked(move |_| {
        let mut stores = settings.stores();
        if let Some(pos) = stores.iter().position(|s| s == &store) {
            stores.remove(pos);
            if let Err(err) = settings.set_stores(stores) {
                log_error(format!("Failed to save stores: {err}"));
            } else {
                list.remove(&row_clone);
            }
        }
    });
}

fn append_store_picker_row(
    list: &ListBox,
    settings: &Preferences,
    window: &ApplicationWindow,
    overlay: &ToastOverlay,
    recipients_page: &StoreRecipientsPageState,
) {
    let row = ActionRow::builder()
        .title("Add password store folder")
        .subtitle("Choose a folder with the system file chooser.")
        .build();
    row.set_activatable(true);

    let button = Button::from_icon_name("folder-open-symbolic");
    button.add_css_class("flat");
    row.add_suffix(&button);
    list.append(&row);

    let settings = settings.clone();
    let list = list.clone();
    let window = window.clone();
    let overlay = overlay.clone();
    let recipients_page = recipients_page.clone();
    connect_row_and_button_action(&row, &button, move || {
        open_store_picker(&window, &list, &settings, &overlay, &recipients_page);
    });
}

fn open_store_picker(
    window: &ApplicationWindow,
    list: &ListBox,
    settings: &Preferences,
    overlay: &ToastOverlay,
    recipients_page: &StoreRecipientsPageState,
) {
    let list = list.clone();
    let settings = settings.clone();
    let window = window.clone();
    let overlay = overlay.clone();
    let window_for_selection = window.clone();
    let overlay_for_selection = overlay.clone();
    let recipients_page = recipients_page.clone();
    open_store_folder_picker(
        &window,
        "Choose password store folder",
        "Select",
        false,
        &overlay,
        move |store| {
            let mut stores = settings.stores();
            if !stores.contains(&store) {
                stores.push(store.clone());
                if let Err(err) = settings.set_stores(stores) {
                    log_error(format!("Failed to save stores: {err}"));
                    overlay_for_selection
                        .add_toast(Toast::new("Couldn't add the password store folder."));
                } else {
                    rebuild_store_list(
                        &list,
                        &settings,
                        &window_for_selection,
                        &overlay_for_selection,
                        &recipients_page,
                    );
                }
            }
        },
    );
}

fn append_store_creator_row(
    list: &ListBox,
    settings: &Preferences,
    window: &ApplicationWindow,
    overlay: &ToastOverlay,
    recipients_page: &StoreRecipientsPageState,
) {
    let row = ActionRow::builder()
        .title("Create password store")
        .subtitle("Choose a folder and initialize it with GPG recipients.")
        .build();
    row.set_activatable(true);

    let button = Button::from_icon_name("folder-new-symbolic");
    button.add_css_class("flat");
    row.add_suffix(&button);
    list.append(&row);

    let settings = settings.clone();
    let window = window.clone();
    let overlay = overlay.clone();
    let recipients_page = recipients_page.clone();
    connect_row_and_button_action(&row, &button, move || {
        open_store_creator_picker(&window, &settings, &overlay, &recipients_page);
    });
}

fn open_store_creator_picker(
    window: &ApplicationWindow,
    settings: &Preferences,
    overlay: &ToastOverlay,
    recipients_page: &StoreRecipientsPageState,
) {
    let settings = settings.clone();
    let overlay = overlay.clone();
    let recipients_page = recipients_page.clone();
    open_store_folder_picker(
        window,
        "Choose new password store folder",
        "Select",
        true,
        &overlay,
        move |store| {
            let mut recipients = read_store_gpg_recipients(&store);
            if recipients.is_empty() {
                recipients = suggested_gpg_recipients(&settings);
            }
            show_store_recipients_page(
                &recipients_page,
                StoreRecipientsRequest {
                    store,
                    mode: StoreRecipientsMode::Create,
                },
                recipients,
            );
        },
    );
}

#[cfg(test)]
mod tests {
    use super::StoreRecipientsMode;

    #[test]
    fn create_mode_has_create_title() {
        assert_eq!(StoreRecipientsMode::Create.page_title(), "Create Password Store");
    }

    #[test]
    fn edit_mode_has_edit_empty_state_copy() {
        assert_eq!(
            StoreRecipientsMode::Edit.empty_state_subtitle(),
            "Add at least one recipient before saving your changes."
        );
    }
}
