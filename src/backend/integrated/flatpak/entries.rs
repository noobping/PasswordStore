use super::super::keys::ensure_ripasso_private_key_is_ready;
use super::crypto::FlatpakCryptoContext;
use super::paths::{cleanup_empty_store_dirs, entry_file_path};
use super::recipients::decryption_candidate_fingerprints_for_entry;
use crate::backend::PasswordEntryError;
use std::fs;
use std::path::Path;

pub(crate) fn read_password_entry(
    store_root: &str,
    label: &str,
) -> Result<String, PasswordEntryError> {
    let entry_path = entry_file_path(store_root, label).map_err(PasswordEntryError::other)?;
    let mut saw_locked_key = false;
    let mut saw_incompatible_key = false;
    let mut last_error = None;

    for fingerprint in decryption_candidate_fingerprints_for_entry(store_root, label)
        .map_err(PasswordEntryError::other)?
    {
        match ensure_ripasso_private_key_is_ready(&fingerprint) {
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
) -> Result<(), String> {
    let entry_path = entry_file_path(store_root, label)?;
    if entry_path.exists() && !overwrite {
        return Err("That password entry already exists.".to_string());
    }

    let context = FlatpakCryptoContext::load_for_label(store_root, label)?;
    let ciphertext = context.encrypt_contents_for_label(store_root, label, contents)?;
    write_entry_ciphertext(&entry_path, &ciphertext)
}

pub(crate) fn rename_password_entry(
    store_root: &str,
    old_label: &str,
    new_label: &str,
) -> Result<(), String> {
    let old_path = entry_file_path(store_root, old_label)?;
    let new_path = entry_file_path(store_root, new_label)?;
    if !old_path.exists() {
        return Err(format!("Password entry '{old_label}' was not found."));
    }
    if new_path.exists() {
        return Err("That password entry already exists.".to_string());
    }

    ensure_parent_dir(&new_path)?;
    fs::rename(&old_path, &new_path).map_err(|err| err.to_string())?;
    cleanup_empty_store_dirs(store_root, &old_path)
}

pub(crate) fn delete_password_entry(store_root: &str, label: &str) -> Result<(), String> {
    let entry_path = entry_file_path(store_root, label)?;
    fs::remove_file(&entry_path).map_err(|err| err.to_string())?;
    cleanup_empty_store_dirs(store_root, &entry_path)
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
