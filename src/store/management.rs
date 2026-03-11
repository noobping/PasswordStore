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
use crate::store::labels::shortened_store_labels;
#[cfg(feature = "flatpak")]
use crate::support::actions::register_window_action;
#[cfg(not(feature = "flatpak"))]
use crate::support::background::spawn_result_task;
#[cfg(not(feature = "flatpak"))]
use crate::support::pass_import::{
    available_pass_import_sources, normalize_optional_text, run_pass_import, PassImportRequest,
};
use crate::support::ui::{append_action_row_with_button, clear_list_box, flat_icon_button};
#[cfg(not(feature = "flatpak"))]
use crate::window::clone_store_repository;
#[cfg(not(feature = "flatpak"))]
use adw::glib::object::IsA;
#[cfg(not(feature = "flatpak"))]
use adw::gtk::{Align, Box as GtkBox, Button, DropDown, Orientation, Spinner};
use adw::gtk::{FileChooserAction, FileChooserNative, ListBox, ResponseType};
use adw::prelude::*;
use adw::{ActionRow, ApplicationWindow, Toast, ToastOverlay};
#[cfg(not(feature = "flatpak"))]
use adw::{
    Dialog, EntryRow, HeaderBar, PreferencesGroup, PreferencesPage, StatusPage, WindowTitle,
};
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

fn selected_store_folder_mode(is_empty: bool) -> SelectedStoreFolderMode {
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

#[cfg(not(feature = "flatpak"))]
fn should_show_pass_import_row(stores: &[String], import_sources: &[String]) -> bool {
    !stores.is_empty() && !import_sources.is_empty()
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

    let stores = settings.stores();
    for store in &stores {
        append_store_row(list, settings, store, recipients_page);
    }

    append_store_picker_row(list, settings, window, overlay, recipients_page);
    #[cfg(not(feature = "flatpak"))]
    {
        append_store_clone_row(list, settings, window, overlay, recipients_page);
        if let Ok(import_sources) = available_pass_import_sources() {
            if should_show_pass_import_row(&stores, &import_sources) {
                append_store_import_row(list, settings, window, overlay);
            }
        }
    }
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
        "Add or create store",
        "Choose a folder. Empty folders become new stores.",
        "folder-new-symbolic",
        move || {
            prompt_add_or_create_store(
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
fn build_progress_dialog(
    window: &ApplicationWindow,
    title: &str,
    subtitle: Option<&str>,
    description: &str,
) -> Dialog {
    let status = StatusPage::builder().description(description).build();
    status.set_child(Some(&Spinner::builder().spinning(true).build()));

    let dialog = Dialog::builder()
        .title(title)
        .content_width(460)
        .child(&dialog_content_shell(title, subtitle, &status))
        .build();
    dialog.set_can_close(false);
    dialog.present(Some(window));
    dialog
}

#[cfg(not(feature = "flatpak"))]
fn build_clone_progress_dialog(window: &ApplicationWindow, store: &str) -> Dialog {
    build_progress_dialog(
        window,
        "Restoring password store",
        Some(store),
        "Please wait.",
    )
}

#[cfg(not(feature = "flatpak"))]
fn build_import_progress_dialog(window: &ApplicationWindow, store: &str) -> Dialog {
    build_progress_dialog(window, "Importing passwords", Some(store), "Please wait.")
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
        .title("Restore password store")
        .content_width(460)
        .child(&dialog_content_shell(
            "Restore password store",
            Some(store),
            &page,
        ))
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
pub(crate) fn prompt_store_clone<F>(
    window: &ApplicationWindow,
    overlay: &ToastOverlay,
    on_submit: F,
) where
    F: Fn(String, String) + 'static,
{
    let window = window.clone();
    let overlay = overlay.clone();
    let on_submit = Rc::new(on_submit);
    let picker_window = window.clone();
    let picker_overlay = overlay.clone();
    open_store_folder_picker(
        &picker_window,
        "Choose password store folder to restore",
        "Select",
        true,
        &picker_overlay,
        move |store| {
            let window_for_dialog = window.clone();
            let overlay_for_dialog = overlay.clone();
            let store_for_dialog = store.clone();
            let on_submit = on_submit.clone();
            present_clone_url_dialog(
                &window_for_dialog,
                &overlay_for_dialog,
                &store_for_dialog,
                {
                    let store = store.clone();
                    move |url| on_submit(store.clone(), url)
                },
            );
        },
    );
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
        "Restore password store",
        "Choose a folder and restore it from a Git repository.",
        "git-symbolic",
        move || {
            let list_for_clone = list_for_action.clone();
            let settings_for_clone = settings.clone();
            let window_for_clone = window.clone();
            let overlay_for_clone = overlay.clone();
            let recipients_page_for_clone = recipients_page.clone();
            prompt_store_clone(&window, &overlay, move |store, url| {
                start_store_clone(
                    &window_for_clone,
                    &list_for_clone,
                    &settings_for_clone,
                    &overlay_for_clone,
                    &recipients_page_for_clone,
                    store,
                    url,
                );
            });
        },
    );
}

#[cfg(not(feature = "flatpak"))]
fn present_pass_import_dialog<F>(
    window: &ApplicationWindow,
    overlay: &ToastOverlay,
    stores: &[String],
    import_sources: &[String],
    on_submit: F,
) where
    F: Fn(PassImportRequest) + 'static,
{
    let store_labels = shortened_store_labels(stores);
    let store_label_refs = store_labels.iter().map(String::as_str).collect::<Vec<_>>();
    let source_refs = import_sources
        .iter()
        .map(String::as_str)
        .collect::<Vec<_>>();

    let store_dropdown = DropDown::from_strings(&store_label_refs);
    store_dropdown.set_valign(Align::Center);
    let store_row = ActionRow::builder().title("Store").build();
    store_row.add_suffix(&store_dropdown);

    let source_dropdown = DropDown::from_strings(&source_refs);
    source_dropdown.set_valign(Align::Center);
    let source_row = ActionRow::builder().title("Importer").build();
    source_row.add_suffix(&source_dropdown);

    let source_path_row = EntryRow::new();
    source_path_row.set_title("Source path");

    let target_path_row = EntryRow::new();
    target_path_row.set_title("Store subfolder");

    let group = PreferencesGroup::builder().build();
    group.add(&store_row);
    group.add(&source_row);
    group.add(&source_path_row);
    group.add(&target_path_row);

    let page = PreferencesPage::new();
    page.add(&group);

    let import_button = Button::builder()
        .label("Import")
        .halign(Align::End)
        .margin_top(12)
        .margin_bottom(12)
        .margin_end(12)
        .css_classes(vec!["suggested-action"])
        .build();

    let content = GtkBox::new(Orientation::Vertical, 0);
    content.append(&page);
    content.append(&import_button);

    let dialog = Dialog::builder()
        .title("Import passwords")
        .content_width(460)
        .child(&dialog_content_shell(
            "Import passwords",
            Some("Use pass import to import into an existing store."),
            &content,
        ))
        .build();

    let dialog_clone = dialog.clone();
    let overlay_clone = overlay.clone();
    let stores = stores.to_vec();
    let import_sources = import_sources.to_vec();
    import_button.connect_clicked(move |_| {
        let Some(store_root) = stores.get(store_dropdown.selected() as usize).cloned() else {
            overlay_clone.add_toast(Toast::new("Choose a store."));
            return;
        };
        let Some(source) = import_sources
            .get(source_dropdown.selected() as usize)
            .cloned()
        else {
            overlay_clone.add_toast(Toast::new("Choose an importer."));
            return;
        };

        dialog_clone.close();
        on_submit(PassImportRequest {
            store_root,
            source,
            source_path: normalize_optional_text(&source_path_row.text()),
            target_path: normalize_optional_text(&target_path_row.text()),
        });
    });

    dialog.present(Some(window));
}

#[cfg(not(feature = "flatpak"))]
fn start_pass_import(
    window: &ApplicationWindow,
    overlay: &ToastOverlay,
    request: PassImportRequest,
) {
    let progress_dialog = build_import_progress_dialog(window, &request.store_root);
    let progress_dialog_for_disconnect = progress_dialog.clone();
    let overlay = overlay.clone();
    let overlay_for_disconnect = overlay.clone();
    let store_for_error = request.store_root.clone();
    let source_for_error = request.source.clone();
    spawn_result_task(
        move || run_pass_import(&request),
        move |result| {
            progress_dialog.force_close();
            match result {
                Ok(()) => overlay.add_toast(Toast::new("Passwords imported.")),
                Err(err) => {
                    log_error(format!(
                        "Failed to import passwords into '{store_for_error}' from '{source_for_error}': {err}"
                    ));
                    overlay.add_toast(Toast::new(&err));
                }
            }
        },
        move || {
            progress_dialog_for_disconnect.force_close();
            overlay_for_disconnect.add_toast(Toast::new("Couldn't import passwords."));
        },
    );
}

#[cfg(not(feature = "flatpak"))]
fn append_store_import_row(
    list: &ListBox,
    settings: &Preferences,
    window: &ApplicationWindow,
    overlay: &ToastOverlay,
) {
    let settings = settings.clone();
    let window = window.clone();
    let overlay = overlay.clone();
    append_action_row_with_button(
        list,
        "Import passwords",
        "Use pass import with an existing store.",
        "document-open-symbolic",
        move || {
            let stores = settings.stores();
            if stores.is_empty() {
                overlay.add_toast(Toast::new("Add a store first."));
                return;
            }

            let import_sources = match available_pass_import_sources() {
                Ok(import_sources) if !import_sources.is_empty() => import_sources,
                _ => {
                    overlay.add_toast(Toast::new("pass import is not available."));
                    return;
                }
            };

            present_pass_import_dialog(&window, &overlay, &stores, &import_sources, {
                let window = window.clone();
                let overlay = overlay.clone();
                move |request| start_pass_import(&window, &overlay, request)
            });
        },
    );
}

pub(crate) fn prompt_add_or_create_store(
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

                    rebuild_store_list(
                        &list,
                        &settings,
                        &window_for_selection,
                        &overlay_for_selection,
                        &recipients_page,
                    );
                    show_store_recipients_edit_page(&recipients_page, &store);
                }
                SelectedStoreFolderMode::CreateNew => {
                    let recipients = initial_recipients_for_store_creation(
                        read_store_gpg_recipients(&store),
                        suggested_gpg_recipients(&settings),
                    );
                    show_store_recipients_create_page(&recipients_page, store, recipients);
                }
            };
        },
    );
}

#[cfg(feature = "flatpak")]
pub(crate) fn register_open_store_picker_action(
    window: &ApplicationWindow,
    list: &ListBox,
    overlay: &ToastOverlay,
    recipients_page: &StoreRecipientsPageState,
) {
    let action_window = window.clone();
    let prompt_window = action_window.clone();
    let list = list.clone();
    let overlay = overlay.clone();
    let recipients_page = recipients_page.clone();
    register_window_action(&action_window, "open-store-picker", move || {
        let settings = Preferences::new();
        prompt_add_or_create_store(&prompt_window, &list, &settings, &overlay, &recipients_page);
    });
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

#[cfg(test)]
mod tests {
    #[cfg(not(feature = "flatpak"))]
    use super::should_show_pass_import_row;
    use super::{
        initial_recipients_for_store_creation, selected_store_folder_mode,
        updated_stores_after_add, updated_stores_after_delete, SelectedStoreFolderMode,
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

    #[cfg(not(feature = "flatpak"))]
    #[test]
    fn pass_import_row_requires_an_existing_store_and_available_sources() {
        assert!(!should_show_pass_import_row(
            &[],
            &["bitwarden".to_string()]
        ));
        assert!(!should_show_pass_import_row(
            &["/tmp/store".to_string()],
            &[]
        ));
        assert!(should_show_pass_import_row(
            &["/tmp/store".to_string()],
            &["bitwarden".to_string()]
        ));
    }
}
