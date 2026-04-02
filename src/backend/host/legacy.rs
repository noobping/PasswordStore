use crate::backend::{PasswordEntryError, PasswordEntryWriteError, StoreRecipientsError};

fn message_contains_any(lowered: &str, patterns: &[&str]) -> bool {
    patterns.iter().any(|pattern| lowered.contains(pattern))
}

fn host_message_is_entry_not_found(lowered: &str) -> bool {
    message_contains_any(
        lowered,
        &[
            "not in the password store",
            "was not found",
            "no such file or directory",
        ],
    )
}

fn host_message_is_already_exists(lowered: &str) -> bool {
    lowered.contains("already exists")
}

fn host_message_is_missing_private_key(message: &str) -> bool {
    message.contains("Import a private key in Preferences")
}

fn host_message_is_locked_private_key(message: &str) -> bool {
    message.contains("A private key for this item is locked.")
}

fn host_message_is_incompatible_private_key(message: &str) -> bool {
    message.contains("cannot decrypt password store entries")
        || message.contains("available private keys cannot decrypt")
        || message.contains("no pkesks managed to decrypt the ciphertext")
        || message.contains("no pkesk managed to decrypt the ciphertext")
}

fn host_message_is_invalid_store_path(lowered: &str) -> bool {
    lowered.contains("selected password store path is not a folder")
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum HostStoreMessageKind {
    EntryNotFound,
    EntryAlreadyExists,
    MissingPrivateKey,
    LockedPrivateKey,
    IncompatiblePrivateKey,
    InvalidStorePath,
    Other,
}

fn classify_host_store_message(message: &str) -> HostStoreMessageKind {
    let lowered = message.to_ascii_lowercase();
    if host_message_is_entry_not_found(&lowered) {
        HostStoreMessageKind::EntryNotFound
    } else if host_message_is_already_exists(&lowered) {
        HostStoreMessageKind::EntryAlreadyExists
    } else if host_message_is_missing_private_key(message) {
        HostStoreMessageKind::MissingPrivateKey
    } else if host_message_is_locked_private_key(message) {
        HostStoreMessageKind::LockedPrivateKey
    } else if host_message_is_incompatible_private_key(message) {
        HostStoreMessageKind::IncompatiblePrivateKey
    } else if host_message_is_invalid_store_path(&lowered) {
        HostStoreMessageKind::InvalidStorePath
    } else {
        HostStoreMessageKind::Other
    }
}

pub(super) fn password_entry_error_from_host_message(
    message: impl Into<String>,
) -> PasswordEntryError {
    let message = message.into();
    match classify_host_store_message(&message) {
        HostStoreMessageKind::EntryNotFound => PasswordEntryError::EntryNotFound(message),
        HostStoreMessageKind::MissingPrivateKey => PasswordEntryError::MissingPrivateKey(message),
        HostStoreMessageKind::LockedPrivateKey => PasswordEntryError::LockedPrivateKey(message),
        HostStoreMessageKind::IncompatiblePrivateKey => {
            PasswordEntryError::IncompatiblePrivateKey(message)
        }
        HostStoreMessageKind::EntryAlreadyExists
        | HostStoreMessageKind::InvalidStorePath
        | HostStoreMessageKind::Other => PasswordEntryError::other(message),
    }
}

pub(super) fn password_entry_write_error_from_host_message(
    message: impl Into<String>,
) -> PasswordEntryWriteError {
    let message = message.into();
    match classify_host_store_message(&message) {
        HostStoreMessageKind::EntryAlreadyExists => {
            PasswordEntryWriteError::already_exists(message)
        }
        HostStoreMessageKind::EntryNotFound => PasswordEntryWriteError::entry_not_found(message),
        HostStoreMessageKind::MissingPrivateKey => {
            PasswordEntryWriteError::MissingPrivateKey(message)
        }
        HostStoreMessageKind::LockedPrivateKey => {
            PasswordEntryWriteError::LockedPrivateKey(message)
        }
        HostStoreMessageKind::IncompatiblePrivateKey => {
            PasswordEntryWriteError::IncompatiblePrivateKey(message)
        }
        HostStoreMessageKind::InvalidStorePath | HostStoreMessageKind::Other => {
            PasswordEntryWriteError::other(message)
        }
    }
}

pub(super) fn store_recipients_error_from_host_message(
    message: impl Into<String>,
) -> StoreRecipientsError {
    let message = message.into();
    match classify_host_store_message(&message) {
        HostStoreMessageKind::InvalidStorePath => StoreRecipientsError::invalid_store_path(message),
        HostStoreMessageKind::MissingPrivateKey => StoreRecipientsError::MissingPrivateKey(message),
        HostStoreMessageKind::LockedPrivateKey => StoreRecipientsError::LockedPrivateKey(message),
        HostStoreMessageKind::IncompatiblePrivateKey => {
            StoreRecipientsError::IncompatiblePrivateKey(message)
        }
        HostStoreMessageKind::EntryNotFound
        | HostStoreMessageKind::EntryAlreadyExists
        | HostStoreMessageKind::Other => StoreRecipientsError::other(message),
    }
}

#[cfg(test)]
pub(super) fn password_entry_write_error_from_host_io(
    err: &std::io::Error,
) -> PasswordEntryWriteError {
    match err.kind() {
        std::io::ErrorKind::AlreadyExists => {
            PasswordEntryWriteError::already_exists(err.to_string())
        }
        std::io::ErrorKind::NotFound => PasswordEntryWriteError::entry_not_found(err.to_string()),
        _ => password_entry_write_error_from_host_message(err.to_string()),
    }
}

#[cfg(test)]
mod tests {
    use super::{
        password_entry_error_from_host_message, password_entry_write_error_from_host_io,
        password_entry_write_error_from_host_message, store_recipients_error_from_host_message,
    };
    use crate::backend::{PasswordEntryError, PasswordEntryWriteError, StoreRecipientsError};
    use std::io;

    #[test]
    fn host_write_errors_classify_existing_and_missing_entries() {
        assert!(matches!(
            password_entry_write_error_from_host_message("That password entry already exists."),
            PasswordEntryWriteError::EntryAlreadyExists(_)
        ));
        assert!(matches!(
            password_entry_write_error_from_host_message(
                "Password entry 'team/demo' was not found."
            ),
            PasswordEntryWriteError::EntryNotFound(_)
        ));
    }

    #[test]
    fn host_write_io_errors_classify_by_io_kind_without_english_matching() {
        assert!(matches!(
            password_entry_write_error_from_host_io(&io::Error::from(io::ErrorKind::NotFound)),
            PasswordEntryWriteError::EntryNotFound(_)
        ));
        assert!(matches!(
            password_entry_write_error_from_host_io(&io::Error::from(io::ErrorKind::AlreadyExists)),
            PasswordEntryWriteError::EntryAlreadyExists(_)
        ));
    }

    #[test]
    fn host_store_recipient_errors_map_to_specific_variants() {
        assert!(matches!(
            store_recipients_error_from_host_message(
                "Import a private key in Preferences before using the password store."
            ),
            StoreRecipientsError::MissingPrivateKey(_)
        ));
        assert!(matches!(
            store_recipients_error_from_host_message(
                "The selected password store path is not a folder."
            ),
            StoreRecipientsError::InvalidStorePath(_)
        ));
    }

    #[test]
    fn host_read_errors_classify_pkesks_failures_as_incompatible_private_keys() {
        assert!(matches!(
            password_entry_error_from_host_message("no pkesks managed to decrypt the ciphertext"),
            PasswordEntryError::IncompatiblePrivateKey(_)
        ));
    }
}
