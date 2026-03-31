use super::search::{SearchRowFieldIndexState, SEARCH_FIELDS_KEY};
use super::{
    refresh_password_list_filter, PASSWORD_LIST_ROW_DEPTH_KEY, PASSWORD_LIST_ROW_EXPANDED_KEY,
    PASSWORD_LIST_ROW_KIND_ENTRY, PASSWORD_LIST_ROW_KIND_FOLDER, PASSWORD_LIST_ROW_KIND_KEY,
    PASSWORD_LIST_ROW_STORE_PATH_KEY,
};
use crate::backend::rename_password_entry;
use crate::clipboard::copy_password_entry_to_clipboard;
use crate::i18n::gettext;
use crate::logging::log_error;
use crate::password::model::{OpenPassFile, PassEntry};
use crate::password::undo::{
    delete_entry_with_optional_undo, move_entry_between_stores_action, move_entry_to_store,
    push_undo_action, rename_entry_action, unavailable_undo_action, unavailable_undo_message,
    UndoError,
};
use crate::preferences::Preferences;
use crate::store::labels::{shortened_store_label_for_path, shortened_store_labels};
use crate::support::background::spawn_result_task;
use crate::support::object_data::{cloned_data, set_cloned_data, set_string_data};
use crate::support::uri::launch_default_uri;
use crate::support::ui::{dim_label_icon, flat_icon_button, flat_icon_button_with_tooltip};
use crate::window::create_main_window;
use adw::gio::{Menu, SimpleAction, SimpleActionGroup};
use adw::gtk::{
    Button, DropDown, Image, ListBox, ListBoxRow, MenuButton, Stack, StringList,
    INVALID_LIST_POSITION,
};
use adw::prelude::*;
use adw::{ActionRow, EntryRow, Toast, ToastOverlay};
use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::rc::Rc;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum TextEditMode {
    RenameFile,
    MoveWithinStore,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum SelectedPasswordRowAction {
    Copy,
    RenameFile,
    MoveWithinStore,
    Delete,
}

const UNREADABLE_PASSWORD_ROW_TOOLTIP: &str =
    "This item can't be opened with the private keys currently available in the app. File actions are still available, but copy and move-to-store are disabled until a compatible private key is available.";
const PASSWORD_ROW_STATE_KEY: &str = "password-row-state";
const PASSWORD_FOLDER_ROW_STATE_KEY: &str = "password-folder-row-state";
const OPEN_IN_NEW_WINDOW_LABEL: &str = "Open in New Window";
const PASSWORD_LIST_INDENT_WIDTH: i32 = 18;
const PASSWORD_LIST_MAX_INDENT_DEPTH: usize = 8;

fn password_row_menu_entries(readable: bool) -> Vec<(&'static str, &'static str)> {
    let mut entries = Vec::new();
    if readable {
        entries.push((OPEN_IN_NEW_WINDOW_LABEL, "entry.open-new-window"));
    }
    entries.push(("Rename pass file", "entry.rename-file"));
    entries.push(("Move pass file", "entry.move"));
    if readable {
        entries.push(("Move to store", "entry.move-store"));
    }
    entries.push(("Open in File Manager", "entry.open-in-file-manager"));
    entries.push(("Delete", "entry.delete"));
    entries
}

#[derive(Clone)]
struct PasswordRowState {
    item: Rc<RefCell<PassEntry>>,
    readable: bool,
    row: ListBoxRow,
    stack: Stack,
    action_row: ActionRow,
    store_labels: Rc<HashMap<String, String>>,
    text_edit_row: EntryRow,
    store_edit_row: ActionRow,
    store_dropdown: DropDown,
    store_roots: Rc<RefCell<Vec<String>>>,
    text_edit_mode: Rc<RefCell<TextEditMode>>,
}

#[derive(Clone)]
struct PasswordFolderRowState {
    row: ListBoxRow,
    folder_icon: Image,
    expand_icon: Image,
    expanded: Rc<Cell<bool>>,
}

pub(super) fn append_password_row(
    list: &ListBox,
    item: PassEntry,
    readable: bool,
    overlay: &ToastOverlay,
    store_labels: Rc<HashMap<String, String>>,
    depth: usize,
) {
    let row = ListBoxRow::new();
    row.set_activatable(readable);
    set_string_data(
        &row,
        PASSWORD_LIST_ROW_KIND_KEY,
        PASSWORD_LIST_ROW_KIND_ENTRY.to_string(),
    );
    set_cloned_data(&row, PASSWORD_LIST_ROW_DEPTH_KEY, depth);
    let stack = Stack::new();

    let action_row = ActionRow::builder()
        .title(item.basename.clone())
        .subtitle(item.relative_path.clone())
        .subtitle_lines(1)
        .activatable(readable)
        .build();
    action_row.set_margin_start(password_list_indent(depth));
    let unreadable_icon = build_unreadable_password_icon(!readable);
    let copy_button = flat_icon_button("edit-copy-symbolic");
    copy_button.set_visible(readable);
    let menu_button = MenuButton::builder()
        .icon_name("view-more-symbolic")
        .has_frame(false)
        .css_classes(vec!["flat"])
        .build();
    action_row.add_prefix(&unreadable_icon);
    action_row.add_suffix(&copy_button);
    action_row.add_suffix(&menu_button);

    let text_edit_row = EntryRow::new();
    text_edit_row.set_show_apply_button(true);
    let text_cancel_button = flat_icon_button_with_tooltip("window-close-symbolic", "Cancel");
    text_edit_row.add_suffix(&text_cancel_button);

    let store_edit_row = ActionRow::builder().title(gettext("Move to store")).build();
    store_edit_row.set_activatable(false);
    let store_dropdown = DropDown::from_strings(&[]);
    store_dropdown.set_valign(adw::gtk::Align::Center);
    let store_apply_button = flat_icon_button_with_tooltip("document-save-symbolic", "Move");
    let store_cancel_button = flat_icon_button_with_tooltip("window-close-symbolic", "Cancel");
    store_edit_row.add_suffix(&store_dropdown);
    store_edit_row.add_suffix(&store_apply_button);
    store_edit_row.add_suffix(&store_cancel_button);

    stack.add_named(&action_row, Some("display"));
    stack.add_named(&text_edit_row, Some("text-edit"));
    stack.add_named(&store_edit_row, Some("store-edit"));
    stack.set_visible_child_name("display");
    row.set_child(Some(&stack));

    let state = PasswordRowState {
        item: Rc::new(RefCell::new(item)),
        readable,
        row: row.clone(),
        stack,
        action_row,
        store_labels,
        text_edit_row,
        store_edit_row,
        store_dropdown,
        store_roots: Rc::new(RefCell::new(Vec::new())),
        text_edit_mode: Rc::new(RefCell::new(TextEditMode::RenameFile)),
    };
    set_cloned_data(&row, PASSWORD_ROW_STATE_KEY, state.clone());
    sync_password_row_display(&state);
    set_cloned_data(&row, SEARCH_FIELDS_KEY, SearchRowFieldIndexState::Unindexed);

    configure_password_row_menu(&menu_button, &state, readable, list, overlay);
    connect_copy_action(&state, &copy_button, overlay);
    connect_text_edit_actions(&state, list, &text_cancel_button, overlay);
    connect_store_move_actions(
        &state,
        list,
        &store_apply_button,
        &store_cancel_button,
        overlay,
    );

    list.append(&row);
}

pub(super) fn append_password_folder_row(
    list: &ListBox,
    store_path: &str,
    title: &str,
    subtitle: &str,
    depth: usize,
) {
    let row = ListBoxRow::new();
    row.set_activatable(true);

    let action_row = ActionRow::builder()
        .title(title)
        .subtitle(subtitle)
        .subtitle_lines(1)
        .activatable(true)
        .build();
    action_row.set_margin_start(password_list_indent(depth));
    let folder_icon = dim_label_icon("folder-open-symbolic");
    let expand_icon = dim_label_icon("go-down-symbolic");
    action_row.add_prefix(&folder_icon);
    action_row.add_suffix(&expand_icon);

    row.set_child(Some(&action_row));
    set_string_data(
        &row,
        PASSWORD_LIST_ROW_KIND_KEY,
        PASSWORD_LIST_ROW_KIND_FOLDER.to_string(),
    );
    set_cloned_data(&row, PASSWORD_LIST_ROW_DEPTH_KEY, depth);
    set_string_data(
        &row,
        PASSWORD_LIST_ROW_STORE_PATH_KEY,
        store_path.to_string(),
    );
    let state = PasswordFolderRowState {
        row: row.clone(),
        folder_icon,
        expand_icon,
        expanded: Rc::new(Cell::new(false)),
    };
    set_cloned_data(&row, PASSWORD_FOLDER_ROW_STATE_KEY, state.clone());
    sync_password_folder_row_display(&state);
    list.append(&row);
}

pub(super) fn toggle_password_folder_row(row: &ListBoxRow) -> bool {
    let Some(state): Option<PasswordFolderRowState> =
        cloned_data(row, PASSWORD_FOLDER_ROW_STATE_KEY)
    else {
        return false;
    };

    state.expanded.set(!state.expanded.get());
    sync_password_folder_row_display(&state);
    true
}

fn configure_password_row_menu(
    menu_button: &MenuButton,
    state: &PasswordRowState,
    readable: bool,
    list: &ListBox,
    overlay: &ToastOverlay,
) {
    let menu = Menu::new();
    for (label, action) in password_row_menu_entries(readable) {
        menu.append(Some(&gettext(label)), Some(action));
    }
    menu_button.set_menu_model(Some(&menu));

    let actions = SimpleActionGroup::new();

    {
        let state = state.clone();
        let overlay = overlay.clone();
        add_menu_action(&actions, "open-new-window", move || {
            open_entry_in_new_window(&state, &overlay);
        });
    }

    {
        let state = state.clone();
        add_menu_action(&actions, "rename-file", move || {
            let entry = state.item.borrow().clone();
            enter_text_edit_mode(&state, TextEditMode::RenameFile, &entry.basename);
        });
    }

    {
        let state = state.clone();
        add_menu_action(&actions, "move", move || {
            let current_dir = {
                let entry = state.item.borrow();
                entry.relative_path.trim_end_matches('/').to_string()
            };
            enter_text_edit_mode(&state, TextEditMode::MoveWithinStore, &current_dir);
        });
    }

    {
        let state = state.clone();
        let overlay = overlay.clone();
        add_menu_action(&actions, "move-store", move || {
            enter_store_edit_mode(&state, &overlay);
        });
    }

    {
        let state = state.clone();
        let overlay = overlay.clone();
        add_menu_action(&actions, "open-in-file-manager", move || {
            open_entry_in_file_manager(&state.item.borrow(), &overlay);
        });
    }

    {
        let state = state.clone();
        let list = list.clone();
        let overlay = overlay.clone();
        add_menu_action(&actions, "delete", move || {
            delete_current_entry(&state, &list, &overlay);
        });
    }

    menu_button.insert_action_group("entry", Some(&actions));
}

fn add_menu_action(actions: &SimpleActionGroup, name: &str, activate: impl Fn() + 'static) {
    let action = SimpleAction::new(name, None);
    action.connect_activate(move |_, _| activate());
    actions.add_action(&action);
}

fn connect_copy_action(state: &PasswordRowState, button: &Button, overlay: &ToastOverlay) {
    let overlay = overlay.clone();
    let state = state.clone();
    let copied_button = button.clone();
    button.connect_clicked(move |_| {
        copy_password_entry_to_clipboard(
            state.item.borrow().clone(),
            overlay.clone(),
            Some(copied_button.clone()),
        );
    });
}

fn enter_text_edit_mode(state: &PasswordRowState, mode: TextEditMode, value: &str) {
    *state.text_edit_mode.borrow_mut() = mode;
    let title = match mode {
        TextEditMode::RenameFile => gettext("Rename pass file"),
        TextEditMode::MoveWithinStore => gettext("Move pass file"),
    };
    state.text_edit_row.set_title(&title);
    state.text_edit_row.set_text(value);
    state.stack.set_visible_child_name("text-edit");
    state.text_edit_row.grab_focus();
}

fn connect_text_edit_actions(
    state: &PasswordRowState,
    list: &ListBox,
    cancel_button: &Button,
    overlay: &ToastOverlay,
) {
    let state_for_cancel = state.clone();
    cancel_button.connect_clicked(move |_| {
        show_password_row_display(&state_for_cancel);
    });

    let state = state.clone();
    let list = list.clone();
    let overlay = overlay.clone();
    let text_edit_row = state.text_edit_row.clone();
    text_edit_row.connect_apply(move |row| {
        let entry = state.item.borrow().clone();
        let new_label = match *state.text_edit_mode.borrow() {
            TextEditMode::RenameFile => match renamed_file_label(&entry, row.text().as_str()) {
                Ok(new_label) => new_label,
                Err(message) => {
                    overlay.add_toast(Toast::new(&gettext(message)));
                    return;
                }
            },
            TextEditMode::MoveWithinStore => moved_file_label(&entry, row.text().as_str()),
        };

        let Some(new_label) = new_label else {
            show_password_row_display(&state);
            return;
        };

        let old_label = entry.label();
        match rename_password_entry(&entry.store_path, &old_label, &new_label) {
            Ok(()) => {
                *state.item.borrow_mut() =
                    PassEntry::from_label(entry.store_path.clone(), &new_label);
                push_row_undo_action(
                    &state.row,
                    state.readable,
                    rename_entry_action(&entry, &new_label),
                );
                sync_password_row_display(&state);
                show_password_row_display(&state);
                request_password_list_reload(&list);
            }
            Err(err) => {
                log_error(format!("Failed to move or rename password entry: {err}"));
                overlay.add_toast(Toast::new(&gettext(err.rename_toast_message())));
            }
        }
    });
}

fn enter_store_edit_mode(state: &PasswordRowState, overlay: &ToastOverlay) {
    let stores = Preferences::new().store_roots();
    if stores.len() < 2 {
        overlay.add_toast(Toast::new(&gettext("Add another store first.")));
        return;
    }

    let labels = shortened_store_labels(&stores);
    let label_refs = labels.iter().map(String::as_str).collect::<Vec<_>>();
    state
        .store_dropdown
        .set_model(Some(&StringList::new(&label_refs)));
    state.store_dropdown.set_selected(
        stores
            .iter()
            .position(|store| store == &state.item.borrow().store_path)
            .and_then(|index| u32::try_from(index).ok())
            .unwrap_or(INVALID_LIST_POSITION),
    );
    *state.store_roots.borrow_mut() = stores;
    state
        .store_edit_row
        .set_subtitle(&state.item.borrow().label());
    state.stack.set_visible_child_name("store-edit");
    state.store_dropdown.grab_focus();
}

fn connect_store_move_actions(
    state: &PasswordRowState,
    list: &ListBox,
    apply_button: &Button,
    cancel_button: &Button,
    overlay: &ToastOverlay,
) {
    let state_for_cancel = state.clone();
    cancel_button.connect_clicked(move |_| {
        show_password_row_display(&state_for_cancel);
    });

    let state = state.clone();
    let list = list.clone();
    let overlay = overlay.clone();
    apply_button.connect_clicked(move |_| {
        let stores = state.store_roots.borrow();
        let Some(target_store) = stores
            .get(state.store_dropdown.selected() as usize)
            .cloned()
        else {
            overlay.add_toast(Toast::new(&gettext("Choose a store.")));
            return;
        };

        let entry = state.item.borrow().clone();
        if target_store == entry.store_path {
            show_password_row_display(&state);
            return;
        }

        let overlay_for_disconnect = overlay.clone();
        let state_for_result = state.clone();
        let overlay_for_result = overlay.clone();
        let list_for_result = list.clone();
        let entry_for_task = entry.clone();
        let target_store_for_task = target_store.clone();
        spawn_result_task(
            move || move_entry_to_store(&entry_for_task, &target_store_for_task),
            move |result| match result {
                Ok(updated_entry) => {
                    push_undo_action(
                        &state_for_result.row,
                        move_entry_between_stores_action(&entry, &target_store),
                    );
                    *state_for_result.item.borrow_mut() = updated_entry;
                    sync_password_row_display(&state_for_result);
                    show_password_row_display(&state_for_result);
                    request_password_list_reload(&list_for_result);
                    overlay_for_result.add_toast(Toast::new(&gettext("Moved.")));
                }
                Err(err) => {
                    log_undo_error("move password entry to another store", &err);
                    overlay_for_result.add_toast(Toast::new(&gettext(err.toast_message())));
                }
            },
            move || {
                overlay_for_disconnect.add_toast(Toast::new(&gettext("Couldn't move the item.")));
            },
        );
    });
}

fn delete_current_entry(state: &PasswordRowState, list: &ListBox, overlay: &ToastOverlay) {
    let entry = state.item.borrow().clone();
    let row = state.row.clone();
    let list = list.clone();
    let overlay = overlay.clone();
    let overlay_for_disconnect = overlay.clone();
    spawn_result_task(
        move || delete_entry_with_optional_undo(&entry),
        move |result| match result {
            Ok(undo_action) => {
                if let Some(undo_action) = undo_action {
                    if let Some(message) = unavailable_undo_message(&undo_action) {
                        overlay.add_toast(Toast::new(&gettext(message)));
                    }
                    push_undo_action(&row, undo_action);
                }
                list.remove(&row);
                request_password_list_reload(&list);
            }
            Err(err) => {
                log_undo_error("delete password entry", &err);
                overlay.add_toast(Toast::new(&gettext(err.toast_message())));
            }
        },
        move || {
            overlay_for_disconnect.add_toast(Toast::new(&gettext("Couldn't delete the item.")));
        },
    );
}

pub(super) fn activate_selected_password_row_action(
    list: &ListBox,
    overlay: &ToastOverlay,
    action: SelectedPasswordRowAction,
) -> bool {
    let Some(state) = focused_password_row_state(list) else {
        return false;
    };
    if state.stack.visible_child_name().as_deref() != Some("display") {
        return false;
    }

    match action {
        SelectedPasswordRowAction::Copy if state.readable => {
            copy_password_entry_to_clipboard(state.item.borrow().clone(), overlay.clone(), None);
            true
        }
        SelectedPasswordRowAction::Copy => false,
        SelectedPasswordRowAction::RenameFile => {
            let entry = state.item.borrow().clone();
            enter_text_edit_mode(&state, TextEditMode::RenameFile, &entry.basename);
            true
        }
        SelectedPasswordRowAction::MoveWithinStore => {
            let current_dir = {
                let entry = state.item.borrow();
                entry.relative_path.trim_end_matches('/').to_string()
            };
            enter_text_edit_mode(&state, TextEditMode::MoveWithinStore, &current_dir);
            true
        }
        SelectedPasswordRowAction::Delete => {
            delete_current_entry(&state, list, overlay);
            true
        }
    }
}

fn renamed_file_label(entry: &PassEntry, new_name: &str) -> Result<Option<String>, &'static str> {
    let new_name = new_name.trim();
    if new_name.is_empty() {
        return Err("Enter a name.");
    }
    if new_name.contains('/') {
        return Err("Use a single file name.");
    }

    let new_label = format!("{}{}", entry.relative_path, new_name);
    if new_label == entry.label() {
        Ok(None)
    } else {
        Ok(Some(new_label))
    }
}

fn moved_file_label(entry: &PassEntry, new_location: &str) -> Option<String> {
    let new_location = new_location.trim().trim_matches('/');
    let new_label = if new_location.is_empty() {
        entry.basename.clone()
    } else {
        format!("{new_location}/{}", entry.basename)
    };

    (new_label != entry.label()).then_some(new_label)
}

fn show_password_row_display(state: &PasswordRowState) {
    state.stack.set_visible_child_name("display");
}

fn focused_password_row_state(list: &ListBox) -> Option<PasswordRowState> {
    let row = focused_password_row(list)?;
    cloned_data(&row, PASSWORD_ROW_STATE_KEY)
}

fn focused_password_row(list: &ListBox) -> Option<ListBoxRow> {
    let mut widget = list.focus_child()?;
    loop {
        if let Ok(row) = widget.clone().downcast::<ListBoxRow>() {
            return Some(row);
        }
        widget = widget.parent()?;
    }
}

fn sync_password_row_display(state: &PasswordRowState) {
    let item = state.item.borrow();
    let store_label = shortened_store_label_for_path(&item.store_path, &state.store_labels);
    state.action_row.set_title(&item.basename);
    state
        .action_row
        .set_subtitle(&password_row_subtitle(&item.relative_path, &store_label));
    state.action_row.set_tooltip_text(None);

    set_string_data(&state.row, "root", item.store_path.clone());
    set_string_data(&state.row, "label", item.label());
    set_string_data(&state.row, "store-label", store_label);
    set_string_data(
        &state.row,
        "openable",
        if state.readable {
            "true".to_string()
        } else {
            "false".to_string()
        },
    );
}

fn sync_password_folder_row_display(state: &PasswordFolderRowState) {
    let expanded = state.expanded.get();
    state.folder_icon.set_icon_name(Some(if expanded {
        "folder-open-symbolic"
    } else {
        "folder-symbolic"
    }));
    state.expand_icon.set_icon_name(Some(if expanded {
        "go-down-symbolic"
    } else {
        "go-next-symbolic"
    }));
    set_cloned_data(&state.row, PASSWORD_LIST_ROW_EXPANDED_KEY, expanded);
}

fn password_list_indent(depth: usize) -> i32 {
    (depth.min(PASSWORD_LIST_MAX_INDENT_DEPTH) as i32) * PASSWORD_LIST_INDENT_WIDTH
}

fn password_row_subtitle(relative_path: &str, store_label: &str) -> String {
    if relative_path.is_empty() {
        store_label.to_string()
    } else {
        format!("{store_label}/{relative_path}")
    }
}

fn build_unreadable_password_icon(visible: bool) -> Image {
    let icon = dim_label_icon("dialog-warning-symbolic");
    icon.set_tooltip_text(Some(&gettext(UNREADABLE_PASSWORD_ROW_TOOLTIP)));
    icon.set_visible(visible);
    icon
}

fn open_entry_in_file_manager(entry: &PassEntry, overlay: &ToastOverlay) {
    let folder_uri = adw::gio::File::for_path(entry_parent_directory(entry)).uri();
    if let Err(error) = launch_default_uri(&folder_uri) {
        log_error(format!(
            "Failed to open entry folder in the file manager.\nfolder: {folder_uri}\nerror: {error}"
        ));
        overlay.add_toast(Toast::new(&gettext("Couldn't open the folder.")));
    }
}

fn open_entry_in_new_window(state: &PasswordRowState, overlay: &ToastOverlay) {
    let Some(window) = state
        .row
        .root()
        .and_then(|root| root.downcast::<adw::ApplicationWindow>().ok())
    else {
        log_error("Couldn't find the current window to open a new one.".to_string());
        overlay.add_toast(Toast::new(&gettext("Couldn't open a new window.")));
        return;
    };

    let Some(app) = window
        .application()
        .and_then(|app| app.downcast::<adw::Application>().ok())
    else {
        log_error("Couldn't find the application to open a new window.".to_string());
        overlay.add_toast(Toast::new(&gettext("Couldn't open a new window.")));
        return;
    };

    let pass_file = OpenPassFile::new(state.item.borrow().clone());
    match create_main_window(&app, None, Some(pass_file)) {
        Ok(new_window) => new_window.present(),
        Err(err) => {
            log_error(format!("Couldn't build a new window.\nerror: {err}"));
            overlay.add_toast(Toast::new(&gettext("Couldn't open a new window.")));
        }
    }
}

fn entry_parent_directory(entry: &PassEntry) -> PathBuf {
    let relative_path = entry.relative_path.trim_end_matches('/');
    if relative_path.is_empty() {
        PathBuf::from(&entry.store_path)
    } else {
        Path::new(&entry.store_path).join(relative_path)
    }
}

fn log_undo_error(action: &str, error: &UndoError) {
    match error {
        UndoError::Read(err) => {
            log_error(format!("Failed to {action}: read step failed: {err}"));
        }
        UndoError::Write(err) => {
            log_error(format!("Failed to {action}: write step failed: {err}"));
        }
        UndoError::Delete(err) => {
            log_error(format!("Failed to {action}: delete step failed: {err}"));
        }
        UndoError::Rename(err) => {
            log_error(format!("Failed to {action}: rename step failed: {err}"));
        }
        UndoError::Rollback {
            action_error,
            rollback_error,
        } => {
            log_error(format!(
                "Failed to {action}: rollback failed.\nAction error: {action_error}\nRollback error: {rollback_error}"
            ));
        }
    }
}

fn refresh_password_list_search(list: &ListBox) {
    refresh_password_list_filter(list);
}

fn request_password_list_reload(list: &ListBox) {
    if list
        .activate_action("win.reload-password-list", None)
        .is_ok()
    {
        return;
    }

    refresh_password_list_search(list);
}

fn push_row_undo_action(
    widget: &impl IsA<adw::gtk::Widget>,
    readable: bool,
    action: crate::password::undo::UndoAction,
) {
    if readable {
        push_undo_action(widget, action);
    } else {
        push_undo_action(widget, unavailable_undo_action());
    }
}

#[cfg(test)]
mod tests {
    use super::{
        entry_parent_directory, moved_file_label, password_row_menu_entries, password_row_subtitle,
        renamed_file_label, OPEN_IN_NEW_WINDOW_LABEL,
    };
    use crate::backend::{PasswordEntryError, PasswordEntryWriteError};
    use crate::password::model::PassEntry;
    use crate::password::undo::UndoError;
    use std::path::PathBuf;

    #[test]
    fn rename_pass_file_changes_only_the_file_name() {
        let entry = PassEntry::from_label("/tmp/store", "work/alice/github");
        assert_eq!(
            renamed_file_label(&entry, "gitlab"),
            Ok(Some("work/alice/gitlab".to_string()))
        );
    }

    #[test]
    fn rename_pass_file_rejects_nested_names() {
        let entry = PassEntry::from_label("/tmp/store", "work/alice/github");
        assert_eq!(
            renamed_file_label(&entry, "team/gitlab"),
            Err("Use a single file name.")
        );
    }

    #[test]
    fn move_pass_file_changes_only_the_directory() {
        let entry = PassEntry::from_label("/tmp/store", "work/alice/github");
        assert_eq!(
            moved_file_label(&entry, "personal"),
            Some("personal/github".to_string())
        );
        assert_eq!(moved_file_label(&entry, ""), Some("github".to_string()));
    }

    #[test]
    fn password_row_subtitle_combines_store_and_relative_path() {
        assert_eq!(
            password_row_subtitle("work/alice/", ".../work/.password-store"),
            ".../work/.password-store/work/alice/".to_string()
        );
        assert_eq!(
            password_row_subtitle("", ".../work/.password-store"),
            ".../work/.password-store".to_string()
        );
    }

    #[test]
    fn entry_parent_directory_uses_the_store_root_for_root_entries() {
        let entry = PassEntry::from_label("/tmp/store", "github");
        assert_eq!(entry_parent_directory(&entry), PathBuf::from("/tmp/store"));
    }

    #[test]
    fn duplicate_store_move_target_uses_a_specific_toast() {
        let error = UndoError::Write(PasswordEntryWriteError::already_exists("duplicate"));
        assert_eq!(
            error.toast_message(),
            "An item with that name already exists."
        );
    }

    #[test]
    fn store_move_read_errors_keep_specific_open_toasts() {
        let error = UndoError::Read(PasswordEntryError::missing_private_key("missing"));
        assert_eq!(error.toast_message(), "Add a private key in Preferences.");

        let error = UndoError::Read(PasswordEntryError::other("missing"));
        assert_eq!(error.toast_message(), "Couldn't undo the last change.");
    }

    #[test]
    fn readable_rows_offer_open_in_new_window() {
        assert!(password_row_menu_entries(true)
            .iter()
            .any(|(label, _)| *label == OPEN_IN_NEW_WINDOW_LABEL));
    }

    #[test]
    fn unreadable_rows_hide_open_in_new_window() {
        assert!(!password_row_menu_entries(false)
            .iter()
            .any(|(label, _)| *label == OPEN_IN_NEW_WINDOW_LABEL));
    }
}
