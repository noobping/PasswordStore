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
