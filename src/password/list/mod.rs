mod placeholder;
mod row;

use self::placeholder::{loading_placeholder, resolved_placeholder};
use self::row::append_password_row;
use crate::backend::password_entry_is_readable;
use crate::logging::{log_error, log_info};
use crate::password::model::{collect_all_password_items_with_options, CollectItemsOptions};
use crate::preferences::Preferences;
use crate::support::background::spawn_result_task;
use crate::support::git::password_store_git_state_summary;
use crate::support::object_data::non_null_to_string_option;
use crate::support::runtime::has_host_permission;
use crate::support::ui::clear_list_box;
use adw::glib::Propagation;
use adw::gtk::{
    gdk, Button, EventControllerKey, ListBox, ListBoxRow, PropagationPhase, SearchEntry,
};
use adw::prelude::*;
use adw::ToastOverlay;
use std::cell::RefCell;
use std::rc::Rc;
use std::sync::{Mutex, OnceLock};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Visibility {
    Hidden,
    Visible,
}

impl Visibility {
    const fn is_visible(self) -> bool {
        matches!(self, Self::Visible)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ListActionsMode {
    Hidden,
    Visible,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum StoreSetup {
    Missing,
    Present,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ListContents {
    Empty,
    Populated,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum GitAvailability {
    Unavailable,
    Available,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct ListActionContext {
    actions: ListActionsMode,
    stores: StoreSetup,
    contents: ListContents,
    git: GitAvailability,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct ListActionVisibility {
    add: Visibility,
    find: Visibility,
    git: Visibility,
    store: Visibility,
    save: Visibility,
}

#[derive(Clone)]
pub struct PasswordListActions {
    pub add: Button,
    pub git: Button,
    pub store: Button,
    pub find: Button,
    pub save: Button,
}

impl PasswordListActions {
    pub fn new(add: &Button, git: &Button, store: &Button, find: &Button, save: &Button) -> Self {
        Self {
            add: add.clone(),
            git: git.clone(),
            store: store.clone(),
            find: find.clone(),
            save: save.clone(),
        }
    }
}

const fn list_action_visibility(context: ListActionContext) -> ListActionVisibility {
    if matches!(context.actions, ListActionsMode::Hidden) {
        return ListActionVisibility {
            add: Visibility::Hidden,
            find: Visibility::Hidden,
            git: Visibility::Hidden,
            store: Visibility::Hidden,
            save: Visibility::Hidden,
        };
    }

    ListActionVisibility {
        add: if matches!(context.stores, StoreSetup::Present) {
            Visibility::Visible
        } else {
            Visibility::Hidden
        },
        find: if matches!(context.contents, ListContents::Populated) {
            Visibility::Visible
        } else {
            Visibility::Hidden
        },
        git: if should_show_root_git_button(context) {
            Visibility::Visible
        } else {
            Visibility::Hidden
        },
        store: if should_show_root_store_button(context) {
            Visibility::Visible
        } else {
            Visibility::Hidden
        },
        save: Visibility::Hidden,
    }
}

const fn should_show_root_git_button(context: ListActionContext) -> bool {
    matches!(context.actions, ListActionsMode::Visible)
        && matches!(context.stores, StoreSetup::Missing)
        && matches!(context.git, GitAvailability::Available)
}

const fn should_show_root_store_button(context: ListActionContext) -> bool {
    matches!(context.actions, ListActionsMode::Visible)
        && matches!(context.stores, StoreSetup::Missing)
}

pub fn load_passwords_async(
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
    let git_available = has_host_permission();
    log_store_git_state(&settings);

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
        move || {
            collect_all_password_items_with_options(collect_items_options(
                show_hidden,
                show_duplicates,
            ))
            .into_iter()
            .map(|item| {
                let label = item.label();
                let readable = password_entry_is_readable(&item.store_path, &label);
                (item, readable)
            })
            .collect::<Vec<_>>()
        },
        move |items| {
            let context = ListActionContext {
                actions: if show_list_actions {
                    ListActionsMode::Visible
                } else {
                    ListActionsMode::Hidden
                },
                stores: if has_store_dirs {
                    StoreSetup::Present
                } else {
                    StoreSetup::Missing
                },
                contents: if items.is_empty() {
                    ListContents::Empty
                } else {
                    ListContents::Populated
                },
                git: if git_available {
                    GitAvailability::Available
                } else {
                    GitAvailability::Unavailable
                },
            };
            for (item, readable) in items {
                append_password_row(&list_clone, item, readable, &overlay_clone);
            }

            update_list_actions(&actions_clone, context);
            list_clone.set_placeholder(Some(&resolved_placeholder(
                matches!(context.contents, ListContents::Empty),
                has_store_dirs,
            )));
        },
        move || {
            let context = ListActionContext {
                actions: if show_list_actions {
                    ListActionsMode::Visible
                } else {
                    ListActionsMode::Hidden
                },
                stores: if has_store_dirs {
                    StoreSetup::Present
                } else {
                    StoreSetup::Missing
                },
                contents: ListContents::Empty,
                git: if git_available {
                    GitAvailability::Available
                } else {
                    GitAvailability::Unavailable
                },
            };
            update_list_actions(&actions_for_disconnect, context);
            list_for_disconnect.set_placeholder(Some(&resolved_placeholder(true, has_store_dirs)));
        },
    );
}

const fn collect_items_options(show_hidden: bool, show_duplicates: bool) -> CollectItemsOptions {
    CollectItemsOptions {
        show_hidden,
        show_duplicates,
    }
}

pub fn setup_search_filter(list: &ListBox, search_entry: &SearchEntry) {
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

    let query_for_entry = query;
    let list_for_entry = list.clone();
    search_entry.connect_search_changed(move |entry| {
        *query_for_entry.borrow_mut() = entry.text().to_string().to_lowercase();
        list_for_entry.invalidate_filter();
    });

    connect_search_arrow_navigation(list, search_entry);
}

fn connect_search_arrow_navigation(list: &ListBox, search_entry: &SearchEntry) {
    let search_controller = EventControllerKey::new();
    search_controller.set_propagation_phase(PropagationPhase::Capture);
    let list_for_search = list.clone();
    search_controller.connect_key_pressed(move |_, key, _, _| {
        if matches!(key, gdk::Key::Down | gdk::Key::KP_Down)
            && focus_first_visible_row(&list_for_search)
        {
            return Propagation::Stop;
        }

        Propagation::Proceed
    });
    search_entry.add_controller(search_controller);

    let list_controller = EventControllerKey::new();
    list_controller.set_propagation_phase(PropagationPhase::Capture);
    let list_for_keys = list.clone();
    let search_entry_for_list = search_entry.clone();
    list_controller.connect_key_pressed(move |_, key, _, _| {
        if !search_entry_for_list.is_visible() {
            return Propagation::Proceed;
        }

        if matches!(key, gdk::Key::Up | gdk::Key::KP_Up)
            && selected_row_is_first_visible(&list_for_keys)
        {
            search_entry_for_list.grab_focus();
            return Propagation::Stop;
        }

        Propagation::Proceed
    });
    list.add_controller(list_controller);
}

fn focus_first_visible_row(list: &ListBox) -> bool {
    let Some(row) = first_visible_row(list) else {
        return false;
    };

    list.select_row(Some(&row));
    list.grab_focus();
    row.grab_focus();
    true
}

fn selected_row_is_first_visible(list: &ListBox) -> bool {
    let Some(selected_row) = list.selected_row() else {
        return false;
    };
    let Some(first_row) = first_visible_row(list) else {
        return false;
    };

    selected_row.index() == first_row.index()
}

fn first_visible_row(list: &ListBox) -> Option<ListBoxRow> {
    let mut index = 0;
    loop {
        let row = list.row_at_index(index)?;
        if row.is_visible() {
            return Some(row);
        }
        index += 1;
    }
}

fn update_list_actions(actions: &PasswordListActions, context: ListActionContext) {
    let visibility = list_action_visibility(context);
    actions.add.set_visible(visibility.add.is_visible());
    actions.save.set_visible(visibility.save.is_visible());
    actions.find.set_visible(visibility.find.is_visible());
    actions.git.set_visible(visibility.git.is_visible());
    actions.store.set_visible(visibility.store.is_visible());
}

fn prune_missing_store_dirs(settings: &Preferences) {
    if let Err(err) = settings.prune_missing_stores() {
        log_error(format!("Failed to remove missing password stores: {err}"));
    }
}

fn log_store_git_state(settings: &Preferences) {
    let stores = settings.store_roots();
    let summary = if stores.is_empty() {
        "Password store configuration: no stores are configured.".to_string()
    } else {
        let mut lines = Vec::with_capacity(stores.len() + 1);
        lines.push(format!(
            "Password store configuration: {} configured store(s).",
            stores.len()
        ));
        for store in stores {
            lines.push(password_store_git_state_summary(&store));
        }
        lines.join("\n")
    };

    if store_git_state_summary_changed(&summary) {
        log_info(summary);
    }
}

fn store_git_state_summary_changed(summary: &str) -> bool {
    static LAST_SUMMARY: OnceLock<Mutex<String>> = OnceLock::new();
    let state = LAST_SUMMARY.get_or_init(|| Mutex::new(String::new()));
    let mut last_summary = match state.lock() {
        Ok(summary) => summary,
        Err(poisoned) => poisoned.into_inner(),
    };

    if last_summary.as_str() == summary {
        return false;
    }

    last_summary.clear();
    last_summary.push_str(summary);
    true
}

#[cfg(test)]
mod tests {
    use super::{
        collect_items_options, list_action_visibility, should_show_root_git_button,
        should_show_root_store_button, GitAvailability, ListActionContext, ListActionVisibility,
        ListActionsMode, ListContents, StoreSetup, Visibility,
    };
    use crate::password::model::CollectItemsOptions;

    fn expected_root_store_button_visibility() -> bool {
        true
    }

    fn expected_root_action_visibility_for_empty_store_setup() -> ListActionVisibility {
        ListActionVisibility {
            add: Visibility::Hidden,
            find: Visibility::Hidden,
            git: Visibility::Visible,
            store: Visibility::Visible,
            save: Visibility::Hidden,
        }
    }

    fn expected_store_visibility_without_git() -> bool {
        true
    }

    #[test]
    fn root_shortcut_buttons_are_hidden_for_existing_store_setup() {
        let existing_store_context = ListActionContext {
            actions: ListActionsMode::Visible,
            stores: StoreSetup::Present,
            contents: ListContents::Populated,
            git: GitAvailability::Available,
        };
        let hidden_actions_context = ListActionContext {
            actions: ListActionsMode::Hidden,
            stores: StoreSetup::Missing,
            contents: ListContents::Empty,
            git: GitAvailability::Available,
        };
        assert!(!should_show_root_git_button(existing_store_context));
        assert!(!should_show_root_store_button(existing_store_context));
        assert!(!should_show_root_git_button(hidden_actions_context));
        assert!(!should_show_root_store_button(hidden_actions_context));
    }

    #[test]
    fn root_shortcut_buttons_match_the_current_build() {
        let context = ListActionContext {
            actions: ListActionsMode::Visible,
            stores: StoreSetup::Missing,
            contents: ListContents::Empty,
            git: GitAvailability::Available,
        };
        assert!(should_show_root_git_button(context));
        assert_eq!(
            should_show_root_store_button(context),
            expected_root_store_button_visibility()
        );
    }

    #[test]
    fn root_git_shortcut_button_requires_runtime_git_availability() {
        assert!(!should_show_root_git_button(ListActionContext {
            actions: ListActionsMode::Visible,
            stores: StoreSetup::Missing,
            contents: ListContents::Empty,
            git: GitAvailability::Unavailable,
        }));
    }

    #[test]
    fn list_actions_hide_everything_when_list_actions_are_disabled() {
        assert_eq!(
            list_action_visibility(ListActionContext {
                actions: ListActionsMode::Hidden,
                stores: StoreSetup::Present,
                contents: ListContents::Populated,
                git: GitAvailability::Available,
            }),
            ListActionVisibility {
                add: Visibility::Hidden,
                find: Visibility::Hidden,
                git: Visibility::Hidden,
                store: Visibility::Hidden,
                save: Visibility::Hidden,
            }
        );
    }

    #[test]
    fn list_actions_show_find_only_when_items_exist() {
        assert_eq!(
            list_action_visibility(ListActionContext {
                actions: ListActionsMode::Visible,
                stores: StoreSetup::Present,
                contents: ListContents::Populated,
                git: GitAvailability::Available,
            }),
            ListActionVisibility {
                add: Visibility::Visible,
                find: Visibility::Visible,
                git: Visibility::Hidden,
                store: Visibility::Hidden,
                save: Visibility::Hidden,
            }
        );
        assert_eq!(
            list_action_visibility(ListActionContext {
                actions: ListActionsMode::Visible,
                stores: StoreSetup::Present,
                contents: ListContents::Empty,
                git: GitAvailability::Available,
            }),
            ListActionVisibility {
                add: Visibility::Visible,
                find: Visibility::Hidden,
                git: Visibility::Hidden,
                store: Visibility::Hidden,
                save: Visibility::Hidden,
            }
        );
    }

    #[test]
    fn list_actions_show_the_build_specific_root_shortcut_for_empty_missing_store_setup() {
        assert_eq!(
            list_action_visibility(ListActionContext {
                actions: ListActionsMode::Visible,
                stores: StoreSetup::Missing,
                contents: ListContents::Empty,
                git: GitAvailability::Available,
            }),
            expected_root_action_visibility_for_empty_store_setup()
        );
    }

    #[test]
    fn list_actions_hide_git_when_runtime_git_is_unavailable() {
        assert_eq!(
            list_action_visibility(ListActionContext {
                actions: ListActionsMode::Visible,
                stores: StoreSetup::Missing,
                contents: ListContents::Empty,
                git: GitAvailability::Unavailable,
            }),
            ListActionVisibility {
                add: Visibility::Hidden,
                find: Visibility::Hidden,
                git: Visibility::Hidden,
                store: if expected_store_visibility_without_git() {
                    Visibility::Visible
                } else {
                    Visibility::Hidden
                },
                save: Visibility::Hidden,
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
