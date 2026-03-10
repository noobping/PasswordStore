#[cfg(feature = "flatpak")]
mod flatpak;
#[cfg(feature = "flatpak")]
mod keys;
#[cfg(not(feature = "flatpak"))]
mod standard;

#[cfg(feature = "flatpak")]
use self::flatpak as imp;
#[cfg(not(feature = "flatpak"))]
use self::standard as imp;

#[cfg(feature = "flatpak")]
pub use self::imp::{
    import_ripasso_private_key_bytes, is_ripasso_private_key_unlocked, list_ripasso_private_keys,
    preferred_ripasso_private_key_fingerprint_for_entry, remove_ripasso_private_key,
    ripasso_private_key_requires_passphrase, ripasso_private_key_requires_session_unlock,
    ripasso_private_key_title, unlock_ripasso_private_key_for_session,
    ManagedRipassoPrivateKey,
};

pub(crate) use self::imp::{
    delete_password_entry, read_password_entry, read_password_line, rename_password_entry,
    save_password_entry, save_store_recipients,
};
