mod errors;
#[cfg(keycord_linux)]
mod host;
mod integrated;
#[cfg(test)]
mod test_support;

pub(crate) use self::errors::PasswordEntryError;
#[cfg(keycord_restricted)]
pub(crate) use self::errors::PrivateKeyError;
pub(crate) use self::errors::{PasswordEntryWriteError, StoreRecipientsError};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum StoreRecipientsPrivateKeyRequirement {
    #[default]
    AnyManagedKey,
    AllManagedKeys,
}

#[cfg(keycord_flatpak)]
pub(crate) use integrated::{
    armored_ripasso_private_key, generate_ripasso_private_key,
    git_commit_private_key_requiring_unlock_for_entry,
    git_commit_private_key_requiring_unlock_for_store_recipients, import_ripasso_private_key_bytes,
    is_ripasso_private_key_unlocked, list_ripasso_private_keys,
    preferred_ripasso_private_key_fingerprint_for_entry, remove_ripasso_private_key,
    ripasso_private_key_requires_passphrase, ripasso_private_key_requires_session_unlock,
    ripasso_private_key_title, unlock_ripasso_private_key_for_session, ManagedRipassoPrivateKey,
};

#[cfg(not(keycord_linux))]
pub(crate) use integrated::{
    armored_ripasso_private_key, generate_ripasso_private_key, import_ripasso_private_key_bytes,
    is_ripasso_private_key_unlocked, list_ripasso_private_keys,
    preferred_ripasso_private_key_fingerprint_for_entry, remove_ripasso_private_key,
    ripasso_private_key_requires_passphrase, ripasso_private_key_requires_session_unlock,
    ripasso_private_key_title, unlock_ripasso_private_key_for_session, ManagedRipassoPrivateKey,
};

#[cfg(keycord_linux)]
use crate::preferences::Preferences;

#[cfg(keycord_linux)]
fn dispatch_backend<T, E>(
    integrated: impl FnOnce() -> Result<T, E>,
    host: impl FnOnce() -> Result<T, E>,
) -> Result<T, E> {
    if Preferences::new().uses_integrated_backend() {
        integrated()
    } else {
        host()
    }
}

#[cfg(not(keycord_linux))]
macro_rules! dispatch_backend_call {
    ($(fn $name:ident($($arg:ident: $arg_ty:ty),* $(,)?) -> $ret:ty;)+) => {
        $(
            pub fn $name($($arg: $arg_ty),*) -> $ret {
                integrated::$name($($arg),*)
            }
        )+
    };
}

#[cfg(keycord_linux)]
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
