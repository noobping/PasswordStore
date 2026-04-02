use crate::backend::{PasswordEntryError, PasswordEntryWriteError, StoreRecipientsError};
use std::io;

pub(in crate::backend::integrated) const MISSING_PRIVATE_KEY_ERROR: &str =
    "Import a private key in Preferences before using the password store.";
pub(in crate::backend::integrated) const LOCKED_PRIVATE_KEY_ERROR: &str =
    "A private key for this item is locked. Unlock it in Preferences.";
pub(in crate::backend::integrated) const INCOMPATIBLE_PRIVATE_KEY_ERROR: &str =
    "The available private keys cannot decrypt this item.";
pub(in crate::backend::integrated) const INVALID_STORE_PATH_ERROR: &str =
    "The selected password store path is not a folder.";

pub(in crate::backend::integrated) fn password_entry_error_from_integrated_message(
    message: impl Into<String>,
) -> PasswordEntryError {
    let message = message.into();
    match message.as_str() {
        MISSING_PRIVATE_KEY_ERROR => PasswordEntryError::missing_private_key(message),
        LOCKED_PRIVATE_KEY_ERROR => PasswordEntryError::locked_private_key(message),
        INCOMPATIBLE_PRIVATE_KEY_ERROR => PasswordEntryError::incompatible_private_key(message),
        _ => PasswordEntryError::other(message),
    }
}

pub(in crate::backend::integrated) fn password_entry_write_error_from_integrated_message(
    message: impl Into<String>,
) -> PasswordEntryWriteError {
    let message = message.into();
    match message.as_str() {
        MISSING_PRIVATE_KEY_ERROR => PasswordEntryWriteError::MissingPrivateKey(message),
        LOCKED_PRIVATE_KEY_ERROR => PasswordEntryWriteError::LockedPrivateKey(message),
        INCOMPATIBLE_PRIVATE_KEY_ERROR => PasswordEntryWriteError::IncompatiblePrivateKey(message),
        _ => PasswordEntryWriteError::other(message),
    }
}

pub(in crate::backend::integrated) fn password_entry_write_error_from_io(
    err: &io::Error,
) -> PasswordEntryWriteError {
    match err.kind() {
        io::ErrorKind::AlreadyExists => PasswordEntryWriteError::already_exists(err.to_string()),
        io::ErrorKind::NotFound => PasswordEntryWriteError::entry_not_found(err.to_string()),
        _ => password_entry_write_error_from_integrated_message(err.to_string()),
    }
}

pub(in crate::backend::integrated) fn store_recipients_error_from_integrated_message(
    message: impl Into<String>,
) -> StoreRecipientsError {
    let message = message.into();
    match message.as_str() {
        INVALID_STORE_PATH_ERROR => StoreRecipientsError::invalid_store_path(message),
        MISSING_PRIVATE_KEY_ERROR => StoreRecipientsError::MissingPrivateKey(message),
        LOCKED_PRIVATE_KEY_ERROR => StoreRecipientsError::LockedPrivateKey(message),
        INCOMPATIBLE_PRIVATE_KEY_ERROR => StoreRecipientsError::IncompatiblePrivateKey(message),
        _ => StoreRecipientsError::other(message),
    }
}

#[cfg(test)]
mod tests {
    use super::{
        password_entry_error_from_integrated_message,
        password_entry_write_error_from_integrated_message, password_entry_write_error_from_io,
        store_recipients_error_from_integrated_message, INCOMPATIBLE_PRIVATE_KEY_ERROR,
        INVALID_STORE_PATH_ERROR, LOCKED_PRIVATE_KEY_ERROR, MISSING_PRIVATE_KEY_ERROR,
    };
    use crate::backend::{PasswordEntryError, PasswordEntryWriteError, StoreRecipientsError};
    use std::io;

    #[test]
    fn integrated_read_errors_map_to_public_variants_and_toasts() {
        let missing = password_entry_error_from_integrated_message(MISSING_PRIVATE_KEY_ERROR);
        assert!(matches!(missing, PasswordEntryError::MissingPrivateKey(_)));
        assert_eq!(
            missing.toast_message(),
            Some("Add a private key in Preferences.")
        );

        let locked = password_entry_error_from_integrated_message(LOCKED_PRIVATE_KEY_ERROR);
        assert!(matches!(locked, PasswordEntryError::LockedPrivateKey(_)));
        assert_eq!(locked.toast_message(), None);

        let incompatible =
            password_entry_error_from_integrated_message(INCOMPATIBLE_PRIVATE_KEY_ERROR);
        assert!(matches!(
            incompatible,
            PasswordEntryError::IncompatiblePrivateKey(_)
        ));
        assert_eq!(
            incompatible.toast_message(),
            Some("This key can't open your items.")
        );
    }

    #[test]
    fn integrated_write_errors_map_to_public_variants_and_toasts() {
        let missing = password_entry_write_error_from_integrated_message(MISSING_PRIVATE_KEY_ERROR);
        assert!(matches!(
            missing,
            PasswordEntryWriteError::MissingPrivateKey(_)
        ));
        assert_eq!(
            missing.save_toast_message(),
            "Add a private key in Preferences."
        );

        let locked = password_entry_write_error_from_integrated_message(LOCKED_PRIVATE_KEY_ERROR);
        assert!(matches!(
            locked,
            PasswordEntryWriteError::LockedPrivateKey(_)
        ));
        assert_eq!(
            locked.save_toast_message(),
            "Unlock the key in Preferences."
        );

        let incompatible =
            password_entry_write_error_from_integrated_message(INCOMPATIBLE_PRIVATE_KEY_ERROR);
        assert!(matches!(
            incompatible,
            PasswordEntryWriteError::IncompatiblePrivateKey(_)
        ));
        assert_eq!(
            incompatible.save_toast_message(),
            "This key can't open your items."
        );
    }

    #[test]
    fn integrated_write_io_errors_classify_by_io_kind() {
        assert!(matches!(
            password_entry_write_error_from_io(&io::Error::from(io::ErrorKind::AlreadyExists)),
            PasswordEntryWriteError::EntryAlreadyExists(_)
        ));
        assert!(matches!(
            password_entry_write_error_from_io(&io::Error::from(io::ErrorKind::NotFound)),
            PasswordEntryWriteError::EntryNotFound(_)
        ));
    }

    #[test]
    fn integrated_store_recipient_errors_map_to_public_variants_and_toasts() {
        let invalid = store_recipients_error_from_integrated_message(INVALID_STORE_PATH_ERROR);
        assert!(matches!(invalid, StoreRecipientsError::InvalidStorePath(_)));
        assert_eq!(
            invalid.toast_message("Couldn't save recipients."),
            "The selected store path is not a folder."
        );

        let locked = store_recipients_error_from_integrated_message(LOCKED_PRIVATE_KEY_ERROR);
        assert!(matches!(locked, StoreRecipientsError::LockedPrivateKey(_)));
        assert_eq!(
            locked.toast_message("Couldn't save recipients."),
            "Unlock the key in Preferences."
        );
    }
}
