use super::recipients::{
    read_store_gpg_recipients, store_gpg_recipients_subtitle, suggested_gpg_recipients,
};
pub(crate) use super::recipients_page::{
    connect_store_recipients_entry, register_store_recipients_save_action,
    show_store_recipients_page, sync_store_recipients_page_header, StoreRecipientsMode,
    StoreRecipientsPageState, StoreRecipientsPlatformState, StoreRecipientsRequest,
};
use crate::logging::log_error;
use crate::preferences::Preferences;
use crate::support::ui::{append_action_row_with_button, clear_list_box};
use adw::gtk::{Button, FileChooserAction, FileChooserNative, ListBox, ResponseType};
use adw::prelude::*;
use adw::{ActionRow, ApplicationWindow, Toast, ToastOverlay};
use std::rc::Rc;

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

    let delete_button = Button::from_icon_name("user-trash-symbolic");
    delete_button.add_css_class("flat");
    row.add_suffix(&delete_button);

    list.append(&row);

    let settings = settings.clone();
    let list = list.clone();
    let row_for_delete = row.clone();
    let store = store.to_string();
    let recipients_page = recipients_page.clone();
    let store_for_edit = store.clone();

    row.connect_activated(move |_| {
        show_store_recipients_page(
            &recipients_page,
            StoreRecipientsRequest {
                store: store_for_edit.clone(),
                mode: StoreRecipientsMode::Edit,
            },
            read_store_gpg_recipients(&store_for_edit),
        );
    });

    delete_button.connect_clicked(move |_| {
        let mut stores = settings.stores();
        if let Some(position) = stores.iter().position(|value| value == &store) {
            stores.remove(position);
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
            let mut stores = settings.stores();
            if !stores.contains(&store) {
                stores.push(store.clone());
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
                    show_store_recipients_page(
                        &recipients_page,
                        StoreRecipientsRequest {
                            store: store.clone(),
                            mode: StoreRecipientsMode::Edit,
                        },
                        read_store_gpg_recipients(&store),
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
