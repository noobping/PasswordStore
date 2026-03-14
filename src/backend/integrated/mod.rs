#[cfg(keycord_flatpak)]
mod flatpak;
#[cfg(keycord_restricted)]
mod keys;
#[cfg(not(keycord_linux))]
mod non_linux;
#[cfg(keycord_standard_linux)]
mod standard;

#[cfg(keycord_flatpak)]
use self::flatpak as imp;
#[cfg(not(keycord_linux))]
use self::non_linux as imp;
#[cfg(keycord_standard_linux)]
use self::standard as imp;

#[cfg(keycord_flatpak)]
pub(crate) use self::imp::{
    armored_ripasso_private_key, generate_ripasso_private_key,
    git_commit_private_key_requiring_unlock_for_entry,
    git_commit_private_key_requiring_unlock_for_store_recipients, import_ripasso_private_key_bytes,
    is_ripasso_private_key_unlocked, list_ripasso_private_keys,
    preferred_ripasso_private_key_fingerprint_for_entry, remove_ripasso_private_key,
    ripasso_private_key_requires_passphrase, ripasso_private_key_requires_session_unlock,
    ripasso_private_key_title, unlock_ripasso_private_key_for_session, ManagedRipassoPrivateKey,
};
#[cfg(not(keycord_linux))]
pub(crate) use self::imp::{
    armored_ripasso_private_key, generate_ripasso_private_key, import_ripasso_private_key_bytes,
    is_ripasso_private_key_unlocked, list_ripasso_private_keys,
    preferred_ripasso_private_key_fingerprint_for_entry, remove_ripasso_private_key,
    ripasso_private_key_requires_passphrase, ripasso_private_key_requires_session_unlock,
    ripasso_private_key_title, unlock_ripasso_private_key_for_session, ManagedRipassoPrivateKey,
};
#[cfg(all(keycord_restricted, test))]
pub(in crate::backend) use self::keys::clear_cached_unlocked_ripasso_private_keys;

pub(crate) use self::imp::{
    delete_password_entry, read_password_entry, read_password_line, rename_password_entry,
    save_password_entry, save_store_recipients,
};
