mod clone;
mod dialogs;
mod import;

use self::clone::append_store_clone_row;
pub use self::clone::prompt_store_clone;
pub use self::import::{
    initialize_store_import_page, schedule_store_import_row, StoreImportPageState,
};
use super::recipients::{
    read_store_gpg_recipients, store_gpg_recipients_subtitle, suggested_gpg_recipients,
};
pub use super::recipients_page::{
    connect_store_recipients_controls, register_store_recipients_reload_action,
    register_store_recipients_save_action, show_store_recipients_create_page,
    show_store_recipients_edit_page, sync_store_recipients_page_header, StoreRecipientsPageState,
    StoreRecipientsPlatformState, StoreRecipientsRequest,
};
use crate::logging::log_error;
use crate::preferences::Preferences;
use crate::support::actions::register_window_action;
use crate::support::ui::{
    append_action_row_with_button, append_info_row, clear_list_box, dim_label_icon,
    flat_icon_button,
};
use adw::gtk::{FileChooserAction, FileChooserNative, ListBox, ResponseType};
use adw::prelude::*;
use adw::{ActionRow, ApplicationWindow, Toast, ToastOverlay};
use std::fs;
use std::io;
use std::path::Path;
use std::rc::Rc;

fn updated_stores_after_add(stores: &[String], new_store: &str) -> Option<Vec<String>> {
    if stores.iter().any(|store| store == new_store) {
        return None;
    }

    let mut updated = stores.to_vec();
    updated.push(new_store.to_string());
    Some(updated)
}

fn updated_stores_after_delete(stores: &[String], store_to_remove: &str) -> Option<Vec<String>> {
    let position = stores.iter().position(|store| store == store_to_remove)?;
    let mut updated = stores.to_vec();
    updated.remove(position);
    Some(updated)
}

fn initial_recipients_for_store_creation(
    existing_recipients: Vec<String>,
    suggested_recipients: Vec<String>,
) -> Vec<String> {
    if existing_recipients.is_empty() {
        suggested_recipients
    } else {
        existing_recipients
    }
}

fn selected_local_folder(dialog: &FileChooserNative, overlay: &ToastOverlay) -> Option<String> {
    let file = dialog.file()?;
    let path = file.path().or_else(|| {
        log_error(
            "The selected folder is not available as a local path. Choose a local folder."
                .to_string(),
        );
        overlay.add_toast(Toast::new("Choose a local folder."));
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

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum SelectedStoreFolderMode {
    AddExisting,
    CreateNew,
}

const fn selected_store_folder_mode(is_empty: bool) -> SelectedStoreFolderMode {
    if is_empty {
        SelectedStoreFolderMode::CreateNew
    } else {
        SelectedStoreFolderMode::AddExisting
    }
}

fn folder_is_empty(path: &str) -> io::Result<bool> {
    let path = Path::new(path);
    if !path.exists() {
        return Ok(true);
    }

    let mut entries = fs::read_dir(path)?;
    Ok(entries.next().is_none())
}

pub fn rebuild_store_list(
    stores_list: &ListBox,
    actions_list: &ListBox,
    settings: &Preferences,
    window: &ApplicationWindow,
    overlay: &ToastOverlay,
    recipients_page: &StoreRecipientsPageState,
) {
    if let Err(err) = settings.prune_missing_stores() {
        log_error(format!("Failed to remove missing password stores: {err}"));
    }

    rebuild_stores_list(stores_list, settings, recipients_page);
    rebuild_store_actions_list(
        actions_list,
        stores_list,
        settings,
        window,
        overlay,
        recipients_page,
    );
}

pub fn rebuild_stores_list(
    stores_list: &ListBox,
    settings: &Preferences,
    recipients_page: &StoreRecipientsPageState,
) {
    clear_list_box(stores_list);

    let stores = settings.stores();
    if stores.is_empty() {
        append_empty_store_list_row(stores_list);
        return;
    }

    for store in &stores {
        append_store_row(stores_list, settings, store, recipients_page);
    }
}

fn append_empty_store_list_row(list: &ListBox) {
    let (title, subtitle) = empty_store_list_placeholder_copy();
    append_info_row(list, title, subtitle);
}

const fn empty_store_list_placeholder_copy() -> (&'static str, &'static str) {
    (
        "No password stores",
        "Add an existing folder or create a new store.",
    )
}

pub fn rebuild_store_actions_list(
    actions_list: &ListBox,
    stores_list: &ListBox,
    settings: &Preferences,
    window: &ApplicationWindow,
    overlay: &ToastOverlay,
    recipients_page: &StoreRecipientsPageState,
) {
    clear_list_box(actions_list);

    append_store_picker_row(
        actions_list,
        stores_list,
        settings,
        window,
        overlay,
        recipients_page,
    );
    append_store_clone_row(
        actions_list,
        stores_list,
        settings,
        window,
        overlay,
        recipients_page,
    );
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

    row.add_suffix(&dim_label_icon("go-next-symbolic"));

    let delete_button = flat_icon_button("window-close-symbolic");
    row.add_suffix(&delete_button);

    list.append(&row);

    let settings = settings.clone();
    let list = list.clone();
    let store = store.to_string();
    let recipients_page_for_edit = recipients_page.clone();
    let recipients_page_for_delete = recipients_page.clone();
    let store_for_edit = store.clone();

    row.connect_activated(move |_| {
        show_store_recipients_edit_page(&recipients_page_for_edit, &store_for_edit);
    });

    delete_button.connect_clicked(move |_| {
        if let Some(stores) = updated_stores_after_delete(&settings.stores(), &store) {
            if let Err(err) = settings.set_stores(stores) {
                log_error(format!("Failed to save stores: {err}"));
            } else {
                rebuild_stores_list(&list, &settings, &recipients_page_for_delete);
            }
        }
    });
}

fn append_store_picker_row(
    list: &ListBox,
    stores_list: &ListBox,
    settings: &Preferences,
    window: &ApplicationWindow,
    overlay: &ToastOverlay,
    recipients_page: &StoreRecipientsPageState,
) {
    let settings = settings.clone();
    let window = window.clone();
    let overlay = overlay.clone();
    let recipients_page = recipients_page.clone();
    let stores_list_for_action = stores_list.clone();
    append_action_row_with_button(
        list,
        "Add or create store",
        "Choose a folder. Empty folders become new stores.",
        "folder-new-symbolic",
        move || {
            prompt_add_or_create_store(
                &window,
                &stores_list_for_action,
                &settings,
                &overlay,
                &recipients_page,
            );
        },
    );
}

pub fn prompt_add_or_create_store(
    window: &ApplicationWindow,
    stores_list: &ListBox,
    settings: &Preferences,
    overlay: &ToastOverlay,
    recipients_page: &StoreRecipientsPageState,
) {
    let stores_list = stores_list.clone();
    let settings = settings.clone();
    let window = window.clone();
    let overlay = overlay.clone();
    let overlay_for_selection = overlay.clone();
    let recipients_page = recipients_page.clone();
    open_store_folder_picker(
        &window,
        "Choose password store folder",
        "Select",
        true,
        &overlay,
        move |store| {
            let mode = match folder_is_empty(&store) {
                Ok(is_empty) => selected_store_folder_mode(is_empty),
                Err(err) => {
                    log_error(format!("Failed to read password store folder: {err}"));
                    overlay_for_selection.add_toast(Toast::new("Couldn't read that folder."));
                    return;
                }
            };

            match mode {
                SelectedStoreFolderMode::AddExisting => {
                    if let Some(stores) = updated_stores_after_add(&settings.stores(), &store) {
                        if let Err(err) = settings.set_stores(stores) {
                            log_error(format!("Failed to save stores: {err}"));
                            overlay_for_selection
                                .add_toast(Toast::new("Couldn't add that folder."));
                            return;
                        }
                    }

                    rebuild_stores_list(&stores_list, &settings, &recipients_page);
                    show_store_recipients_edit_page(&recipients_page, &store);
                }
                SelectedStoreFolderMode::CreateNew => {
                    let recipients = initial_recipients_for_store_creation(
                        read_store_gpg_recipients(&store),
                        suggested_gpg_recipients(&settings),
                    );
                    show_store_recipients_create_page(&recipients_page, store, recipients);
                }
            }
        },
    );
}

pub fn register_open_store_picker_action(
    window: &ApplicationWindow,
    stores_list: &ListBox,
    overlay: &ToastOverlay,
    recipients_page: &StoreRecipientsPageState,
) {
    let action_window = window.clone();
    let prompt_window = action_window.clone();
    let stores_list = stores_list.clone();
    let overlay = overlay.clone();
    let recipients_page = recipients_page.clone();
    register_window_action(&action_window, "open-store-picker", move || {
        let settings = Preferences::new();
        prompt_add_or_create_store(
            &prompt_window,
            &stores_list,
            &settings,
            &overlay,
            &recipients_page,
        );
    });
}

#[cfg(test)]
mod tests {
    use super::{
        empty_store_list_placeholder_copy, initial_recipients_for_store_creation,
        selected_store_folder_mode, updated_stores_after_add, updated_stores_after_delete,
        SelectedStoreFolderMode,
    };

    #[test]
    fn adding_a_new_store_appends_it_once() {
        let stores = vec!["/tmp/one".to_string()];

        assert_eq!(
            updated_stores_after_add(&stores, "/tmp/two"),
            Some(vec!["/tmp/one".to_string(), "/tmp/two".to_string()])
        );
        assert_eq!(updated_stores_after_add(&stores, "/tmp/one"), None);
    }

    #[test]
    fn deleting_a_store_removes_only_the_requested_entry() {
        let stores = vec![
            "/tmp/one".to_string(),
            "/tmp/two".to_string(),
            "/tmp/three".to_string(),
        ];

        assert_eq!(
            updated_stores_after_delete(&stores, "/tmp/two"),
            Some(vec!["/tmp/one".to_string(), "/tmp/three".to_string()])
        );
        assert_eq!(updated_stores_after_delete(&stores, "/tmp/missing"), None);
    }

    #[test]
    fn store_creation_prefers_existing_recipients_over_suggested_ones() {
        assert_eq!(
            initial_recipients_for_store_creation(
                vec!["existing@example.com".to_string()],
                vec!["suggested@example.com".to_string()],
            ),
            vec!["existing@example.com".to_string()]
        );
        assert_eq!(
            initial_recipients_for_store_creation(
                Vec::new(),
                vec!["suggested@example.com".to_string()],
            ),
            vec!["suggested@example.com".to_string()]
        );
    }

    #[test]
    fn empty_selected_folders_start_store_creation_while_non_empty_ones_are_added() {
        assert_eq!(
            selected_store_folder_mode(true),
            SelectedStoreFolderMode::CreateNew
        );
        assert_eq!(
            selected_store_folder_mode(false),
            SelectedStoreFolderMode::AddExisting
        );
    }

    #[test]
    fn empty_store_list_has_placeholder_copy() {
        assert_eq!(
            empty_store_list_placeholder_copy(),
            (
                "No password stores",
                "Add an existing folder or create a new store."
            )
        );
    }
}
