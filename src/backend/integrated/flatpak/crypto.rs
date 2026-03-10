use super::super::keys::{
    available_unlocked_private_key_fingerprints, build_ripasso_crypto_from_key_ring,
    load_ripasso_key_ring, load_stored_ripasso_key_ring, locked_private_key_error,
};
use super::paths::recipients_file_for_label;
use super::recipients::{
    encryption_context_fingerprint_from_contents, recipients_for_encryption_from_contents,
};
use ripasso::crypto::{Crypto, Sequoia};
use ripasso::pass::Recipient;
use std::fs;
use std::path::Path;

pub(super) struct FlatpakCryptoContext {
    crypto: Sequoia,
    recipients: Vec<Recipient>,
}

impl FlatpakCryptoContext {
    pub(super) fn load_for_fingerprint(fingerprint: &str) -> Result<Self, String> {
        let key_ring = load_ripasso_key_ring(fingerprint)?;
        let crypto = build_ripasso_crypto_from_key_ring(fingerprint, key_ring)?;
        Ok(Self {
            crypto,
            recipients: Vec::new(),
        })
    }

    pub(super) fn load_for_label(store_root: &str, label: &str) -> Result<Self, String> {
        let recipients_file = recipients_file_for_label(store_root, label)?;
        Self::load_for_recipients_file(&recipients_file)
    }

    fn load_for_recipients_file(recipients_file: &Path) -> Result<Self, String> {
        let contents = fs::read_to_string(recipients_file).map_err(|err| err.to_string())?;
        Self::load_for_recipient_contents(&contents)
    }

    pub(super) fn load_for_recipient_contents(contents: &str) -> Result<Self, String> {
        let key_ring = load_stored_ripasso_key_ring()?;
        let recipients = recipients_for_encryption_from_contents(contents, &key_ring)?;
        let fingerprint = encryption_context_fingerprint_from_contents(contents, &key_ring)?;
        let crypto = build_ripasso_crypto_from_key_ring(&fingerprint, key_ring)?;
        Ok(Self { crypto, recipients })
    }

    pub(super) fn decrypt_entry(&self, entry_path: &Path) -> Result<String, String> {
        decrypt_password_entry_with_crypto(&self.crypto, entry_path)
    }

    pub(super) fn encrypt_contents(&self, contents: &str) -> Result<Vec<u8>, String> {
        encrypt_password_entry_with_crypto(&self.crypto, &self.recipients, contents)
    }
}

fn read_entry_ciphertext(entry_path: &Path) -> Result<Vec<u8>, String> {
    let metadata = fs::metadata(entry_path).map_err(|err| err.to_string())?;
    if metadata.len() == 0 {
        return Err("empty password file".to_string());
    }
    fs::read(entry_path).map_err(|err| err.to_string())
}

fn decrypt_password_entry_with_crypto(
    crypto: &Sequoia,
    entry_path: &Path,
) -> Result<String, String> {
    let ciphertext = read_entry_ciphertext(entry_path)?;
    crypto
        .decrypt_string(&ciphertext)
        .map_err(|err| err.to_string())
}

pub(super) fn decrypt_password_entry_with_any_available_key(
    preferred_fingerprint: &str,
    entry_path: &Path,
) -> Result<String, String> {
    let mut last_error = None;
    for fingerprint in available_unlocked_private_key_fingerprints(preferred_fingerprint) {
        let context = FlatpakCryptoContext::load_for_fingerprint(&fingerprint)?;
        match context.decrypt_entry(entry_path) {
            Ok(secret) => return Ok(secret),
            Err(err) => {
                last_error = Some(err);
            }
        }
    }

    Err(last_error.unwrap_or_else(locked_private_key_error))
}

fn encrypt_password_entry_with_crypto(
    crypto: &Sequoia,
    recipients: &[Recipient],
    contents: &str,
) -> Result<Vec<u8>, String> {
    crypto
        .encrypt_string(contents, recipients)
        .map_err(|err| err.to_string())
}
