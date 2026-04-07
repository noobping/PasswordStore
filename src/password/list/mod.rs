mod placeholder;
mod row;
mod search;

use self::placeholder::{
    register_placeholder_state, show_loading_placeholder, show_resolved_placeholder,
};
use self::row::{
    activate_selected_password_row_action, append_clear_search_action_row,
    append_new_password_action_row, append_password_folder_row, append_password_row,
    SelectedPasswordRowAction,
};
use self::search::{search_controller_for_list, SearchFilterController};
use crate::backend::password_entry_is_readable;
use crate::logging::{log_error, log_info};
use crate::password::model::{
    collect_all_password_items_with_options, CollectItemsOptions, PassEntry,
};
use crate::preferences::{PasswordListSortMode, Preferences};
use crate::store::labels::shortened_store_label_map;
use crate::support::background::spawn_result_task;
use crate::support::git::password_store_git_state_summary;
use crate::support::object_data::{cloned_data, non_null_to_string_option, set_cloned_data};
use crate::support::runtime::has_host_permission;
use crate::support::ui::{clear_list_box, connect_search_list_arrow_navigation};
use adw::glib::{self, Propagation};
use adw::gtk::{
    gdk, Button, EventControllerKey, ListBox, ListBoxRow, PropagationPhase, SearchEntry, Widget,
};
use adw::prelude::*;
use adw::ToastOverlay;
use std::collections::{BTreeMap, HashMap};
use std::rc::Rc;
use std::sync::{Mutex, OnceLock};

#[cfg(all(target_os = "linux", not(feature = "hardening")))]
pub(crate) use self::search::search_password_entries;

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

const PASSWORD_LIST_RENDER_GENERATION_KEY: &str = "password-list-render-generation";
const PASSWORD_ROW_RENDER_BATCH_SIZE: usize = 100;
const PASSWORD_LIST_ROW_KIND_KEY: &str = "password-list-row-kind";
const PASSWORD_LIST_ROW_DEPTH_KEY: &str = "password-list-row-depth";
const PASSWORD_LIST_ROW_STORE_PATH_KEY: &str = "password-list-row-store-path";
const PASSWORD_LIST_ROW_EXPANDED_KEY: &str = "password-list-row-expanded";
const PASSWORD_LIST_ROW_KIND_ENTRY: &str = "entry";
const PASSWORD_LIST_ROW_KIND_FOLDER: &str = "folder";
const PASSWORD_LIST_ROW_KIND_NEW_PASSWORD_ACTION: &str = "new-password-action";
const PASSWORD_LIST_ROW_KIND_CLEAR_SEARCH_ACTION: &str = "clear-search-action";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum PasswordListActionRowKind {
    NewPassword,
    ClearSearch,
}

impl PasswordListActionRowKind {
    const fn storage_key(self) -> &'static str {
        match self {
            Self::NewPassword => PASSWORD_LIST_ROW_KIND_NEW_PASSWORD_ACTION,
            Self::ClearSearch => PASSWORD_LIST_ROW_KIND_CLEAR_SEARCH_ACTION,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum RenderedPasswordListRow {
    Folder {
        store_path: String,
        folder_path: String,
        depth: usize,
    },
    Entry {
        item: PassEntry,
        readable: bool,
        depth: usize,
    },
}

#[derive(Debug, Default)]
struct PasswordFolderTree {
    folders: BTreeMap<String, PasswordFolderTree>,
    entries: Vec<(PassEntry, bool)>,
}

#[derive(Clone)]
struct PasswordListRenderContext {
    store_labels: Rc<HashMap<String, String>>,
    sort_mode: PasswordListSortMode,
    has_store_dirs: bool,
    generation: u64,
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

const fn list_action_context(
    show_list_actions: bool,
    has_store_dirs: bool,
    contents: ListContents,
    git_available: bool,
) -> ListActionContext {
    ListActionContext {
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
        contents,
        git: if git_available {
            GitAvailability::Available
        } else {
            GitAvailability::Unavailable
        },
    }
}

pub fn load_passwords_async(
    list: &ListBox,
    actions: &PasswordListActions,
    overlay: &ToastOverlay,
    should_show_list_actions: Rc<dyn Fn() -> bool>,
    show_hidden: bool,
    show_duplicates: bool,
) {
    clear_list_box(list);
    let render_generation = start_password_list_render_cycle(list);

    let settings = Preferences::new();
    prune_missing_store_dirs(&settings);
    let has_store_dirs = !settings.stores().is_empty();
    let sort_mode = settings.password_list_sort_mode();
    let store_labels = Rc::new(shortened_store_label_map(&settings.store_roots()));
    if let Some(controller) = search_controller_for_list(list) {
        controller.begin_reload(has_store_dirs);
    }
    let git_available = has_host_permission();
    log_store_git_state(&settings);

    if should_show_list_actions() {
        actions.git.set_visible(false);
        actions.store.set_visible(false);
        actions.add.set_visible(has_store_dirs);
        actions.find.set_visible(true);
    }
    show_loading_placeholder(list);

    let list_clone = list.clone();
    let actions_clone = actions.clone();
    let overlay_clone = overlay.clone();
    let list_for_disconnect = list_clone.clone();
    let actions_for_disconnect = actions_clone.clone();
    let should_show_list_actions_for_result = should_show_list_actions.clone();
    let should_show_list_actions_for_disconnect = should_show_list_actions.clone();
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
            if !password_list_render_cycle_is_current(&list_clone, render_generation) {
                return;
            }

            let show_list_actions = should_show_list_actions_for_result();
            let context = list_action_context(
                show_list_actions,
                has_store_dirs,
                if items.is_empty() {
                    ListContents::Empty
                } else {
                    ListContents::Populated
                },
                git_available,
            );
            render_password_rows_in_batches(
                &list_clone,
                &overlay_clone,
                items,
                PasswordListRenderContext {
                    store_labels: store_labels.clone(),
                    sort_mode,
                    has_store_dirs,
                    generation: render_generation,
                },
                {
                    let list = list_clone.clone();
                    let actions = actions_clone.clone();
                    move || {
                        if show_list_actions {
                            update_list_actions(&actions, context);
                        }
                        if let Some(controller) = search_controller_for_list(&list) {
                            controller.finish_reload(&list);
                        } else {
                            show_resolved_placeholder(
                                &list,
                                matches!(context.contents, ListContents::Empty),
                                has_store_dirs,
                            );
                        }
                        autofocus_first_password_list_row_if_needed(&list);
                    }
                },
            );
        },
        move || {
            if !password_list_render_cycle_is_current(&list_for_disconnect, render_generation) {
                return;
            }

            let show_list_actions = should_show_list_actions_for_disconnect();
            let context = list_action_context(
                show_list_actions,
                has_store_dirs,
                ListContents::Empty,
                git_available,
            );
            if show_list_actions {
                update_list_actions(&actions_for_disconnect, context);
            }
            if let Some(controller) = search_controller_for_list(&list_for_disconnect) {
                controller.finish_reload_failure(&list_for_disconnect);
            } else {
                show_resolved_placeholder(&list_for_disconnect, true, has_store_dirs);
            }
        },
    );
}

fn render_password_rows_in_batches(
    list: &ListBox,
    overlay: &ToastOverlay,
    items: Vec<(PassEntry, bool)>,
    render_context: PasswordListRenderContext,
    on_complete: impl FnOnce() + 'static,
) {
    let rows = build_password_list_rows(items, render_context.sort_mode);
    let show_new_password_action =
        should_append_new_password_action_row(render_context.has_store_dirs, !rows.is_empty());
    let show_clear_search_action = should_append_clear_search_action_row(!rows.is_empty());
    if rows.is_empty() {
        on_complete();
        return;
    }

    let list = list.clone();
    let overlay = overlay.clone();
    let store_labels = render_context.store_labels;
    let generation = render_context.generation;
    let mut rows = rows.into_iter();
    let mut on_complete = Some(on_complete);
    glib::idle_add_local(move || {
        if !password_list_render_cycle_is_current(&list, generation) {
            return glib::ControlFlow::Break;
        }

        for _ in 0..PASSWORD_ROW_RENDER_BATCH_SIZE {
            let Some(row) = rows.next() else {
                if show_new_password_action {
                    append_new_password_action_row(&list);
                }
                if show_clear_search_action {
                    append_clear_search_action_row(&list);
                }
                if let Some(on_complete) = on_complete.take() {
                    on_complete();
                }
                return glib::ControlFlow::Break;
            };

            match row {
                RenderedPasswordListRow::Folder {
                    store_path,
                    folder_path,
                    depth,
                } => {
                    let store_label = store_labels
                        .get(&store_path)
                        .map_or(store_path.as_str(), String::as_str);
                    append_password_folder_row(
                        &list,
                        &store_path,
                        password_list_folder_title(&folder_path),
                        &password_list_folder_subtitle(store_label, &folder_path),
                        depth,
                    );
                }
                RenderedPasswordListRow::Entry {
                    item,
                    readable,
                    depth,
                } => append_password_row(
                    &list,
                    item,
                    readable,
                    &overlay,
                    store_labels.clone(),
                    depth,
                ),
            }
        }

        glib::ControlFlow::Continue
    });
}

fn build_password_list_rows(
    items: Vec<(PassEntry, bool)>,
    sort_mode: PasswordListSortMode,
) -> Vec<RenderedPasswordListRow> {
    match sort_mode {
        PasswordListSortMode::Filename => items
            .into_iter()
            .map(|(item, readable)| RenderedPasswordListRow::Entry {
                item,
                readable,
                depth: 0,
            })
            .collect(),
        PasswordListSortMode::StorePath => build_store_path_password_list_rows(items),
    }
}

fn build_store_path_password_list_rows(
    items: Vec<(PassEntry, bool)>,
) -> Vec<RenderedPasswordListRow> {
    let mut rows = Vec::new();
    let mut current_store_path = None::<String>;
    let mut tree = PasswordFolderTree::default();

    for (item, readable) in items {
        if current_store_path.as_deref() != Some(item.store_path.as_str()) {
            if let Some(store_path) = current_store_path.replace(item.store_path.clone()) {
                append_store_folder_rows(&mut rows, &store_path, &tree, 0, None);
                tree = PasswordFolderTree::default();
            }
        }

        insert_password_tree_entry(&mut tree, item, readable);
    }

    if let Some(store_path) = current_store_path {
        append_store_folder_rows(&mut rows, &store_path, &tree, 0, None);
    }

    rows
}

fn insert_password_tree_entry(tree: &mut PasswordFolderTree, item: PassEntry, readable: bool) {
    let mut node = tree;
    for segment in password_list_folder_segments(&item.relative_path) {
        node = node.folders.entry(segment).or_default();
    }
    node.entries.push((item, readable));
}

fn append_store_folder_rows(
    rows: &mut Vec<RenderedPasswordListRow>,
    store_path: &str,
    tree: &PasswordFolderTree,
    depth: usize,
    parent_path: Option<&str>,
) {
    enum RenderTask<'a> {
        VisitNode {
            tree: &'a PasswordFolderTree,
            depth: usize,
            parent_path: Option<String>,
        },
        PushFolder {
            folder_path: String,
            depth: usize,
        },
        PushEntry {
            item: &'a PassEntry,
            readable: bool,
            depth: usize,
        },
    }

    let store_path = store_path.to_string();
    let mut tasks = vec![RenderTask::VisitNode {
        tree,
        depth,
        parent_path: parent_path.map(str::to_string),
    }];

    while let Some(task) = tasks.pop() {
        match task {
            RenderTask::VisitNode {
                tree,
                depth,
                parent_path,
            } => {
                for (item, readable) in tree.entries.iter().rev() {
                    tasks.push(RenderTask::PushEntry {
                        item,
                        readable: *readable,
                        depth,
                    });
                }

                for (segment, child) in tree.folders.iter().rev() {
                    let folder_path = parent_path
                        .as_deref()
                        .map(|parent_path| format!("{parent_path}/{segment}"))
                        .unwrap_or_else(|| segment.clone());
                    tasks.push(RenderTask::VisitNode {
                        tree: child,
                        depth: depth + 1,
                        parent_path: Some(folder_path.clone()),
                    });
                    tasks.push(RenderTask::PushFolder { folder_path, depth });
                }
            }
            RenderTask::PushFolder { folder_path, depth } => {
                rows.push(RenderedPasswordListRow::Folder {
                    store_path: store_path.clone(),
                    folder_path,
                    depth,
                });
            }
            RenderTask::PushEntry {
                item,
                readable,
                depth,
            } => {
                rows.push(RenderedPasswordListRow::Entry {
                    item: item.clone(),
                    readable,
                    depth,
                });
            }
        }
    }
}

fn password_list_folder_segments(relative_path: &str) -> Vec<String> {
    relative_path
        .trim_end_matches('/')
        .split('/')
        .filter(|segment| !segment.is_empty())
        .map(str::to_string)
        .collect()
}

fn password_list_folder_title(folder_path: &str) -> &str {
    folder_path.rsplit('/').next().unwrap_or(folder_path)
}

fn password_list_folder_subtitle(store_label: &str, folder_path: &str) -> String {
    match folder_path.rsplit_once('/') {
        Some((parent, _)) => format!("{store_label}/{parent}/"),
        None => store_label.to_string(),
    }
}

const fn should_append_new_password_action_row(
    has_store_dirs: bool,
    has_password_rows: bool,
) -> bool {
    has_store_dirs && has_password_rows
}

const fn should_append_clear_search_action_row(has_password_rows: bool) -> bool {
    has_password_rows
}

const fn collect_items_options(show_hidden: bool, show_duplicates: bool) -> CollectItemsOptions {
    CollectItemsOptions {
        show_hidden,
        show_duplicates,
    }
}

pub fn setup_search_filter(
    list: &ListBox,
    search_entry: &SearchEntry,
    header_focus_target: &Widget,
    placeholder_stack: &adw::gtk::Stack,
    placeholder_status: &adw::StatusPage,
    placeholder_spinner: &adw::gtk::Spinner,
    list_view: &adw::gtk::ScrolledWindow,
) {
    register_placeholder_state(
        list,
        placeholder_stack,
        placeholder_status,
        placeholder_spinner,
        list_view,
    );
    let controller = SearchFilterController::new();
    controller.register_for_list(list);

    let controller_for_filter = controller.clone();
    list.set_filter_func(move |row: &ListBoxRow| controller_for_filter.matches_row(row));

    let controller_for_entry = controller;
    let list_for_entry = list.clone();
    search_entry.connect_search_changed(move |entry| {
        controller_for_entry.update_query(entry.text().as_str());
        controller_for_entry.refresh_row_visibility(&list_for_entry);
        controller_for_entry.start_indexing_if_needed(&list_for_entry);
        list_for_entry.invalidate_filter();
        controller_for_entry.update_placeholder(&list_for_entry);
    });

    connect_search_list_arrow_navigation(list, search_entry, password_list_row_is_focusable);
    connect_home_list_up_navigation(list, search_entry, header_focus_target);
}

pub fn clear_password_search(search_entry: &SearchEntry, list: &ListBox) {
    if search_entry.text().is_empty() {
        return;
    }

    search_entry.set_text("");
    if !focus_first_password_list_row(list) {
        search_entry.grab_focus();
    }
}

pub fn connect_selected_pass_file_shortcuts(list: &ListBox, overlay: &ToastOverlay) {
    let controller = EventControllerKey::new();
    controller.set_propagation_phase(PropagationPhase::Capture);
    let list_for_handler = list.clone();
    let overlay = overlay.clone();
    controller.connect_key_pressed(move |_, key, _, modifiers| {
        let Some(action) = selected_pass_file_shortcut_action(key, modifiers) else {
            return Propagation::Proceed;
        };
        if activate_selected_password_row_action(&list_for_handler, &overlay, action) {
            Propagation::Stop
        } else {
            Propagation::Proceed
        }
    });
    list.add_controller(controller);
}

pub fn focus_first_password_list_row(list: &ListBox) -> bool {
    let Some(row) = first_password_list_row(list) else {
        return false;
    };

    list.select_row(Some(&row));
    list.grab_focus();
    row.grab_focus();
    true
}

fn autofocus_first_password_list_row_if_needed(list: &ListBox) {
    let Some(root) = list.root() else {
        return;
    };
    let Some(focus) = adw::gtk::prelude::RootExt::focus(&root) else {
        let _ = focus_first_password_list_row(list);
        return;
    };
    if focus.is_ancestor(list)
        || focus.is::<SearchEntry>()
        || focus.ancestor(SearchEntry::static_type()).is_some()
    {
        return;
    }

    let _ = focus_first_password_list_row(list);
}

fn connect_home_list_up_navigation(
    list: &ListBox,
    search_entry: &SearchEntry,
    header_focus_target: &Widget,
) {
    let header_focus_target = header_focus_target.clone();
    let list_controller = EventControllerKey::new();
    list_controller.set_propagation_phase(PropagationPhase::Capture);
    let list_for_keys = list.clone();
    let search_entry_for_list = search_entry.clone();
    list_controller.connect_key_pressed(move |_, key, _, _| {
        if search_entry_for_list.is_visible() {
            return Propagation::Proceed;
        }

        if matches!(key, gdk::Key::Up | gdk::Key::KP_Up)
            && focused_password_list_row_is_first(&list_for_keys)
        {
            header_focus_target.grab_focus();
            return Propagation::Stop;
        }

        Propagation::Proceed
    });
    list.add_controller(list_controller);
}

fn focused_password_list_row_is_first(list: &ListBox) -> bool {
    let Some(root) = list.root() else {
        return false;
    };
    let Some(focus) = adw::gtk::prelude::RootExt::focus(&root) else {
        return false;
    };
    let Some(focused_row) = focus
        .ancestor(ListBoxRow::static_type())
        .and_then(|widget| widget.downcast::<ListBoxRow>().ok())
    else {
        return false;
    };
    let Some(first_row) = first_password_list_row(list) else {
        return false;
    };

    focused_row.is_ancestor(list) && focused_row.index() == first_row.index()
}

fn first_password_list_row(list: &ListBox) -> Option<ListBoxRow> {
    let mut index = 0;
    loop {
        let row = list.row_at_index(index)?;
        if password_list_row_is_focusable(&row) {
            return Some(row);
        }
        index += 1;
    }
}

fn selected_pass_file_shortcut_action(
    key: gdk::Key,
    modifiers: gdk::ModifierType,
) -> Option<SelectedPasswordRowAction> {
    if has_primary_shortcut_modifier(modifiers) {
        return match key {
            gdk::Key::c | gdk::Key::C => Some(SelectedPasswordRowAction::Copy),
            gdk::Key::m | gdk::Key::M => Some(SelectedPasswordRowAction::MoveWithinStore),
            _ => None,
        };
    }

    if has_plain_shortcut_modifiers(modifiers) {
        return match key {
            gdk::Key::F2 => Some(SelectedPasswordRowAction::RenameFile),
            gdk::Key::Delete | gdk::Key::KP_Delete => Some(SelectedPasswordRowAction::Delete),
            _ => None,
        };
    }

    None
}

fn has_primary_shortcut_modifier(modifiers: gdk::ModifierType) -> bool {
    modifiers.contains(gdk::ModifierType::CONTROL_MASK)
        && !modifiers.contains(gdk::ModifierType::SHIFT_MASK)
        && !modifiers.contains(gdk::ModifierType::ALT_MASK)
        && !modifiers.contains(gdk::ModifierType::SUPER_MASK)
        && !modifiers.contains(gdk::ModifierType::META_MASK)
}

fn has_plain_shortcut_modifiers(modifiers: gdk::ModifierType) -> bool {
    !modifiers.contains(gdk::ModifierType::CONTROL_MASK)
        && !modifiers.contains(gdk::ModifierType::SHIFT_MASK)
        && !modifiers.contains(gdk::ModifierType::ALT_MASK)
        && !modifiers.contains(gdk::ModifierType::SUPER_MASK)
        && !modifiers.contains(gdk::ModifierType::META_MASK)
}

fn password_list_row_is_focusable(row: &ListBoxRow) -> bool {
    row.is_child_visible()
        && (non_null_to_string_option(row, "openable").is_some()
            || password_list_row_is_folder(row)
            || password_list_row_action_kind(row).is_some())
}

pub(crate) fn password_list_row_action_kind(row: &ListBoxRow) -> Option<PasswordListActionRowKind> {
    match non_null_to_string_option(row, PASSWORD_LIST_ROW_KIND_KEY).as_deref() {
        Some(kind) if kind == PasswordListActionRowKind::NewPassword.storage_key() => {
            Some(PasswordListActionRowKind::NewPassword)
        }
        Some(kind) if kind == PasswordListActionRowKind::ClearSearch.storage_key() => {
            Some(PasswordListActionRowKind::ClearSearch)
        }
        _ => None,
    }
}

pub(crate) fn refresh_password_list_filter(list: &ListBox) {
    if let Some(controller) = search_controller_for_list(list) {
        controller.refresh_row_visibility(list);
    }
    list.invalidate_filter();
    if let Some(controller) = search_controller_for_list(list) {
        controller.update_placeholder(list);
    }
}

pub(crate) fn toggle_password_list_folder_row(list: &ListBox, row: &ListBoxRow) -> bool {
    if !password_list_row_is_folder(row) {
        return false;
    }

    if row::toggle_password_folder_row(row) {
        refresh_password_list_filter(list);
        true
    } else {
        false
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

fn start_password_list_render_cycle(list: &ListBox) -> u64 {
    let generation = next_password_list_render_generation(cloned_data(
        list,
        PASSWORD_LIST_RENDER_GENERATION_KEY,
    ));
    set_cloned_data(list, PASSWORD_LIST_RENDER_GENERATION_KEY, generation);
    generation
}

pub(crate) fn password_list_render_generation(list: &ListBox) -> Option<u64> {
    cloned_data(list, PASSWORD_LIST_RENDER_GENERATION_KEY)
}

fn password_list_render_cycle_is_current(list: &ListBox, generation: u64) -> bool {
    cloned_data(list, PASSWORD_LIST_RENDER_GENERATION_KEY) == Some(generation)
}

fn next_password_list_render_generation(current: Option<u64>) -> u64 {
    current.unwrap_or(0_u64).wrapping_add(1).max(1)
}

fn password_list_row_is_folder(row: &ListBoxRow) -> bool {
    non_null_to_string_option(row, PASSWORD_LIST_ROW_KIND_KEY).as_deref()
        == Some(PASSWORD_LIST_ROW_KIND_FOLDER)
}

fn password_list_row_depth(row: &ListBoxRow) -> usize {
    cloned_data(row, PASSWORD_LIST_ROW_DEPTH_KEY).unwrap_or(0)
}

fn password_list_row_store_path(row: &ListBoxRow) -> Option<String> {
    non_null_to_string_option(row, PASSWORD_LIST_ROW_STORE_PATH_KEY)
        .or_else(|| non_null_to_string_option(row, "root"))
}

fn password_list_folder_row_is_expanded(row: &ListBoxRow) -> bool {
    cloned_data(row, PASSWORD_LIST_ROW_EXPANDED_KEY).unwrap_or(false)
}

#[cfg(test)]
mod tests {
    use super::{
        build_password_list_rows, collect_items_options, list_action_visibility,
        next_password_list_render_generation, password_list_folder_segments,
        selected_pass_file_shortcut_action, should_append_new_password_action_row,
        should_show_root_git_button, should_show_root_store_button, GitAvailability,
        ListActionContext, ListActionVisibility, ListActionsMode, ListContents,
        RenderedPasswordListRow, StoreSetup, Visibility,
    };
    use crate::password::list::row::SelectedPasswordRowAction;
    use crate::password::model::{CollectItemsOptions, PassEntry};
    use crate::preferences::PasswordListSortMode;
    use adw::gtk::gdk;

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
    fn bottom_add_row_requires_visible_store_items() {
        assert!(should_append_new_password_action_row(true, true));
        assert!(!should_append_new_password_action_row(true, false));
        assert!(!should_append_new_password_action_row(false, true));
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

    #[test]
    fn password_list_render_cycles_increment_from_one() {
        assert_eq!(next_password_list_render_generation(None), 1);
        assert_eq!(next_password_list_render_generation(Some(1)), 2);
    }

    #[test]
    fn selected_pass_file_shortcuts_match_expected_keys() {
        assert_eq!(
            selected_pass_file_shortcut_action(gdk::Key::c, gdk::ModifierType::CONTROL_MASK),
            Some(SelectedPasswordRowAction::Copy)
        );
        assert_eq!(
            selected_pass_file_shortcut_action(gdk::Key::F2, gdk::ModifierType::empty()),
            Some(SelectedPasswordRowAction::RenameFile)
        );
        assert_eq!(
            selected_pass_file_shortcut_action(gdk::Key::m, gdk::ModifierType::CONTROL_MASK),
            Some(SelectedPasswordRowAction::MoveWithinStore)
        );
        assert_eq!(
            selected_pass_file_shortcut_action(gdk::Key::Delete, gdk::ModifierType::empty()),
            Some(SelectedPasswordRowAction::Delete)
        );
    }

    #[test]
    fn selected_pass_file_shortcuts_ignore_conflicting_modifiers() {
        assert_eq!(
            selected_pass_file_shortcut_action(
                gdk::Key::c,
                gdk::ModifierType::CONTROL_MASK | gdk::ModifierType::SHIFT_MASK,
            ),
            None
        );
        assert_eq!(
            selected_pass_file_shortcut_action(gdk::Key::Delete, gdk::ModifierType::ALT_MASK),
            None
        );
        assert_eq!(
            selected_pass_file_shortcut_action(gdk::Key::m, gdk::ModifierType::empty()),
            None
        );
    }

    #[test]
    fn filename_sort_rows_stay_flat() {
        let rows = build_password_list_rows(
            vec![
                (PassEntry::from_label("/tmp/store", "accounts/github"), true),
                (PassEntry::from_label("/tmp/store", "github"), false),
            ],
            PasswordListSortMode::Filename,
        );

        assert_eq!(
            rows,
            vec![
                RenderedPasswordListRow::Entry {
                    item: PassEntry::from_label("/tmp/store", "accounts/github"),
                    readable: true,
                    depth: 0,
                },
                RenderedPasswordListRow::Entry {
                    item: PassEntry::from_label("/tmp/store", "github"),
                    readable: false,
                    depth: 0,
                },
            ]
        );
    }

    #[test]
    fn store_path_sort_rows_insert_folder_headers_per_store() {
        let rows = build_password_list_rows(
            vec![
                (PassEntry::from_label("/tmp/personal", "github"), true),
                (PassEntry::from_label("/tmp/personal", "work/email"), true),
                (PassEntry::from_label("/tmp/personal", "work/github"), false),
                (PassEntry::from_label("/tmp/work", "work/alice/slack"), true),
                (PassEntry::from_label("/tmp/work", "work/bob/matrix"), true),
            ],
            PasswordListSortMode::StorePath,
        );

        assert_eq!(
            rows,
            vec![
                RenderedPasswordListRow::Folder {
                    store_path: "/tmp/personal".to_string(),
                    folder_path: "work".to_string(),
                    depth: 0,
                },
                RenderedPasswordListRow::Entry {
                    item: PassEntry::from_label("/tmp/personal", "work/email"),
                    readable: true,
                    depth: 1,
                },
                RenderedPasswordListRow::Entry {
                    item: PassEntry::from_label("/tmp/personal", "work/github"),
                    readable: false,
                    depth: 1,
                },
                RenderedPasswordListRow::Entry {
                    item: PassEntry::from_label("/tmp/personal", "github"),
                    readable: true,
                    depth: 0,
                },
                RenderedPasswordListRow::Folder {
                    store_path: "/tmp/work".to_string(),
                    folder_path: "work".to_string(),
                    depth: 0,
                },
                RenderedPasswordListRow::Folder {
                    store_path: "/tmp/work".to_string(),
                    folder_path: "work/alice".to_string(),
                    depth: 1,
                },
                RenderedPasswordListRow::Entry {
                    item: PassEntry::from_label("/tmp/work", "work/alice/slack"),
                    readable: true,
                    depth: 2,
                },
                RenderedPasswordListRow::Folder {
                    store_path: "/tmp/work".to_string(),
                    folder_path: "work/bob".to_string(),
                    depth: 1,
                },
                RenderedPasswordListRow::Entry {
                    item: PassEntry::from_label("/tmp/work", "work/bob/matrix"),
                    readable: true,
                    depth: 2,
                },
            ]
        );
    }

    #[test]
    fn store_path_sort_rows_put_nested_folders_before_direct_files() {
        let rows = build_password_list_rows(
            vec![
                (PassEntry::from_label("/tmp/personal", "work/github"), true),
                (
                    PassEntry::from_label("/tmp/personal", "work/team/email"),
                    true,
                ),
            ],
            PasswordListSortMode::StorePath,
        );

        assert_eq!(
            rows,
            vec![
                RenderedPasswordListRow::Folder {
                    store_path: "/tmp/personal".to_string(),
                    folder_path: "work".to_string(),
                    depth: 0,
                },
                RenderedPasswordListRow::Folder {
                    store_path: "/tmp/personal".to_string(),
                    folder_path: "work/team".to_string(),
                    depth: 1,
                },
                RenderedPasswordListRow::Entry {
                    item: PassEntry::from_label("/tmp/personal", "work/team/email"),
                    readable: true,
                    depth: 2,
                },
                RenderedPasswordListRow::Entry {
                    item: PassEntry::from_label("/tmp/personal", "work/github"),
                    readable: true,
                    depth: 1,
                },
            ]
        );
    }

    #[test]
    fn folder_segments_ignore_empty_path_parts() {
        assert_eq!(
            password_list_folder_segments("work/alice/"),
            vec!["work".to_string(), "alice".to_string()]
        );
        assert!(password_list_folder_segments("").is_empty());
    }

    #[test]
    fn store_path_sort_rows_handle_deep_folder_chains() {
        let folder_depth = 2048;
        let relative_path = std::iter::repeat_n("team", folder_depth)
            .collect::<Vec<_>>()
            .join("/");
        let rows = build_password_list_rows(
            vec![(
                PassEntry::from_label("/tmp/store", &format!("{relative_path}/entry")),
                true,
            )],
            PasswordListSortMode::StorePath,
        );

        assert_eq!(rows.len(), folder_depth + 1);
        assert!(matches!(
            rows.first(),
            Some(RenderedPasswordListRow::Folder { depth: 0, .. })
        ));
        assert!(matches!(
            rows.last(),
            Some(RenderedPasswordListRow::Entry { depth, .. }) if *depth == folder_depth
        ));
    }
}
