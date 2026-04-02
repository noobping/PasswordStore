use super::crypto::{decrypt_any_managed_entry_for_fingerprint, IntegratedCryptoContext};
use super::git::{maybe_commit_git_paths, password_entry_git_path};
use super::keys::ensure_ripasso_private_key_is_ready;
use super::paths::{
    cleanup_empty_store_dirs, desired_entry_file_path, entry_file_path, existing_entry_file_path,
};
use super::recipients::{
    decryption_candidate_fingerprints_for_entry,
    password_entry_fido2_recipient_count as fido2_recipient_count,
    password_entry_is_readable as recipients_password_entry_is_readable,
    private_key_requirement_for_label, required_private_key_fingerprints_for_entry,
};
use crate::backend::{
    PasswordEntryError, PasswordEntryReadProgress, PasswordEntryWriteError,
    PasswordEntryWriteProgress, StoreRecipientsPrivateKeyRequirement,
};
use crate::fido2_recipient::is_fido2_recipient_string;
use crate::logging::log_error;
use crate::support::secure_fs::write_atomic_file;
use std::fs;
use std::path::Path;

pub fn read_password_entry(store_root: &str, label: &str) -> Result<String, PasswordEntryError> {
    read_password_entry_with_progress(store_root, label, &mut |_| {})
}

pub fn read_password_entry_with_progress(
    store_root: &str,
    label: &str,
    report_progress: &mut dyn FnMut(PasswordEntryReadProgress),
) -> Result<String, PasswordEntryError> {
    let entry_path = entry_file_path(store_root, label).map_err(PasswordEntryError::other)?;
    if matches!(
        private_key_requirement_for_label(store_root, label),
        Ok(StoreRecipientsPrivateKeyRequirement::AllManagedKeys)
    ) {
        let required_private_key_fingerprints =
            required_private_key_fingerprints_for_entry(store_root, label).map_err(|_| {
                PasswordEntryError::missing_private_key(
                    "Import a private key in Preferences before using the password store.",
                )
            })?;
        ensure_required_private_keys_are_ready(&required_private_key_fingerprints)?;
        let context = IntegratedCryptoContext::load_for_label(store_root, label)
            .map_err(PasswordEntryError::from_store_message)?;
        return context
            .decrypt_entry_with_progress(
                &entry_path,
                Some(&mut |progress| {
                    report_progress(PasswordEntryReadProgress {
                        current_step: progress.current_step,
                        total_steps: progress.total_steps,
                    });
                }),
            )
            .map_err(PasswordEntryError::from_store_message);
    }

    if let Ok(context) = IntegratedCryptoContext::load_for_label(store_root, label) {
        if context.uses_parallel_fido2_decrypt_for_any_managed() {
            return context
                .decrypt_entry_with_progress(
                    &entry_path,
                    Some(&mut |progress| {
                        report_progress(PasswordEntryReadProgress {
                            current_step: progress.current_step,
                            total_steps: progress.total_steps,
                        });
                    }),
                )
                .map_err(PasswordEntryError::from_store_message);
        }
    }

    let mut saw_locked_key = false;
    let mut saw_incompatible_key = false;
    let mut last_error = None;
    let candidate_fingerprints = decryption_candidate_fingerprints_for_entry(store_root, label)
        .map_err(PasswordEntryError::other)?;
    let total_fido2_steps = candidate_fingerprints
        .iter()
        .filter(|fingerprint| is_fido2_recipient_string(fingerprint))
        .count();
    let mut current_fido2_step = 0usize;

    for fingerprint in candidate_fingerprints {
        if is_fido2_recipient_string(&fingerprint) && total_fido2_steps > 0 {
            current_fido2_step += 1;
            report_progress(PasswordEntryReadProgress {
                current_step: current_fido2_step,
                total_steps: total_fido2_steps,
            });
        }

        match decrypt_any_managed_entry_for_fingerprint(&fingerprint, &entry_path) {
            Ok(secret) => return Ok(secret),
            Err(err) => match PasswordEntryError::from_store_message(err) {
                PasswordEntryError::LockedPrivateKey(message) => {
                    saw_locked_key = true;
                    last_error = Some(PasswordEntryError::LockedPrivateKey(message));
                }
                PasswordEntryError::IncompatiblePrivateKey(message) => {
                    saw_incompatible_key = true;
                    last_error = Some(PasswordEntryError::IncompatiblePrivateKey(message));
                }
                other => last_error = Some(other),
            },
        }
    }

    if saw_locked_key {
        return Err(PasswordEntryError::locked_private_key(
            "A private key for this item is locked. Unlock it in Preferences.",
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

pub fn read_password_line(store_root: &str, label: &str) -> Result<String, PasswordEntryError> {
    let secret = read_password_entry(store_root, label)?;
    Ok(secret.lines().next().unwrap_or_default().to_string())
}

pub fn password_entry_is_readable(store_root: &str, label: &str) -> bool {
    recipients_password_entry_is_readable(store_root, label)
}

pub fn password_entry_fido2_recipient_count(store_root: &str, label: &str) -> usize {
    fido2_recipient_count(store_root, label).unwrap_or(0)
}

pub fn save_password_entry(
    store_root: &str,
    label: &str,
    contents: &str,
    overwrite: bool,
) -> Result<(), PasswordEntryWriteError> {
    save_password_entry_with_progress(store_root, label, contents, overwrite, &mut |_| {})
}

pub fn save_password_entry_with_progress(
    store_root: &str,
    label: &str,
    contents: &str,
    overwrite: bool,
    report_progress: &mut dyn FnMut(PasswordEntryWriteProgress),
) -> Result<(), PasswordEntryWriteError> {
    let existing_entry_path = existing_entry_file_path(store_root, label)
        .map_err(PasswordEntryWriteError::from_store_message)?;
    let entry_path = desired_entry_file_path(store_root, label)
        .map_err(PasswordEntryWriteError::from_store_message)?;
    let git_message = if existing_entry_path.is_some() {
        format!("Update password for {label}")
    } else {
        format!("Add password for {label}")
    };
    if existing_entry_path.is_some() && !overwrite {
        return Err(PasswordEntryWriteError::already_exists(
            "That password entry already exists.",
        ));
    }

    let context = IntegratedCryptoContext::load_for_label(store_root, label)
        .map_err(PasswordEntryWriteError::from_store_message)?;
    let previous_ciphertext = existing_entry_path
        .as_ref()
        .map(fs::read)
        .transpose()
        .map_err(|err| PasswordEntryWriteError::from_store_message(err.to_string()))?;
    let ciphertext = context
        .encrypt_contents_with_existing_and_progress(
            contents,
            previous_ciphertext.as_deref(),
            Some(&mut |progress| {
                report_progress(PasswordEntryWriteProgress {
                    current_step: progress.current_step,
                    total_steps: progress.total_steps,
                });
            }),
        )
        .map_err(PasswordEntryWriteError::from_store_message)?;
    let existing_git_path = existing_entry_path
        .as_ref()
        .filter(|existing_path| **existing_path != entry_path)
        .map(|existing_path| password_entry_git_path(Path::new(store_root), existing_path))
        .transpose()
        .map_err(PasswordEntryWriteError::from_store_message)?;
    let new_git_path = password_entry_git_path(Path::new(store_root), &entry_path)
        .map_err(PasswordEntryWriteError::from_store_message)?;
    let result = write_entry_ciphertext(&entry_path, &ciphertext)
        .and_then(|()| {
            if let Some(existing_path) = existing_entry_path
                .as_ref()
                .filter(|existing_path| **existing_path != entry_path)
            {
                fs::remove_file(existing_path).map_err(|err| err.to_string())?;
            }
            Ok(())
        })
        .map_err(PasswordEntryWriteError::from_store_message);
    if result.is_ok() {
        maybe_commit_git_paths(
            store_root,
            &git_message,
            existing_git_path
                .into_iter()
                .chain(std::iter::once(new_git_path)),
            Some(context.fingerprint()),
        );
    }
    result
}

pub fn rename_password_entry(
    store_root: &str,
    old_label: &str,
    new_label: &str,
) -> Result<(), PasswordEntryWriteError> {
    let commit_fingerprint = commit_identity_fingerprint_for_label(store_root, old_label);
    let old_path = existing_entry_file_path(store_root, old_label)
        .map_err(PasswordEntryWriteError::from_store_message)?
        .ok_or_else(|| {
            PasswordEntryWriteError::entry_not_found(format!(
                "Password entry '{old_label}' was not found."
            ))
        })?;
    if existing_entry_file_path(store_root, new_label)
        .map_err(PasswordEntryWriteError::from_store_message)?
        .is_some()
    {
        return Err(PasswordEntryWriteError::already_exists(
            "That password entry already exists.",
        ));
    }
    let new_path = desired_entry_file_path(store_root, new_label)
        .map_err(PasswordEntryWriteError::from_store_message)?;

    ensure_parent_dir(&new_path).map_err(PasswordEntryWriteError::from_store_message)?;
    fs::rename(&old_path, &new_path).map_err(|err| PasswordEntryWriteError::from_io_error(&err))?;
    let old_git_path = password_entry_git_path(Path::new(store_root), &old_path)
        .map_err(PasswordEntryWriteError::from_store_message)?;
    let new_git_path = password_entry_git_path(Path::new(store_root), &new_path)
        .map_err(PasswordEntryWriteError::from_store_message)?;
    let result = cleanup_empty_store_dirs(store_root, &old_path)
        .map_err(PasswordEntryWriteError::from_store_message);
    if result.is_ok() {
        maybe_commit_git_paths(
            store_root,
            &format!("Rename password from {old_label} to {new_label}"),
            [old_git_path, new_git_path],
            commit_fingerprint.as_deref(),
        );
    }
    result
}

pub fn delete_password_entry(store_root: &str, label: &str) -> Result<(), PasswordEntryWriteError> {
    let commit_fingerprint = commit_identity_fingerprint_for_label(store_root, label);
    let entry_path = existing_entry_file_path(store_root, label)
        .map_err(PasswordEntryWriteError::from_store_message)?
        .ok_or_else(|| {
            PasswordEntryWriteError::entry_not_found(format!(
                "Password entry '{label}' was not found."
            ))
        })?;
    let git_path = password_entry_git_path(Path::new(store_root), &entry_path)
        .map_err(PasswordEntryWriteError::from_store_message)?;
    fs::remove_file(&entry_path).map_err(|err| PasswordEntryWriteError::from_io_error(&err))?;
    let result = cleanup_empty_store_dirs(store_root, &entry_path)
        .map_err(PasswordEntryWriteError::from_store_message);
    if result.is_ok() {
        maybe_commit_git_paths(
            store_root,
            &format!("Remove password for {label}"),
            [git_path],
            commit_fingerprint.as_deref(),
        );
    }
    result
}

fn commit_identity_fingerprint_for_label(store_root: &str, label: &str) -> Option<String> {
    if !password_entry_is_readable(store_root, label) {
        return None;
    }

    match IntegratedCryptoContext::fingerprint_for_label(store_root, label) {
        Ok(fingerprint) => Some(fingerprint),
        Err(err) => {
            log_error(format!(
                "Failed to resolve integrated Git commit identity for {store_root}/{label}: {err}"
            ));
            None
        }
    }
}

fn ensure_required_private_keys_are_ready(
    fingerprints: &[String],
) -> Result<(), PasswordEntryError> {
    for fingerprint in fingerprints {
        if is_fido2_recipient_string(fingerprint) {
            continue;
        }
        ensure_ripasso_private_key_is_ready(fingerprint)?;
    }

    Ok(())
}

fn write_entry_ciphertext(entry_path: &Path, ciphertext: &[u8]) -> Result<(), String> {
    write_atomic_file(entry_path, ciphertext).map_err(|err| err.to_string())
}

fn ensure_parent_dir(entry_path: &Path) -> Result<(), String> {
    if let Some(parent) = entry_path.parent() {
        fs::create_dir_all(parent).map_err(|err| err.to_string())?;
    }
    Ok(())
}
