use std::fmt;

fn message_contains_any(lowered: &str, patterns: &[&str]) -> bool {
    patterns.iter().any(|pattern| lowered.contains(pattern))
}

fn store_message_is_entry_not_found(lowered: &str) -> bool {
    message_contains_any(
        lowered,
        &[
            "not in the password store",
            "was not found",
            "no such file or directory",
        ],
    )
}

fn store_message_is_already_exists(lowered: &str) -> bool {
    lowered.contains("already exists")
}

fn store_message_is_missing_private_key(message: &str) -> bool {
    message.contains("Import a private key in Preferences")
}

fn store_message_is_locked_private_key(message: &str) -> bool {
    message.contains("A private key for this item is locked.")
}

fn store_message_is_incompatible_private_key(message: &str) -> bool {
    message.contains("cannot decrypt password store entries")
        || message.contains("available private keys cannot decrypt")
}

fn store_message_is_invalid_store_path(lowered: &str) -> bool {
    lowered.contains("selected password store path is not a folder")
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PasswordEntryError {
    EntryNotFound(String),
    #[cfg(keycord_restricted)]
    MissingPrivateKey(String),
    #[cfg(keycord_restricted)]
    LockedPrivateKey(String),
    #[cfg(keycord_restricted)]
    IncompatiblePrivateKey(String),
    Other(String),
}

impl PasswordEntryError {
    #[cfg(keycord_restricted)]
    pub(crate) fn missing_private_key(message: impl Into<String>) -> Self {
        Self::MissingPrivateKey(message.into())
    }

    #[cfg(keycord_restricted)]
    pub(crate) fn locked_private_key(message: impl Into<String>) -> Self {
        Self::LockedPrivateKey(message.into())
    }

    #[cfg(keycord_restricted)]
    pub(crate) fn incompatible_private_key(message: impl Into<String>) -> Self {
        Self::IncompatiblePrivateKey(message.into())
    }

    pub(crate) fn other(message: impl Into<String>) -> Self {
        Self::Other(message.into())
    }

    pub(crate) fn from_store_message(message: impl Into<String>) -> Self {
        let message = message.into();
        let lowered = message.to_ascii_lowercase();
        if store_message_is_entry_not_found(&lowered) {
            Self::EntryNotFound(message)
        } else {
            Self::other(message)
        }
    }

    pub(crate) fn toast_message(&self) -> Option<&'static str> {
        match self {
            #[cfg(keycord_restricted)]
            Self::MissingPrivateKey(_) => Some("Add a private key in Preferences."),
            #[cfg(keycord_restricted)]
            Self::IncompatiblePrivateKey(_) => Some("This key can't open your items."),
            _ => None,
        }
    }

    pub(crate) fn detail(&self) -> &str {
        match self {
            Self::EntryNotFound(message) => message,
            #[cfg(keycord_restricted)]
            Self::MissingPrivateKey(message)
            | Self::LockedPrivateKey(message)
            | Self::IncompatiblePrivateKey(message) => message,
            Self::Other(message) => message,
        }
    }
}

impl fmt::Display for PasswordEntryError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.detail())
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PasswordEntryWriteError {
    EntryAlreadyExists(String),
    EntryNotFound(String),
    MissingPrivateKey(String),
    LockedPrivateKey(String),
    IncompatiblePrivateKey(String),
    Other(String),
}

impl PasswordEntryWriteError {
    pub(crate) fn already_exists(message: impl Into<String>) -> Self {
        Self::EntryAlreadyExists(message.into())
    }

    pub(crate) fn entry_not_found(message: impl Into<String>) -> Self {
        Self::EntryNotFound(message.into())
    }

    pub(crate) fn other(message: impl Into<String>) -> Self {
        Self::Other(message.into())
    }

    pub(crate) fn from_store_message(message: impl Into<String>) -> Self {
        let message = message.into();
        let lowered = message.to_ascii_lowercase();
        if store_message_is_already_exists(&lowered) {
            Self::already_exists(message)
        } else if store_message_is_entry_not_found(&lowered) {
            Self::entry_not_found(message)
        } else if store_message_is_missing_private_key(&message) {
            Self::MissingPrivateKey(message)
        } else if store_message_is_locked_private_key(&message) {
            Self::LockedPrivateKey(message)
        } else if store_message_is_incompatible_private_key(&message) {
            Self::IncompatiblePrivateKey(message)
        } else {
            Self::other(message)
        }
    }

    pub(crate) fn save_toast_message(&self) -> &'static str {
        match self {
            Self::EntryAlreadyExists(_) => "An item with that name already exists.",
            Self::MissingPrivateKey(_) => "Add a private key in Preferences.",
            Self::LockedPrivateKey(_) => "Unlock the key in Preferences.",
            Self::IncompatiblePrivateKey(_) => "This key can't open your items.",
            Self::EntryNotFound(_) | Self::Other(_) => "Couldn't save changes.",
        }
    }

    pub(crate) fn rename_toast_message(&self) -> &'static str {
        match self {
            Self::EntryAlreadyExists(_) => "An item with that name already exists.",
            Self::EntryNotFound(_) => "That item no longer exists.",
            Self::MissingPrivateKey(_)
            | Self::LockedPrivateKey(_)
            | Self::IncompatiblePrivateKey(_)
            | Self::Other(_) => "Couldn't rename the item.",
        }
    }

    pub(crate) fn delete_toast_message(&self) -> &'static str {
        match self {
            Self::EntryNotFound(_) => "That item no longer exists.",
            Self::EntryAlreadyExists(_)
            | Self::MissingPrivateKey(_)
            | Self::LockedPrivateKey(_)
            | Self::IncompatiblePrivateKey(_)
            | Self::Other(_) => "Couldn't delete the item.",
        }
    }

    pub(crate) fn detail(&self) -> &str {
        match self {
            Self::EntryAlreadyExists(message)
            | Self::EntryNotFound(message)
            | Self::MissingPrivateKey(message)
            | Self::LockedPrivateKey(message)
            | Self::IncompatiblePrivateKey(message)
            | Self::Other(message) => message,
        }
    }
}

impl fmt::Display for PasswordEntryWriteError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.detail())
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum StoreRecipientsError {
    InvalidStorePath(String),
    MissingPrivateKey(String),
    LockedPrivateKey(String),
    IncompatiblePrivateKey(String),
    Other(String),
}

impl StoreRecipientsError {
    pub(crate) fn invalid_store_path(message: impl Into<String>) -> Self {
        Self::InvalidStorePath(message.into())
    }

    pub(crate) fn other(message: impl Into<String>) -> Self {
        Self::Other(message.into())
    }

    pub(crate) fn from_store_message(message: impl Into<String>) -> Self {
        let message = message.into();
        let lowered = message.to_ascii_lowercase();
        if store_message_is_invalid_store_path(&lowered) {
            Self::invalid_store_path(message)
        } else if store_message_is_missing_private_key(&message) {
            Self::MissingPrivateKey(message)
        } else if store_message_is_locked_private_key(&message) {
            Self::LockedPrivateKey(message)
        } else if store_message_is_incompatible_private_key(&message) {
            Self::IncompatiblePrivateKey(message)
        } else {
            Self::other(message)
        }
    }

    pub(crate) fn toast_message(&self, fallback: &'static str) -> &'static str {
        match self {
            Self::InvalidStorePath(_) => "The selected store path is not a folder.",
            Self::MissingPrivateKey(_) => "Add a private key in Preferences.",
            Self::LockedPrivateKey(_) => "Unlock the key in Preferences.",
            Self::IncompatiblePrivateKey(_) => "This key can't open your items.",
            Self::Other(_) => fallback,
        }
    }

    pub(crate) fn detail(&self) -> &str {
        match self {
            Self::InvalidStorePath(message)
            | Self::MissingPrivateKey(message)
            | Self::LockedPrivateKey(message)
            | Self::IncompatiblePrivateKey(message)
            | Self::Other(message) => message,
        }
    }
}

impl fmt::Display for StoreRecipientsError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.detail())
    }
}

#[cfg(keycord_restricted)]
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PrivateKeyError {
    NotStored(String),
    MissingPrivateKeyMaterial(String),
    PassphraseRequired(String),
    IncorrectPassphrase(String),
    RequiresPasswordProtection(String),
    Incompatible(String),
    Other(String),
}

#[cfg(keycord_restricted)]
impl PrivateKeyError {
    pub(crate) fn not_stored(message: impl Into<String>) -> Self {
        Self::NotStored(message.into())
    }

    pub(crate) fn missing_private_key_material(message: impl Into<String>) -> Self {
        Self::MissingPrivateKeyMaterial(message.into())
    }

    pub(crate) fn passphrase_required(message: impl Into<String>) -> Self {
        Self::PassphraseRequired(message.into())
    }

    pub(crate) fn incorrect_passphrase(message: impl Into<String>) -> Self {
        Self::IncorrectPassphrase(message.into())
    }

    pub(crate) fn requires_password_protection(message: impl Into<String>) -> Self {
        Self::RequiresPasswordProtection(message.into())
    }

    pub(crate) fn incompatible(message: impl Into<String>) -> Self {
        Self::Incompatible(message.into())
    }

    pub(crate) fn other(message: impl Into<String>) -> Self {
        Self::Other(message.into())
    }

    pub(crate) fn unlock_message(&self) -> &'static str {
        match self {
            Self::Incompatible(_) => "This key can't open your items.",
            _ => "Couldn't unlock the key.",
        }
    }

    pub(crate) fn import_message(&self) -> &'static str {
        match self {
            Self::MissingPrivateKeyMaterial(_) => "That file does not contain a private key.",
            Self::RequiresPasswordProtection(_) => "Add a password to that key first.",
            Self::Incompatible(_) => "This key can't open your items.",
            Self::PassphraseRequired(_) | Self::IncorrectPassphrase(_) => {
                "Couldn't unlock the key."
            }
            _ => "Couldn't import the key.",
        }
    }

    pub(crate) fn inspection_message(&self) -> &'static str {
        match self {
            Self::MissingPrivateKeyMaterial(_) => "That file does not contain a private key.",
            _ => "Couldn't read that key.",
        }
    }

    pub(crate) fn detail(&self) -> &str {
        match self {
            Self::NotStored(message)
            | Self::MissingPrivateKeyMaterial(message)
            | Self::PassphraseRequired(message)
            | Self::IncorrectPassphrase(message)
            | Self::RequiresPasswordProtection(message)
            | Self::Incompatible(message)
            | Self::Other(message) => message,
        }
    }
}

#[cfg(keycord_restricted)]
impl fmt::Display for PrivateKeyError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.detail())
    }
}

#[cfg(test)]
mod tests {
    use super::{PasswordEntryWriteError, StoreRecipientsError};

    #[test]
    fn write_errors_classify_existing_and_missing_entries() {
        assert!(matches!(
            PasswordEntryWriteError::from_store_message("That password entry already exists."),
            PasswordEntryWriteError::EntryAlreadyExists(_)
        ));
        assert!(matches!(
            PasswordEntryWriteError::from_store_message(
                "Password entry 'team/demo' was not found."
            ),
            PasswordEntryWriteError::EntryNotFound(_)
        ));
    }

    #[test]
    fn write_errors_map_to_user_toasts() {
        assert_eq!(
            PasswordEntryWriteError::EntryAlreadyExists("duplicate".to_string())
                .save_toast_message(),
            "An item with that name already exists."
        );
        assert_eq!(
            PasswordEntryWriteError::EntryNotFound("missing".to_string()).delete_toast_message(),
            "That item no longer exists."
        );
    }

    #[test]
    fn store_recipient_errors_use_specific_toasts_when_available() {
        assert_eq!(
            StoreRecipientsError::from_store_message(
                "Import a private key in Preferences before using the password store."
            )
            .toast_message("Couldn't save recipients."),
            "Add a private key in Preferences."
        );
        assert_eq!(
            StoreRecipientsError::from_store_message(
                "The selected password store path is not a folder."
            )
            .toast_message("Couldn't create the store."),
            "The selected store path is not a folder."
        );
    }
}
