use crate::logging::log_error;
use crate::preferences::Preferences;
use crate::store::labels::shortened_store_labels;
use crate::support::background::spawn_result_task;
use crate::support::pass_import::{
    available_pass_import_sources, normalize_optional_text, run_pass_import, PassImportRequest,
};
use crate::support::ui::{append_action_row_with_button, flat_icon_button_with_tooltip};
use adw::gtk::{
    Align, Box as GtkBox, Button, DropDown, FileChooserAction, FileChooserNative, ListBox,
    Orientation, ResponseType,
};
use adw::prelude::*;
use adw::{
    ActionRow, ApplicationWindow, Dialog, EntryRow, PreferencesGroup, PreferencesPage, Toast,
    ToastOverlay,
};
use std::cell::RefCell;
use std::rc::Rc;

use super::dialogs::{build_progress_dialog, dialog_content_shell};

fn selected_local_path(dialog: &FileChooserNative, overlay: &ToastOverlay) -> Option<String> {
    let file = dialog.file()?;
    let path = file.path().or_else(|| {
        log_error(
            "The selected file is not available as a local path. Choose a local file or folder."
                .to_string(),
        );
        overlay.add_toast(Toast::new("Choose a local file or folder."));
        None
    })?;

    Some(path.to_string_lossy().to_string())
}

pub(super) fn should_show_pass_import_row(stores: &[String], import_sources: &[String]) -> bool {
    !stores.is_empty() && !import_sources.is_empty()
}

fn import_source_subtitle(source_path: Option<&str>) -> &'static str {
    if source_path.is_some() {
        ""
    } else {
        "Choose a file or folder if the importer needs one."
    }
}

fn build_import_progress_dialog(window: &ApplicationWindow, store: &str) -> Dialog {
    build_progress_dialog(window, "Importing passwords", Some(store), "Please wait.")
}

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

    let source_path = Rc::new(RefCell::new(None::<String>));
    let source_path_row = ActionRow::builder()
        .title("Import source")
        .subtitle(import_source_subtitle(None))
        .build();
    let source_file_button = flat_icon_button_with_tooltip("paper-symbolic", "Choose source file");
    let source_folder_button =
        flat_icon_button_with_tooltip("folder-open-symbolic", "Choose source folder");
    let source_clear_button =
        flat_icon_button_with_tooltip("edit-clear-symbolic", "Clear source path");
    source_path_row.add_suffix(&source_file_button);
    source_path_row.add_suffix(&source_folder_button);
    source_path_row.add_suffix(&source_clear_button);

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

    {
        let window = window.clone();
        let overlay = overlay.clone();
        let source_path = source_path.clone();
        let source_path_row = source_path_row.clone();
        source_file_button.connect_clicked(move |_| {
            let dialog = FileChooserNative::new(
                Some("Choose import source file"),
                Some(&window),
                FileChooserAction::Open,
                Some("Select"),
                Some("Cancel"),
            );
            let overlay = overlay.clone();
            let source_path = source_path.clone();
            let source_path_row = source_path_row.clone();
            dialog.connect_response(move |dialog, response| {
                if response == ResponseType::Accept {
                    if let Some(path) = selected_local_path(dialog, &overlay) {
                        *source_path.borrow_mut() = Some(path.clone());
                        source_path_row.set_subtitle(&path);
                    }
                }
                dialog.hide();
            });
            dialog.show();
        });
    }

    {
        let window = window.clone();
        let overlay = overlay.clone();
        let source_path = source_path.clone();
        let source_path_row = source_path_row.clone();
        source_folder_button.connect_clicked(move |_| {
            let dialog = FileChooserNative::new(
                Some("Choose import source folder"),
                Some(&window),
                FileChooserAction::SelectFolder,
                Some("Select"),
                Some("Cancel"),
            );
            let overlay = overlay.clone();
            let source_path = source_path.clone();
            let source_path_row = source_path_row.clone();
            dialog.connect_response(move |dialog, response| {
                if response == ResponseType::Accept {
                    if let Some(path) = selected_local_path(dialog, &overlay) {
                        *source_path.borrow_mut() = Some(path.clone());
                        source_path_row.set_subtitle(&path);
                    }
                }
                dialog.hide();
            });
            dialog.show();
        });
    }

    {
        let source_path = source_path.clone();
        let source_path_row = source_path_row.clone();
        source_clear_button.connect_clicked(move |_| {
            *source_path.borrow_mut() = None;
            source_path_row.set_subtitle(import_source_subtitle(None));
        });
    }

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
            source_path: source_path.borrow().clone(),
            target_path: normalize_optional_text(&target_path_row.text()),
        });
    });

    dialog.present(Some(window));
}

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

pub(super) fn schedule_store_import_row(
    list: &ListBox,
    settings: &Preferences,
    window: &ApplicationWindow,
    overlay: &ToastOverlay,
    stores: Vec<String>,
) {
    if stores.is_empty() {
        return;
    }

    append_action_row_with_button(
        list,
        "Import passwords",
        "Use pass import with an existing store.",
        "document-open-symbolic",
        {
            let settings = settings.clone();
            let window = window.clone();
            let overlay = overlay.clone();
            move || {
                let stores = settings.stores();
                if stores.is_empty() {
                    overlay.add_toast(Toast::new("Add a store first."));
                    return;
                }

                let window_for_result = window.clone();
                let overlay_for_result = overlay.clone();
                let stores_for_result = stores.clone();
                let overlay_for_disconnect = overlay.clone();
                spawn_result_task(
                    available_pass_import_sources,
                    move |result| match result {
                        Ok(import_sources) => {
                            if !should_show_pass_import_row(&stores_for_result, &import_sources) {
                                overlay_for_result
                                    .add_toast(Toast::new("pass import is not available."));
                                return;
                            }

                            present_pass_import_dialog(
                                &window_for_result,
                                &overlay_for_result,
                                &stores_for_result,
                                &import_sources,
                                {
                                    let window = window_for_result.clone();
                                    let overlay = overlay_for_result.clone();
                                    move |request| start_pass_import(&window, &overlay, request)
                                },
                            );
                        }
                        Err(err) => {
                            log_error(format!("Failed to read pass import sources: {err}"));
                            overlay_for_result.add_toast(Toast::new(&err));
                        }
                    },
                    move || {
                        overlay_for_disconnect.add_toast(Toast::new("Couldn't read importers."));
                    },
                );
            }
        },
    );
}
