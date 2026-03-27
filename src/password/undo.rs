use crate::backend::{
    delete_password_entry, read_password_entry, rename_password_entry, save_password_entry,
    PasswordEntryError, PasswordEntryWriteError,
};
use crate::password::model::PassEntry;
use crate::window::session::window_session_for_widget;
#[cfg(test)]
use crate::window::session::WindowSessionState;
use adw::gtk::Widget;
use adw::prelude::*;

const UNAVAILABLE_UNDO_MESSAGE: &str = "Undo unavailable for that change.";

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum UndoAction {
    Unavailable {
        message: String,
    },
    RestoreSavedEntry {
        previous_store: String,
        previous_label: String,
        previous_contents: Option<String>,
        current_store: String,
        current_label: String,
    },
    RenameEntry {
        store: String,
        old_label: String,
        new_label: String,
    },
    MoveEntryBetweenStores {
        source_store: String,
        target_store: String,
        label: String,
    },
    RestoreDeletedEntry {
        store: String,
        label: String,
        contents: String,
    },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum UndoError {
    Read(PasswordEntryError),
    Write(PasswordEntryWriteError),
    Delete(PasswordEntryWriteError),
    Rename(PasswordEntryWriteError),
    Rollback {
        action_error: PasswordEntryWriteError,
        rollback_error: PasswordEntryWriteError,
    },
}

impl UndoError {
    pub fn toast_message(&self) -> &'static str {
        match self {
            Self::Read(err) => err
                .toast_message()
                .unwrap_or("Couldn't undo the last change."),
            Self::Write(PasswordEntryWriteError::EntryAlreadyExists(_)) => {
                "An item with that name already exists."
            }
            Self::Write(PasswordEntryWriteError::MissingPrivateKey(_))
            | Self::Delete(PasswordEntryWriteError::MissingPrivateKey(_))
            | Self::Rename(PasswordEntryWriteError::MissingPrivateKey(_))
            | Self::Rollback {
                action_error: PasswordEntryWriteError::MissingPrivateKey(_),
                ..
            } => "Add a private key in Preferences.",
            Self::Write(PasswordEntryWriteError::LockedPrivateKey(_))
            | Self::Delete(PasswordEntryWriteError::LockedPrivateKey(_))
            | Self::Rename(PasswordEntryWriteError::LockedPrivateKey(_))
            | Self::Rollback {
                action_error: PasswordEntryWriteError::LockedPrivateKey(_),
                ..
            } => "Unlock the key in Preferences.",
            Self::Write(PasswordEntryWriteError::IncompatiblePrivateKey(_))
            | Self::Delete(PasswordEntryWriteError::IncompatiblePrivateKey(_))
            | Self::Rename(PasswordEntryWriteError::IncompatiblePrivateKey(_))
            | Self::Rollback {
                action_error: PasswordEntryWriteError::IncompatiblePrivateKey(_),
                ..
            } => "This key can't open your items.",
            Self::Delete(err) => err.delete_toast_message(),
            Self::Rename(err) => err.rename_toast_message(),
            Self::Write(_) | Self::Rollback { .. } => "Couldn't undo the last change.",
        }
    }
}

pub fn unavailable_undo_action() -> UndoAction {
    UndoAction::Unavailable {
        message: UNAVAILABLE_UNDO_MESSAGE.to_string(),
    }
}

pub fn unavailable_undo_message(action: &UndoAction) -> Option<&str> {
    match action {
        UndoAction::Unavailable { message } => Some(message.as_str()),
        UndoAction::RestoreSavedEntry { .. }
        | UndoAction::RenameEntry { .. }
        | UndoAction::MoveEntryBetweenStores { .. }
        | UndoAction::RestoreDeletedEntry { .. } => None,
    }
}

pub fn push_undo_action(widget: &impl IsA<Widget>, action: UndoAction) {
    if let Some(session) = window_session_for_widget(widget) {
        session.push_undo_action(action);
    }
}

pub fn pop_undo_action(widget: &impl IsA<Widget>) -> Option<UndoAction> {
    window_session_for_widget(widget).and_then(|session| session.pop_undo_action())
}

#[cfg(test)]
pub fn has_undo_actions(session: &WindowSessionState) -> bool {
    session.has_undo_actions()
}

#[cfg(test)]
pub fn clear_undo_actions(session: &WindowSessionState) {
    session.clear_undo_actions();
}

pub fn restore_deleted_entry_action(entry: &PassEntry, contents: String) -> UndoAction {
    UndoAction::RestoreDeletedEntry {
        store: entry.store_path.clone(),
        label: entry.label(),
        contents,
    }
}

pub fn restore_saved_entry_action(
    previous_store: &str,
    previous_label: &str,
    previous_contents: Option<&str>,
    current_store: &str,
    current_label: &str,
) -> UndoAction {
    UndoAction::RestoreSavedEntry {
        previous_store: previous_store.to_string(),
        previous_label: previous_label.to_string(),
        previous_contents: previous_contents.map(str::to_string),
        current_store: current_store.to_string(),
        current_label: current_label.to_string(),
    }
}

pub fn rename_entry_action(entry: &PassEntry, new_label: &str) -> UndoAction {
    UndoAction::RenameEntry {
        store: entry.store_path.clone(),
        old_label: entry.label(),
        new_label: new_label.to_string(),
    }
}

pub fn move_entry_between_stores_action(entry: &PassEntry, target_store: &str) -> UndoAction {
    UndoAction::MoveEntryBetweenStores {
        source_store: entry.store_path.clone(),
        target_store: target_store.to_string(),
        label: entry.label(),
    }
}

pub fn delete_entry_with_optional_undo(entry: &PassEntry) -> Result<Option<UndoAction>, UndoError> {
    match read_password_entry(&entry.store_path, &entry.label()) {
        Ok(contents) => {
            delete_password_entry(&entry.store_path, &entry.label()).map_err(UndoError::Delete)?;
            Ok(Some(restore_deleted_entry_action(entry, contents)))
        }
        Err(err) if can_delete_without_undo_after_read_error(&err) => {
            delete_password_entry(&entry.store_path, &entry.label()).map_err(UndoError::Delete)?;
            Ok(Some(unavailable_undo_action()))
        }
        Err(err) => Err(UndoError::Read(err)),
    }
}

const fn can_delete_without_undo_after_read_error(error: &PasswordEntryError) -> bool {
    matches!(
        error,
        PasswordEntryError::MissingPrivateKey(_)
            | PasswordEntryError::LockedPrivateKey(_)
            | PasswordEntryError::IncompatiblePrivateKey(_)
    )
}

pub fn move_entry_to_store(entry: &PassEntry, target_store: &str) -> Result<PassEntry, UndoError> {
    let label = entry.label();
    move_entry_between_stores(&entry.store_path, target_store, &label)?;
    Ok(PassEntry::from_label(target_store.to_string(), &label))
}

pub fn execute_undo_action(action: &UndoAction) -> Result<(), UndoError> {
    match action {
        UndoAction::Unavailable { .. } => Ok(()),
        UndoAction::RestoreSavedEntry {
            previous_store,
            previous_label,
            previous_contents,
            current_store,
            current_label,
        } => restore_saved_entry(
            previous_store,
            previous_label,
            previous_contents.as_deref(),
            current_store,
            current_label,
        ),
        UndoAction::RenameEntry {
            store,
            old_label,
            new_label,
        } => rename_password_entry(store, new_label, old_label).map_err(UndoError::Rename),
        UndoAction::MoveEntryBetweenStores {
            source_store,
            target_store,
            label,
        } => move_entry_between_stores(target_store, source_store, label),
        UndoAction::RestoreDeletedEntry {
            store,
            label,
            contents,
        } => save_password_entry(store, label, contents, false).map_err(UndoError::Write),
    }
}

pub fn undo_action_restored_entry(action: &UndoAction) -> Option<(String, String)> {
    match action {
        UndoAction::Unavailable { .. } => None,
        UndoAction::RestoreSavedEntry {
            previous_store,
            previous_label,
            previous_contents,
            ..
        } => previous_contents
            .as_ref()
            .map(|_| (previous_store.clone(), previous_label.clone())),
        UndoAction::RenameEntry {
            store, old_label, ..
        } => Some((store.clone(), old_label.clone())),
        UndoAction::MoveEntryBetweenStores {
            source_store,
            label,
            ..
        } => Some((source_store.clone(), label.clone())),
        UndoAction::RestoreDeletedEntry { store, label, .. } => {
            Some((store.clone(), label.clone()))
        }
    }
}

fn restore_saved_entry(
    previous_store: &str,
    previous_label: &str,
    previous_contents: Option<&str>,
    current_store: &str,
    current_label: &str,
) -> Result<(), UndoError> {
    let Some(previous_contents) = previous_contents else {
        return delete_password_entry(current_store, current_label).map_err(UndoError::Delete);
    };

    if previous_store == current_store && previous_label == current_label {
        return save_password_entry(current_store, current_label, previous_contents, true)
            .map_err(UndoError::Write);
    }

    save_password_entry(previous_store, previous_label, previous_contents, false)
        .map_err(UndoError::Write)?;

    if let Err(delete_error) = delete_password_entry(current_store, current_label) {
        if let Err(rollback_error) = delete_password_entry(previous_store, previous_label) {
            return Err(UndoError::Rollback {
                action_error: delete_error,
                rollback_error,
            });
        }
        return Err(UndoError::Delete(delete_error));
    }

    Ok(())
}

fn move_entry_between_stores(
    source_store: &str,
    target_store: &str,
    label: &str,
) -> Result<(), UndoError> {
    let contents = read_password_entry(source_store, label).map_err(UndoError::Read)?;
    save_password_entry(target_store, label, &contents, false).map_err(UndoError::Write)?;

    if let Err(delete_error) = delete_password_entry(source_store, label) {
        if let Err(rollback_error) = delete_password_entry(target_store, label) {
            return Err(UndoError::Rollback {
                action_error: delete_error,
                rollback_error,
            });
        }
        return Err(UndoError::Delete(delete_error));
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{
        can_delete_without_undo_after_read_error, clear_undo_actions, has_undo_actions,
        move_entry_between_stores_action, rename_entry_action, restore_deleted_entry_action,
        restore_saved_entry_action, unavailable_undo_action, unavailable_undo_message,
        undo_action_restored_entry, UndoAction,
    };
    use crate::backend::PasswordEntryError;
    use crate::password::model::PassEntry;
    use crate::window::session::WindowSessionState;

    #[test]
    fn undo_stack_round_trips_the_most_recent_action() {
        let session = WindowSessionState::default();
        clear_undo_actions(&session);
        session.push_undo_action(UndoAction::RenameEntry {
            store: "/tmp/store".to_string(),
            old_label: "a".to_string(),
            new_label: "b".to_string(),
        });

        assert!(has_undo_actions(&session));
        let action = session.pop_undo_action();
        assert!(matches!(action, Some(UndoAction::RenameEntry { .. })));
        clear_undo_actions(&session);
    }

    #[test]
    fn restored_entry_points_to_the_undone_location() {
        let action = UndoAction::MoveEntryBetweenStores {
            source_store: "/tmp/one".to_string(),
            target_store: "/tmp/two".to_string(),
            label: "work/github".to_string(),
        };

        assert_eq!(
            undo_action_restored_entry(&action),
            Some(("/tmp/one".to_string(), "work/github".to_string()))
        );
    }

    #[test]
    fn helper_actions_capture_the_original_location() {
        let entry = PassEntry::from_label("/tmp/store", "work/github");

        assert!(matches!(
            rename_entry_action(&entry, "work/gitlab"),
            UndoAction::RenameEntry { .. }
        ));
        assert!(matches!(
            move_entry_between_stores_action(&entry, "/tmp/other"),
            UndoAction::MoveEntryBetweenStores { .. }
        ));
        assert!(matches!(
            restore_deleted_entry_action(&entry, "secret".to_string()),
            UndoAction::RestoreDeletedEntry { .. }
        ));
        assert!(matches!(
            restore_saved_entry_action(
                "/tmp/store",
                "work/github",
                Some("secret"),
                "/tmp/store",
                "work/gitlab"
            ),
            UndoAction::RestoreSavedEntry { .. }
        ));
    }

    #[test]
    fn unavailable_undo_actions_expose_a_message() {
        let action = unavailable_undo_action();

        assert_eq!(
            unavailable_undo_message(&action),
            Some("Undo unavailable for that change.")
        );
        assert_eq!(undo_action_restored_entry(&action), None);
    }

    #[test]
    fn delete_without_undo_is_allowed_only_for_private_key_read_failures() {
        assert!(can_delete_without_undo_after_read_error(
            &PasswordEntryError::missing_private_key("missing"),
        ));
        assert!(can_delete_without_undo_after_read_error(
            &PasswordEntryError::locked_private_key("locked"),
        ));
        assert!(can_delete_without_undo_after_read_error(
            &PasswordEntryError::incompatible_private_key("incompatible"),
        ));

        assert!(!can_delete_without_undo_after_read_error(
            &PasswordEntryError::other("other"),
        ));
    }
}
