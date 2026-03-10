mod placeholder;
mod row;

use self::placeholder::{loading_placeholder, resolved_placeholder, should_show_restore_button};
use self::row::append_password_row;
use crate::logging::log_error;
use crate::password::model::{collect_all_password_items_with_options, CollectItemsOptions};
use crate::preferences::Preferences;
use crate::support::background::spawn_result_task;
use crate::support::object_data::non_null_to_string_option;
use crate::support::ui::clear_list_box;
use adw::gtk::{Button, ListBox, ListBoxRow, SearchEntry};
use adw::prelude::*;
use adw::ToastOverlay;
use std::cell::RefCell;
use std::rc::Rc;

pub(crate) fn load_passwords_async(
    list: &ListBox,
    git: &Button,
    find: &Button,
    save: &Button,
    overlay: &ToastOverlay,
    show_list_actions: bool,
    show_hidden: bool,
) {
    clear_list_box(list);

    let settings = Preferences::new();
    prune_missing_store_dirs(&settings);
    let has_store_dirs = !settings.stores().is_empty();

    git.set_visible(false);
    find.set_visible(show_list_actions);
    list.set_placeholder(Some(&loading_placeholder()));

    let list_clone = list.clone();
    let git_clone = git.clone();
    let find_clone = find.clone();
    let save_clone = save.clone();
    let overlay_clone = overlay.clone();
    let list_for_disconnect = list_clone.clone();
    let git_for_disconnect = git_clone.clone();
    let find_for_disconnect = find_clone.clone();
    let save_for_disconnect = save_clone.clone();
    spawn_result_task(
        move || match collect_all_password_items_with_options(CollectItemsOptions { show_hidden }) {
            Ok(items) => items,
            Err(err) => {
                log_error(format!("Error scanning pass stores: {err}"));
                Vec::new()
            }
        },
        move |items| {
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
        },
        move || {
            save_for_disconnect.set_visible(false);
            git_for_disconnect.set_visible(should_show_restore_button(
                show_list_actions,
                has_store_dirs,
                true,
            ));
            find_for_disconnect.set_visible(false);
            list_for_disconnect.set_placeholder(Some(&resolved_placeholder(true, has_store_dirs)));
        },
    );
}

pub(crate) fn setup_search_filter(list: &ListBox, search_entry: &SearchEntry) {
    let query = Rc::new(RefCell::new(String::new()));

    let query_for_filter = query.clone();
    list.set_filter_func(move |row: &ListBoxRow| {
        let query = query_for_filter.borrow();
        if query.is_empty() {
            return true;
        }

        if let Some(label) = non_null_to_string_option(row, "label") {
            return label.to_lowercase().contains(query.as_str());
        }

        true
    });

    let query_for_entry = query.clone();
    let list_for_entry = list.clone();
    search_entry.connect_search_changed(move |entry| {
        *query_for_entry.borrow_mut() = entry.text().to_string().to_lowercase();
        list_for_entry.invalidate_filter();
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
    save.set_visible(false);
    if !show_list_actions {
        find.set_visible(false);
        git.set_visible(false);
        return;
    }

    find.set_visible(!empty);
    git.set_visible(should_show_restore_button(
        show_list_actions,
        has_store_dirs,
        empty,
    ));
}

fn prune_missing_store_dirs(settings: &Preferences) {
    if let Err(err) = settings.prune_missing_stores() {
        log_error(format!("Failed to remove missing password stores: {err}"));
    }
}
