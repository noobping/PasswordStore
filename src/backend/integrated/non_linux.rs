#[path = "flatpak/crypto.rs"]
mod crypto;
#[path = "flatpak/paths.rs"]
mod paths;
#[path = "flatpak/recipients.rs"]
mod recipients;

use self::crypto::FlatpakCryptoContext;
use self::paths::{
    cleanup_empty_store_dirs, collect_password_entry_files, ensure_store_directory,
    entry_file_path, label_from_entry_path, with_updated_recipients_file,
};
use crate::backend::{
    PasswordEntryError, PasswordEntryWriteError, StoreRecipientsError,
    StoreRecipientsPrivateKeyRequirement,
};
use std::fs;
use std::path::{Path, PathBuf};

pub(crate) use self::recipients::preferred_ripasso_private_key_fingerprint_for_entry;

pub use super::keys::{
    armored_ripasso_private_key, generate_ripasso_private_key, import_ripasso_private_key_bytes,
    is_ripasso_private_key_unlocked, list_ripasso_private_keys, remove_ripasso_private_key,
    ripasso_private_key_requires_passphrase, ripasso_private_key_requires_session_unlock,
    ripasso_private_key_title, unlock_ripasso_private_key_for_session, ManagedRipassoPrivateKey,
};

pub(crate) fn read_password_entry(
    store_root: &str,
    label: &str,
) -> Result<String, PasswordEntryError> {
    let entry_path = entry_file_path(store_root, label).map_err(PasswordEntryError::other)?;
    if matches!(
        recipients::private_key_requirement_for_label(store_root, label),
        Ok(StoreRecipientsPrivateKeyRequirement::AllManagedKeys)
    ) {
        let required_private_key_fingerprints =
            recipients::required_private_key_fingerprints_for_label(store_root, label).map_err(
                |_| {
                    PasswordEntryError::missing_private_key(
                        "Import a private key in Preferences before using the password store.",
                    )
                },
            )?;
        ensure_required_private_keys_are_ready(&required_private_key_fingerprints)?;
        let context = FlatpakCryptoContext::load_for_label(store_root, label)
            .map_err(PasswordEntryError::other)?;
        return context
            .decrypt_entry(&entry_path)
            .map_err(PasswordEntryError::other);
    }

    let mut saw_locked_key = false;
    let mut saw_incompatible_key = false;
    let mut last_error = None;

    for fingerprint in recipients::decryption_candidate_fingerprints_for_entry(store_root, label)
        .map_err(PasswordEntryError::other)?
    {
        match super::keys::ensure_ripasso_private_key_is_ready(&fingerprint) {
            Ok(()) => {}
            Err(PasswordEntryError::LockedPrivateKey(_)) => {
                saw_locked_key = true;
                continue;
            }
            Err(PasswordEntryError::IncompatiblePrivateKey(_)) => {
                saw_incompatible_key = true;
                last_error = Some(PasswordEntryError::incompatible_private_key(
                    "The available private keys cannot decrypt this item.",
                ));
                continue;
            }
            Err(err) => {
                last_error = Some(err);
                continue;
            }
        }

        match FlatpakCryptoContext::load_for_fingerprint(&fingerprint)
            .and_then(|context| context.decrypt_entry(&entry_path))
        {
            Ok(secret) => return Ok(secret),
            Err(err) => last_error = Some(PasswordEntryError::other(err)),
        }
    }

    if saw_locked_key {
        return Err(PasswordEntryError::locked_private_key(
            "A private key for this item is locked. Unlock it in Preferences and enter its password.",
        ));
    }
    if saw_incompatible_key {
        return Err(PasswordEntryError::incompatible_private_key(
            "The available private keys cannot decrypt this item.",
        ));
    }

    Err(last_error.unwrap_or_else(|| {
        PasswordEntryError::missing_private_key(
            "Import a private key in Preferences before using the password store.",
        )
    }))
}

pub(crate) fn read_password_line(
    store_root: &str,
    label: &str,
) -> Result<String, PasswordEntryError> {
    let secret = read_password_entry(store_root, label)?;
    Ok(secret.lines().next().unwrap_or_default().to_string())
}

pub(crate) fn save_password_entry(
    store_root: &str,
    label: &str,
    contents: &str,
    overwrite: bool,
) -> Result<(), PasswordEntryWriteError> {
    let entry_path =
        entry_file_path(store_root, label).map_err(PasswordEntryWriteError::from_store_message)?;
    if entry_path.exists() && !overwrite {
        return Err(PasswordEntryWriteError::already_exists(
            "That password entry already exists.",
        ));
    }

    let context = FlatpakCryptoContext::load_for_label(store_root, label)
        .map_err(PasswordEntryWriteError::from_store_message)?;
    let ciphertext = context
        .encrypt_contents(contents)
        .map_err(PasswordEntryWriteError::from_store_message)?;
    write_entry_ciphertext(&entry_path, &ciphertext)
        .map_err(PasswordEntryWriteError::from_store_message)
}

pub(crate) fn rename_password_entry(
    store_root: &str,
    old_label: &str,
    new_label: &str,
) -> Result<(), PasswordEntryWriteError> {
    let old_path = entry_file_path(store_root, old_label)
        .map_err(PasswordEntryWriteError::from_store_message)?;
    let new_path = entry_file_path(store_root, new_label)
        .map_err(PasswordEntryWriteError::from_store_message)?;
    if !old_path.exists() {
        return Err(PasswordEntryWriteError::entry_not_found(format!(
            "Password entry '{old_label}' was not found."
        )));
    }
    if new_path.exists() {
        return Err(PasswordEntryWriteError::already_exists(
            "That password entry already exists.",
        ));
    }

    ensure_parent_dir(&new_path).map_err(PasswordEntryWriteError::from_store_message)?;
    fs::rename(&old_path, &new_path)
        .map_err(|err| PasswordEntryWriteError::from_store_message(err.to_string()))?;
    cleanup_empty_store_dirs(store_root, &old_path)
        .map_err(PasswordEntryWriteError::from_store_message)
}

pub(crate) fn delete_password_entry(
    store_root: &str,
    label: &str,
) -> Result<(), PasswordEntryWriteError> {
    let entry_path =
        entry_file_path(store_root, label).map_err(PasswordEntryWriteError::from_store_message)?;
    fs::remove_file(&entry_path)
        .map_err(|err| PasswordEntryWriteError::from_store_message(err.to_string()))?;
    cleanup_empty_store_dirs(store_root, &entry_path)
        .map_err(PasswordEntryWriteError::from_store_message)
}

fn decrypted_store_entries(
    store_dir: &Path,
    store_root: &str,
) -> Result<Vec<(PathBuf, String)>, String> {
    let mut decrypted = Vec::new();

    for entry_path in collect_password_entry_files(store_dir)? {
        let label = label_from_entry_path(store_dir, &entry_path)?;
        let secret = read_password_entry(store_root, &label).map_err(|err| err.to_string())?;
        decrypted.push((entry_path, secret));
    }

    Ok(decrypted)
}

pub(crate) fn save_store_recipients(
    store_root: &str,
    recipients: &[String],
    private_key_requirement: StoreRecipientsPrivateKeyRequirement,
) -> Result<(), StoreRecipientsError> {
    let store_dir =
        ensure_store_directory(store_root).map_err(StoreRecipientsError::from_store_message)?;
    let decrypted_entries = decrypted_store_entries(&store_dir, store_root)
        .map_err(StoreRecipientsError::from_store_message)?;
    let recipients_contents = recipients::recipient_contents(recipients, private_key_requirement);
    let context = FlatpakCryptoContext::load_for_recipient_contents(&recipients_contents)
        .map_err(StoreRecipientsError::from_store_message)?;
    let recipients_path = store_dir.join(".gpg-id");

    with_updated_recipients_file(&recipients_path, &recipients_contents, || {
        for (entry_path, secret) in &decrypted_entries {
            let ciphertext = context.encrypt_contents(secret)?;
            fs::write(entry_path, ciphertext).map_err(|err| err.to_string())?;
        }
        Ok(())
    })
    .map_err(StoreRecipientsError::from_store_message)?;

    Ok(())
}

fn write_entry_ciphertext(entry_path: &Path, ciphertext: &[u8]) -> Result<(), String> {
    ensure_parent_dir(entry_path)?;
    fs::write(entry_path, ciphertext).map_err(|err| err.to_string())
}

fn ensure_parent_dir(entry_path: &Path) -> Result<(), String> {
    if let Some(parent) = entry_path.parent() {
        fs::create_dir_all(parent).map_err(|err| err.to_string())?;
    }
    Ok(())
}

fn ensure_required_private_keys_are_ready(
    fingerprints: &[String],
) -> Result<(), PasswordEntryError> {
    for fingerprint in fingerprints {
        super::keys::ensure_ripasso_private_key_is_ready(fingerprint)?;
    }

    Ok(())
}
