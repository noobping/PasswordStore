use thiserror::Error;

fn save_toast_message_for_fido2_store_message(message: &str) -> Option<&'static str> {
    if message.contains("Enter the FIDO2 security key PIN.") {
        Some("Enter the FIDO2 security key PIN.")
    } else if message.contains("Set a PIN on the FIDO2 security key first.") {
        Some("Set a PIN on the FIDO2 security key first.")
    } else if message.contains("That FIDO2 security key must support PIN protection.") {
        Some("That FIDO2 security key must support PIN protection.")
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
    #[cfg(feature = "smartcard")]
    #[error("{0}")]
    HardwareTokenNotPresent(String),
    #[cfg(feature = "smartcard")]
    #[error("{0}")]
    HardwareTokenMismatch(String),
    #[error("{0}")]
    HardwarePinRequired(String),
    #[cfg(feature = "smartcard")]
    #[error("{0}")]
    IncorrectHardwarePin(String),
    #[cfg(feature = "smartcard")]
    #[error("{0}")]
    HardwarePinBlocked(String),
    #[error("{0}")]
    UnsupportedHardwareKey(String),
    #[cfg(feature = "smartcard")]
    #[error("{0}")]
    HardwareTokenRemoved(String),
    #[cfg(any(feature = "fidostore", feature = "fidokey"))]
    #[error("{0}")]
    Fido2TokenNotPresent(String),
    #[cfg(any(feature = "fidostore", feature = "fidokey"))]
    #[error("{0}")]
    Fido2PinNotSet(String),
    #[cfg(any(feature = "fidostore", feature = "fidokey"))]
    #[error("{0}")]
    Fido2PinRequired(String),
    #[cfg(any(feature = "fidostore", feature = "fidokey"))]
    #[error("{0}")]
    IncorrectFido2Pin(String),
    #[cfg(any(feature = "fidostore", feature = "fidokey"))]
    #[error("{0}")]
    Fido2PinUnsupported(String),
    #[error("{0}")]
    UnsupportedFido2Key(String),
    #[cfg(any(feature = "fidostore", feature = "fidokey"))]
    #[error("{0}")]
    Fido2UserActionTimeout(String),
    #[cfg(any(feature = "fidostore", feature = "fidokey"))]
    #[error("{0}")]
    Fido2TokenRemoved(String),
    #[error("{0}")]
    Other(String),
}

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

    #[cfg(feature = "smartcard")]
    pub fn hardware_token_not_present(message: impl Into<String>) -> Self {
        Self::HardwareTokenNotPresent(message.into())
    }

    #[cfg(feature = "smartcard")]
    pub fn hardware_token_mismatch(message: impl Into<String>) -> Self {
        Self::HardwareTokenMismatch(message.into())
    }

    pub fn hardware_pin_required(message: impl Into<String>) -> Self {
        Self::HardwarePinRequired(message.into())
    }

    #[cfg(feature = "smartcard")]
    pub fn incorrect_hardware_pin(message: impl Into<String>) -> Self {
        Self::IncorrectHardwarePin(message.into())
    }

    #[cfg(feature = "smartcard")]
    pub fn hardware_pin_blocked(message: impl Into<String>) -> Self {
        Self::HardwarePinBlocked(message.into())
    }

    pub fn unsupported_hardware_key(message: impl Into<String>) -> Self {
        Self::UnsupportedHardwareKey(message.into())
    }

    #[cfg(feature = "smartcard")]
    pub fn hardware_token_removed(message: impl Into<String>) -> Self {
        Self::HardwareTokenRemoved(message.into())
    }

    #[cfg(any(feature = "fidostore", feature = "fidokey"))]
    pub fn fido2_token_not_present(message: impl Into<String>) -> Self {
        Self::Fido2TokenNotPresent(message.into())
    }

    #[cfg(any(feature = "fidostore", feature = "fidokey"))]
    pub fn fido2_pin_not_set(message: impl Into<String>) -> Self {
        Self::Fido2PinNotSet(message.into())
    }

    #[cfg(any(feature = "fidostore", feature = "fidokey"))]
    pub fn fido2_pin_required(message: impl Into<String>) -> Self {
        Self::Fido2PinRequired(message.into())
    }

    #[cfg(any(feature = "fidostore", feature = "fidokey"))]
    pub fn incorrect_fido2_pin(message: impl Into<String>) -> Self {
        Self::IncorrectFido2Pin(message.into())
    }

    #[cfg(any(feature = "fidostore", feature = "fidokey"))]
    pub fn fido2_pin_unsupported(message: impl Into<String>) -> Self {
        Self::Fido2PinUnsupported(message.into())
    }

    pub fn unsupported_fido2_key(message: impl Into<String>) -> Self {
        Self::UnsupportedFido2Key(message.into())
    }

    #[cfg(any(feature = "fidostore", feature = "fidokey"))]
    pub fn fido2_user_action_timeout(message: impl Into<String>) -> Self {
        Self::Fido2UserActionTimeout(message.into())
    }

    #[cfg(any(feature = "fidostore", feature = "fidokey"))]
    pub fn fido2_token_removed(message: impl Into<String>) -> Self {
        Self::Fido2TokenRemoved(message.into())
    }

    pub fn other(message: impl Into<String>) -> Self {
        Self::Other(message.into())
    }

    pub const fn is_fido2_pin_required(&self) -> bool {
        #[cfg(any(feature = "fidostore", feature = "fidokey"))]
        {
            matches!(self, Self::Fido2PinRequired(_))
        }

        #[cfg(not(any(feature = "fidostore", feature = "fidokey")))]
        {
            false
        }
    }

    pub const fn is_fido2_pin_not_set(&self) -> bool {
        #[cfg(any(feature = "fidostore", feature = "fidokey"))]
        {
            matches!(self, Self::Fido2PinNotSet(_))
        }

        #[cfg(not(any(feature = "fidostore", feature = "fidokey")))]
        {
            false
        }
    }

    pub const fn is_fido2_token_not_present(&self) -> bool {
        #[cfg(any(feature = "fidostore", feature = "fidokey"))]
        {
            matches!(self, Self::Fido2TokenNotPresent(_))
        }

        #[cfg(not(any(feature = "fidostore", feature = "fidokey")))]
        {
            false
        }
    }

    pub const fn unlock_message(&self) -> &'static str {
        match self {
            Self::Incompatible(_) => "This key can't open your items.",
            #[cfg(feature = "smartcard")]
            Self::HardwareTokenNotPresent(_) => "Connect the hardware key and try again.",
            #[cfg(feature = "smartcard")]
            Self::HardwareTokenMismatch(_) => "Use the matching hardware key.",
            #[cfg(feature = "smartcard")]
            Self::HardwarePinRequired(_)
            | Self::IncorrectHardwarePin(_)
            | Self::HardwarePinBlocked(_) => "Couldn't unlock the hardware key.",
            #[cfg(not(feature = "smartcard"))]
            Self::HardwarePinRequired(_) => "Couldn't unlock the hardware key.",
            Self::UnsupportedHardwareKey(_) => "This hardware key can't open your items.",
            #[cfg(feature = "smartcard")]
            Self::HardwareTokenRemoved(_) => "Reconnect the hardware key and try again.",
            #[cfg(any(feature = "fidostore", feature = "fidokey"))]
            Self::Fido2TokenNotPresent(_) => "Connect the FIDO2 security key and try again.",
            #[cfg(any(feature = "fidostore", feature = "fidokey"))]
            Self::Fido2PinNotSet(_) => "Set a PIN on the FIDO2 security key first.",
            #[cfg(any(feature = "fidostore", feature = "fidokey"))]
            Self::Fido2PinRequired(_) | Self::IncorrectFido2Pin(_) => {
                "Couldn't unlock the FIDO2 security key."
            }
            #[cfg(any(feature = "fidostore", feature = "fidokey"))]
            Self::Fido2PinUnsupported(_) => "That FIDO2 security key must support PIN protection.",
            Self::UnsupportedFido2Key(_) => "This FIDO2 security key can't open your items.",
            #[cfg(any(feature = "fidostore", feature = "fidokey"))]
            Self::Fido2UserActionTimeout(_) => "Touch the FIDO2 security key and try again.",
            #[cfg(any(feature = "fidostore", feature = "fidokey"))]
            Self::Fido2TokenRemoved(_) => "Reconnect the FIDO2 security key and try again.",
            _ => "Couldn't unlock the key.",
        }
    }

    pub fn import_message(&self) -> &'static str {
        match self {
            Self::MissingPrivateKeyMaterial(_) => "That file does not contain a private key.",
            Self::RequiresPasswordProtection(_) => "Add a password to that key first.",
            Self::Incompatible(_) => "This key can't open your items.",
            #[cfg(feature = "smartcard")]
            Self::HardwareTokenNotPresent(_) => "Connect the hardware key first.",
            #[cfg(feature = "smartcard")]
            Self::HardwareTokenMismatch(_) => "Use the matching hardware key.",
            #[cfg(feature = "smartcard")]
            Self::HardwarePinRequired(_) | Self::IncorrectHardwarePin(_) => {
                "Couldn't unlock the hardware key."
            }
            #[cfg(not(feature = "smartcard"))]
            Self::HardwarePinRequired(_) => "Couldn't unlock the hardware key.",
            #[cfg(feature = "smartcard")]
            Self::HardwarePinBlocked(_) => "The hardware key PIN is blocked.",
            Self::UnsupportedHardwareKey(_) => "This hardware key can't open your items.",
            #[cfg(feature = "smartcard")]
            Self::HardwareTokenRemoved(_) => "Reconnect the hardware key and try again.",
            #[cfg(any(feature = "fidostore", feature = "fidokey"))]
            Self::Fido2TokenNotPresent(_) => "Connect the FIDO2 security key first.",
            #[cfg(any(feature = "fidostore", feature = "fidokey"))]
            Self::Fido2PinNotSet(_) => "Set a PIN on the FIDO2 security key first.",
            #[cfg(any(feature = "fidostore", feature = "fidokey"))]
            Self::Fido2PinRequired(_) | Self::IncorrectFido2Pin(_) => {
                "Couldn't unlock the FIDO2 security key."
            }
            #[cfg(any(feature = "fidostore", feature = "fidokey"))]
            Self::Fido2PinUnsupported(_) => "That FIDO2 security key must support PIN protection.",
            Self::UnsupportedFido2Key(_) => "This FIDO2 security key can't open your items.",
            #[cfg(any(feature = "fidostore", feature = "fidokey"))]
            Self::Fido2UserActionTimeout(_) => "Touch the FIDO2 security key and try again.",
            #[cfg(any(feature = "fidostore", feature = "fidokey"))]
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
    use super::{PasswordEntryWriteError, PrivateKeyError, StoreRecipientsError};

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
    fn store_recipient_errors_use_specific_toasts_when_available() {
        assert_eq!(
            StoreRecipientsError::MissingPrivateKey("missing".to_string())
                .toast_message("Couldn't save recipients."),
            "Add a private key in Preferences."
        );
        assert_eq!(
            StoreRecipientsError::invalid_store_path("invalid".to_string())
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
}
