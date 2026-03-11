use crate::backend::{
    delete_password_entry, read_password_entry, rename_password_entry, save_password_entry,
    PasswordEntryError, PasswordEntryWriteError,
};
use crate::clipboard::copy_password_entry_to_clipboard;
use crate::logging::log_error;
use crate::password::model::PassEntry;
use crate::preferences::Preferences;
use crate::store::labels::shortened_store_labels;
use crate::support::background::spawn_result_task;
use crate::support::ui::{flat_icon_button, flat_icon_button_with_tooltip};
use adw::gio::{Menu, SimpleAction, SimpleActionGroup};
use adw::gtk::{
    gdk::Display, Button, DropDown, ListBox, ListBoxRow, MenuButton, Stack, StringList,
    INVALID_LIST_POSITION,
};
use adw::prelude::*;
use adw::{ActionRow, EntryRow, Toast, ToastOverlay};
use std::cell::RefCell;
use std::path::{Path, PathBuf};
use std::rc::Rc;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum TextEditMode {
    RenameFile,
    MoveWithinStore,
}

#[derive(Debug)]
enum StoreMoveError {
    Read(PasswordEntryError),
    Save(PasswordEntryWriteError),
    DeleteSource(PasswordEntryWriteError),
    RollbackFailed {
        delete_error: PasswordEntryWriteError,
        rollback_error: PasswordEntryWriteError,
    },
}

#[derive(Clone)]
struct PasswordRowState {
    item: Rc<RefCell<PassEntry>>,
    row: ListBoxRow,
    stack: Stack,
    action_row: ActionRow,
    text_edit_row: EntryRow,
    store_edit_row: ActionRow,
    store_dropdown: DropDown,
    store_roots: Rc<RefCell<Vec<String>>>,
    text_edit_mode: Rc<RefCell<TextEditMode>>,
}

pub(super) fn append_password_row(list: &ListBox, item: PassEntry, overlay: &ToastOverlay) {
    let row = ListBoxRow::new();
    let stack = Stack::new();

    let action_row = ActionRow::builder()
        .title(item.basename.clone())
        .subtitle(item.relative_path.clone())
        .activatable(true)
        .build();
    let copy_button = flat_icon_button("edit-copy-symbolic");
    let menu_button = MenuButton::builder()
        .icon_name("view-more-symbolic")
        .has_frame(false)
        .css_classes(vec!["flat"])
        .build();
    action_row.add_suffix(&copy_button);
    action_row.add_suffix(&menu_button);

    let text_edit_row = EntryRow::new();
    text_edit_row.set_show_apply_button(true);
    let text_cancel_button = flat_icon_button_with_tooltip("window-close-symbolic", "Cancel");
    text_edit_row.add_suffix(&text_cancel_button);

    let store_edit_row = ActionRow::builder().title("Move to store").build();
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
        row: row.clone(),
        stack: stack.clone(),
        action_row: action_row.clone(),
        text_edit_row: text_edit_row.clone(),
        store_edit_row: store_edit_row.clone(),
        store_dropdown: store_dropdown.clone(),
        store_roots: Rc::new(RefCell::new(Vec::new())),
        text_edit_mode: Rc::new(RefCell::new(TextEditMode::RenameFile)),
    };
    sync_password_row_display(&state);

    configure_password_row_menu(&menu_button, &state, list, overlay);
    connect_copy_action(&state, &copy_button, overlay);
    connect_text_edit_actions(&state, &text_cancel_button, overlay);
    connect_store_move_actions(
        &state,
        list,
        &store_apply_button,
        &store_cancel_button,
        overlay,
    );

    list.append(&row);
}

fn configure_password_row_menu(
    menu_button: &MenuButton,
    state: &PasswordRowState,
    list: &ListBox,
    overlay: &ToastOverlay,
) {
    let menu = Menu::new();
    menu.append(Some("Rename pass file"), Some("entry.rename-file"));
    menu.append(Some("Move pass file"), Some("entry.move"));
    menu.append(Some("Move to store"), Some("entry.move-store"));
    menu.append(
        Some("Open in File Manager"),
        Some("entry.open-in-file-manager"),
    );
    menu.append(Some("Delete"), Some("entry.delete"));
    menu_button.set_menu_model(Some(&menu));

    let actions = SimpleActionGroup::new();

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
    state.text_edit_row.set_title(match mode {
        TextEditMode::RenameFile => "Rename pass file",
        TextEditMode::MoveWithinStore => "Move pass file",
    });
    state.text_edit_row.set_text(value);
    state.stack.set_visible_child_name("text-edit");
}

fn connect_text_edit_actions(
    state: &PasswordRowState,
    cancel_button: &Button,
    overlay: &ToastOverlay,
) {
    let state_for_cancel = state.clone();
    cancel_button.connect_clicked(move |_| {
        show_password_row_display(&state_for_cancel);
    });

    let state = state.clone();
    let overlay = overlay.clone();
    let text_edit_row = state.text_edit_row.clone();
    text_edit_row.connect_apply(move |row| {
        let entry = state.item.borrow().clone();
        let new_label = match *state.text_edit_mode.borrow() {
            TextEditMode::RenameFile => match renamed_file_label(&entry, row.text().as_str()) {
                Ok(new_label) => new_label,
                Err(message) => {
                    overlay.add_toast(Toast::new(message));
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
                sync_password_row_display(&state);
                show_password_row_display(&state);
            }
            Err(err) => {
                log_error(format!("Failed to move or rename password entry: {err}"));
                overlay.add_toast(Toast::new(err.rename_toast_message()));
            }
        }
    });
}

fn enter_store_edit_mode(state: &PasswordRowState, overlay: &ToastOverlay) {
    let stores = Preferences::new().store_roots();
    if stores.len() < 2 {
        overlay.add_toast(Toast::new("Add another store first."));
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
            .map(|index| index as u32)
            .unwrap_or(INVALID_LIST_POSITION),
    );
    *state.store_roots.borrow_mut() = stores;
    state
        .store_edit_row
        .set_subtitle(&state.item.borrow().label());
    state.stack.set_visible_child_name("store-edit");
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
            overlay.add_toast(Toast::new("Choose a store."));
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
        spawn_result_task(
            move || move_password_entry_to_store(&entry, &target_store),
            move |result| match result {
                Ok(updated_entry) => {
                    *state_for_result.item.borrow_mut() = updated_entry;
                    sync_password_row_display(&state_for_result);
                    show_password_row_display(&state_for_result);
                    list_for_result.invalidate_filter();
                    overlay_for_result.add_toast(Toast::new("Moved."));
                }
                Err(err) => {
                    log_store_move_error(&err);
                    overlay_for_result.add_toast(Toast::new(store_move_failure_message(&err)));
                }
            },
            move || {
                overlay_for_disconnect.add_toast(Toast::new("Couldn't move the item."));
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
        move || delete_password_entry(&entry.store_path, &entry.label()),
        move |result| match result {
            Ok(()) => {
                list.remove(&row);
            }
            Err(err) => {
                log_error(format!("Failed to delete password entry: {err}"));
                overlay.add_toast(Toast::new(err.delete_toast_message()));
            }
        },
        move || {
            overlay_for_disconnect.add_toast(Toast::new("Couldn't delete the item."));
        },
    );
}

fn move_password_entry_to_store(
    entry: &PassEntry,
    target_store: &str,
) -> Result<PassEntry, StoreMoveError> {
    let label = entry.label();
    let contents = read_password_entry(&entry.store_path, &label).map_err(StoreMoveError::Read)?;
    save_password_entry(target_store, &label, &contents, false).map_err(StoreMoveError::Save)?;

    if let Err(delete_error) = delete_password_entry(&entry.store_path, &label) {
        if let Err(rollback_error) = delete_password_entry(target_store, &label) {
            return Err(StoreMoveError::RollbackFailed {
                delete_error,
                rollback_error,
            });
        }
        return Err(StoreMoveError::DeleteSource(delete_error));
    }

    Ok(PassEntry::from_label(target_store.to_string(), &label))
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

fn sync_password_row_display(state: &PasswordRowState) {
    let item = state.item.borrow();
    state.action_row.set_title(&item.basename);
    state.action_row.set_subtitle(&item.relative_path);

    unsafe {
        state.row.set_data("root", item.store_path.clone());
        state.row.set_data("label", item.label());
    }
}

fn open_entry_in_file_manager(entry: &PassEntry, overlay: &ToastOverlay) {
    let folder_uri = adw::gio::File::for_path(entry_parent_directory(entry)).uri();
    let launch_result = Display::default().map_or_else(
        || {
            adw::gio::AppInfo::launch_default_for_uri(
                &folder_uri,
                None::<&adw::gio::AppLaunchContext>,
            )
        },
        |display| {
            let context = display.app_launch_context();
            adw::gio::AppInfo::launch_default_for_uri(&folder_uri, Some(&context))
        },
    );

    if let Err(error) = launch_result {
        log_error(format!(
            "Failed to open entry folder in the file manager.\nfolder: {folder_uri}\nerror: {error}"
        ));
        overlay.add_toast(Toast::new("Couldn't open the folder."));
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

fn store_move_failure_message(error: &StoreMoveError) -> &'static str {
    match error {
        StoreMoveError::Read(err) => err.toast_message().unwrap_or("Couldn't move the item."),
        StoreMoveError::Save(PasswordEntryWriteError::EntryAlreadyExists(_)) => {
            "An item with that name already exists in that store."
        }
        StoreMoveError::Save(PasswordEntryWriteError::MissingPrivateKey(_)) => {
            "Add a private key in Preferences."
        }
        StoreMoveError::Save(PasswordEntryWriteError::LockedPrivateKey(_)) => {
            "Unlock the key in Preferences."
        }
        StoreMoveError::Save(PasswordEntryWriteError::IncompatiblePrivateKey(_)) => {
            "This key can't open your items."
        }
        StoreMoveError::Save(_)
        | StoreMoveError::DeleteSource(_)
        | StoreMoveError::RollbackFailed { .. } => "Couldn't move the item.",
    }
}

fn log_store_move_error(error: &StoreMoveError) {
    match error {
        StoreMoveError::Read(err) => {
            log_error(format!(
                "Failed to read password entry before moving stores: {err}"
            ));
        }
        StoreMoveError::Save(err) => {
            log_error(format!(
                "Failed to save password entry into the target store: {err}"
            ));
        }
        StoreMoveError::DeleteSource(err) => {
            log_error(format!(
                "Failed to delete the original password entry after store move: {err}"
            ));
        }
        StoreMoveError::RollbackFailed {
            delete_error,
            rollback_error,
        } => {
            log_error(format!(
                "Failed to finish or roll back a store move.\nDelete error: {delete_error}\nRollback error: {rollback_error}"
            ));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{
        entry_parent_directory, moved_file_label, renamed_file_label, store_move_failure_message,
        StoreMoveError,
    };
    use crate::backend::{PasswordEntryError, PasswordEntryWriteError};
    use crate::password::model::PassEntry;
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
    fn entry_parent_directory_uses_the_store_root_for_root_entries() {
        let entry = PassEntry::from_label("/tmp/store", "github");
        assert_eq!(entry_parent_directory(&entry), PathBuf::from("/tmp/store"));
    }

    #[test]
    fn duplicate_store_move_target_uses_a_specific_toast() {
        let error = StoreMoveError::Save(PasswordEntryWriteError::already_exists("duplicate"));
        assert_eq!(
            store_move_failure_message(&error),
            "An item with that name already exists in that store."
        );
    }

    #[test]
    fn store_move_read_errors_keep_specific_open_toasts() {
        #[cfg(feature = "flatpak")]
        {
            let error = StoreMoveError::Read(PasswordEntryError::missing_private_key("missing"));
            assert_eq!(
                store_move_failure_message(&error),
                "Add a private key in Preferences."
            );
        }

        #[cfg(not(feature = "flatpak"))]
        {
            let error = StoreMoveError::Read(PasswordEntryError::other("missing"));
            assert_eq!(
                store_move_failure_message(&error),
                "Couldn't move the item."
            );
        }
    }
}
