#[cfg(any(feature = "setup", feature = "flatpak"))]
use crate::backend::{delete_password_entry, rename_password_entry};
use crate::clipboard::copy_password_entry_to_clipboard;
use crate::config::APP_ID;
use crate::item::{collect_all_password_items, PassEntry};
use crate::logging::log_error;
#[cfg(all(not(feature = "setup"), not(feature = "flatpak")))]
use crate::logging::{run_command_status, CommandLogOptions};
use crate::methods::non_null_to_string_option;
use crate::preferences::Preferences;
use adw::prelude::*;
use adw::{glib, ActionRow, EntryRow, StatusPage, Toast, ToastOverlay};
use adw::gtk::{Button, ListBox, ListBoxRow, MenuButton, Popover, SearchEntry, Spinner};
use std::cell::RefCell;
use std::rc::Rc;
use std::sync::mpsc;
use std::sync::mpsc::TryRecvError;
use std::thread;
use std::time::Duration;

pub(crate) fn load_passwords_async(
    list: &ListBox,
    git: Button,
    find: Button,
    save: Button,
    overlay: ToastOverlay,
    show_list_actions: bool,
) {
    clear_list(list);

    let settings = Preferences::new();
    prune_missing_store_dirs(&settings);
    let has_store_dirs = !settings.stores().is_empty();

    git.set_visible(false);
    find.set_visible(show_list_actions);
    list.set_placeholder(Some(&loading_placeholder()));

    let (tx, rx) = mpsc::channel::<Vec<PassEntry>>();
    thread::spawn(move || {
        let all_items = match collect_all_password_items() {
            Ok(items) => items,
            Err(err) => {
                log_error(format!("Error scanning pass stores: {err}"));
                Vec::new()
            }
        };
        let _ = tx.send(all_items);
    });

    let list_clone = list.clone();
    let git_clone = git.clone();
    let find_clone = find.clone();
    let save_clone = save.clone();
    let overlay_clone = overlay.clone();
    glib::timeout_add_local(Duration::from_millis(50), move || match rx.try_recv() {
        Ok(items) => {
            let empty = items.is_empty();
            for item in items {
                append_password_row(&list_clone, item, &overlay_clone);
            }

            update_list_actions(
                &find_clone,
                &git_clone,
                &save_clone,
                show_list_actions,
                has_store_dirs,
                empty,
            );
            list_clone.set_placeholder(Some(&resolved_placeholder(empty, has_store_dirs)));

            glib::ControlFlow::Break
        }
        Err(TryRecvError::Empty) => glib::ControlFlow::Continue,
        Err(TryRecvError::Disconnected) => {
            save_clone.set_visible(false);
            git_clone.set_visible(should_show_restore_button(
                show_list_actions,
                has_store_dirs,
                true,
            ));
            find_clone.set_visible(false);
            list_clone.set_placeholder(Some(&resolved_placeholder(true, has_store_dirs)));

            glib::ControlFlow::Break
        }
    });
}

pub(crate) fn setup_search_filter(list: &ListBox, search_entry: &SearchEntry) {
    let query = Rc::new(RefCell::new(String::new()));

    let query_for_filter = query.clone();
    list.set_filter_func(move |row: &ListBoxRow| {
        let q_ref = query_for_filter.borrow();
        let q = q_ref.as_str();
        if q.is_empty() {
            return true;
        }

        if let Some(label) = non_null_to_string_option(row, "label") {
            let query_lower = q.to_lowercase();
            return label.to_lowercase().contains(&query_lower);
        }

        true
    });

    let query_for_entry = query.clone();
    let list_for_entry = list.clone();
    search_entry.connect_search_changed(move |entry| {
        *query_for_entry.borrow_mut() = entry.text().to_string();
        list_for_entry.invalidate_filter();
    });
}

fn append_password_row(list: &ListBox, item: PassEntry, overlay: &ToastOverlay) {
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
    rename_row.set_title("Rename or move");
    rename_row.set_show_apply_button(true);
    rename_row.set_text(&item.label());
    let copy_btn = Button::from_icon_name("edit-copy-symbolic");
    copy_btn.add_css_class("flat");
    action_row.add_suffix(&copy_btn);
    let delete_btn = Button::from_icon_name("user-trash-symbolic");
    delete_btn.add_css_class("flat");
    delete_btn.add_css_class("destructive-action");
    rename_row.add_suffix(&delete_btn);

    popover.set_child(Some(&rename_row));
    menu_button.set_popover(Some(&popover));
    action_row.add_suffix(&menu_button);
    row.set_child(Some(&action_row));

    unsafe {
        row.set_data("root", item.store_path.clone());
        row.set_data("label", item.label());
    }

    connect_copy_action(&item, &popover, &copy_btn, overlay);
    connect_rename_action(&item, &action_row, &rename_row, overlay);
    connect_delete_action(&item, &row, list, &delete_btn, overlay);

    list.append(&row);
}

fn connect_copy_action(item: &PassEntry, popover: &Popover, button: &Button, overlay: &ToastOverlay) {
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
            overlay.add_toast(Toast::new("Enter a new name."));
            return;
        }

        let old_label = entry.label();
        if new_label == old_label {
            return;
        }

        let root = entry.store_path.clone();
        #[cfg(any(feature = "setup", feature = "flatpak"))]
        let rename_result = rename_password_entry(&root, &old_label, &new_label);
        #[cfg(all(not(feature = "setup"), not(feature = "flatpak")))]
        let rename_result = {
            let settings = Preferences::new();
            let mut cmd = settings.command();
            cmd.env("PASSWORD_STORE_DIR", &root)
                .arg("mv")
                .arg(&old_label)
                .arg(&new_label);
            match run_command_status(
                &mut cmd,
                "Rename password entry",
                CommandLogOptions::DEFAULT,
            ) {
                Ok(status) if status.success() => Ok(()),
                Ok(_) => Err(()),
                Err(_) => Err(()),
            }
        };

        match rename_result {
            Ok(()) => {
                let (parent, tail) = match new_label.rsplit_once('/') {
                    Some((parent, tail)) => (parent, tail),
                    None => ("", new_label.as_str()),
                };
                action_row.set_title(tail);
                action_row.set_subtitle(parent);
            }
            Err(_) => {
                overlay.add_toast(Toast::new("Couldn't rename the password entry."));
            }
        }
    });
}

fn connect_delete_action(
    item: &PassEntry,
    row: &ListBoxRow,
    list: &ListBox,
    delete_btn: &Button,
    _overlay: &ToastOverlay,
) {
    let entry = item.clone();
    let row = row.clone();
    let list = list.clone();
    #[cfg(any(feature = "setup", feature = "flatpak"))]
    let overlay = _overlay.clone();
    delete_btn.connect_clicked(move |_| {
        #[cfg(any(feature = "setup", feature = "flatpak"))]
        {
            let overlay = overlay.clone();
            let (tx, rx) = mpsc::channel::<Result<(), String>>();
            let root = entry.store_path.clone();
            let label = entry.label();
            thread::spawn(move || {
                let result = delete_password_entry(&root, &label);
                let _ = tx.send(result);
            });

            let list = list.clone();
            let row = row.clone();
            let overlay = overlay.clone();
            glib::timeout_add_local(Duration::from_millis(50), move || match rx.try_recv() {
                Ok(Ok(())) => {
                    list.remove(&row);
                    glib::ControlFlow::Break
                }
                Ok(Err(err)) => {
                    log_error(format!("Failed to delete password entry: {err}"));
                    overlay.add_toast(Toast::new("Couldn't delete the password entry."));
                    glib::ControlFlow::Break
                }
                Err(TryRecvError::Empty) => glib::ControlFlow::Continue,
                Err(TryRecvError::Disconnected) => {
                    overlay.add_toast(Toast::new("Couldn't delete the password entry."));
                    glib::ControlFlow::Break
                }
            });
        }

        #[cfg(all(not(feature = "setup"), not(feature = "flatpak")))]
        {
            std::thread::spawn({
                let root = entry.store_path.clone();
                let label = entry.label();
                move || {
                    let settings = Preferences::new();
                    let mut cmd = settings.command();
                    cmd.env("PASSWORD_STORE_DIR", root)
                        .arg("rm")
                        .arg("-rf")
                        .arg(&label);
                    let _ = run_command_status(
                        &mut cmd,
                        "Delete password entry",
                        CommandLogOptions::DEFAULT,
                    );
                }
            });
            list.remove(&row);
        }
    });
}

fn update_list_actions(
    find: &Button,
    git: &Button,
    save: &Button,
    show_list_actions: bool,
    has_store_dirs: bool,
    empty: bool,
) {
    if show_list_actions {
        if empty {
            save.set_visible(false);
            find.set_visible(false);
        } else {
            find.set_visible(true);
        }
        git.set_visible(should_show_restore_button(
            show_list_actions,
            has_store_dirs,
            empty,
        ));
    } else {
        find.set_visible(false);
        git.set_visible(false);
    }
}

fn clear_list(list: &ListBox) {
    while let Some(child) = list.first_child() {
        list.remove(&child);
    }
}

fn loading_placeholder() -> StatusPage {
    let spinner = Spinner::new();
    spinner.start();

    StatusPage::builder()
        .icon_name(format!("{APP_ID}-symbolic"))
        .child(&spinner)
        .build()
}

fn resolved_placeholder(empty: bool, has_store_dirs: bool) -> StatusPage {
    if empty {
        build_empty_password_list_placeholder(&format!("{APP_ID}-symbolic"), has_store_dirs)
    } else {
        StatusPage::builder()
            .icon_name("edit-find-symbolic")
            .title("No passwords found")
            .description("Try another query.")
            .build()
    }
}

#[cfg(not(feature = "flatpak"))]
fn should_show_restore_button(show_list_actions: bool, has_store_dirs: bool, empty: bool) -> bool {
    show_list_actions && empty && !has_store_dirs
}

#[cfg(feature = "flatpak")]
fn should_show_restore_button(
    _show_list_actions: bool,
    _has_store_dirs: bool,
    _empty: bool,
) -> bool {
    false
}

fn build_empty_password_list_placeholder(symbolic: &str, has_store_dirs: bool) -> StatusPage {
    let builder = StatusPage::builder().icon_name(symbolic);
    if has_store_dirs {
        builder
            .title("Empty")
            .description("Create a new password to get started.")
            .build()
    } else {
        builder
            .title("No password store folders added")
            .description("Open Preferences and choose a password store folder to get started.")
            .build()
    }
}

fn prune_missing_store_dirs(settings: &Preferences) {
    if let Err(err) = settings.prune_missing_stores() {
        log_error(format!("Failed to remove missing password stores: {err}"));
    }
}

#[cfg(test)]
mod tests {
    use super::should_show_restore_button;

    #[test]
    fn restore_button_is_hidden_for_an_empty_existing_store() {
        assert!(!should_show_restore_button(true, true, true));
    }

    #[test]
    fn restore_button_stays_hidden_off_the_list_page() {
        assert!(!should_show_restore_button(false, false, true));
    }
}
