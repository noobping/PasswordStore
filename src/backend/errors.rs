use std::fmt;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PasswordEntryError {
    #[cfg(not(feature = "flatpak"))]
    EntryNotFound(String),
    #[cfg(feature = "flatpak")]
    MissingPrivateKey(String),
    #[cfg(feature = "flatpak")]
    LockedPrivateKey(String),
    #[cfg(feature = "flatpak")]
    IncompatiblePrivateKey(String),
    Other(String),
}

impl PasswordEntryError {
    #[cfg(feature = "flatpak")]
    pub(crate) fn missing_private_key(message: impl Into<String>) -> Self {
        Self::MissingPrivateKey(message.into())
    }

    #[cfg(feature = "flatpak")]
    pub(crate) fn locked_private_key(message: impl Into<String>) -> Self {
        Self::LockedPrivateKey(message.into())
    }

    #[cfg(feature = "flatpak")]
    pub(crate) fn incompatible_private_key(message: impl Into<String>) -> Self {
        Self::IncompatiblePrivateKey(message.into())
    }

    pub(crate) fn other(message: impl Into<String>) -> Self {
        Self::Other(message.into())
    }

    #[cfg(not(feature = "flatpak"))]
    pub(crate) fn from_store_message(message: impl Into<String>) -> Self {
        let message = message.into();
        let lowered = message.to_ascii_lowercase();
        if lowered.contains("not in the password store")
            || lowered.contains("was not found")
            || lowered.contains("no such file or directory")
        {
            Self::EntryNotFound(message)
        } else {
            Self::other(message)
        }
    }

    pub(crate) fn toast_message(&self) -> Option<&'static str> {
        match self {
            #[cfg(feature = "flatpak")]
            Self::MissingPrivateKey(_) => Some("Add a private key in Preferences."),
            #[cfg(feature = "flatpak")]
            Self::IncompatiblePrivateKey(_) => Some("This key can't open your items."),
            _ => None,
        }
    }

    pub(crate) fn detail(&self) -> &str {
        match self {
            #[cfg(not(feature = "flatpak"))]
            Self::EntryNotFound(message) => message,
            #[cfg(feature = "flatpak")]
            Self::MissingPrivateKey(message)
            | Self::LockedPrivateKey(message)
            | Self::IncompatiblePrivateKey(message)
            | Self::Other(message) => message,
            #[cfg(not(feature = "flatpak"))]
            Self::Other(message) => message,
        }
    }
}

impl fmt::Display for PasswordEntryError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.detail())
    }
}

#[cfg(feature = "flatpak")]
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

#[cfg(feature = "flatpak")]
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

#[cfg(feature = "flatpak")]
impl fmt::Display for PrivateKeyError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.detail())
    }
}
