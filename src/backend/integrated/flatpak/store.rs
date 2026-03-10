use super::crypto::{decrypt_password_entry_with_any_available_key, FlatpakCryptoContext};
use super::paths::{
    collect_password_entry_files, ensure_store_directory, label_from_entry_path,
    with_updated_recipients_file,
};
use super::recipients::preferred_ripasso_private_key_fingerprint_for_entry;
use std::fs;

pub(crate) fn save_store_recipients(
    store_root: &str,
    recipients: &[String],
) -> Result<(), String> {
    let store_dir = ensure_store_directory(store_root)?;
    let recipients_contents = format!("{}\n", recipients.join("\n"));
    let context = FlatpakCryptoContext::load_for_recipient_contents(&recipients_contents)?;
    let recipients_path = store_dir.join(".gpg-id");

    with_updated_recipients_file(&recipients_path, recipients, || {
        for entry_path in collect_password_entry_files(&store_dir)? {
            let label = label_from_entry_path(&store_dir, &entry_path)?;
            let preferred =
                preferred_ripasso_private_key_fingerprint_for_entry(store_root, &label)?;
            let secret = decrypt_password_entry_with_any_available_key(&preferred, &entry_path)?;
            let ciphertext = context.encrypt_contents_for_label(store_root, &label, &secret)?;
            fs::write(&entry_path, ciphertext).map_err(|err| err.to_string())?;
        }
        Ok(())
    })
}
