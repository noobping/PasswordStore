use super::keys::{
    build_ripasso_crypto_from_key_ring, load_ripasso_key_ring, load_stored_ripasso_key_ring,
};
use super::paths::recipients_file_for_label;
use super::recipients::{
    encryption_context_fingerprint_from_contents, private_key_requirement_from_contents,
    recipients_for_encryption_from_contents, required_private_key_fingerprints_from_contents,
};
use crate::backend::StoreRecipientsPrivateKeyRequirement;
use ripasso::crypto::{Crypto, Sequoia};
use ripasso::pass::Recipient;
use std::fmt::Write as _;
use std::fs;
use std::path::Path;

const REQUIRE_ALL_PRIVATE_KEYS_LAYER_HEADER: &str = "keycord-require-all-private-keys-v1";

pub(super) struct IntegratedCryptoContext {
    crypto: Sequoia,
    recipients: Vec<Recipient>,
    fingerprint: String,
    private_key_requirement: StoreRecipientsPrivateKeyRequirement,
    required_private_key_fingerprints: Vec<String>,
}

impl IntegratedCryptoContext {
    pub(super) fn load_for_fingerprint(fingerprint: &str) -> Result<Self, String> {
        let key_ring = load_ripasso_key_ring(fingerprint)?;
        let crypto = build_ripasso_crypto_from_key_ring(fingerprint, key_ring)?;
        Ok(Self {
            crypto,
            recipients: Vec::new(),
            fingerprint: fingerprint.to_string(),
            private_key_requirement: StoreRecipientsPrivateKeyRequirement::AnyManagedKey,
            required_private_key_fingerprints: vec![fingerprint.to_string()],
        })
    }

    pub(super) fn load_for_label(store_root: &str, label: &str) -> Result<Self, String> {
        let recipients_file = recipients_file_for_label(store_root, label)?;
        Self::load_for_recipients_file(&recipients_file)
    }

    pub(super) fn fingerprint_for_label(store_root: &str, label: &str) -> Result<String, String> {
        let recipients_file = recipients_file_for_label(store_root, label)?;
        Self::fingerprint_for_recipients_file(&recipients_file)
    }

    fn load_for_recipients_file(recipients_file: &Path) -> Result<Self, String> {
        let contents = fs::read_to_string(recipients_file).map_err(|err| err.to_string())?;
        Self::load_for_recipient_contents(&contents)
    }

    fn fingerprint_for_recipients_file(recipients_file: &Path) -> Result<String, String> {
        let contents = fs::read_to_string(recipients_file).map_err(|err| err.to_string())?;
        Self::fingerprint_for_recipient_contents(&contents)
    }

    pub(super) fn load_for_recipient_contents(contents: &str) -> Result<Self, String> {
        let key_ring = load_stored_ripasso_key_ring()?;
        let recipients = recipients_for_encryption_from_contents(contents, &key_ring)?;
        let fingerprint = encryption_context_fingerprint_from_contents(contents, &key_ring)?;
        let private_key_requirement = private_key_requirement_from_contents(contents);
        let required_private_key_fingerprints =
            required_private_key_fingerprints_from_contents(contents, &key_ring)?;
        let crypto = build_ripasso_crypto_from_key_ring(&fingerprint, key_ring)?;
        Ok(Self {
            crypto,
            recipients,
            fingerprint,
            private_key_requirement,
            required_private_key_fingerprints,
        })
    }

    pub(super) fn fingerprint_for_recipient_contents(contents: &str) -> Result<String, String> {
        let key_ring = load_stored_ripasso_key_ring()?;
        encryption_context_fingerprint_from_contents(contents, &key_ring)
    }

    pub(super) fn fingerprint(&self) -> &str {
        &self.fingerprint
    }

    pub(super) fn decrypt_entry(&self, entry_path: &Path) -> Result<String, String> {
        let ciphertext = read_entry_ciphertext(entry_path)?;
        match self.private_key_requirement {
            StoreRecipientsPrivateKeyRequirement::AnyManagedKey => {
                decrypt_ciphertext_with_crypto(&self.crypto, &ciphertext)
            }
            StoreRecipientsPrivateKeyRequirement::AllManagedKeys => {
                decrypt_password_entry_requiring_all_private_keys(
                    &ciphertext,
                    &self.required_private_key_fingerprints,
                )
            }
        }
    }

    pub(super) fn encrypt_contents(&self, contents: &str) -> Result<Vec<u8>, String> {
        match self.private_key_requirement {
            StoreRecipientsPrivateKeyRequirement::AnyManagedKey => {
                encrypt_password_entry_with_crypto(&self.crypto, &self.recipients, contents)
            }
            StoreRecipientsPrivateKeyRequirement::AllManagedKeys => {
                encrypt_password_entry_requiring_all_private_keys(
                    contents,
                    &self.required_private_key_fingerprints,
                )
            }
        }
    }
}

fn read_entry_ciphertext(entry_path: &Path) -> Result<Vec<u8>, String> {
    let metadata = fs::metadata(entry_path).map_err(|err| err.to_string())?;
    if metadata.len() == 0 {
        return Err("empty password file".to_string());
    }
    fs::read(entry_path).map_err(|err| err.to_string())
}

fn decrypt_ciphertext_with_crypto(crypto: &Sequoia, ciphertext: &[u8]) -> Result<String, String> {
    crypto
        .decrypt_string(ciphertext)
        .map_err(|err| err.to_string())
}

fn decrypt_password_entry_requiring_all_private_keys(
    ciphertext: &[u8],
    required_private_key_fingerprints: &[String],
) -> Result<String, String> {
    let mut current = ciphertext.to_vec();

    for (index, fingerprint) in required_private_key_fingerprints.iter().enumerate() {
        let context = IntegratedCryptoContext::load_for_fingerprint(fingerprint)?;
        let decrypted = decrypt_ciphertext_with_crypto(&context.crypto, &current)?;
        let is_final_layer = index + 1 == required_private_key_fingerprints.len();
        if is_final_layer {
            return Ok(decrypted);
        }

        current = unwrap_required_private_key_layer(&decrypted)?;
    }

    Err("No recipients were found for this password entry.".to_string())
}

fn encrypt_password_entry_requiring_all_private_keys(
    contents: &str,
    required_private_key_fingerprints: &[String],
) -> Result<Vec<u8>, String> {
    let Some((last_fingerprint, outer_fingerprints)) =
        required_private_key_fingerprints.split_last()
    else {
        return Err("No recipients were found for this password entry.".to_string());
    };

    let last_context =
        IntegratedCryptoContext::load_for_recipient_contents(&format!("{last_fingerprint}\n"))?;
    let mut current = encrypt_password_entry_with_crypto(
        &last_context.crypto,
        &last_context.recipients,
        contents,
    )?;

    for fingerprint in outer_fingerprints.iter().rev() {
        let context =
            IntegratedCryptoContext::load_for_recipient_contents(&format!("{fingerprint}\n"))?;
        let wrapped = wrap_required_private_key_layer(&current);
        current =
            encrypt_password_entry_with_crypto(&context.crypto, &context.recipients, &wrapped)?;
    }

    Ok(current)
}

fn wrap_required_private_key_layer(ciphertext: &[u8]) -> String {
    format!(
        "{REQUIRE_ALL_PRIVATE_KEYS_LAYER_HEADER}\n{}",
        encode_hex(ciphertext)
    )
}

fn unwrap_required_private_key_layer(payload: &str) -> Result<Vec<u8>, String> {
    let (header, body) = payload
        .split_once('\n')
        .ok_or_else(|| "Invalid all-keys encrypted password entry.".to_string())?;
    if header.trim() != REQUIRE_ALL_PRIVATE_KEYS_LAYER_HEADER {
        return Err("Invalid all-keys encrypted password entry.".to_string());
    }

    decode_hex(body.trim())
}

fn encode_hex(bytes: &[u8]) -> String {
    let mut encoded = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        write!(encoded, "{byte:02x}").expect("writing hex into a string should not fail");
    }
    encoded
}

fn decode_hex(value: &str) -> Result<Vec<u8>, String> {
    if !value.len().is_multiple_of(2) {
        return Err("Invalid all-keys encrypted password entry.".to_string());
    }

    let mut decoded = Vec::with_capacity(value.len() / 2);
    let mut index = 0;
    while index < value.len() {
        let byte = u8::from_str_radix(&value[index..index + 2], 16)
            .map_err(|_| "Invalid all-keys encrypted password entry.".to_string())?;
        decoded.push(byte);
        index += 2;
    }

    Ok(decoded)
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
