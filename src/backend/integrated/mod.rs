#[path = "shared/crypto.rs"]
mod crypto;
mod entries;
mod git;
mod keys;
#[path = "shared/paths.rs"]
mod paths;
#[path = "shared/recipients.rs"]
mod recipients;
mod store;
#[cfg(test)]
mod tests;

pub use self::git::{
    git_commit_private_key_requiring_unlock_for_entry,
    git_commit_private_key_requiring_unlock_for_store_recipients,
};
#[cfg(test)]
pub(in crate::backend) use self::keys::clear_cached_unlocked_ripasso_private_keys;
pub use self::keys::{
    armored_ripasso_private_key, armored_ripasso_public_key, discover_ripasso_hardware_keys,
    generate_ripasso_private_key, import_ripasso_hardware_key_bytes,
    import_ripasso_private_key_bytes, is_ripasso_private_key_unlocked, list_ripasso_private_keys,
    remove_ripasso_private_key, ripasso_private_key_requires_passphrase,
    ripasso_private_key_requires_session_unlock, ripasso_private_key_title,
    store_ripasso_private_key_bytes, unlock_ripasso_private_key_for_session,
    DiscoveredHardwareToken, ManagedRipassoHardwareKey, ManagedRipassoPrivateKey,
    ManagedRipassoPrivateKeyProtection, PrivateKeyUnlockRequest,
};
pub use self::recipients::preferred_ripasso_private_key_fingerprint_for_entry;

pub use self::entries::{
    delete_password_entry, password_entry_is_readable, read_password_entry, read_password_line,
    rename_password_entry, save_password_entry,
};
pub use self::store::{save_store_recipients, store_recipients_private_key_requiring_unlock};
