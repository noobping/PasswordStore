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
pub(in crate::backend) use self::keys::clear_integrated_runtime_secret_state;
#[cfg(target_os = "linux")]
pub use self::keys::store_ripasso_private_key_bytes;
pub use self::keys::{
    armored_ripasso_private_key, armored_ripasso_public_key, create_fido2_store_recipient,
    discover_ripasso_hardware_keys, generate_fido2_private_key, generate_ripasso_hardware_key,
    generate_ripasso_private_key, import_ripasso_hardware_key_bytes,
    import_ripasso_private_key_bytes, is_ripasso_private_key_unlocked,
    list_connected_smartcard_keys, list_ripasso_private_keys, remove_ripasso_private_key,
    ripasso_private_key_requires_passphrase, ripasso_private_key_requires_session_unlock,
    ripasso_private_key_title, set_fido2_security_key_pin,
    unlock_fido2_store_recipient_for_session, unlock_ripasso_private_key_for_session,
    ConnectedSmartcardKey, DiscoveredHardwareToken, ManagedRipassoHardwareKey,
    ManagedRipassoPrivateKey, ManagedRipassoPrivateKeyProtection, PrivateKeyUnlockKind,
    PrivateKeyUnlockRequest,
};
pub(crate) use self::keys::{
    continue_after_managed_key_storage_recovery, prepare_managed_private_key_storage_for_startup,
    ManagedKeyStorageRecovery, ManagedKeyStorageStartup,
};
pub use self::recipients::preferred_ripasso_private_key_fingerprint_for_entry;
#[cfg(test)]
pub use self::recipients::required_private_key_fingerprints_for_entry;

pub use self::entries::{
    delete_password_entry, password_entry_fido2_recipient_count, password_entry_is_readable,
    read_password_entry, read_password_entry_with_progress, read_password_line,
    rename_password_entry, save_password_entry, save_password_entry_with_progress,
};
pub use self::store::{
    save_store_recipients, save_store_recipients_for_relative_dir,
    save_store_recipients_with_progress, save_store_recipients_with_progress_for_relative_dir,
    store_recipients_private_key_requiring_unlock,
    store_recipients_private_key_requiring_unlock_for_relative_dir,
};
