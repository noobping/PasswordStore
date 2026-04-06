use super::{
    dialogs::build_progress_dialog, open_store_folder_picker, rebuild_stores_list,
    updated_stores_after_add, StoreRecipientsPageState,
};
use crate::i18n::gettext;
use crate::logging::log_error;
use crate::preferences::Preferences;
use crate::support::background::spawn_result_task;
use crate::support::runtime::supports_host_command_features;
use crate::support::ui::{
    append_action_row_with_button, connect_entry_row_apply_button_to_nonempty_text,
    dialog_content_shell, dim_label_icon,
};
use crate::window::clone_store_repository;
use adw::gtk::{Align, Box as GtkBox, Label, ListBox, Orientation};
use adw::prelude::*;
use adw::{
    ActionRow, ApplicationWindow, Dialog, EntryRow, PreferencesGroup, PreferencesPage, Toast,
    ToastOverlay,
};
use std::rc::Rc;

fn build_clone_progress_dialog(window: &ApplicationWindow, store: &str) -> Dialog {
    build_progress_dialog(
        window,
        "Restoring password store",
        Some(store),
        "Wait a moment.",
    )
}

fn clone_url_dialog_error_message(url: &str) -> Option<&'static str> {
    url.trim().is_empty().then_some("Enter a repository URL.")
}

fn present_clone_url_dialog<F>(window: &ApplicationWindow, store: &str, on_submit: F)
where
    F: Fn(String) + 'static,
{
    let url_row = EntryRow::new();
    url_row.set_title(&gettext("Repository URL"));
    url_row.set_show_apply_button(true);
    connect_entry_row_apply_button_to_nonempty_text(&url_row);

    let group = PreferencesGroup::builder().build();
    group.add(&url_row);

    let page = PreferencesPage::new();
    page.add(&group);

    let error_label = Label::new(None);
    error_label.set_halign(Align::Start);
    error_label.set_wrap(true);
    error_label.add_css_class("error");
    error_label.add_css_class("caption");
    error_label.set_margin_top(6);
    error_label.set_margin_start(18);
    error_label.set_margin_end(18);
    error_label.set_margin_bottom(18);
    error_label.set_visible(false);

    let content = GtkBox::new(Orientation::Vertical, 0);
    content.append(&page);
    content.append(&error_label);

    let dialog = Dialog::builder()
        .title(gettext("Restore password store"))
        .content_height(280)
        .content_width(800)
        .follows_content_size(true)
        .child(&dialog_content_shell(
            "Restore password store",
            Some(store),
            &content,
        ))
        .build();

    let dialog_clone = dialog.clone();
    let error_label_for_apply = error_label.clone();
    url_row.connect_apply(move |row| {
        let url = row.text().trim().to_string();
        if let Some(message) = clone_url_dialog_error_message(&url) {
            error_label_for_apply.set_label(&gettext(message));
            error_label_for_apply.set_visible(true);
            return;
        }
        error_label_for_apply.set_visible(false);

        dialog_clone.close();
        on_submit(url);
    });

    {
        let error_label = error_label.clone();
        url_row.connect_changed(move |_| {
            error_label.set_visible(false);
        });
    }

    dialog.present(Some(window));
}

pub fn prompt_store_clone<F>(window: &ApplicationWindow, overlay: &ToastOverlay, on_submit: F)
where
    F: Fn(String, String) + 'static,
{
    let window = window.clone();
    let overlay = overlay.clone();
    let on_submit = Rc::new(on_submit);
    let picker_window = window.clone();
    let picker_overlay = overlay.clone();
    open_store_folder_picker(
        &picker_window,
        "Choose store folder to restore",
        "Select",
        true,
        &picker_overlay,
        move |store| {
            let window_for_dialog = window.clone();
            let store_for_dialog = store.clone();
            let on_submit = on_submit.clone();
            present_clone_url_dialog(&window_for_dialog, &store_for_dialog, move |url| {
                on_submit(store.clone(), url)
            });
        },
    );
}

pub(super) fn append_store_clone_row(
    list: &ListBox,
    stores_list: &ListBox,
    settings: &Preferences,
    window: &ApplicationWindow,
    overlay: &ToastOverlay,
    recipients_page: &StoreRecipientsPageState,
) {
    if !supports_host_command_features() {
        return;
    }

    if !settings.uses_host_command_backend() {
        let row = ActionRow::builder()
            .title(gettext("Restore password store"))
            .subtitle(gettext(
                "Switch Backend to Host to restore a store from a Git repository.",
            ))
            .build();
        row.set_sensitive(false);
        row.set_activatable(false);
        row.add_suffix(&dim_label_icon("git-symbolic"));
        list.append(&row);
        return;
    }

    let settings = settings.clone();
    let window = window.clone();
    let overlay = overlay.clone();
    let recipients_page = recipients_page.clone();
    let stores_list_for_action = stores_list.clone();
    append_action_row_with_button(
        list,
        "Restore password store",
        "Choose a folder and restore it from a Git repository.",
        "git-symbolic",
        move || {
            let stores_list_for_clone = stores_list_for_action.clone();
            let settings_for_clone = settings.clone();
            let window_for_clone = window.clone();
            let overlay_for_clone = overlay.clone();
            let recipients_page_for_clone = recipients_page.clone();
            prompt_store_clone(&window, &overlay, move |store, url| {
                start_store_clone(
                    &window_for_clone,
                    &stores_list_for_clone,
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
    stores_list: &ListBox,
    settings: &Preferences,
    overlay: &ToastOverlay,
    recipients_page: &StoreRecipientsPageState,
    store: String,
    url: String,
) {
    let progress_dialog = build_clone_progress_dialog(window, &store);
    let progress_dialog_for_disconnect = progress_dialog.clone();
    let stores_list = stores_list.clone();
    let settings = settings.clone();
    let overlay = overlay.clone();
    let recipients_page = recipients_page.clone();
    let store_for_thread = store.clone();
    let store_for_result = store.clone();
    let store_for_disconnect = store;
    let overlay_for_disconnect = overlay.clone();
    let settings_for_result = settings;
    let stores_list_for_result = stores_list;
    let recipients_page_for_result = recipients_page;
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
                        overlay.add_toast(Toast::new(&gettext("Couldn't add that folder.")));
                        return;
                    }
                }
                rebuild_stores_list(
                    &stores_list_for_result,
                    &settings_for_result,
                    &recipients_page_for_result,
                );
                overlay.add_toast(Toast::new(&gettext("Store restored.")));
            }
            Err(message) => {
                progress_dialog.force_close();
                overlay.add_toast(Toast::new(&gettext(&message)));
            }
        },
        move || {
            progress_dialog_for_disconnect.force_close();
            log_error(format!(
                "Restore stopped unexpectedly for store '{store_for_disconnect}'."
            ));
            overlay_for_disconnect.add_toast(Toast::new(&gettext("Restore stopped unexpectedly.")));
        },
    );
}

#[cfg(test)]
mod tests {
    use super::clone_url_dialog_error_message;

    #[test]
    fn clone_url_dialog_requires_a_repository_url() {
        assert_eq!(
            clone_url_dialog_error_message(""),
            Some("Enter a repository URL.")
        );
        assert_eq!(
            clone_url_dialog_error_message("   "),
            Some("Enter a repository URL.")
        );
        assert_eq!(
            clone_url_dialog_error_message("ssh://git@example.test/repo.git"),
            None
        );
    }
}
