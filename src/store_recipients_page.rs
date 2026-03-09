use crate::background::spawn_result_task;
#[cfg(feature = "flatpak")]
use crate::backend::{
    import_ripasso_private_key_bytes, is_ripasso_private_key_unlocked, list_ripasso_private_keys,
    remove_ripasso_private_key, ripasso_private_key_requires_passphrase,
    ripasso_private_key_requires_session_unlock, ManagedRipassoPrivateKey,
};
use crate::logging::log_error;
#[cfg(feature = "flatpak")]
use crate::private_key_dialog::{
    build_private_key_progress_dialog, present_private_key_password_dialog,
};
use crate::preferences::Preferences;
#[cfg(not(feature = "flatpak"))]
use crate::stores::append_gpg_recipients;
use crate::stores::{
    apply_password_store_recipients, read_store_gpg_recipients, stores_with_preferred_first,
};
use crate::store_management::rebuild_store_list;
#[cfg(feature = "flatpak")]
use crate::ripasso_unlock::prompt_private_key_unlock_for_action;
use crate::ui_helpers::{clear_list_box, navigation_stack_contains_page};
use crate::window_messages::with_logs_hint;
use crate::window_navigation::set_save_button_for_password;
#[cfg(feature = "flatpak")]
use adw::gio;
use adw::gio::SimpleAction;
use adw::prelude::*;
use adw::{
    ActionRow, ApplicationWindow, NavigationPage, NavigationView, Toast, ToastOverlay,
    WindowTitle,
};
#[cfg(not(feature = "flatpak"))]
use adw::EntryRow;
use adw::gtk::{Button, Image, ListBox};
#[cfg(feature = "flatpak")]
use adw::gtk::CheckButton;
#[cfg(feature = "flatpak")]
use adw::gtk::{FileChooserAction, FileChooserNative, ResponseType};
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

#[cfg(feature = "flatpak")]
fn inspect_private_key_lock_state(fingerprint: &str) -> (bool, bool) {
    let unlocked = match is_ripasso_private_key_unlocked(fingerprint) {
        Ok(unlocked) => unlocked,
        Err(err) => {
            log_error(format!(
                "Failed to inspect whether private key '{fingerprint}' is unlocked: {err}"
            ));
            false
        }
    };
    let requires_unlock = match ripasso_private_key_requires_session_unlock(fingerprint) {
        Ok(requires_unlock) => requires_unlock,
        Err(err) => {
            log_error(format!(
                "Failed to inspect whether private key '{fingerprint}' requires unlocking: {err}"
            ));
            false
        }
    };

    (unlocked, requires_unlock)
}

#[cfg(feature = "flatpak")]
fn finish_private_key_import(
    state: &StoreRecipientsPageState,
    result: Result<ManagedRipassoPrivateKey, String>,
) {
    match result {
        Ok(_) => {
            rebuild_store_recipients_list(state);
            state.overlay.add_toast(Toast::new("Key imported."));
        }
        Err(err) => {
            log_error(format!("Failed to import private key: {err}"));
            let message = if err.contains("does not include a private key") {
                "That file does not contain a private key."
            } else if err.contains("must be password protected") {
                "Add a password to that key first."
            } else if err.contains("cannot decrypt password store entries") {
                "This key can't open your items."
            } else if err.contains("password protected") || err.contains("incorrect") {
                "Couldn't unlock the key."
            } else {
                "Couldn't import the key."
            };
            state.overlay.add_toast(Toast::new(message));
        }
    }
}

#[cfg(feature = "flatpak")]
fn start_private_key_import(
    state: &StoreRecipientsPageState,
    bytes: Vec<u8>,
    passphrase: Option<String>,
) {
    let progress_dialog =
        build_private_key_progress_dialog(&state.window, "Importing key", None, "Please wait.");
    let state = state.clone();
    let progress_dialog_for_disconnect = progress_dialog.clone();
    let state_for_disconnect = state.clone();
    spawn_result_task(
        move || import_ripasso_private_key_bytes(&bytes, passphrase.as_deref()),
        move |result| {
            progress_dialog.force_close();
            finish_private_key_import(&state, result);
        },
        move || {
            progress_dialog_for_disconnect.force_close();
            log_error("Private key import worker disconnected unexpectedly.".to_string());
            state_for_disconnect
                .overlay
                .add_toast(Toast::new("Couldn't import the key."));
        },
    );
}

#[cfg(feature = "flatpak")]
fn prompt_private_key_passphrase(state: &StoreRecipientsPageState, bytes: Vec<u8>) {
    let bytes = Rc::new(bytes);
    let window = state.window.clone();
    let overlay = state.overlay.clone();
    let state = state.clone();
    present_private_key_password_dialog(&window, &overlay, "Unlock key", None, move |passphrase| {
        start_private_key_import(&state, bytes.as_slice().to_vec(), Some(passphrase));
    });
}

#[cfg(feature = "flatpak")]
fn open_private_key_picker(state: &StoreRecipientsPageState) {
    let dialog = FileChooserNative::new(
        Some("Import private key"),
        Some(&state.window),
        FileChooserAction::Open,
        Some("Import"),
        Some("Cancel"),
    );
    let state_for_response = state.clone();
    dialog.connect_response(move |dialog, response| {
        if response != ResponseType::Accept {
            dialog.hide();
            return;
        }

        let Some(file) = dialog.file() else {
            dialog.hide();
            return;
        };

        match file.load_bytes(None::<&gio::Cancellable>) {
            Ok((bytes, _)) => {
                let bytes = bytes.as_ref().to_vec();
                match ripasso_private_key_requires_passphrase(&bytes) {
                    Ok(true) => prompt_private_key_passphrase(&state_for_response, bytes),
                    Ok(false) => start_private_key_import(&state_for_response, bytes, None),
                    Err(err) => {
                        log_error(format!("Failed to inspect private key: {err}"));
                        let message = if err.contains("does not include a private key") {
                            "That file does not contain a private key."
                        } else {
                            "Couldn't read that key."
                        };
                        state_for_response.overlay.add_toast(Toast::new(message));
                    }
                }
            }
            Err(err) => {
                log_error(format!("Failed to read the selected private key file: {err}"));
                state_for_response
                    .overlay
                    .add_toast(Toast::new("Couldn't read that file."));
            }
        }

        dialog.hide();
    });

    dialog.show();
}

#[cfg(feature = "flatpak")]
fn append_private_key_import_row(state: &StoreRecipientsPageState) {
    let row = ActionRow::builder()
        .title("Import private key")
        .subtitle("Choose a private key file.")
        .build();
    row.set_activatable(true);

    let button = Button::from_icon_name("document-open-symbolic");
    button.add_css_class("flat");
    row.add_suffix(&button);
    state.list.append(&row);

    let row_state = state.clone();
    row.connect_activated(move |_| open_private_key_picker(&row_state));

    let button_state = state.clone();
    button.connect_clicked(move |_| open_private_key_picker(&button_state));
}

#[cfg(feature = "flatpak")]
fn recipient_matches_private_key(recipient: &str, key: &ManagedRipassoPrivateKey) -> bool {
    let recipient = recipient.trim();
    recipient.eq_ignore_ascii_case(&key.fingerprint)
        || key
            .user_ids
            .iter()
            .any(|user_id| user_id.eq_ignore_ascii_case(recipient))
}

#[cfg(feature = "flatpak")]
fn set_private_key_recipient_enabled(
    state: &StoreRecipientsPageState,
    key: &ManagedRipassoPrivateKey,
    enabled: bool,
) -> bool {
    let mut recipients = state.recipients.borrow_mut();
    let before = recipients.clone();
    recipients.retain(|value| !recipient_matches_private_key(value, key));
    if enabled {
        recipients.push(key.fingerprint.clone());
    }
    *recipients != before
}

#[cfg(feature = "flatpak")]
fn append_flatpak_private_key_rows(state: &StoreRecipientsPageState) {
    let keys = match list_ripasso_private_keys() {
        Ok(keys) => keys,
        Err(err) => {
            log_error(format!("Failed to load private keys for recipients: {err}"));
            let row = ActionRow::builder()
                .title("Couldn't load private keys")
                .subtitle("Try again from Preferences.")
                .build();
            row.set_activatable(false);
            state.list.append(&row);
            append_private_key_import_row(state);
            return;
        }
    };

    if keys.is_empty() {
        let row = ActionRow::builder()
            .title("No private keys yet")
            .subtitle("Import a private key first.")
            .build();
        row.set_activatable(false);
        state.list.append(&row);
        append_private_key_import_row(state);
        return;
    }

    for key in keys {
        let active = state
            .recipients
            .borrow()
            .iter()
            .any(|recipient| recipient_matches_private_key(recipient, &key));
        let title = adw::glib::markup_escape_text(&key.title());
        let row = ActionRow::builder()
            .title(title.as_str())
            .subtitle(&key.fingerprint)
            .build();
        row.set_activatable(true);

        let key_icon = Image::from_icon_name("dialog-password-symbolic");
        key_icon.add_css_class("dim-label");
        row.add_prefix(&key_icon);

        let (unlocked, requires_unlock) = inspect_private_key_lock_state(&key.fingerprint);
        let toggle = CheckButton::new();
        toggle.set_active(active);
        row.add_suffix(&toggle);

        if requires_unlock {
            let unlock_button = Button::from_icon_name("system-lock-screen-symbolic");
            unlock_button.add_css_class("flat");
            unlock_button.set_tooltip_text(Some("Unlock key"));
            row.add_suffix(&unlock_button);

            let unlock_state = state.clone();
            let fingerprint = key.fingerprint.clone();
            unlock_button.connect_clicked(move |_| {
                let refresh_state = unlock_state.clone();
                prompt_private_key_unlock_for_action(
                    &unlock_state.overlay,
                    fingerprint.clone(),
                    Rc::new(move || rebuild_store_recipients_list(&refresh_state)),
                );
            });
        } else if unlocked {
            let unlocked_icon = Image::from_icon_name("changes-allow-symbolic");
            unlocked_icon.add_css_class("accent");
            row.add_suffix(&unlocked_icon);
        }

        let delete_button = Button::from_icon_name("user-trash-symbolic");
        delete_button.add_css_class("flat");
        delete_button.set_tooltip_text(Some("Remove key"));
        row.add_suffix(&delete_button);
        state.list.append(&row);

        let toggle_for_row = toggle.clone();
        row.connect_activated(move |_| {
            toggle_for_row.set_active(!toggle_for_row.is_active());
        });

        let page_state = state.clone();
        let key_for_toggle = key.clone();
        toggle.connect_toggled(move |button| {
            if set_private_key_recipient_enabled(&page_state, &key_for_toggle, button.is_active()) {
                queue_store_recipients_autosave(&page_state);
            }
        });

        let page_state = state.clone();
        let key_for_delete = key.clone();
        delete_button.connect_clicked(move |_| {
            if let Err(err) = remove_ripasso_private_key(&key_for_delete.fingerprint) {
                log_error(format!(
                    "Failed to remove private key '{}': {err}",
                    key_for_delete.fingerprint
                ));
                page_state
                    .overlay
                    .add_toast(Toast::new("Couldn't remove that key."));
                return;
            }

            if Preferences::new().ripasso_own_fingerprint().as_deref()
                == Some(key_for_delete.fingerprint.as_str())
            {
                let _ = Preferences::new().set_ripasso_own_fingerprint(None);
            }
            let recipients_changed =
                set_private_key_recipient_enabled(&page_state, &key_for_delete, false);
            rebuild_store_recipients_list(&page_state);
            if recipients_changed {
                queue_store_recipients_autosave(&page_state);
            }
        });
    }

    append_private_key_import_row(state);
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
    clear_list_box(&state.list);
    append_flatpak_private_key_rows(state);
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
    #[cfg(feature = "flatpak")]
    use super::recipient_matches_private_key;
    #[cfg(feature = "flatpak")]
    use crate::backend::ManagedRipassoPrivateKey;

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

    #[cfg(feature = "flatpak")]
    #[test]
    fn imported_private_keys_match_existing_user_id_recipients() {
        let key = ManagedRipassoPrivateKey {
            fingerprint: "10F4487A3768155709168A8E3D00743E10EA9232".to_string(),
            user_ids: vec!["pass@store.local".to_string()],
        };

        assert!(recipient_matches_private_key("pass@store.local", &key));
        assert!(recipient_matches_private_key(
            "10F4487A3768155709168A8E3D00743E10EA9232",
            &key
        ));
        assert!(!recipient_matches_private_key("other@example.com", &key));
    }
}
