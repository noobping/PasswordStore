mod placeholder;
mod row;

use self::placeholder::{loading_placeholder, resolved_placeholder};
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

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct ListActionVisibility {
    add_visible: bool,
    find_visible: bool,
    git_visible: bool,
    store_visible: bool,
    save_visible: bool,
}

#[derive(Clone)]
pub(crate) struct PasswordListActions {
    pub(crate) add: Button,
    pub(crate) git: Button,
    pub(crate) store: Button,
    pub(crate) find: Button,
    pub(crate) save: Button,
}

impl PasswordListActions {
    pub(crate) fn new(
        add: &Button,
        git: &Button,
        store: &Button,
        find: &Button,
        save: &Button,
    ) -> Self {
        Self {
            add: add.clone(),
            git: git.clone(),
            store: store.clone(),
            find: find.clone(),
            save: save.clone(),
        }
    }
}

fn list_action_visibility(
    show_list_actions: bool,
    has_store_dirs: bool,
    empty: bool,
) -> ListActionVisibility {
    if !show_list_actions {
        return ListActionVisibility {
            add_visible: false,
            find_visible: false,
            git_visible: false,
            store_visible: false,
            save_visible: false,
        };
    }

    ListActionVisibility {
        add_visible: has_store_dirs,
        find_visible: !empty,
        git_visible: should_show_root_git_button(show_list_actions, has_store_dirs),
        store_visible: should_show_root_store_button(show_list_actions, has_store_dirs),
        save_visible: false,
    }
}

fn should_show_root_git_button(show_list_actions: bool, has_store_dirs: bool) -> bool {
    show_list_actions && !has_store_dirs
}

fn should_show_root_store_button(show_list_actions: bool, has_store_dirs: bool) -> bool {
    #[cfg(feature = "flatpak")]
    {
        show_list_actions && !has_store_dirs
    }
    #[cfg(not(feature = "flatpak"))]
    {
        let _ = (show_list_actions, has_store_dirs);
        false
    }
}

pub(crate) fn load_passwords_async(
    list: &ListBox,
    actions: &PasswordListActions,
    overlay: &ToastOverlay,
    show_list_actions: bool,
    show_hidden: bool,
    show_duplicates: bool,
) {
    clear_list_box(list);

    let settings = Preferences::new();
    prune_missing_store_dirs(&settings);
    let has_store_dirs = !settings.stores().is_empty();

    actions.git.set_visible(false);
    actions.store.set_visible(false);
    actions.add.set_visible(show_list_actions && has_store_dirs);
    actions.find.set_visible(show_list_actions);
    list.set_placeholder(Some(&loading_placeholder()));

    let list_clone = list.clone();
    let actions_clone = actions.clone();
    let overlay_clone = overlay.clone();
    let list_for_disconnect = list_clone.clone();
    let actions_for_disconnect = actions_clone.clone();
    spawn_result_task(
        move || match collect_all_password_items_with_options(collect_items_options(
            show_hidden,
            show_duplicates,
        )) {
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

            update_list_actions(&actions_clone, show_list_actions, has_store_dirs, empty);
            list_clone.set_placeholder(Some(&resolved_placeholder(empty, has_store_dirs)));
        },
        move || {
            actions_for_disconnect
                .add
                .set_visible(show_list_actions && has_store_dirs);
            actions_for_disconnect.save.set_visible(false);
            actions_for_disconnect
                .git
                .set_visible(should_show_root_git_button(
                    show_list_actions,
                    has_store_dirs,
                ));
            actions_for_disconnect
                .store
                .set_visible(should_show_root_store_button(
                    show_list_actions,
                    has_store_dirs,
                ));
            actions_for_disconnect.find.set_visible(false);
            list_for_disconnect.set_placeholder(Some(&resolved_placeholder(true, has_store_dirs)));
        },
    );
}

fn collect_items_options(show_hidden: bool, show_duplicates: bool) -> CollectItemsOptions {
    CollectItemsOptions {
        show_hidden,
        show_duplicates,
    }
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
    actions: &PasswordListActions,
    show_list_actions: bool,
    has_store_dirs: bool,
    empty: bool,
) {
    let visibility = list_action_visibility(show_list_actions, has_store_dirs, empty);
    actions.add.set_visible(visibility.add_visible);
    actions.save.set_visible(visibility.save_visible);
    actions.find.set_visible(visibility.find_visible);
    actions.git.set_visible(visibility.git_visible);
    actions.store.set_visible(visibility.store_visible);
}

fn prune_missing_store_dirs(settings: &Preferences) {
    if let Err(err) = settings.prune_missing_stores() {
        log_error(format!("Failed to remove missing password stores: {err}"));
    }
}

#[cfg(test)]
mod tests {
    use super::{
        collect_items_options, list_action_visibility, should_show_root_git_button,
        should_show_root_store_button, ListActionVisibility,
    };
    use crate::password::model::CollectItemsOptions;

    #[test]
    fn root_shortcut_buttons_are_hidden_for_existing_store_setup() {
        assert!(!should_show_root_git_button(true, true));
        assert!(!should_show_root_store_button(true, true));
        assert!(!should_show_root_git_button(false, false));
        assert!(!should_show_root_store_button(false, false));
    }

    #[test]
    fn root_shortcut_buttons_match_the_current_build() {
        #[cfg(not(feature = "flatpak"))]
        {
            assert!(should_show_root_git_button(true, false));
            assert!(!should_show_root_store_button(true, false));
        }

        #[cfg(feature = "flatpak")]
        {
            assert!(should_show_root_git_button(true, false));
            assert!(should_show_root_store_button(true, false));
        }
    }

    #[test]
    fn list_actions_hide_everything_when_list_actions_are_disabled() {
        assert_eq!(
            list_action_visibility(false, true, false),
            ListActionVisibility {
                add_visible: false,
                find_visible: false,
                git_visible: false,
                store_visible: false,
                save_visible: false,
            }
        );
    }

    #[test]
    fn list_actions_show_find_only_when_items_exist() {
        assert_eq!(
            list_action_visibility(true, true, false),
            ListActionVisibility {
                add_visible: true,
                find_visible: true,
                git_visible: false,
                store_visible: false,
                save_visible: false,
            }
        );
        assert_eq!(
            list_action_visibility(true, true, true),
            ListActionVisibility {
                add_visible: true,
                find_visible: false,
                git_visible: false,
                store_visible: false,
                save_visible: false,
            }
        );
    }

    #[test]
    fn list_actions_show_the_build_specific_root_shortcut_for_empty_missing_store_setup() {
        #[cfg(not(feature = "flatpak"))]
        assert_eq!(
            list_action_visibility(true, false, true),
            ListActionVisibility {
                add_visible: false,
                find_visible: false,
                git_visible: true,
                store_visible: false,
                save_visible: false,
            }
        );

        #[cfg(feature = "flatpak")]
        assert_eq!(
            list_action_visibility(true, false, true),
            ListActionVisibility {
                add_visible: false,
                find_visible: false,
                git_visible: true,
                store_visible: true,
                save_visible: false,
            }
        );
    }

    #[test]
    fn collect_items_options_keeps_hidden_and_duplicate_flags_separate() {
        assert_eq!(
            collect_items_options(false, true),
            CollectItemsOptions {
                show_hidden: false,
                show_duplicates: true,
            }
        );
        assert_eq!(
            collect_items_options(true, false),
            CollectItemsOptions {
                show_hidden: true,
                show_duplicates: false,
            }
        );
    }
}
