use super::cache::{
    cache_unlocked_ripasso_private_key, cached_unlocked_ripasso_private_key,
    remove_cached_unlocked_ripasso_private_key,
};
use super::cert::{
    cert_can_decrypt_password_entries, cert_requires_passphrase, fingerprint_from_string,
    normalized_fingerprint, parse_managed_private_key_bytes, prepare_managed_private_key_bytes,
    ManagedRipassoPrivateKey,
};
use crate::backend::{PasswordEntryError, PrivateKeyError};
use crate::logging::log_error;
use crate::preferences::Preferences;
use ripasso::crypto::{slice_to_20_bytes, Sequoia};
use sequoia_openpgp::{cert::CertBuilder, crypto::Password, serialize::Serialize, Cert};
use std::collections::HashMap;
use std::fs;
use std::fs::File;
use std::path::{Path, PathBuf};
use std::sync::Arc;

const PRIVATE_KEY_NOT_STORED_ERROR: &str = "That private key is not stored in the app.";
const MISSING_PRIVATE_KEY_ERROR: &str =
    "Import a private key in Preferences before using the password store.";
const LOCKED_PRIVATE_KEY_ERROR: &str =
    "A private key for this item is locked. Unlock it in Preferences and enter its password.";
const INCOMPATIBLE_PRIVATE_KEY_ERROR: &str = "The available private keys cannot decrypt this item.";

struct StoredPrivateKeyEntry {
    path: PathBuf,
    cert: Cert,
    key: ManagedRipassoPrivateKey,
}

pub(in crate::backend::integrated) fn ripasso_keys_dir() -> Result<PathBuf, String> {
    let data_dir = dirs_next::data_local_dir()
        .ok_or_else(|| "Could not determine the data folder.".to_string())?;
    Ok(data_dir.join(env!("CARGO_PKG_NAME")).join("keys"))
}

pub(in crate::backend::integrated) fn missing_private_key_error() -> String {
    MISSING_PRIVATE_KEY_ERROR.to_string()
}

pub(in crate::backend::integrated) fn locked_private_key_error() -> String {
    LOCKED_PRIVATE_KEY_ERROR.to_string()
}

pub(in crate::backend::integrated) fn incompatible_private_key_error() -> String {
    INCOMPATIBLE_PRIVATE_KEY_ERROR.to_string()
}

fn private_key_not_stored_error() -> String {
    PRIVATE_KEY_NOT_STORED_ERROR.to_string()
}

fn read_ripasso_private_key_entry(path: &Path) -> Result<StoredPrivateKeyEntry, String> {
    let data = fs::read(path).map_err(|err| err.to_string())?;
    let (cert, key) = parse_managed_private_key_bytes(&data).map_err(|err| err.to_string())?;
    Ok(StoredPrivateKeyEntry {
        path: path.to_path_buf(),
        cert,
        key,
    })
}

fn stored_private_key_paths(keys_dir: &Path) -> Result<Vec<PathBuf>, String> {
    if !keys_dir.exists() {
        return Ok(Vec::new());
    }

    let mut paths = Vec::new();
    for entry in fs::read_dir(keys_dir).map_err(|err| err.to_string())? {
        let entry = entry.map_err(|err| err.to_string())?;
        let path = entry.path();
        if path.is_file() {
            paths.push(path);
        }
    }
    Ok(paths)
}

fn find_stored_private_key(fingerprint: &str) -> Result<StoredPrivateKeyEntry, String> {
    let requested = normalized_fingerprint(fingerprint)?;
    let keys_dir = ripasso_keys_dir()?;
    let direct_path = keys_dir.join(requested.to_ascii_lowercase());
    if direct_path.exists() {
        return read_ripasso_private_key_entry(&direct_path);
    }

    for path in stored_private_key_paths(&keys_dir)? {
        let Ok(entry) = read_ripasso_private_key_entry(&path) else {
            continue;
        };
        if entry.key.fingerprint.eq_ignore_ascii_case(&requested) {
            return Ok(entry);
        }
    }

    Err(private_key_not_stored_error())
}

pub(in crate::backend::integrated) fn build_ripasso_crypto_from_key_ring(
    fingerprint: &str,
    key_ring: HashMap<[u8; 20], Arc<Cert>>,
) -> Result<Sequoia, String> {
    let user_key_id = fingerprint_from_string(fingerprint)?;
    let home =
        dirs_next::home_dir().ok_or_else(|| "Could not determine the home folder.".to_string())?;
    Ok(Sequoia::from_values(user_key_id, key_ring, &home))
}

pub(in crate::backend::integrated) fn load_stored_ripasso_key_ring(
) -> Result<HashMap<[u8; 20], Arc<Cert>>, String> {
    let keys_dir = ripasso_keys_dir()?;
    let mut key_ring = HashMap::new();

    for path in stored_private_key_paths(&keys_dir)? {
        let entry = read_ripasso_private_key_entry(&path)?;
        let fingerprint = slice_to_20_bytes(entry.cert.fingerprint().as_bytes())
            .map_err(|err| err.to_string())?;
        key_ring.insert(fingerprint, Arc::new(entry.cert));
    }

    Ok(key_ring)
}

pub(in crate::backend::integrated) fn load_ripasso_key_ring(
    fingerprint: &str,
) -> Result<HashMap<[u8; 20], Arc<Cert>>, String> {
    let user_key_id = fingerprint_from_string(fingerprint)?;
    let mut key_ring = load_stored_ripasso_key_ring()?;

    if let Some(cert) = cached_unlocked_ripasso_private_key(fingerprint)? {
        key_ring.insert(user_key_id, cert);
    }

    Ok(key_ring)
}

pub(in crate::backend::integrated) fn imported_private_key_fingerprints(
) -> Result<Vec<String>, String> {
    Ok(list_ripasso_private_keys()?
        .into_iter()
        .map(|key| key.fingerprint)
        .collect())
}

pub(in crate::backend::integrated) fn selected_ripasso_own_fingerprint(
) -> Result<Option<String>, String> {
    let settings = Preferences::new();
    let Some(configured) = settings.ripasso_own_fingerprint() else {
        return Ok(None);
    };

    let selected = list_ripasso_private_keys()?
        .into_iter()
        .find(|key| key.fingerprint.eq_ignore_ascii_case(&configured))
        .map(|key| key.fingerprint);

    if selected.is_none() {
        let _ = settings.set_ripasso_own_fingerprint(None);
    }

    Ok(selected)
}

pub(in crate::backend::integrated) fn ensure_ripasso_private_key_is_ready(
    fingerprint: &str,
) -> Result<(), PasswordEntryError> {
    if let Some(cert) =
        cached_unlocked_ripasso_private_key(fingerprint).map_err(PasswordEntryError::other)?
    {
        if !cert_can_decrypt_password_entries(&cert) {
            return Err(PasswordEntryError::incompatible_private_key(
                incompatible_private_key_error(),
            ));
        }
        return Ok(());
    }

    let entry = find_stored_private_key(fingerprint).map_err(|err| {
        if err == PRIVATE_KEY_NOT_STORED_ERROR {
            PasswordEntryError::missing_private_key(err)
        } else {
            PasswordEntryError::other(err)
        }
    })?;
    if cert_requires_passphrase(&entry.cert) {
        return Err(PasswordEntryError::locked_private_key(
            locked_private_key_error(),
        ));
    }
    if !cert_can_decrypt_password_entries(&entry.cert) {
        return Err(PasswordEntryError::incompatible_private_key(
            incompatible_private_key_error(),
        ));
    }
    Ok(())
}

pub fn is_ripasso_private_key_unlocked(fingerprint: &str) -> Result<bool, String> {
    Ok(cached_unlocked_ripasso_private_key(fingerprint)?.is_some())
}

pub fn ripasso_private_key_requires_session_unlock(fingerprint: &str) -> Result<bool, String> {
    if cached_unlocked_ripasso_private_key(fingerprint)?.is_some() {
        return Ok(false);
    }

    let entry = find_stored_private_key(fingerprint)?;
    Ok(cert_requires_passphrase(&entry.cert))
}

pub fn unlock_ripasso_private_key_for_session(
    fingerprint: &str,
    passphrase: &str,
) -> Result<ManagedRipassoPrivateKey, PrivateKeyError> {
    let entry = find_stored_private_key(fingerprint).map_err(|err| {
        if err == PRIVATE_KEY_NOT_STORED_ERROR {
            PrivateKeyError::not_stored(err)
        } else {
            PrivateKeyError::other(err)
        }
    })?;
    let unlocked = if cert_requires_passphrase(&entry.cert) {
        prepare_managed_private_key_bytes(
            &fs::read(&entry.path).map_err(|err| PrivateKeyError::other(err.to_string()))?,
            Some(passphrase),
        )?
        .0
    } else {
        entry.cert
    };

    if !cert_can_decrypt_password_entries(&unlocked) {
        return Err(PrivateKeyError::incompatible(
            "That private key cannot decrypt password store entries.",
        ));
    }

    cache_unlocked_ripasso_private_key(unlocked);
    Ok(entry.key)
}

pub fn ripasso_private_key_requires_passphrase(bytes: &[u8]) -> Result<bool, PrivateKeyError> {
    let (cert, _) = parse_managed_private_key_bytes(bytes)?;
    Ok(cert_requires_passphrase(&cert))
}

pub fn list_ripasso_private_keys() -> Result<Vec<ManagedRipassoPrivateKey>, String> {
    let keys_dir = ripasso_keys_dir()?;
    let mut keys: Vec<ManagedRipassoPrivateKey> = Vec::new();
    for path in stored_private_key_paths(&keys_dir)? {
        match read_ripasso_private_key_entry(&path) {
            Ok(entry) => {
                if !keys.iter().any(|existing: &ManagedRipassoPrivateKey| {
                    existing.fingerprint == entry.key.fingerprint
                }) {
                    keys.push(entry.key);
                }
            }
            Err(err) => {
                log_error(format!(
                    "Failed to load managed private key '{}': {err}",
                    path.display()
                ));
            }
        }
    }

    keys.sort_by(
        |left: &ManagedRipassoPrivateKey, right: &ManagedRipassoPrivateKey| {
            left.title()
                .to_ascii_lowercase()
                .cmp(&right.title().to_ascii_lowercase())
                .then_with(|| left.fingerprint.cmp(&right.fingerprint))
        },
    );
    Ok(keys)
}

pub fn import_ripasso_private_key_bytes(
    bytes: &[u8],
    passphrase: Option<&str>,
) -> Result<ManagedRipassoPrivateKey, PrivateKeyError> {
    let keys_dir = ripasso_keys_dir().map_err(PrivateKeyError::other)?;
    fs::create_dir_all(&keys_dir).map_err(|err| PrivateKeyError::other(err.to_string()))?;

    let (parsed_cert, key) = parse_managed_private_key_bytes(bytes)?;
    let stored_cert = if cert_requires_passphrase(&parsed_cert) {
        parsed_cert.clone()
    } else {
        return Err(PrivateKeyError::requires_password_protection(
            "That private key must be password protected before you can import it.",
        ));
    };
    let (unlocked_cert, _) = prepare_managed_private_key_bytes(bytes, passphrase)?;
    let mut file = File::create(keys_dir.join(key.fingerprint.to_ascii_lowercase()))
        .map_err(|err| PrivateKeyError::other(err.to_string()))?;
    stored_cert
        .as_tsk()
        .serialize(&mut file)
        .map_err(|err| PrivateKeyError::other(err.to_string()))?;
    cache_unlocked_ripasso_private_key(unlocked_cert);

    Ok(key)
}

pub fn generate_ripasso_private_key(
    name: &str,
    email: &str,
    passphrase: &str,
) -> Result<ManagedRipassoPrivateKey, PrivateKeyError> {
    let name = name.trim();
    if name.is_empty() {
        return Err(PrivateKeyError::other("Enter a name for the private key."));
    }

    let email = email.trim();
    if email.is_empty() {
        return Err(PrivateKeyError::other(
            "Enter an email address for the private key.",
        ));
    }

    let trimmed_passphrase = passphrase.trim();
    if trimmed_passphrase.is_empty() {
        return Err(PrivateKeyError::passphrase_required(
            "Enter the private key password.",
        ));
    }

    let password: Password = trimmed_passphrase.into();
    let user_id = format!("{name} <{email}>");
    let (cert, _) = CertBuilder::general_purpose(Some(user_id.as_str()))
        .set_password(Some(password))
        .generate()
        .map_err(|err| PrivateKeyError::other(err.to_string()))?;

    let mut bytes = Vec::new();
    cert.as_tsk()
        .serialize(&mut bytes)
        .map_err(|err| PrivateKeyError::other(err.to_string()))?;

    import_ripasso_private_key_bytes(&bytes, Some(trimmed_passphrase))
}

pub fn remove_ripasso_private_key(fingerprint: &str) -> Result<(), String> {
    let entry = find_stored_private_key(fingerprint)?;
    fs::remove_file(entry.path).map_err(|err| err.to_string())?;
    remove_cached_unlocked_ripasso_private_key(fingerprint)?;
    Ok(())
}

#[cfg(test)]
pub fn resolved_ripasso_own_fingerprint() -> Result<String, String> {
    selected_ripasso_own_fingerprint()?.ok_or_else(missing_private_key_error)
}

pub fn ripasso_private_key_title(fingerprint: &str) -> Result<String, String> {
    Ok(find_stored_private_key(fingerprint)?.key.title())
}
