use crate::backend::{delete_password_entry, rename_password_entry};
use crate::clipboard::copy_password_entry_to_clipboard;
use crate::logging::log_error;
use crate::password::model::PassEntry;
use crate::support::background::spawn_result_task;
use adw::gtk::{Button, ListBox, ListBoxRow, MenuButton, Popover};
use adw::prelude::*;
use adw::{ActionRow, EntryRow, Toast, ToastOverlay};

pub(super) fn append_password_row(list: &ListBox, item: PassEntry, overlay: &ToastOverlay) {
    let row = ListBoxRow::new();
    let action_row = ActionRow::builder()
        .title(item.basename.clone())
        .subtitle(item.relative_path.clone())
        .activatable(true)
        .build();
    let menu_button = MenuButton::builder()
        .icon_name("view-more-symbolic")
        .has_frame(false)
        .css_classes(vec!["flat"])
        .build();
    let popover = Popover::new();
    let rename_row = EntryRow::new();
    rename_row.set_title("Move or rename");
    rename_row.set_show_apply_button(true);
    rename_row.set_text(&item.label());
    let copy_button = Button::from_icon_name("edit-copy-symbolic");
    copy_button.add_css_class("flat");
    action_row.add_suffix(&copy_button);
    let delete_button = Button::from_icon_name("user-trash-symbolic");
    delete_button.add_css_class("flat");
    delete_button.add_css_class("destructive-action");
    rename_row.add_suffix(&delete_button);

    popover.set_child(Some(&rename_row));
    menu_button.set_popover(Some(&popover));
    action_row.add_suffix(&menu_button);
    row.set_child(Some(&action_row));

    unsafe {
        row.set_data("root", item.store_path.clone());
        row.set_data("label", item.label());
    }

    connect_copy_action(&item, &popover, &copy_button, overlay);
    connect_rename_action(&item, &action_row, &rename_row, overlay);
    connect_delete_action(&item, &row, list, &delete_button, overlay);

    list.append(&row);
}

fn connect_copy_action(
    item: &PassEntry,
    popover: &Popover,
    button: &Button,
    overlay: &ToastOverlay,
) {
    let entry = item.clone();
    let popover = popover.clone();
    let overlay = overlay.clone();
    let copied_button = button.clone();
    button.connect_clicked(move |_| {
        popover.popdown();
        copy_password_entry_to_clipboard(
            entry.clone(),
            overlay.clone(),
            Some(copied_button.clone()),
        );
    });
}

fn connect_rename_action(
    item: &PassEntry,
    action_row: &ActionRow,
    rename_row: &EntryRow,
    overlay: &ToastOverlay,
) {
    let entry = item.clone();
    let action_row = action_row.clone();
    let overlay = overlay.clone();
    rename_row.connect_apply(move |row| {
        let new_label = row.text().to_string();
        if new_label.is_empty() {
            overlay.add_toast(Toast::new("Enter a name."));
            return;
        }

        let old_label = entry.label();
        if new_label == old_label {
            return;
        }

        let root = entry.store_path.clone();
        match rename_password_entry(&root, &old_label, &new_label) {
            Ok(()) => {
                let (parent, tail) = match new_label.rsplit_once('/') {
                    Some((parent, tail)) => (parent, tail),
                    None => ("", new_label.as_str()),
                };
                action_row.set_title(tail);
                action_row.set_subtitle(parent);
            }
            Err(_) => {
                overlay.add_toast(Toast::new("Couldn't rename the item."));
            }
        }
    });
}

fn connect_delete_action(
    item: &PassEntry,
    row: &ListBoxRow,
    list: &ListBox,
    delete_button: &Button,
    overlay: &ToastOverlay,
) {
    let entry = item.clone();
    let row = row.clone();
    let list = list.clone();
    let overlay = overlay.clone();
    delete_button.connect_clicked(move |_| {
        let root = entry.store_path.clone();
        let label = entry.label();
        let list = list.clone();
        let row = row.clone();
        let overlay = overlay.clone();
        let overlay_for_disconnect = overlay.clone();
        spawn_result_task(
            move || delete_password_entry(&root, &label),
            move |result| match result {
                Ok(()) => {
                    list.remove(&row);
                }
                Err(err) => {
                    log_error(format!("Failed to delete password entry: {err}"));
                    overlay.add_toast(Toast::new("Couldn't delete the item."));
                }
            },
            move || {
                overlay_for_disconnect.add_toast(Toast::new("Couldn't delete the item."));
            },
        );
    });
}
