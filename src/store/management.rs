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
#[cfg(not(feature = "flatpak"))]
use crate::support::background::spawn_result_task;
use crate::support::ui::{append_action_row_with_button, clear_list_box, flat_icon_button};
#[cfg(not(feature = "flatpak"))]
use crate::window::clone_store_repository;
#[cfg(not(feature = "flatpak"))]
use adw::glib::object::IsA;
#[cfg(not(feature = "flatpak"))]
use adw::gtk::{Box as GtkBox, Orientation, Spinner};
use adw::gtk::{FileChooserAction, FileChooserNative, ListBox, ResponseType};
use adw::prelude::*;
use adw::{ActionRow, ApplicationWindow, Toast, ToastOverlay};
#[cfg(not(feature = "flatpak"))]
use adw::{
    Dialog, EntryRow, HeaderBar, PreferencesGroup, PreferencesPage, StatusPage, WindowTitle,
};
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
    #[cfg(not(feature = "flatpak"))]
    append_store_clone_row(list, settings, window, overlay, recipients_page);
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

#[cfg(not(feature = "flatpak"))]
fn dialog_content_shell(
    title: &str,
    subtitle: Option<&str>,
    child: &impl IsA<adw::gtk::Widget>,
) -> GtkBox {
    let window_title = WindowTitle::builder().title(title).build();
    if let Some(subtitle) = subtitle.filter(|subtitle| !subtitle.trim().is_empty()) {
        window_title.set_subtitle(subtitle);
    }

    let header = HeaderBar::new();
    header.set_title_widget(Some(&window_title));

    let shell = GtkBox::new(Orientation::Vertical, 0);
    shell.append(&header);
    shell.append(child);
    shell
}

#[cfg(not(feature = "flatpak"))]
fn build_clone_progress_dialog(window: &ApplicationWindow, store: &str) -> Dialog {
    let status = StatusPage::builder().description("Please wait.").build();
    status.set_child(Some(&Spinner::builder().spinning(true).build()));

    let dialog = Dialog::builder()
        .title("Cloning store")
        .content_width(460)
        .child(&dialog_content_shell("Cloning store", Some(store), &status))
        .build();
    dialog.set_can_close(false);
    dialog.present(Some(window));
    dialog
}

#[cfg(not(feature = "flatpak"))]
fn present_clone_url_dialog<F>(
    window: &ApplicationWindow,
    overlay: &ToastOverlay,
    store: &str,
    on_submit: F,
) where
    F: Fn(String) + 'static,
{
    let url_row = EntryRow::new();
    url_row.set_title("Repository URL");
    url_row.set_show_apply_button(true);

    let group = PreferencesGroup::builder().build();
    group.add(&url_row);

    let page = PreferencesPage::new();
    page.add(&group);

    let dialog = Dialog::builder()
        .title("Clone store")
        .content_width(460)
        .child(&dialog_content_shell("Clone store", Some(store), &page))
        .build();

    let dialog_clone = dialog.clone();
    let overlay_clone = overlay.clone();
    url_row.connect_apply(move |row| {
        let url = row.text().trim().to_string();
        if url.is_empty() {
            overlay_clone.add_toast(Toast::new("Enter a repository URL."));
            return;
        }

        dialog_clone.close();
        on_submit(url);
    });

    dialog.present(Some(window));
}

#[cfg(not(feature = "flatpak"))]
fn append_store_clone_row(
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
        "Clone store",
        "Choose a folder and clone a Git repository into it.",
        "git-symbolic",
        move || {
            open_store_clone_picker(
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

#[cfg(not(feature = "flatpak"))]
fn open_store_clone_picker(
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
    let recipients_page = recipients_page.clone();
    let window_for_selection = window.clone();
    let overlay_for_selection = overlay.clone();
    open_store_folder_picker(
        &window,
        "Choose password store folder for the clone",
        "Select",
        true,
        &overlay,
        move |store| {
            let list_for_clone = list.clone();
            let settings_for_clone = settings.clone();
            let window_for_clone = window_for_selection.clone();
            let overlay_for_clone = overlay_for_selection.clone();
            let recipients_page_for_clone = recipients_page.clone();
            let store_for_dialog = store.clone();
            present_clone_url_dialog(
                &window_for_selection,
                &overlay_for_selection,
                &store_for_dialog,
                move |url| {
                    start_store_clone(
                        &window_for_clone,
                        &list_for_clone,
                        &settings_for_clone,
                        &overlay_for_clone,
                        &recipients_page_for_clone,
                        store.clone(),
                        url,
                    );
                },
            );
        },
    );
}

#[cfg(not(feature = "flatpak"))]
fn start_store_clone(
    window: &ApplicationWindow,
    list: &ListBox,
    settings: &Preferences,
    overlay: &ToastOverlay,
    recipients_page: &StoreRecipientsPageState,
    store: String,
    url: String,
) {
    let progress_dialog = build_clone_progress_dialog(window, &store);
    let progress_dialog_for_disconnect = progress_dialog.clone();
    let list = list.clone();
    let settings = settings.clone();
    let window = window.clone();
    let overlay = overlay.clone();
    let recipients_page = recipients_page.clone();
    let store_for_thread = store.clone();
    let store_for_result = store.clone();
    let store_for_disconnect = store;
    let window_for_result = window.clone();
    let overlay_for_disconnect = overlay.clone();
    let settings_for_result = settings.clone();
    let list_for_result = list.clone();
    let recipients_page_for_result = recipients_page.clone();
    spawn_result_task(
        move || clone_store_repository(&url, &store_for_thread),
        move |result| match result {
            Ok(()) => {
                progress_dialog.force_close();
                if let Some(stores) =
                    updated_stores_after_add(&settings_for_result.stores(), &store_for_result)
                {
                    if let Err(err) = settings_for_result.set_stores(stores) {
                        log_error(format!("Failed to save stores: {err}"));
                        overlay.add_toast(Toast::new("Couldn't add that folder."));
                        return;
                    }
                }
                rebuild_store_list(
                    &list_for_result,
                    &settings_for_result,
                    &window_for_result,
                    &overlay,
                    &recipients_page_for_result,
                );
                overlay.add_toast(Toast::new("Store restored."));
            }
            Err(message) => {
                progress_dialog.force_close();
                overlay.add_toast(Toast::new(&message));
            }
        },
        move || {
            progress_dialog_for_disconnect.force_close();
            log_error(format!(
                "Restore stopped unexpectedly for store '{}'.",
                store_for_disconnect
            ));
            overlay_for_disconnect.add_toast(Toast::new("Restore stopped unexpectedly."));
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
