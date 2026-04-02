#[cfg(test)]
use std::io;
use thiserror::Error;

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
        || message.contains("no pkesks managed to decrypt the ciphertext")
        || message.contains("no pkesk managed to decrypt the ciphertext")
}

fn store_message_is_invalid_store_path(lowered: &str) -> bool {
    lowered.contains("selected password store path is not a folder")
}

fn save_toast_message_for_fido2_store_message(message: &str) -> Option<&'static str> {
    if message.contains("Enter the FIDO2 security key PIN.") {
        Some("Enter the FIDO2 security key PIN.")
    } else if message.contains("Touch the FIDO2 security key and try again.") {
        Some("Touch the FIDO2 security key and try again.")
    } else if message.contains("Reconnect the FIDO2 security key and try again.") {
        Some("Reconnect the FIDO2 security key and try again.")
    } else if message.contains("Connect the matching FIDO2 security key.") {
        Some("Connect the matching FIDO2 security key.")
    } else if message
        .contains("That FIDO2 security key does not support the hmac-secret extension.")
    {
        Some("That FIDO2 security key does not support the hmac-secret extension.")
    } else {
        None
    }
}

fn import_toast_message_for_private_key_other(message: &str) -> Option<&'static str> {
    if message.contains("Connect only one FIDO2 security key before continuing.") {
        Some("Unplug the other security keys, then try again.")
    } else {
        save_toast_message_for_fido2_store_message(message)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum StoreMessageKind {
    EntryNotFound,
    EntryAlreadyExists,
    MissingPrivateKey,
    LockedPrivateKey,
    IncompatiblePrivateKey,
    InvalidStorePath,
    Other,
}

fn classify_store_message(message: &str) -> StoreMessageKind {
    let lowered = message.to_ascii_lowercase();
    if store_message_is_entry_not_found(&lowered) {
        StoreMessageKind::EntryNotFound
    } else if store_message_is_already_exists(&lowered) {
        StoreMessageKind::EntryAlreadyExists
    } else if store_message_is_missing_private_key(message) {
        StoreMessageKind::MissingPrivateKey
    } else if store_message_is_locked_private_key(message) {
        StoreMessageKind::LockedPrivateKey
    } else if store_message_is_incompatible_private_key(message) {
        StoreMessageKind::IncompatiblePrivateKey
    } else if store_message_is_invalid_store_path(&lowered) {
        StoreMessageKind::InvalidStorePath
    } else {
        StoreMessageKind::Other
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Error)]
pub enum PasswordEntryError {
    #[error("{0}")]
    EntryNotFound(String),
    #[error("{0}")]
    MissingPrivateKey(String),
    #[error("{0}")]
    LockedPrivateKey(String),
    #[error("{0}")]
    IncompatiblePrivateKey(String),
    #[error("{0}")]
    Other(String),
}

impl PasswordEntryError {
    pub fn missing_private_key(message: impl Into<String>) -> Self {
        Self::MissingPrivateKey(message.into())
    }

    pub fn locked_private_key(message: impl Into<String>) -> Self {
        Self::LockedPrivateKey(message.into())
    }

    pub fn incompatible_private_key(message: impl Into<String>) -> Self {
        Self::IncompatiblePrivateKey(message.into())
    }

    pub fn other(message: impl Into<String>) -> Self {
        Self::Other(message.into())
    }

    pub fn from_store_message(message: impl Into<String>) -> Self {
        let message = message.into();
        match classify_store_message(&message) {
            StoreMessageKind::EntryNotFound => Self::EntryNotFound(message),
            StoreMessageKind::MissingPrivateKey => Self::MissingPrivateKey(message),
            StoreMessageKind::LockedPrivateKey => Self::LockedPrivateKey(message),
            StoreMessageKind::IncompatiblePrivateKey => Self::IncompatiblePrivateKey(message),
            StoreMessageKind::EntryAlreadyExists
            | StoreMessageKind::InvalidStorePath
            | StoreMessageKind::Other => Self::other(message),
        }
    }

    pub const fn toast_message(&self) -> Option<&'static str> {
        match self {
            Self::MissingPrivateKey(_) => Some("Add a private key in Preferences."),
            Self::IncompatiblePrivateKey(_) => Some("This key can't open your items."),
            _ => None,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Error)]
pub enum PasswordEntryWriteError {
    #[error("{0}")]
    EntryAlreadyExists(String),
    #[error("{0}")]
    EntryNotFound(String),
    #[error("{0}")]
    MissingPrivateKey(String),
    #[error("{0}")]
    LockedPrivateKey(String),
    #[error("{0}")]
    IncompatiblePrivateKey(String),
    #[error("{0}")]
    Other(String),
}

impl PasswordEntryWriteError {
    pub fn already_exists(message: impl Into<String>) -> Self {
        Self::EntryAlreadyExists(message.into())
    }

    pub fn entry_not_found(message: impl Into<String>) -> Self {
        Self::EntryNotFound(message.into())
    }

    pub fn other(message: impl Into<String>) -> Self {
        Self::Other(message.into())
    }

    pub fn from_store_message(message: impl Into<String>) -> Self {
        let message = message.into();
        match classify_store_message(&message) {
            StoreMessageKind::EntryAlreadyExists => Self::already_exists(message),
            StoreMessageKind::EntryNotFound => Self::entry_not_found(message),
            StoreMessageKind::MissingPrivateKey => Self::MissingPrivateKey(message),
            StoreMessageKind::LockedPrivateKey => Self::LockedPrivateKey(message),
            StoreMessageKind::IncompatiblePrivateKey => Self::IncompatiblePrivateKey(message),
            StoreMessageKind::InvalidStorePath | StoreMessageKind::Other => Self::other(message),
        }
    }

    #[cfg(test)]
    pub fn from_io_error(err: &io::Error) -> Self {
        match err.kind() {
            io::ErrorKind::AlreadyExists => Self::already_exists(err.to_string()),
            io::ErrorKind::NotFound => Self::entry_not_found(err.to_string()),
            _ => Self::from_store_message(err.to_string()),
        }
    }

    pub fn save_toast_message(&self) -> &'static str {
        match self {
            Self::EntryAlreadyExists(_) => "An item with that name already exists.",
            Self::MissingPrivateKey(_) => "Add a private key in Preferences.",
            Self::LockedPrivateKey(_) => "Unlock the key in Preferences.",
            Self::IncompatiblePrivateKey(_) => "This key can't open your items.",
            Self::Other(message) => save_toast_message_for_fido2_store_message(message)
                .unwrap_or("Couldn't save changes."),
            Self::EntryNotFound(_) => "Couldn't save changes.",
        }
    }

    pub const fn rename_toast_message(&self) -> &'static str {
        match self {
            Self::EntryAlreadyExists(_) => "An item with that name already exists.",
            Self::EntryNotFound(_) => "That item no longer exists.",
            Self::MissingPrivateKey(_)
            | Self::LockedPrivateKey(_)
            | Self::IncompatiblePrivateKey(_)
            | Self::Other(_) => "Couldn't rename the item.",
        }
    }

    pub const fn delete_toast_message(&self) -> &'static str {
        match self {
            Self::EntryNotFound(_) => "That item no longer exists.",
            Self::EntryAlreadyExists(_)
            | Self::MissingPrivateKey(_)
            | Self::LockedPrivateKey(_)
            | Self::IncompatiblePrivateKey(_)
            | Self::Other(_) => "Couldn't delete the item.",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Error)]
pub enum StoreRecipientsError {
    #[error("{0}")]
    InvalidStorePath(String),
    #[error("{0}")]
    MissingPrivateKey(String),
    #[error("{0}")]
    LockedPrivateKey(String),
    #[error("{0}")]
    IncompatiblePrivateKey(String),
    #[error("{0}")]
    Other(String),
}

impl StoreRecipientsError {
    pub fn invalid_store_path(message: impl Into<String>) -> Self {
        Self::InvalidStorePath(message.into())
    }

    pub fn other(message: impl Into<String>) -> Self {
        Self::Other(message.into())
    }

    pub fn from_store_message(message: impl Into<String>) -> Self {
        let message = message.into();
        match classify_store_message(&message) {
            StoreMessageKind::InvalidStorePath => Self::invalid_store_path(message),
            StoreMessageKind::MissingPrivateKey => Self::MissingPrivateKey(message),
            StoreMessageKind::LockedPrivateKey => Self::LockedPrivateKey(message),
            StoreMessageKind::IncompatiblePrivateKey => Self::IncompatiblePrivateKey(message),
            StoreMessageKind::EntryNotFound
            | StoreMessageKind::EntryAlreadyExists
            | StoreMessageKind::Other => Self::other(message),
        }
    }

    pub fn toast_message(&self, fallback: &'static str) -> &'static str {
        match self {
            Self::InvalidStorePath(_) => "The selected store path is not a folder.",
            Self::MissingPrivateKey(_) => "Add a private key in Preferences.",
            Self::LockedPrivateKey(_) => "Unlock the key in Preferences.",
            Self::IncompatiblePrivateKey(_) => "This key can't open your items.",
            Self::Other(message) => {
                save_toast_message_for_fido2_store_message(message).unwrap_or(fallback)
            }
        }
    }
}

#[cfg_attr(not(any(feature = "fidostore", feature = "fidokey")), allow(dead_code))]
#[derive(Clone, Debug, PartialEq, Eq, Error)]
pub enum PrivateKeyError {
    #[error("{0}")]
    NotStored(String),
    #[error("{0}")]
    MissingPrivateKeyMaterial(String),
    #[error("{0}")]
    PassphraseRequired(String),
    #[error("{0}")]
    IncorrectPassphrase(String),
    #[error("{0}")]
    RequiresPasswordProtection(String),
    #[error("{0}")]
    Incompatible(String),
    #[error("{0}")]
    HardwareTokenNotPresent(String),
    #[error("{0}")]
    HardwareTokenMismatch(String),
    #[error("{0}")]
    HardwarePinRequired(String),
    #[error("{0}")]
    IncorrectHardwarePin(String),
    #[error("{0}")]
    UnsupportedHardwareKey(String),
    #[error("{0}")]
    HardwareTokenRemoved(String),
    #[error("{0}")]
    Fido2TokenNotPresent(String),
    #[error("{0}")]
    Fido2PinRequired(String),
    #[error("{0}")]
    IncorrectFido2Pin(String),
    #[error("{0}")]
    UnsupportedFido2Key(String),
    #[error("{0}")]
    Fido2UserActionTimeout(String),
    #[error("{0}")]
    Fido2TokenRemoved(String),
    #[error("{0}")]
    Other(String),
}

#[cfg_attr(not(any(feature = "fidostore", feature = "fidokey")), allow(dead_code))]
impl PrivateKeyError {
    pub fn not_stored(message: impl Into<String>) -> Self {
        Self::NotStored(message.into())
    }

    pub fn missing_private_key_material(message: impl Into<String>) -> Self {
        Self::MissingPrivateKeyMaterial(message.into())
    }

    pub fn passphrase_required(message: impl Into<String>) -> Self {
        Self::PassphraseRequired(message.into())
    }

    pub fn incorrect_passphrase(message: impl Into<String>) -> Self {
        Self::IncorrectPassphrase(message.into())
    }

    pub fn requires_password_protection(message: impl Into<String>) -> Self {
        Self::RequiresPasswordProtection(message.into())
    }

    pub fn incompatible(message: impl Into<String>) -> Self {
        Self::Incompatible(message.into())
    }

    pub fn hardware_token_not_present(message: impl Into<String>) -> Self {
        Self::HardwareTokenNotPresent(message.into())
    }

    pub fn hardware_token_mismatch(message: impl Into<String>) -> Self {
        Self::HardwareTokenMismatch(message.into())
    }

    pub fn hardware_pin_required(message: impl Into<String>) -> Self {
        Self::HardwarePinRequired(message.into())
    }

    pub fn incorrect_hardware_pin(message: impl Into<String>) -> Self {
        Self::IncorrectHardwarePin(message.into())
    }

    pub fn unsupported_hardware_key(message: impl Into<String>) -> Self {
        Self::UnsupportedHardwareKey(message.into())
    }

    pub fn hardware_token_removed(message: impl Into<String>) -> Self {
        Self::HardwareTokenRemoved(message.into())
    }

    pub fn fido2_token_not_present(message: impl Into<String>) -> Self {
        Self::Fido2TokenNotPresent(message.into())
    }

    pub fn fido2_pin_required(message: impl Into<String>) -> Self {
        Self::Fido2PinRequired(message.into())
    }

    pub fn incorrect_fido2_pin(message: impl Into<String>) -> Self {
        Self::IncorrectFido2Pin(message.into())
    }

    pub fn unsupported_fido2_key(message: impl Into<String>) -> Self {
        Self::UnsupportedFido2Key(message.into())
    }

    pub fn fido2_user_action_timeout(message: impl Into<String>) -> Self {
        Self::Fido2UserActionTimeout(message.into())
    }

    pub fn fido2_token_removed(message: impl Into<String>) -> Self {
        Self::Fido2TokenRemoved(message.into())
    }

    pub fn other(message: impl Into<String>) -> Self {
        Self::Other(message.into())
    }

    pub const fn unlock_message(&self) -> &'static str {
        match self {
            Self::Incompatible(_) => "This key can't open your items.",
            Self::HardwareTokenNotPresent(_) => "Connect the hardware key and try again.",
            Self::HardwareTokenMismatch(_) => "Use the matching hardware key.",
            Self::HardwarePinRequired(_) | Self::IncorrectHardwarePin(_) => {
                "Couldn't unlock the hardware key."
            }
            Self::UnsupportedHardwareKey(_) => "This hardware key can't open your items.",
            Self::HardwareTokenRemoved(_) => "Reconnect the hardware key and try again.",
            Self::Fido2TokenNotPresent(_) => "Connect the FIDO2 security key and try again.",
            Self::Fido2PinRequired(_) | Self::IncorrectFido2Pin(_) => {
                "Couldn't unlock the FIDO2 security key."
            }
            Self::UnsupportedFido2Key(_) => "This FIDO2 security key can't open your items.",
            Self::Fido2UserActionTimeout(_) => "Touch the FIDO2 security key and try again.",
            Self::Fido2TokenRemoved(_) => "Reconnect the FIDO2 security key and try again.",
            _ => "Couldn't unlock the key.",
        }
    }

    pub fn import_message(&self) -> &'static str {
        match self {
            Self::MissingPrivateKeyMaterial(_) => "That file does not contain a private key.",
            Self::RequiresPasswordProtection(_) => "Add a password to that key first.",
            Self::Incompatible(_) => "This key can't open your items.",
            Self::HardwareTokenNotPresent(_) => "Connect the hardware key first.",
            Self::HardwareTokenMismatch(_) => "Use the matching hardware key.",
            Self::HardwarePinRequired(_) | Self::IncorrectHardwarePin(_) => {
                "Couldn't unlock the hardware key."
            }
            Self::UnsupportedHardwareKey(_) => "This hardware key can't open your items.",
            Self::HardwareTokenRemoved(_) => "Reconnect the hardware key and try again.",
            Self::Fido2TokenNotPresent(_) => "Connect the FIDO2 security key first.",
            Self::Fido2PinRequired(_) | Self::IncorrectFido2Pin(_) => {
                "Couldn't unlock the FIDO2 security key."
            }
            Self::UnsupportedFido2Key(_) => "This FIDO2 security key can't open your items.",
            Self::Fido2UserActionTimeout(_) => "Touch the FIDO2 security key and try again.",
            Self::Fido2TokenRemoved(_) => "Reconnect the FIDO2 security key and try again.",
            Self::PassphraseRequired(_) | Self::IncorrectPassphrase(_) => {
                "Couldn't unlock the key."
            }
            Self::Other(message) => import_toast_message_for_private_key_other(message)
                .unwrap_or("Couldn't import the key."),
            _ => "Couldn't import the key.",
        }
    }

    pub const fn inspection_message(&self) -> &'static str {
        match self {
            Self::MissingPrivateKeyMaterial(_) => "That data does not contain a private key.",
            _ => "Couldn't read that key.",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{
        PasswordEntryError, PasswordEntryWriteError, PrivateKeyError, StoreRecipientsError,
    };
    use std::io;

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
        assert_eq!(
            PasswordEntryWriteError::Other(
                "Touch the FIDO2 security key and try again.".to_string()
            )
            .save_toast_message(),
            "Touch the FIDO2 security key and try again."
        );
    }

    #[test]
    fn write_errors_classify_io_error_kinds_without_matching_english_text() {
        assert!(matches!(
            PasswordEntryWriteError::from_io_error(&io::Error::from(io::ErrorKind::NotFound)),
            PasswordEntryWriteError::EntryNotFound(_)
        ));
        assert!(matches!(
            PasswordEntryWriteError::from_io_error(&io::Error::from(io::ErrorKind::AlreadyExists)),
            PasswordEntryWriteError::EntryAlreadyExists(_)
        ));
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
        assert_eq!(
            StoreRecipientsError::other("Touch the FIDO2 security key and try again.")
                .toast_message("Couldn't save recipients."),
            "Touch the FIDO2 security key and try again."
        );
    }

    #[test]
    fn private_key_import_errors_keep_specific_fido2_guidance() {
        assert_eq!(
            PrivateKeyError::other("Connect only one FIDO2 security key before continuing.")
                .import_message(),
            "Unplug the other security keys, then try again."
        );
    }

    #[test]
    fn read_errors_classify_pkesks_failures_as_incompatible_private_keys() {
        assert!(matches!(
            PasswordEntryError::from_store_message("no pkesks managed to decrypt the ciphertext"),
            PasswordEntryError::IncompatiblePrivateKey(_)
        ));
    }
}
