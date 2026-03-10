use super::recipients::{
    read_store_gpg_recipients, store_gpg_recipients_subtitle, suggested_gpg_recipients,
};
pub(crate) use super::recipients_page::{
    connect_store_recipients_entry, register_store_recipients_save_action,
    show_store_recipients_create_page, show_store_recipients_edit_page,
    sync_store_recipients_page_header, StoreRecipientsPageState, StoreRecipientsPlatformState,
    StoreRecipientsRequest,
};
use crate::logging::log_error;
use crate::preferences::Preferences;
use crate::support::ui::{append_action_row_with_button, clear_list_box, flat_icon_button};
use adw::gtk::{FileChooserAction, FileChooserNative, ListBox, ResponseType};
use adw::prelude::*;
use adw::{ActionRow, ApplicationWindow, Toast, ToastOverlay};
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

pub(crate) fn rebuild_store_list(
    list: &ListBox,
    settings: &Preferences,
    window: &ApplicationWindow,
    overlay: &ToastOverlay,
    recipients_page: &StoreRecipientsPageState,
) {
    clear_list_box(list);

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

    let delete_button = flat_icon_button("user-trash-symbolic");
    row.add_suffix(&delete_button);

    list.append(&row);

    let settings = settings.clone();
    let list = list.clone();
    let row_for_delete = row.clone();
    let store = store.to_string();
    let recipients_page = recipients_page.clone();
    let store_for_edit = store.clone();

    row.connect_activated(move |_| {
        show_store_recipients_edit_page(&recipients_page, &store_for_edit)
    });

    delete_button.connect_clicked(move |_| {
        if let Some(stores) = updated_stores_after_delete(&settings.stores(), &store) {
            if let Err(err) = settings.set_stores(stores) {
                log_error(format!("Failed to save stores: {err}"));
            } else {
                list.remove(&row_for_delete);
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
    let settings = settings.clone();
    let window = window.clone();
    let overlay = overlay.clone();
    let recipients_page = recipients_page.clone();
    let list_for_action = list.clone();
    append_action_row_with_button(
        list,
        "Add store folder",
        "Choose an existing folder.",
        "folder-open-symbolic",
        move || {
            open_store_picker(
                &window,
                &list_for_action,
                &settings,
                &overlay,
                &recipients_page,
            )
        },
    );
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
            if let Some(stores) = updated_stores_after_add(&settings.stores(), &store) {
                if let Err(err) = settings.set_stores(stores) {
                    log_error(format!("Failed to save stores: {err}"));
                    overlay_for_selection.add_toast(Toast::new("Couldn't add that folder."));
                } else {
                    rebuild_store_list(
                        &list,
                        &settings,
                        &window_for_selection,
                        &overlay_for_selection,
                        &recipients_page,
                    );
                    show_store_recipients_edit_page(&recipients_page, &store);
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
    let settings = settings.clone();
    let window = window.clone();
    let overlay = overlay.clone();
    let recipients_page = recipients_page.clone();
    append_action_row_with_button(
        list,
        "Create store",
        "Choose a folder and add recipients.",
        "folder-new-symbolic",
        move || open_store_creator_picker(&window, &settings, &overlay, &recipients_page),
    );
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
            let recipients = initial_recipients_for_store_creation(
                read_store_gpg_recipients(&store),
                suggested_gpg_recipients(&settings),
            );
            show_store_recipients_create_page(&recipients_page, store, recipients);
        },
    );
}

#[cfg(test)]
mod tests {
    use super::{
        initial_recipients_for_store_creation, updated_stores_after_add,
        updated_stores_after_delete,
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
}
