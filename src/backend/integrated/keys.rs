use crate::logging::log_error;
use crate::preferences::Preferences;
use ripasso::crypto::{slice_to_20_bytes, Sequoia};
use sequoia_openpgp::{
    cert::amalgamation::key::PrimaryKey, crypto::Password, parse::Parse, serialize::Serialize,
    Cert, Fingerprint, Packet,
};
use std::collections::HashMap;
use std::fs;
use std::fs::File;
use std::path::{Path, PathBuf};
use std::sync::{Arc, OnceLock, RwLock};

const PRIVATE_KEY_NOT_STORED_ERROR: &str = "That private key is not stored in the app.";
const MISSING_PRIVATE_KEY_ERROR: &str =
    "Import a private key in Preferences before using the password store.";
const LOCKED_PRIVATE_KEY_ERROR: &str =
    "A private key for this item is locked. Unlock it in Preferences and enter its password.";
const INCOMPATIBLE_PRIVATE_KEY_ERROR: &str =
    "The available private keys cannot decrypt this item.";

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ManagedRipassoPrivateKey {
    pub fingerprint: String,
    pub user_ids: Vec<String>,
}

impl ManagedRipassoPrivateKey {
    pub fn title(&self) -> String {
        self.user_ids
            .first()
            .cloned()
            .filter(|value| !value.trim().is_empty())
            .unwrap_or_else(|| "Unnamed private key".to_string())
    }
}

pub(super) fn ripasso_keys_dir() -> Result<PathBuf, String> {
    let data_dir = dirs_next::data_local_dir()
        .ok_or_else(|| "Could not determine the data folder.".to_string())?;
    Ok(data_dir.join(env!("CARGO_PKG_NAME")).join("keys"))
}

fn unlocked_ripasso_private_keys() -> &'static RwLock<HashMap<String, Arc<Cert>>> {
    static UNLOCKED_KEYS: OnceLock<RwLock<HashMap<String, Arc<Cert>>>> = OnceLock::new();
    UNLOCKED_KEYS.get_or_init(|| RwLock::new(HashMap::new()))
}

fn with_unlocked_ripasso_keys_read<T>(f: impl FnOnce(&HashMap<String, Arc<Cert>>) -> T) -> T {
    match unlocked_ripasso_private_keys().read() {
        Ok(keys) => f(&keys),
        Err(poisoned) => {
            let keys = poisoned.into_inner();
            f(&keys)
        }
    }
}

fn with_unlocked_ripasso_keys_write<T>(
    f: impl FnOnce(&mut HashMap<String, Arc<Cert>>) -> T,
) -> T {
    match unlocked_ripasso_private_keys().write() {
        Ok(mut keys) => f(&mut keys),
        Err(poisoned) => {
            let mut keys = poisoned.into_inner();
            f(&mut keys)
        }
    }
}

pub(super) fn fingerprint_from_string(value: &str) -> Result<[u8; 20], String> {
    let fingerprint = Fingerprint::from_hex(value)
        .map_err(|err| format!("Invalid private key fingerprint '{value}': {err}"))?;
    let bytes = fingerprint.as_bytes();
    if bytes.len() != 20 {
        return Err(format!(
            "Private key fingerprint '{value}' does not have the expected length."
        ));
    }

    let mut parsed = [0u8; 20];
    parsed.copy_from_slice(bytes);
    Ok(parsed)
}

fn normalized_fingerprint(value: &str) -> Result<String, String> {
    Ok(Fingerprint::from_hex(value)
        .map_err(|err| format!("Invalid private key fingerprint '{value}': {err}"))?
        .to_hex())
}

pub(super) fn cached_unlocked_ripasso_private_key(
    fingerprint: &str,
) -> Result<Option<Arc<Cert>>, String> {
    let fingerprint = normalized_fingerprint(fingerprint)?;
    Ok(with_unlocked_ripasso_keys_read(|keys| {
        keys.get(&fingerprint).cloned()
    }))
}

fn cache_unlocked_ripasso_private_key(cert: Cert) {
    let fingerprint = cert.fingerprint().to_hex();
    with_unlocked_ripasso_keys_write(|keys| {
        keys.insert(fingerprint, Arc::new(cert));
    });
}

fn remove_cached_unlocked_ripasso_private_key(fingerprint: &str) -> Result<(), String> {
    let fingerprint = normalized_fingerprint(fingerprint)?;
    with_unlocked_ripasso_keys_write(|keys| {
        keys.remove(&fingerprint);
    });
    Ok(())
}

#[cfg(test)]
pub(super) fn clear_cached_unlocked_ripasso_private_keys() {
    with_unlocked_ripasso_keys_write(|keys| keys.clear());
}

pub(super) fn missing_private_key_error() -> String {
    MISSING_PRIVATE_KEY_ERROR.to_string()
}

pub(super) fn locked_private_key_error() -> String {
    LOCKED_PRIVATE_KEY_ERROR.to_string()
}

pub(super) fn incompatible_private_key_error() -> String {
    INCOMPATIBLE_PRIVATE_KEY_ERROR.to_string()
}

fn private_key_not_stored_error() -> String {
    PRIVATE_KEY_NOT_STORED_ERROR.to_string()
}

pub(super) fn parse_managed_private_key_bytes(
    bytes: &[u8],
) -> Result<(Cert, ManagedRipassoPrivateKey), String> {
    let cert = Cert::from_bytes(bytes).map_err(|err| err.to_string())?;
    if !cert.is_tsk() {
        return Err("That OpenPGP key file does not include a private key.".to_string());
    }

    let key = ManagedRipassoPrivateKey {
        fingerprint: cert.fingerprint().to_hex(),
        user_ids: cert
            .userids()
            .map(|user_id| user_id.userid().to_string())
            .filter(|value| !value.trim().is_empty())
            .collect(),
    };

    Ok((cert, key))
}

fn cert_requires_passphrase(cert: &Cert) -> bool {
    cert.keys()
        .secret()
        .any(|key_amalgamation| !key_amalgamation.key().has_unencrypted_secret())
}

pub(super) fn cert_can_decrypt_password_entries(cert: &Cert) -> bool {
    let policy = sequoia_openpgp::policy::StandardPolicy::new();
    cert.keys()
        .with_policy(&policy, None)
        .supported()
        .alive()
        .revoked(false)
        .for_transport_encryption()
        .unencrypted_secret()
        .next()
        .is_some()
}

fn unlock_managed_private_key_cert(cert: &Cert, passphrase: &str) -> Result<Cert, String> {
    let trimmed = passphrase.trim();
    if trimmed.is_empty() {
        return Err("Enter the private key password.".to_string());
    }

    let password: Password = trimmed.into();
    let mut unlocked = cert.clone();
    for key_amalgamation in cert.keys().secret() {
        if key_amalgamation.key().has_unencrypted_secret() {
            continue;
        }

        let key = key_amalgamation
            .key()
            .clone()
            .decrypt_secret(&password)
            .map_err(|_| "The private key password is incorrect.".to_string())?;
        let packet: Packet = if key_amalgamation.primary() {
            key.role_into_primary().into()
        } else {
            key.role_into_subordinate().into()
        };
        unlocked = unlocked
            .insert_packets(vec![packet])
            .map_err(|err| err.to_string())?
            .0;
    }

    Ok(unlocked)
}

pub(super) fn prepare_managed_private_key_bytes(
    bytes: &[u8],
    passphrase: Option<&str>,
) -> Result<(Cert, ManagedRipassoPrivateKey), String> {
    let (parsed_cert, key) = parse_managed_private_key_bytes(bytes)?;
    let cert = if cert_requires_passphrase(&parsed_cert) {
        let passphrase =
            passphrase.ok_or_else(|| "This private key is password protected.".to_string())?;
        unlock_managed_private_key_cert(&parsed_cert, passphrase)?
    } else {
        parsed_cert
    };

    if !cert_can_decrypt_password_entries(&cert) {
        return Err("That private key cannot decrypt password store entries.".to_string());
    }

    Ok((cert, key))
}

fn read_ripasso_private_key_cert(path: &Path) -> Result<(Cert, ManagedRipassoPrivateKey), String> {
    let data = fs::read(path).map_err(|err| err.to_string())?;
    parse_managed_private_key_bytes(&data)
}

fn find_ripasso_private_key_cert(
    fingerprint: &str,
) -> Result<(PathBuf, Cert, ManagedRipassoPrivateKey), String> {
    let requested = normalized_fingerprint(fingerprint)?;
    let keys_dir = ripasso_keys_dir()?;
    let direct_path = keys_dir.join(requested.to_ascii_lowercase());
    if direct_path.exists() {
        let (cert, key) = read_ripasso_private_key_cert(&direct_path)?;
        return Ok((direct_path, cert, key));
    }

    if !keys_dir.exists() {
        return Err(private_key_not_stored_error());
    }

    for entry in fs::read_dir(&keys_dir).map_err(|err| err.to_string())? {
        let entry = entry.map_err(|err| err.to_string())?;
        let path = entry.path();
        if !path.is_file() {
            continue;
        }

        let Ok((cert, key)) = read_ripasso_private_key_cert(&path) else {
            continue;
        };
        if key.fingerprint.eq_ignore_ascii_case(&requested) {
            return Ok((path, cert, key));
        }
    }

    Err(private_key_not_stored_error())
}

pub(super) fn build_ripasso_crypto_from_key_ring(
    fingerprint: &str,
    key_ring: HashMap<[u8; 20], Arc<Cert>>,
) -> Result<Sequoia, String> {
    let user_key_id = fingerprint_from_string(fingerprint)?;
    let home =
        dirs_next::home_dir().ok_or_else(|| "Could not determine the home folder.".to_string())?;
    Ok(Sequoia::from_values(user_key_id, key_ring, &home))
}

pub(super) fn load_stored_ripasso_key_ring() -> Result<HashMap<[u8; 20], Arc<Cert>>, String> {
    let keys_dir = ripasso_keys_dir()?;
    let mut key_ring = HashMap::new();

    if keys_dir.exists() {
        for entry in fs::read_dir(&keys_dir).map_err(|err| err.to_string())? {
            let entry = entry.map_err(|err| err.to_string())?;
            let path = entry.path();
            if !path.is_file() {
                continue;
            }

            let data = fs::read(&path).map_err(|err| err.to_string())?;
            let cert = Cert::from_bytes(&data).map_err(|err| err.to_string())?;
            let entry_fingerprint =
                slice_to_20_bytes(cert.fingerprint().as_bytes()).map_err(|err| err.to_string())?;
            key_ring.insert(entry_fingerprint, Arc::new(cert));
        }
    }

    Ok(key_ring)
}

pub(super) fn load_ripasso_key_ring(
    fingerprint: &str,
) -> Result<HashMap<[u8; 20], Arc<Cert>>, String> {
    let user_key_id = fingerprint_from_string(fingerprint)?;
    let mut key_ring = load_stored_ripasso_key_ring()?;

    if let Some(cert) = cached_unlocked_ripasso_private_key(fingerprint)? {
        key_ring.insert(user_key_id, cert);
    }

    Ok(key_ring)
}

pub(super) fn available_unlocked_private_key_fingerprints(preferred: &str) -> Vec<String> {
    let mut fingerprints = vec![preferred.to_string()];
    with_unlocked_ripasso_keys_read(|keys| {
        for fingerprint in keys.keys() {
            if !fingerprints
                .iter()
                .any(|existing| existing.eq_ignore_ascii_case(fingerprint))
            {
                fingerprints.push(fingerprint.clone());
            }
        }
    });
    fingerprints
}

pub(super) fn imported_private_key_fingerprints() -> Result<Vec<String>, String> {
    Ok(list_ripasso_private_keys()?
        .into_iter()
        .map(|key| key.fingerprint)
        .collect())
}

pub(super) fn selected_ripasso_own_fingerprint() -> Result<Option<String>, String> {
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

pub(super) fn ensure_ripasso_private_key_is_ready(fingerprint: &str) -> Result<(), String> {
    if let Some(cert) = cached_unlocked_ripasso_private_key(fingerprint)? {
        if !cert_can_decrypt_password_entries(&cert) {
            return Err(incompatible_private_key_error());
        }
        return Ok(());
    }

    let (_, cert, _) = find_ripasso_private_key_cert(fingerprint)?;
    if cert_requires_passphrase(&cert) {
        return Err(locked_private_key_error());
    }
    if !cert_can_decrypt_password_entries(&cert) {
        return Err(incompatible_private_key_error());
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

    let (_, cert, _) = find_ripasso_private_key_cert(fingerprint)?;
    Ok(cert_requires_passphrase(&cert))
}

pub fn unlock_ripasso_private_key_for_session(
    fingerprint: &str,
    passphrase: &str,
) -> Result<ManagedRipassoPrivateKey, String> {
    let (_, cert, key) = find_ripasso_private_key_cert(fingerprint)?;
    let unlocked = if cert_requires_passphrase(&cert) {
        unlock_managed_private_key_cert(&cert, passphrase)?
    } else {
        cert
    };

    if !cert_can_decrypt_password_entries(&unlocked) {
        return Err("That private key cannot decrypt password store entries.".to_string());
    }

    cache_unlocked_ripasso_private_key(unlocked);
    Ok(key)
}

pub fn ripasso_private_key_requires_passphrase(bytes: &[u8]) -> Result<bool, String> {
    let (cert, _) = parse_managed_private_key_bytes(bytes)?;
    Ok(cert_requires_passphrase(&cert))
}

pub fn list_ripasso_private_keys() -> Result<Vec<ManagedRipassoPrivateKey>, String> {
    let keys_dir = ripasso_keys_dir()?;
    if !keys_dir.exists() {
        return Ok(Vec::new());
    }

    let mut keys: Vec<ManagedRipassoPrivateKey> = Vec::new();
    for entry in fs::read_dir(&keys_dir).map_err(|err| err.to_string())? {
        let entry = entry.map_err(|err| err.to_string())?;
        let path = entry.path();
        if !path.is_file() {
            continue;
        }

        let data = match fs::read(&path) {
            Ok(data) => data,
            Err(err) => {
                log_error(format!(
                    "Failed to read managed private key '{}': {err}",
                    path.display()
                ));
                continue;
            }
        };

        match parse_managed_private_key_bytes(&data) {
            Ok((_, key)) => {
                if !keys
                    .iter()
                    .any(|existing: &ManagedRipassoPrivateKey| existing.fingerprint == key.fingerprint)
                {
                    keys.push(key);
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

    keys.sort_by(|left: &ManagedRipassoPrivateKey, right: &ManagedRipassoPrivateKey| {
        left.title()
            .to_ascii_lowercase()
            .cmp(&right.title().to_ascii_lowercase())
            .then_with(|| left.fingerprint.cmp(&right.fingerprint))
    });
    Ok(keys)
}

pub fn import_ripasso_private_key_bytes(
    bytes: &[u8],
    passphrase: Option<&str>,
) -> Result<ManagedRipassoPrivateKey, String> {
    let keys_dir = ripasso_keys_dir()?;
    fs::create_dir_all(&keys_dir).map_err(|err| err.to_string())?;

    let (parsed_cert, key) = parse_managed_private_key_bytes(bytes)?;
    let stored_cert = if cert_requires_passphrase(&parsed_cert) {
        parsed_cert.clone()
    } else {
        return Err(
            "That private key must be password protected before you can import it.".to_string(),
        );
    };
    let (unlocked_cert, _) = prepare_managed_private_key_bytes(bytes, passphrase)?;
    let mut file = File::create(keys_dir.join(key.fingerprint.to_ascii_lowercase()))
        .map_err(|err| err.to_string())?;
    stored_cert
        .as_tsk()
        .serialize(&mut file)
        .map_err(|err| err.to_string())?;
    cache_unlocked_ripasso_private_key(unlocked_cert);

    Ok(key)
}

pub fn remove_ripasso_private_key(fingerprint: &str) -> Result<(), String> {
    let (path, _, _) = find_ripasso_private_key_cert(fingerprint)?;
    fs::remove_file(path).map_err(|err| err.to_string())?;
    remove_cached_unlocked_ripasso_private_key(fingerprint)?;
    Ok(())
}

#[cfg_attr(not(test), allow(dead_code))]
pub fn resolved_ripasso_own_fingerprint() -> Result<String, String> {
    selected_ripasso_own_fingerprint()?.ok_or_else(missing_private_key_error)
}

pub fn ripasso_private_key_title(fingerprint: &str) -> Result<String, String> {
    let (_, _, key) = find_ripasso_private_key_cert(fingerprint)?;
    Ok(key.title())
}
