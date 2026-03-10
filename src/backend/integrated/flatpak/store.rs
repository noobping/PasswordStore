use super::crypto::FlatpakCryptoContext;
use super::entries::read_password_entry;
use super::paths::{
    collect_password_entry_files, ensure_store_directory, label_from_entry_path,
    with_updated_recipients_file,
};
use crate::backend::StoreRecipientsError;
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

pub(crate) fn save_store_recipients(
    store_root: &str,
    recipients: &[String],
) -> Result<(), StoreRecipientsError> {
    let store_dir =
        ensure_store_directory(store_root).map_err(StoreRecipientsError::from_store_message)?;
    let decrypted_entries = decrypted_store_entries(&store_dir, store_root)
        .map_err(StoreRecipientsError::from_store_message)?;
    let recipients_contents = format!("{}\n", recipients.join("\n"));
    let context = FlatpakCryptoContext::load_for_recipient_contents(&recipients_contents)
        .map_err(StoreRecipientsError::from_store_message)?;
    let recipients_path = store_dir.join(".gpg-id");

    with_updated_recipients_file(&recipients_path, recipients, || {
        for (entry_path, secret) in &decrypted_entries {
            let ciphertext = context.encrypt_contents(secret)?;
            fs::write(entry_path, ciphertext).map_err(|err| err.to_string())?;
        }
        Ok(())
    })
    .map_err(StoreRecipientsError::from_store_message)
}
