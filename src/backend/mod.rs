mod command;
mod errors;
mod host;
mod integrated;
#[cfg(test)]
mod test_support;

pub use self::errors::PasswordEntryError;
pub use self::errors::PrivateKeyError;
pub use self::errors::{PasswordEntryWriteError, StoreRecipientsError};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum StoreRecipientsPrivateKeyRequirement {
    #[default]
    AnyManagedKey,
    AllManagedKeys,
}

pub use self::host::{
    armored_host_gpg_private_key, delete_host_gpg_private_key, import_host_gpg_private_key_bytes,
    list_host_gpg_private_keys, HostGpgPrivateKeySummary,
};
pub use integrated::{
    armored_ripasso_private_key, armored_ripasso_public_key, discover_ripasso_hardware_keys,
    generate_ripasso_private_key, import_ripasso_hardware_key_bytes,
    import_ripasso_private_key_bytes, is_ripasso_private_key_unlocked, list_ripasso_private_keys,
    preferred_ripasso_private_key_fingerprint_for_entry, remove_ripasso_private_key,
    ripasso_private_key_requires_passphrase, ripasso_private_key_requires_session_unlock,
    ripasso_private_key_title, store_ripasso_private_key_bytes,
    unlock_ripasso_private_key_for_session, DiscoveredHardwareToken, ManagedRipassoHardwareKey,
    ManagedRipassoPrivateKey, ManagedRipassoPrivateKeyProtection, PrivateKeyUnlockRequest,
};
pub use integrated::{
    git_commit_private_key_requiring_unlock_for_entry,
    git_commit_private_key_requiring_unlock_for_store_recipients,
};

use crate::preferences::Preferences;

fn dispatch_backend<T>(integrated: impl FnOnce() -> T, host: impl FnOnce() -> T) -> T {
    if Preferences::new().uses_integrated_backend() {
        integrated()
    } else {
        host()
    }
}

macro_rules! dispatch_backend_call {
    ($(fn $name:ident($($arg:ident: $arg_ty:ty),* $(,)?) -> $ret:ty;)+) => {
        $(
            pub fn $name($($arg: $arg_ty),*) -> $ret {
                dispatch_backend(
                    || integrated::$name($($arg),*),
                    || host::$name($($arg),*),
                )
            }
        )+
    };
}

dispatch_backend_call! {
    fn read_password_entry(store_root: &str, label: &str) -> Result<String, PasswordEntryError>;
    fn read_password_line(store_root: &str, label: &str) -> Result<String, PasswordEntryError>;
    fn save_password_entry(
        store_root: &str,
        label: &str,
        contents: &str,
        overwrite: bool,
    ) -> Result<(), PasswordEntryWriteError>;
    fn rename_password_entry(
        store_root: &str,
        old_label: &str,
        new_label: &str,
    ) -> Result<(), PasswordEntryWriteError>;
    fn delete_password_entry(store_root: &str, label: &str) -> Result<(), PasswordEntryWriteError>;
    fn save_store_recipients(
        store_root: &str,
        recipients: &[String],
        private_key_requirement: StoreRecipientsPrivateKeyRequirement,
    ) -> Result<(), StoreRecipientsError>;
}

pub fn password_entry_is_readable(store_root: &str, label: &str) -> bool {
    dispatch_backend(
        || integrated::password_entry_is_readable(store_root, label),
        || host::password_entry_is_readable(store_root, label),
    )
}

pub fn store_recipients_private_key_requiring_unlock(
    store_root: &str,
) -> Result<Option<String>, String> {
    dispatch_backend(
        || integrated::store_recipients_private_key_requiring_unlock(store_root),
        || host::store_recipients_private_key_requiring_unlock(store_root),
    )
}
