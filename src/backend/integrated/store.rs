use super::crypto::IntegratedCryptoContext;
use super::entries::read_password_entry;
use super::git::{maybe_commit_git_paths, password_entry_git_path};
use super::paths::{
    collect_password_entry_files, ensure_store_directory, label_from_entry_path,
    with_updated_recipients_file,
};
use super::recipients::{preferred_ripasso_private_key_fingerprint_for_entry, recipient_contents};
use crate::backend::{
    PasswordEntryError, StoreRecipientsError, StoreRecipientsPrivateKeyRequirement,
};
use crate::support::git::{ensure_store_git_repository, has_git_repository};
use std::fs;
use std::path::{Path, PathBuf};

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

pub fn store_recipients_private_key_requiring_unlock(
    store_root: &str,
) -> Result<Option<String>, String> {
    let store_dir = ensure_store_directory(store_root)?;

    for entry_path in collect_password_entry_files(&store_dir)? {
        let label = label_from_entry_path(&store_dir, &entry_path)?;
        if !matches!(
            read_password_entry(store_root, &label),
            Err(PasswordEntryError::LockedPrivateKey(_))
        ) {
            continue;
        }

        return preferred_ripasso_private_key_fingerprint_for_entry(store_root, &label).map(Some);
    }

    Ok(None)
}

pub fn save_store_recipients(
    store_root: &str,
    recipients: &[String],
    private_key_requirement: StoreRecipientsPrivateKeyRequirement,
) -> Result<(), StoreRecipientsError> {
    let store_dir =
        ensure_store_directory(store_root).map_err(StoreRecipientsError::from_store_message)?;
    let decrypted_entries = decrypted_store_entries(&store_dir, store_root)
        .map_err(StoreRecipientsError::from_store_message)?;
    let recipients_contents = recipient_contents(recipients, private_key_requirement);
    let context = IntegratedCryptoContext::load_for_recipient_contents(&recipients_contents)
        .map_err(StoreRecipientsError::from_store_message)?;
    let recipients_path = store_dir.join(".gpg-id");
    let should_initialize_git = !recipients_path.exists() && !has_git_repository(store_root);

    with_updated_recipients_file(&recipients_path, &recipients_contents, || {
        for (entry_path, secret) in &decrypted_entries {
            let ciphertext = context.encrypt_contents(secret)?;
            fs::write(entry_path, ciphertext).map_err(|err| err.to_string())?;
        }
        Ok(())
    })
    .map_err(StoreRecipientsError::from_store_message)?;

    if should_initialize_git {
        ensure_store_git_repository(store_root)
            .map_err(StoreRecipientsError::from_store_message)?;
    }

    maybe_commit_git_paths(
        store_root,
        "Update password store recipients",
        std::iter::once(".gpg-id".to_string()).chain(
            decrypted_entries
                .iter()
                .filter_map(|(entry_path, _)| label_from_entry_path(&store_dir, entry_path).ok())
                .map(|label| password_entry_git_path(&label)),
        ),
        Some(context.fingerprint()),
    );

    Ok(())
}
