#[cfg(feature = "fidokey")]
use super::cache::clear_cached_fido2_pin;
use super::cache::{
    cache_unlocked_hardware_private_key, cache_unlocked_ripasso_private_key,
    cached_unlocked_hardware_private_key, cached_unlocked_ripasso_private_key,
    remove_cached_unlocked_ripasso_private_key,
};
#[cfg(feature = "fidokey")]
use super::cert::parse_fido2_public_key_bytes;
use super::cert::{
    cert_can_decrypt_password_entries, cert_has_transport_encryption_key, cert_requires_passphrase,
    fingerprint_from_string, normalized_fingerprint, parse_hardware_public_key_bytes,
    parse_managed_private_key_bytes, prepare_managed_private_key_bytes, ManagedRipassoHardwareKey,
    ManagedRipassoPrivateKey, ManagedRipassoPrivateKeyProtection, PrivateKeyUnlockRequest,
};
use super::hardware::{
    list_hardware_tokens, verify_hardware_session, HardwareSessionPolicy, HardwareUnlockMode,
};
use crate::backend::{PasswordEntryError, PrivateKeyError};
#[cfg(feature = "fidokey")]
use crate::fido2_recipient::parse_fido2_recipient_string;
use crate::logging::log_error;
use crate::preferences::Preferences;
use ripasso::crypto::{slice_to_20_bytes, Sequoia};
use sequoia_openpgp::{
    cert::CertBuilder,
    crypto::Password,
    serialize::{Serialize, SerializeInto},
    Cert,
};
use serde::{Deserialize, Serialize as SerdeSerialize};
use std::collections::HashMap;
use std::fs;
use std::fs::File;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use zeroize::Zeroizing;

const PRIVATE_KEY_NOT_STORED_ERROR: &str = "That private key is not stored in the app.";
const MISSING_PRIVATE_KEY_ERROR: &str =
    "Import a private key in Preferences before using the password store.";
const LOCKED_PRIVATE_KEY_ERROR: &str =
    "A private key for this item is locked. Unlock it in Preferences.";
const INCOMPATIBLE_PRIVATE_KEY_ERROR: &str = "The available private keys cannot decrypt this item.";
#[cfg(not(feature = "fidokey"))]
const FIDO2_PRIVATE_KEY_FEATURE_DISABLED_ERROR: &str =
    "FIDO2 private-key support is disabled in this build of Keycord.";
const HARDWARE_MANIFEST_FORMAT: u32 = 1;
const HARDWARE_PROTECTION_KIND: &str = "hardware-openpgp-card";
#[cfg(feature = "fidokey")]
const FIDO2_PRIVATE_KEY_MANIFEST_FORMAT: u32 = 1;
#[cfg(feature = "fidokey")]
const FIDO2_PRIVATE_KEY_PROTECTION_KIND: &str = "fido2-hmac-secret";

#[derive(Clone, Debug)]
pub(in crate::backend::integrated) enum StoredPrivateKeyLocation {
    Password {
        path: PathBuf,
    },
    Hardware {
        dir: PathBuf,
        hardware: ManagedRipassoHardwareKey,
    },
    #[cfg(feature = "fidokey")]
    Fido2 {
        path: PathBuf,
    },
}

#[derive(Clone, Debug)]
pub(in crate::backend::integrated) struct StoredPrivateKeyEntry {
    pub(in crate::backend::integrated) cert: Option<Cert>,
    pub(in crate::backend::integrated) key: ManagedRipassoPrivateKey,
    pub(in crate::backend::integrated) location: StoredPrivateKeyLocation,
}

#[derive(Debug, Clone, SerdeSerialize, Deserialize)]
struct HardwarePrivateKeyManifest {
    format: u32,
    protection: String,
    fingerprint: String,
    user_ids: Vec<String>,
    ident: String,
    signing_fingerprint: Option<String>,
    decryption_fingerprint: Option<String>,
    reader_hint: Option<String>,
}

#[cfg(feature = "fidokey")]
#[derive(Debug, Clone, SerdeSerialize, Deserialize)]
struct Fido2PrivateKeyManifest {
    format: u32,
    protection: String,
    fingerprint: String,
    public_key: String,
    encrypted_private_key: String,
}

impl HardwarePrivateKeyManifest {
    fn from_key(key: &ManagedRipassoPrivateKey, hardware: &ManagedRipassoHardwareKey) -> Self {
        Self {
            format: HARDWARE_MANIFEST_FORMAT,
            protection: HARDWARE_PROTECTION_KIND.to_string(),
            fingerprint: key.fingerprint.clone(),
            user_ids: key.user_ids.clone(),
            ident: hardware.ident.clone(),
            signing_fingerprint: hardware.signing_fingerprint.clone(),
            decryption_fingerprint: hardware.decryption_fingerprint.clone(),
            reader_hint: hardware.reader_hint.clone(),
        }
    }

    fn hardware(&self) -> ManagedRipassoHardwareKey {
        ManagedRipassoHardwareKey {
            ident: self.ident.clone(),
            signing_fingerprint: self.signing_fingerprint.clone(),
            decryption_fingerprint: self.decryption_fingerprint.clone(),
            reader_hint: self.reader_hint.clone(),
        }
    }
}

pub(in crate::backend::integrated) fn ripasso_keys_dir() -> Result<PathBuf, String> {
    let data_dir = dirs_next::data_local_dir()
        .ok_or_else(|| "Could not determine the data folder.".to_string())?;
    Ok(data_dir.join(env!("CARGO_PKG_NAME")).join("keys"))
}

fn ripasso_keys_v2_dir() -> Result<PathBuf, String> {
    let data_dir = dirs_next::data_local_dir()
        .ok_or_else(|| "Could not determine the data folder.".to_string())?;
    Ok(data_dir.join(env!("CARGO_PKG_NAME")).join("keys-v2"))
}

#[cfg(feature = "fidokey")]
fn ripasso_fido_keys_dir() -> Result<PathBuf, String> {
    let data_dir = dirs_next::data_local_dir()
        .ok_or_else(|| "Could not determine the data folder.".to_string())?;
    Ok(data_dir.join(env!("CARGO_PKG_NAME")).join("keys-fido"))
}

fn hardware_manifest_path(dir: &Path) -> PathBuf {
    dir.join("manifest.toml")
}

fn hardware_public_key_path(dir: &Path) -> PathBuf {
    dir.join("public.asc")
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

fn private_key_error_from_hardware_message(message: impl Into<String>) -> PrivateKeyError {
    let message = message.into();
    let lowered = message.to_ascii_lowercase();

    if lowered.contains("couldn't find card")
        || lowered.contains("no smartcard")
        || lowered.contains("reader error")
        || lowered.contains("context error")
    {
        PrivateKeyError::hardware_token_not_present(message)
    } else if lowered.contains("does not match the stored")
        || lowered.contains("does not match the hardware")
        || lowered.contains("connect the matching hardware key")
    {
        PrivateKeyError::hardware_token_mismatch(message)
    } else if lowered.contains("enter the hardware key pin") {
        PrivateKeyError::hardware_pin_required(message)
    } else if lowered.contains("password not checked")
        || lowered.contains("authentication method blocked")
        || lowered.contains("security status not satisfied")
    {
        PrivateKeyError::incorrect_hardware_pin(message)
    } else if lowered.contains("not transacted")
        || lowered.contains("reset")
        || lowered.contains("removed")
    {
        PrivateKeyError::hardware_token_removed(message)
    } else if lowered.contains("does not support")
        || lowered.contains("cannot decrypt password store entries")
        || lowered.contains("cannot sign git commits")
        || lowered.contains("unsupported")
    {
        PrivateKeyError::unsupported_hardware_key(message)
    } else {
        PrivateKeyError::other(message)
    }
}

fn read_password_private_key_entry(path: &Path) -> Result<StoredPrivateKeyEntry, String> {
    let data = fs::read(path).map_err(|err| err.to_string())?;
    let (cert, key) = parse_managed_private_key_bytes(&data).map_err(|err| err.to_string())?;
    Ok(StoredPrivateKeyEntry {
        cert: Some(cert),
        key,
        location: StoredPrivateKeyLocation::Password {
            path: path.to_path_buf(),
        },
    })
}

fn read_hardware_private_key_entry(dir: &Path) -> Result<StoredPrivateKeyEntry, String> {
    let manifest_path = hardware_manifest_path(dir);
    let manifest: HardwarePrivateKeyManifest =
        toml::from_str(&fs::read_to_string(&manifest_path).map_err(|err| err.to_string())?)
            .map_err(|err| err.to_string())?;
    if manifest.format != HARDWARE_MANIFEST_FORMAT {
        return Err(format!(
            "Unsupported hardware key manifest format {}.",
            manifest.format
        ));
    }
    if manifest.protection != HARDWARE_PROTECTION_KIND {
        return Err(format!(
            "Unsupported hardware key protection '{}'.",
            manifest.protection
        ));
    }

    let hardware = manifest.hardware();
    let (cert, mut key) = parse_hardware_public_key_bytes(
        &fs::read(hardware_public_key_path(dir)).map_err(|err| err.to_string())?,
        hardware.clone(),
    )
    .map_err(|err| err.to_string())?;
    key.user_ids = manifest.user_ids;

    Ok(StoredPrivateKeyEntry {
        cert: Some(cert),
        key,
        location: StoredPrivateKeyLocation::Hardware {
            dir: dir.to_path_buf(),
            hardware,
        },
    })
}

fn stored_private_key_file_paths(keys_dir: &Path) -> Result<Vec<PathBuf>, String> {
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

#[cfg(feature = "fidokey")]
fn managed_fido2_private_key_from_cert(cert: &Cert) -> ManagedRipassoPrivateKey {
    ManagedRipassoPrivateKey {
        fingerprint: cert.fingerprint().to_hex(),
        user_ids: cert
            .userids()
            .map(|user_id| user_id.userid().to_string())
            .filter(|value| !value.trim().is_empty())
            .collect(),
        protection: ManagedRipassoPrivateKeyProtection::Fido2HmacSecret,
        hardware: None,
    }
}

#[cfg(feature = "fidokey")]
fn parse_fido2_private_key_manifest(
    contents: &str,
) -> Result<Option<Fido2PrivateKeyManifest>, String> {
    toml::from_str(contents).map(Some).or_else(|_| Ok(None))
}

#[cfg(feature = "fidokey")]
fn parse_fido2_private_key_manifest_bytes(
    bytes: &[u8],
) -> Result<Option<Fido2PrivateKeyManifest>, String> {
    let Ok(contents) = std::str::from_utf8(bytes) else {
        return Ok(None);
    };
    parse_fido2_private_key_manifest(contents)
}

#[cfg(feature = "fidokey")]
fn read_fido2_private_key_manifest_entry(
    path: &Path,
    manifest: Fido2PrivateKeyManifest,
) -> Result<StoredPrivateKeyEntry, String> {
    if manifest.format != FIDO2_PRIVATE_KEY_MANIFEST_FORMAT {
        return Err(format!(
            "Unsupported FIDO2 private key format {}.",
            manifest.format
        ));
    }
    if manifest.protection != FIDO2_PRIVATE_KEY_PROTECTION_KIND {
        return Err(format!(
            "Unsupported FIDO2 private key protection '{}'.",
            manifest.protection
        ));
    }

    let (cert, key) = parse_fido2_public_key_bytes(manifest.public_key.as_bytes())
        .map_err(|err| err.to_string())?;
    let expected = normalized_fingerprint(&manifest.fingerprint)?;
    if !key.fingerprint.eq_ignore_ascii_case(&expected) {
        return Err("That FIDO2-protected key is invalid.".to_string());
    }

    Ok(StoredPrivateKeyEntry {
        cert: Some(cert),
        key,
        location: StoredPrivateKeyLocation::Fido2 {
            path: path.to_path_buf(),
        },
    })
}

#[cfg(feature = "fidokey")]
fn read_fido2_private_key_entry(path: &Path) -> Result<StoredPrivateKeyEntry, String> {
    let contents = fs::read_to_string(path).map_err(|err| err.to_string())?;
    let manifest = parse_fido2_private_key_manifest(&contents)?
        .ok_or_else(|| "That FIDO2-protected key is invalid.".to_string())?;
    read_fido2_private_key_manifest_entry(path, manifest)
}

fn stored_hardware_private_key_dirs(keys_dir: &Path) -> Result<Vec<PathBuf>, String> {
    if !keys_dir.exists() {
        return Ok(Vec::new());
    }

    let mut dirs = Vec::new();
    for entry in fs::read_dir(keys_dir).map_err(|err| err.to_string())? {
        let entry = entry.map_err(|err| err.to_string())?;
        let path = entry.path();
        if path.is_dir() {
            dirs.push(path);
        }
    }
    Ok(dirs)
}

pub(in crate::backend::integrated) fn find_stored_private_key(
    fingerprint: &str,
) -> Result<StoredPrivateKeyEntry, String> {
    let requested = normalized_fingerprint(fingerprint)?;

    let legacy_dir = ripasso_keys_dir()?;
    let direct_legacy_path = legacy_dir.join(requested.to_ascii_lowercase());
    if direct_legacy_path.exists() {
        return read_password_private_key_entry(&direct_legacy_path);
    }

    let hardware_dir = ripasso_keys_v2_dir()?;
    let direct_hardware_dir = hardware_dir.join(requested.to_ascii_lowercase());
    if direct_hardware_dir.exists() {
        return read_hardware_private_key_entry(&direct_hardware_dir);
    }

    #[cfg(feature = "fidokey")]
    {
        let fido2_dir = ripasso_fido_keys_dir()?;
        let direct_fido2_path = fido2_dir.join(requested.to_ascii_lowercase());
        if direct_fido2_path.exists() {
            return read_fido2_private_key_entry(&direct_fido2_path);
        }
    }

    for path in stored_private_key_file_paths(&legacy_dir)? {
        let Ok(entry) = read_password_private_key_entry(&path) else {
            continue;
        };
        if entry.key.fingerprint.eq_ignore_ascii_case(&requested) {
            return Ok(entry);
        }
    }

    for dir in stored_hardware_private_key_dirs(&hardware_dir)? {
        let Ok(entry) = read_hardware_private_key_entry(&dir) else {
            continue;
        };
        if entry.key.fingerprint.eq_ignore_ascii_case(&requested) {
            return Ok(entry);
        }
    }

    #[cfg(feature = "fidokey")]
    for path in stored_private_key_file_paths(&ripasso_fido_keys_dir()?)? {
        let Ok(entry) = read_fido2_private_key_entry(&path) else {
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
    let mut key_ring = HashMap::new();

    for path in stored_private_key_file_paths(&ripasso_keys_dir()?)? {
        let entry = read_password_private_key_entry(&path)?;
        let cert = entry
            .cert
            .as_ref()
            .ok_or_else(|| "Missing OpenPGP certificate for stored private key.".to_string())?;
        let fingerprint =
            slice_to_20_bytes(cert.fingerprint().as_bytes()).map_err(|err| err.to_string())?;
        key_ring.insert(fingerprint, Arc::new(cert.clone()));
    }

    for dir in stored_hardware_private_key_dirs(&ripasso_keys_v2_dir()?)? {
        let entry = read_hardware_private_key_entry(&dir)?;
        let cert = entry
            .cert
            .as_ref()
            .ok_or_else(|| "Missing OpenPGP certificate for stored hardware key.".to_string())?;
        let fingerprint =
            slice_to_20_bytes(cert.fingerprint().as_bytes()).map_err(|err| err.to_string())?;
        key_ring.insert(fingerprint, Arc::new(cert.clone()));
    }

    #[cfg(feature = "fidokey")]
    for path in stored_private_key_file_paths(&ripasso_fido_keys_dir()?)? {
        let entry = read_fido2_private_key_entry(&path)?;
        let Some(cert) = entry.cert.as_ref() else {
            continue;
        };
        let fingerprint =
            slice_to_20_bytes(cert.fingerprint().as_bytes()).map_err(|err| err.to_string())?;
        key_ring.insert(fingerprint, Arc::new(cert.clone()));
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

fn stored_key_can_decrypt(entry: &StoredPrivateKeyEntry) -> bool {
    match entry.key.protection {
        ManagedRipassoPrivateKeyProtection::Password => entry
            .cert
            .as_ref()
            .is_some_and(cert_can_decrypt_password_entries),
        ManagedRipassoPrivateKeyProtection::HardwareOpenPgpCard => entry
            .cert
            .as_ref()
            .is_some_and(cert_has_transport_encryption_key),
        #[cfg(feature = "fidokey")]
        ManagedRipassoPrivateKeyProtection::Fido2HmacSecret => entry
            .cert
            .as_ref()
            .is_some_and(cert_has_transport_encryption_key),
    }
}

#[cfg(feature = "fidokey")]
fn cached_fido2_private_key_is_unlocked(fingerprint: &str) -> Result<bool, String> {
    Ok(cached_unlocked_ripasso_private_key(fingerprint)?.is_some())
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

    match entry.key.protection {
        ManagedRipassoPrivateKeyProtection::Password => {
            let cert = entry
                .cert
                .as_ref()
                .ok_or_else(|| PasswordEntryError::other(private_key_not_stored_error()))?;
            if cert_requires_passphrase(cert) {
                return Err(PasswordEntryError::locked_private_key(
                    locked_private_key_error(),
                ));
            }
            if !cert_can_decrypt_password_entries(cert) {
                return Err(PasswordEntryError::incompatible_private_key(
                    incompatible_private_key_error(),
                ));
            }
            Ok(())
        }
        ManagedRipassoPrivateKeyProtection::HardwareOpenPgpCard => {
            if cached_unlocked_hardware_private_key(fingerprint)
                .map_err(PasswordEntryError::other)?
                .is_none()
            {
                return Err(PasswordEntryError::locked_private_key(
                    locked_private_key_error(),
                ));
            }
            if !stored_key_can_decrypt(&entry) {
                return Err(PasswordEntryError::incompatible_private_key(
                    incompatible_private_key_error(),
                ));
            }
            Ok(())
        }
        #[cfg(feature = "fidokey")]
        ManagedRipassoPrivateKeyProtection::Fido2HmacSecret => {
            if !cached_fido2_private_key_is_unlocked(fingerprint)
                .map_err(PasswordEntryError::other)?
            {
                return Err(PasswordEntryError::locked_private_key(
                    locked_private_key_error(),
                ));
            }
            if !stored_key_can_decrypt(&entry) {
                return Err(PasswordEntryError::incompatible_private_key(
                    incompatible_private_key_error(),
                ));
            }
            Ok(())
        }
    }
}

pub fn is_ripasso_private_key_unlocked(fingerprint: &str) -> Result<bool, String> {
    let entry = find_stored_private_key(fingerprint)?;
    match entry.key.protection {
        ManagedRipassoPrivateKeyProtection::Password => {
            Ok(cached_unlocked_ripasso_private_key(fingerprint)?.is_some())
        }
        ManagedRipassoPrivateKeyProtection::HardwareOpenPgpCard => {
            Ok(cached_unlocked_hardware_private_key(fingerprint)?.is_some())
        }
        #[cfg(feature = "fidokey")]
        ManagedRipassoPrivateKeyProtection::Fido2HmacSecret => {
            cached_fido2_private_key_is_unlocked(fingerprint)
        }
    }
}

pub fn ripasso_private_key_requires_session_unlock(fingerprint: &str) -> Result<bool, String> {
    let entry = find_stored_private_key(fingerprint)?;
    match entry.key.protection {
        ManagedRipassoPrivateKeyProtection::Password => {
            if cached_unlocked_ripasso_private_key(fingerprint)?.is_some() {
                return Ok(false);
            }
            let cert = entry
                .cert
                .as_ref()
                .ok_or_else(private_key_not_stored_error)?;
            Ok(cert_requires_passphrase(cert))
        }
        ManagedRipassoPrivateKeyProtection::HardwareOpenPgpCard => {
            Ok(cached_unlocked_hardware_private_key(fingerprint)?.is_none())
        }
        #[cfg(feature = "fidokey")]
        ManagedRipassoPrivateKeyProtection::Fido2HmacSecret => {
            Ok(!cached_fido2_private_key_is_unlocked(fingerprint)?)
        }
    }
}

fn password_unlock_request(request: PrivateKeyUnlockRequest) -> Result<String, PrivateKeyError> {
    match request {
        PrivateKeyUnlockRequest::Password(passphrase) => Ok(passphrase),
        PrivateKeyUnlockRequest::HardwarePin(_)
        | PrivateKeyUnlockRequest::HardwareExternal
        | PrivateKeyUnlockRequest::Fido2(_) => Err(PrivateKeyError::other(
            "This private key is password protected.",
        )),
    }
}

fn hardware_unlock_mode(
    request: PrivateKeyUnlockRequest,
) -> Result<HardwareUnlockMode, PrivateKeyError> {
    match request {
        PrivateKeyUnlockRequest::HardwarePin(pin) => {
            let trimmed = pin.trim();
            if trimmed.is_empty() {
                return Err(PrivateKeyError::hardware_pin_required(
                    "Enter the hardware key PIN.",
                ));
            }
            Ok(HardwareUnlockMode::Pin(Arc::new(Zeroizing::new(
                trimmed.as_bytes().to_vec(),
            ))))
        }
        PrivateKeyUnlockRequest::HardwareExternal => Ok(HardwareUnlockMode::External),
        PrivateKeyUnlockRequest::Password(_) | PrivateKeyUnlockRequest::Fido2(_) => Err(
            PrivateKeyError::other("This private key requires a hardware key."),
        ),
    }
}

#[cfg(feature = "fidokey")]
fn fido2_unlock_pin(request: PrivateKeyUnlockRequest) -> Result<Option<String>, PrivateKeyError> {
    match request {
        PrivateKeyUnlockRequest::Fido2(Some(pin)) => {
            let trimmed = pin.trim();
            if trimmed.is_empty() {
                return Err(PrivateKeyError::fido2_pin_required(
                    "Enter the FIDO2 security key PIN.",
                ));
            }
            Ok(Some(trimmed.to_string()))
        }
        PrivateKeyUnlockRequest::Fido2(None) => Ok(None),
        PrivateKeyUnlockRequest::Password(_)
        | PrivateKeyUnlockRequest::HardwarePin(_)
        | PrivateKeyUnlockRequest::HardwareExternal => Err(PrivateKeyError::other(
            "This private key requires a FIDO2 security key.",
        )),
    }
}

#[cfg(feature = "fidokey")]
fn unlock_fido2_private_key_for_session(
    fingerprint: &str,
    request: PrivateKeyUnlockRequest,
) -> Result<ManagedRipassoPrivateKey, PrivateKeyError> {
    let pin = fido2_unlock_pin(request)?;
    let entry = find_stored_private_key(fingerprint).map_err(|err| {
        if err == PRIVATE_KEY_NOT_STORED_ERROR {
            PrivateKeyError::not_stored(err)
        } else {
            PrivateKeyError::other(err)
        }
    })?;
    let key = entry.key.clone();
    let StoredPrivateKeyLocation::Fido2 { path } = entry.location else {
        return Err(PrivateKeyError::other(
            "This private key requires a different unlock method.",
        ));
    };
    let manifest = parse_fido2_private_key_manifest(
        &fs::read_to_string(&path).map_err(|err| PrivateKeyError::other(err.to_string()))?,
    )
    .map_err(PrivateKeyError::other)?
    .ok_or_else(|| PrivateKeyError::other("That FIDO2-protected key is invalid."))?;
    let unlocked_bytes = super::fido2::unlock_fido2_private_key_material_for_session(
        manifest.encrypted_private_key.as_bytes(),
        pin.as_deref(),
    )?;
    let unlocked = prepare_managed_private_key_bytes(&unlocked_bytes, None)?.0;
    cache_unlocked_ripasso_private_key(unlocked);
    Ok(key)
}

pub fn unlock_ripasso_private_key_for_session(
    fingerprint: &str,
    request: PrivateKeyUnlockRequest,
) -> Result<ManagedRipassoPrivateKey, PrivateKeyError> {
    let entry = find_stored_private_key(fingerprint).map_err(|err| {
        if err == PRIVATE_KEY_NOT_STORED_ERROR {
            PrivateKeyError::not_stored(err)
        } else {
            PrivateKeyError::other(err)
        }
    })?;

    match entry.location {
        StoredPrivateKeyLocation::Password { path } => {
            let passphrase = password_unlock_request(request)?;
            let cert = entry
                .cert
                .as_ref()
                .ok_or_else(|| PrivateKeyError::other(private_key_not_stored_error()))?;
            let unlocked = if cert_requires_passphrase(cert) {
                prepare_managed_private_key_bytes(
                    &fs::read(&path).map_err(|err| PrivateKeyError::other(err.to_string()))?,
                    Some(&passphrase),
                )?
                .0
            } else {
                cert.clone()
            };

            if !cert_can_decrypt_password_entries(&unlocked) {
                return Err(PrivateKeyError::incompatible(
                    "That private key cannot decrypt password store entries.",
                ));
            }

            cache_unlocked_ripasso_private_key(unlocked);
            Ok(entry.key)
        }
        StoredPrivateKeyLocation::Hardware { ref hardware, .. } => {
            if !stored_key_can_decrypt(&entry) {
                return Err(PrivateKeyError::incompatible(
                    "That hardware key cannot decrypt password store entries.",
                ));
            }
            let cert = entry
                .cert
                .clone()
                .ok_or_else(|| PrivateKeyError::other(private_key_not_stored_error()))?;

            let session =
                HardwareSessionPolicy::from_key(hardware, cert, hardware_unlock_mode(request)?);
            verify_hardware_session(&session).map_err(private_key_error_from_hardware_message)?;
            cache_unlocked_hardware_private_key(fingerprint, session)
                .map_err(PrivateKeyError::other)?;
            Ok(entry.key)
        }
        #[cfg(feature = "fidokey")]
        StoredPrivateKeyLocation::Fido2 { .. } => {
            unlock_fido2_private_key_for_session(&entry.key.fingerprint, request)
        }
    }
}

pub fn ripasso_private_key_requires_passphrase(bytes: &[u8]) -> Result<bool, PrivateKeyError> {
    #[cfg(feature = "fidokey")]
    if parse_fido2_private_key_manifest_bytes(bytes)
        .map_err(PrivateKeyError::other)?
        .is_some()
    {
        return Ok(false);
    }

    let (cert, _) = parse_managed_private_key_bytes(bytes)?;
    Ok(cert_requires_passphrase(&cert))
}

pub fn create_fido2_store_recipient(pin: Option<&str>) -> Result<String, PrivateKeyError> {
    super::fido2::create_fido2_store_recipient(pin)
}

pub fn unlock_fido2_store_recipient_for_session(
    recipient: &str,
    pin: Option<&str>,
) -> Result<(), PrivateKeyError> {
    super::fido2::unlock_fido2_store_recipient_for_session(recipient, pin)
}

#[cfg(feature = "fidokey")]
fn fido2_private_key_manifest_contents(
    manifest: &Fido2PrivateKeyManifest,
) -> Result<String, PrivateKeyError> {
    toml::to_string_pretty(manifest).map_err(|err| PrivateKeyError::other(err.to_string()))
}

#[cfg(feature = "fidokey")]
fn store_fido2_private_key_manifest(
    manifest: Fido2PrivateKeyManifest,
) -> Result<ManagedRipassoPrivateKey, PrivateKeyError> {
    let keys_dir = ripasso_fido_keys_dir().map_err(PrivateKeyError::other)?;
    fs::create_dir_all(&keys_dir).map_err(|err| PrivateKeyError::other(err.to_string()))?;
    if manifest.format != FIDO2_PRIVATE_KEY_MANIFEST_FORMAT {
        return Err(PrivateKeyError::other(format!(
            "Unsupported FIDO2-protected key format {}.",
            manifest.format
        )));
    }
    if manifest.protection != FIDO2_PRIVATE_KEY_PROTECTION_KIND {
        return Err(PrivateKeyError::other(format!(
            "Unsupported FIDO2-protected key protection '{}'.",
            manifest.protection
        )));
    }
    let (cert, key) = parse_fido2_public_key_bytes(manifest.public_key.as_bytes())?;
    let expected = normalized_fingerprint(&manifest.fingerprint).map_err(PrivateKeyError::other)?;
    if !key.fingerprint.eq_ignore_ascii_case(&expected) {
        return Err(PrivateKeyError::other(
            "That FIDO2-protected key is invalid.",
        ));
    }
    if !cert_has_transport_encryption_key(&cert) {
        return Err(PrivateKeyError::incompatible(
            "That private key cannot decrypt password store entries.",
        ));
    }

    fs::write(
        keys_dir.join(key.fingerprint.to_ascii_lowercase()),
        fido2_private_key_manifest_contents(&manifest)?,
    )
    .map_err(|err| PrivateKeyError::other(err.to_string()))?;

    Ok(key)
}

#[cfg(feature = "fidokey")]
fn store_fido2_private_key_cert(
    cert: Cert,
    binding_recipient: &str,
) -> Result<ManagedRipassoPrivateKey, PrivateKeyError> {
    let keys_dir = ripasso_fido_keys_dir().map_err(PrivateKeyError::other)?;
    fs::create_dir_all(&keys_dir).map_err(|err| PrivateKeyError::other(err.to_string()))?;

    if !cert_can_decrypt_password_entries(&cert) {
        return Err(PrivateKeyError::incompatible(
            "That private key cannot decrypt password store entries.",
        ));
    }

    let binding = super::fido2::direct_binding_from_store_recipient(binding_recipient)
        .map_err(PrivateKeyError::other)?
        .ok_or_else(|| PrivateKeyError::other("That FIDO2 security key is invalid."))?;
    let key = managed_fido2_private_key_from_cert(&cert);
    let mut private_key_bytes = Vec::new();
    cert.as_tsk()
        .serialize(&mut private_key_bytes)
        .map_err(|err| PrivateKeyError::other(err.to_string()))?;
    let public_key = String::from_utf8(
        cert.clone()
            .strip_secret_key_material()
            .armored()
            .to_vec()
            .map_err(|err| PrivateKeyError::other(err.to_string()))?,
    )
    .map_err(|err| PrivateKeyError::other(err.to_string()))?;
    let encrypted_private_key = String::from_utf8(
        super::fido2::encrypt_fido2_direct_required_layer(&binding, &private_key_bytes)
            .map_err(PrivateKeyError::other)?,
    )
    .map_err(|err| PrivateKeyError::other(err.to_string()))?;
    let manifest = Fido2PrivateKeyManifest {
        format: FIDO2_PRIVATE_KEY_MANIFEST_FORMAT,
        protection: FIDO2_PRIVATE_KEY_PROTECTION_KIND.to_string(),
        fingerprint: key.fingerprint.clone(),
        public_key,
        encrypted_private_key,
    };
    fs::write(
        keys_dir.join(key.fingerprint.to_ascii_lowercase()),
        fido2_private_key_manifest_contents(&manifest)?,
    )
    .map_err(|err| PrivateKeyError::other(err.to_string()))?;
    cache_unlocked_ripasso_private_key(cert);
    Ok(key)
}

pub fn list_ripasso_private_keys() -> Result<Vec<ManagedRipassoPrivateKey>, String> {
    let mut keys: Vec<ManagedRipassoPrivateKey> = Vec::new();

    for path in stored_private_key_file_paths(&ripasso_keys_dir()?)? {
        match read_password_private_key_entry(&path) {
            Ok(entry) => {
                if !keys
                    .iter()
                    .any(|existing| existing.fingerprint == entry.key.fingerprint)
                {
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

    for dir in stored_hardware_private_key_dirs(&ripasso_keys_v2_dir()?)? {
        match read_hardware_private_key_entry(&dir) {
            Ok(entry) => {
                if !keys
                    .iter()
                    .any(|existing| existing.fingerprint == entry.key.fingerprint)
                {
                    keys.push(entry.key);
                }
            }
            Err(err) => {
                log_error(format!(
                    "Failed to load managed hardware key '{}': {err}",
                    dir.display()
                ));
            }
        }
    }

    #[cfg(feature = "fidokey")]
    for path in stored_private_key_file_paths(&ripasso_fido_keys_dir()?)? {
        match read_fido2_private_key_entry(&path) {
            Ok(entry) => {
                if !keys
                    .iter()
                    .any(|existing| existing.fingerprint == entry.key.fingerprint)
                {
                    keys.push(entry.key);
                }
            }
            Err(err) => {
                log_error(format!(
                    "Failed to load managed FIDO2 key '{}': {err}",
                    path.display()
                ));
            }
        }
    }

    keys.sort_by(|left, right| {
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
) -> Result<ManagedRipassoPrivateKey, PrivateKeyError> {
    let key = store_ripasso_private_key_bytes(bytes)?;
    #[cfg(feature = "fidokey")]
    let should_cache_unlocked =
        key.protection != ManagedRipassoPrivateKeyProtection::Fido2HmacSecret;
    #[cfg(not(feature = "fidokey"))]
    let should_cache_unlocked = true;

    if should_cache_unlocked {
        let (unlocked_cert, _) = prepare_managed_private_key_bytes(bytes, passphrase)?;
        cache_unlocked_ripasso_private_key(unlocked_cert);
    }

    Ok(key)
}

pub fn store_ripasso_private_key_bytes(
    bytes: &[u8],
) -> Result<ManagedRipassoPrivateKey, PrivateKeyError> {
    #[cfg(feature = "fidokey")]
    if let Some(manifest) =
        parse_fido2_private_key_manifest_bytes(bytes).map_err(PrivateKeyError::other)?
    {
        return store_fido2_private_key_manifest(manifest);
    }

    let keys_dir = ripasso_keys_dir().map_err(PrivateKeyError::other)?;
    fs::create_dir_all(&keys_dir).map_err(|err| PrivateKeyError::other(err.to_string()))?;

    let (parsed_cert, key) = parse_managed_private_key_bytes(bytes)?;
    let stored_cert = if cert_requires_passphrase(&parsed_cert) {
        parsed_cert
    } else {
        return Err(PrivateKeyError::requires_password_protection(
            "That private key must be password protected before you can import it.",
        ));
    };
    let mut file = File::create(keys_dir.join(key.fingerprint.to_ascii_lowercase()))
        .map_err(|err| PrivateKeyError::other(err.to_string()))?;
    stored_cert
        .as_tsk()
        .serialize(&mut file)
        .map_err(|err| PrivateKeyError::other(err.to_string()))?;

    Ok(key)
}

fn validate_hardware_key_material(
    cert: &Cert,
    hardware: &ManagedRipassoHardwareKey,
) -> Result<(), PrivateKeyError> {
    if !cert_has_transport_encryption_key(cert) {
        return Err(PrivateKeyError::incompatible(
            "That hardware key cannot decrypt password store entries.",
        ));
    }

    if let Some(expected) = hardware.decryption_fingerprint.as_ref() {
        let expected = normalized_fingerprint(expected).map_err(PrivateKeyError::other)?;
        if !cert.keys().any(|key| {
            key.key()
                .fingerprint()
                .to_hex()
                .eq_ignore_ascii_case(&expected)
        }) {
            return Err(PrivateKeyError::hardware_token_mismatch(
                "That public key does not match the hardware decryption key.",
            ));
        }
    }

    if let Some(expected) = hardware.signing_fingerprint.as_ref() {
        let expected = normalized_fingerprint(expected).map_err(PrivateKeyError::other)?;
        if !cert.keys().any(|key| {
            key.key()
                .fingerprint()
                .to_hex()
                .eq_ignore_ascii_case(&expected)
        }) {
            return Err(PrivateKeyError::hardware_token_mismatch(
                "That public key does not match the hardware signing key.",
            ));
        }
    }

    let discovered = list_hardware_tokens().map_err(private_key_error_from_hardware_message)?;
    let Some(found) = discovered
        .iter()
        .find(|token| token.ident == hardware.ident)
    else {
        return Err(PrivateKeyError::hardware_token_not_present(
            "Connect the matching hardware key before importing it.",
        ));
    };
    if hardware
        .signing_fingerprint
        .as_ref()
        .is_some_and(|expected| found.signing_fingerprint.as_ref() != Some(expected))
    {
        return Err(PrivateKeyError::hardware_token_mismatch(
            "The connected hardware key does not match the stored signing key.",
        ));
    }
    if hardware
        .decryption_fingerprint
        .as_ref()
        .is_some_and(|expected| found.decryption_fingerprint.as_ref() != Some(expected))
    {
        return Err(PrivateKeyError::hardware_token_mismatch(
            "The connected hardware key does not match the stored decryption key.",
        ));
    }

    Ok(())
}

pub fn store_ripasso_hardware_key_bytes(
    bytes: &[u8],
    hardware: ManagedRipassoHardwareKey,
) -> Result<ManagedRipassoPrivateKey, PrivateKeyError> {
    let keys_dir = ripasso_keys_v2_dir().map_err(PrivateKeyError::other)?;
    fs::create_dir_all(&keys_dir).map_err(|err| PrivateKeyError::other(err.to_string()))?;

    let (cert, key) = parse_hardware_public_key_bytes(bytes, hardware.clone())?;
    validate_hardware_key_material(&cert, &hardware)?;
    let dir = keys_dir.join(key.fingerprint.to_ascii_lowercase());
    fs::create_dir_all(&dir).map_err(|err| PrivateKeyError::other(err.to_string()))?;
    fs::write(
        hardware_manifest_path(&dir),
        toml::to_string_pretty(&HardwarePrivateKeyManifest::from_key(&key, &hardware))
            .map_err(|err| PrivateKeyError::other(err.to_string()))?,
    )
    .map_err(|err| PrivateKeyError::other(err.to_string()))?;
    let armored = cert
        .armored()
        .to_vec()
        .map_err(|err| PrivateKeyError::other(err.to_string()))?;
    fs::write(hardware_public_key_path(&dir), armored)
        .map_err(|err| PrivateKeyError::other(err.to_string()))?;

    Ok(key)
}

pub fn import_ripasso_hardware_key_bytes(
    bytes: &[u8],
    hardware: ManagedRipassoHardwareKey,
) -> Result<ManagedRipassoPrivateKey, PrivateKeyError> {
    store_ripasso_hardware_key_bytes(bytes, hardware)
}

pub fn discover_ripasso_hardware_keys(
) -> Result<Vec<super::hardware::DiscoveredHardwareToken>, String> {
    list_hardware_tokens()
}

#[cfg(feature = "fidokey")]
pub fn generate_fido2_private_key(
    pin: Option<&str>,
) -> Result<ManagedRipassoPrivateKey, PrivateKeyError> {
    let recipient = super::fido2::create_fido2_private_key_binding(pin)?;
    let parsed = parse_fido2_recipient_string(&recipient)
        .map_err(PrivateKeyError::other)?
        .ok_or_else(|| PrivateKeyError::other("That FIDO2 security key is invalid."))?;
    let short_id = &parsed.id[parsed.id.len().saturating_sub(6)..];
    let user_id = format!("{} ({short_id})", parsed.label);
    let (cert, _) = CertBuilder::general_purpose(Some(user_id.as_str()))
        .generate()
        .map_err(|err| PrivateKeyError::other(err.to_string()))?;
    store_fido2_private_key_cert(cert, &recipient)
}

#[cfg(not(feature = "fidokey"))]
pub fn generate_fido2_private_key(
    _pin: Option<&str>,
) -> Result<ManagedRipassoPrivateKey, PrivateKeyError> {
    Err(PrivateKeyError::unsupported_fido2_key(
        FIDO2_PRIVATE_KEY_FEATURE_DISABLED_ERROR,
    ))
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

pub fn armored_ripasso_public_key(fingerprint: &str) -> Result<String, String> {
    let entry = find_stored_private_key(fingerprint)?;
    let cert = entry
        .cert
        .as_ref()
        .ok_or_else(|| "That key does not have an exportable public key.".to_string())?;
    let armored = cert.armored().to_vec().map_err(|err| err.to_string())?;
    String::from_utf8(armored).map_err(|err| err.to_string())
}

pub fn armored_ripasso_private_key(fingerprint: &str) -> Result<String, String> {
    let entry = find_stored_private_key(fingerprint)?;
    let armored = match entry.location {
        #[cfg(feature = "fidokey")]
        StoredPrivateKeyLocation::Fido2 { ref path } => {
            return fs::read_to_string(path).map_err(|err| err.to_string());
        }
        _ => match entry.key.protection {
            ManagedRipassoPrivateKeyProtection::Password => entry
                .cert
                .as_ref()
                .ok_or_else(private_key_not_stored_error)?
                .as_tsk()
                .armored()
                .to_vec()
                .map_err(|err| err.to_string())?,
            ManagedRipassoPrivateKeyProtection::HardwareOpenPgpCard => {
                return Err(
                    "That hardware-backed key does not have an exportable private key.".to_string(),
                );
            }
            #[cfg(feature = "fidokey")]
            ManagedRipassoPrivateKeyProtection::Fido2HmacSecret => {
                return Err("That FIDO2-protected key could not be exported.".to_string());
            }
        },
    };
    String::from_utf8(armored).map_err(|err| err.to_string())
}

pub fn remove_ripasso_private_key(fingerprint: &str) -> Result<(), String> {
    let entry = find_stored_private_key(fingerprint)?;
    match entry.location {
        StoredPrivateKeyLocation::Password { path } => {
            fs::remove_file(path).map_err(|err| err.to_string())?;
        }
        StoredPrivateKeyLocation::Hardware { dir, .. } => {
            fs::remove_dir_all(dir).map_err(|err| err.to_string())?;
        }
        #[cfg(feature = "fidokey")]
        StoredPrivateKeyLocation::Fido2 { path } => {
            fs::remove_file(path).map_err(|err| err.to_string())?;
            let _ = clear_cached_fido2_pin(&entry.key.fingerprint);
        }
    }
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
