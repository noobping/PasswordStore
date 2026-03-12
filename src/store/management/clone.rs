use super::{
    dialogs::{build_progress_dialog, dialog_content_shell},
    open_store_folder_picker, rebuild_store_list, updated_stores_after_add,
    StoreRecipientsPageState,
};
use crate::logging::log_error;
use crate::preferences::Preferences;
use crate::support::background::spawn_result_task;
use crate::support::ui::append_action_row_with_button;
use crate::window::clone_store_repository;
use adw::gtk::ListBox;
use adw::prelude::*;
use adw::{
    ApplicationWindow, Dialog, EntryRow, PreferencesGroup, PreferencesPage, Toast, ToastOverlay,
};
use std::rc::Rc;

fn build_clone_progress_dialog(window: &ApplicationWindow, store: &str) -> Dialog {
    build_progress_dialog(
        window,
        "Restoring password store",
        Some(store),
        "Please wait.",
    )
}

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

pub(super) fn append_store_clone_row(
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
