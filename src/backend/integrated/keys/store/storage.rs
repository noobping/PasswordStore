#[cfg(feature = "fidokey")]
use super::super::cache::clear_cached_fido2_pin;
use super::super::cache::{
    borrow_unlocked_ripasso_private_key, cache_unlocked_ripasso_private_key,
    remove_cached_unlocked_ripasso_private_key,
};
#[cfg(feature = "fidokey")]
use super::super::cert::cert_can_decrypt_password_entries;
use super::super::cert::{
    cert_has_transport_encryption_key, cert_requires_passphrase, connected_smartcard_key_from_cert,
    fingerprint_from_string, normalized_fingerprint, parse_hardware_public_key_bytes,
    parse_managed_private_key_bytes, prepare_managed_private_key_bytes, ConnectedSmartcardKey,
    ManagedRipassoHardwareKey, ManagedRipassoPrivateKey, ManagedRipassoPrivateKeyProtection,
};
use super::super::hardware::list_hardware_tokens;
#[cfg(feature = "hardwarekey")]
use super::super::hardware::{
    generate_hardware_key_material, private_key_error_from_hardware_transport_error,
    HardwareKeyGenerationRequest,
};
#[cfg(feature = "hardwarekey")]
use super::manifest::HardwarePrivateKeyManifest;
#[cfg(feature = "fidokey")]
use super::manifest::{
    fido2_private_key_manifest_contents, managed_fido2_private_key_from_cert,
    parse_fido2_private_key_manifest, parse_fido2_private_key_manifest_bytes,
    read_fido2_private_key_manifest_entry, validate_fido2_private_key_manifest,
    Fido2PrivateKeyManifest,
};
use super::manifest::{
    read_hardware_private_key_manifest, read_hardware_private_key_manifest_entry,
};
#[cfg(test)]
use super::missing_private_key_error;
#[cfg(feature = "fidokey")]
use super::paths::ripasso_fido_keys_dir;
#[cfg(feature = "hardwarekey")]
use super::paths::{hardware_manifest_path, hardware_public_key_path};
use super::paths::{ripasso_keys_dir, ripasso_keys_v2_dir};
use super::private_key_not_stored_error;
#[cfg(not(feature = "fidokey"))]
use super::FIDO2_PRIVATE_KEY_FEATURE_DISABLED_ERROR;
#[cfg(not(feature = "hardwarekey"))]
const HARDWAREKEY_FEATURE_DISABLED_ERROR: &str =
    "Managed hardware-key add/import/setup is disabled in this build of Keycord.";
use crate::backend::PrivateKeyError;
#[cfg(feature = "fidokey")]
use crate::fido2_recipient::parse_fido2_recipient_string;
use crate::logging::log_error;
use crate::preferences::Preferences;
use crate::support::runtime::{has_smartcard_permission, supports_legacy_compat_features};
use crate::support::secure_fs::{ensure_private_dir, write_private_file};
use ripasso::crypto::{slice_to_20_bytes, Sequoia};
#[cfg(feature = "hardwarekey")]
use secrecy::ExposeSecret;
use secrecy::SecretString;
use sequoia_openpgp::{
    cert::CertBuilder,
    crypto::Password,
    serialize::{Serialize, SerializeInto},
    Cert,
};
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;

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

#[derive(Clone, Debug)]
pub(in crate::backend::integrated) struct ConnectedSmartcardEntry {
    pub(in crate::backend::integrated) cert: Cert,
    pub(in crate::backend::integrated) key: ConnectedSmartcardKey,
}

fn stored_private_key_location_path(location: &StoredPrivateKeyLocation) -> &Path {
    match location {
        StoredPrivateKeyLocation::Password { path } => path,
        StoredPrivateKeyLocation::Hardware { dir, .. } => dir,
        #[cfg(feature = "fidokey")]
        StoredPrivateKeyLocation::Fido2 { path } => path,
    }
}

fn validate_direct_stored_private_key(
    requested: &str,
    entry: StoredPrivateKeyEntry,
) -> Result<StoredPrivateKeyEntry, String> {
    if entry.key.fingerprint.eq_ignore_ascii_case(requested) {
        return Ok(entry);
    }

    Err(format!(
        "Managed private-key data '{}' does not match requested fingerprint '{}'.",
        stored_private_key_location_path(&entry.location).display(),
        requested
    ))
}

pub(super) fn read_password_private_key_entry(
    path: &Path,
) -> Result<StoredPrivateKeyEntry, String> {
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

pub(super) fn read_hardware_private_key_entry(dir: &Path) -> Result<StoredPrivateKeyEntry, String> {
    let manifest = read_hardware_private_key_manifest(dir)?;
    read_hardware_private_key_manifest_entry(dir, manifest)
}

fn stored_private_key_file_paths(keys_dir: &Path) -> Result<Vec<PathBuf>, String> {
    if !keys_dir.exists() {
        return Ok(Vec::new());
    }

    let mut paths = Vec::new();
    for entry in fs::read_dir(keys_dir).map_err(|err| err.to_string())? {
        let entry = entry.map_err(|err| err.to_string())?;
        let path = entry.path();
        if path.is_file() && include_managed_key_scan_path(&path, "file") {
            paths.push(path);
        }
    }
    Ok(paths)
}

#[cfg(feature = "fidokey")]
pub(super) fn read_fido2_private_key_entry(path: &Path) -> Result<StoredPrivateKeyEntry, String> {
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
        if path.is_dir() && include_managed_key_scan_path(&path, "folder") {
            dirs.push(path);
        }
    }
    Ok(dirs)
}

fn canonical_managed_key_path_name(path: &Path) -> Option<String> {
    path.file_name()
        .and_then(|value| value.to_str())
        .and_then(|value| {
            normalized_fingerprint(value)
                .ok()
                .map(|fingerprint| fingerprint.to_ascii_lowercase())
        })
}

fn include_managed_key_scan_path(path: &Path, artifact_kind: &str) -> bool {
    if supports_legacy_compat_features()
        || path
            .file_name()
            .and_then(|value| value.to_str())
            .zip(canonical_managed_key_path_name(path))
            .is_some_and(|(name, canonical)| name == canonical)
    {
        return true;
    }

    log_error(format!(
        "Ignoring non-canonical managed private-key {artifact_kind} '{}'.",
        path.display()
    ));
    false
}

fn managed_key_path_matches_fingerprint(path: &Path, fingerprint: &str) -> bool {
    path.file_name()
        .and_then(|value| value.to_str())
        .is_some_and(|name| name == fingerprint.to_ascii_lowercase())
}

fn validate_scanned_managed_key_path<T>(
    path: &Path,
    artifact_kind: &str,
    fingerprint: &str,
    entry: T,
    strict: bool,
) -> Result<Option<T>, String> {
    if managed_key_path_matches_fingerprint(path, fingerprint) {
        return Ok(Some(entry));
    }

    let expected = path
        .parent()
        .unwrap_or_else(|| Path::new("."))
        .join(fingerprint.to_ascii_lowercase());
    let message = format!(
        "Managed private-key {artifact_kind} '{}' does not match canonical path '{}'.",
        path.display(),
        expected.display()
    );
    if strict {
        Err(message)
    } else {
        log_error(message);
        Ok(None)
    }
}

fn scan_managed_key_entry<T>(
    path: &Path,
    artifact_kind: &str,
    load: impl FnOnce() -> Result<T, String>,
) -> Result<Option<T>, String> {
    match load() {
        Ok(entry) => Ok(Some(entry)),
        Err(err) if supports_legacy_compat_features() => Err(err),
        Err(err) => {
            log_error(format!(
                "Ignoring invalid managed private-key {artifact_kind} '{}': {err}",
                path.display()
            ));
            Ok(None)
        }
    }
}

pub(in crate::backend::integrated) fn find_stored_private_key(
    fingerprint: &str,
) -> Result<StoredPrivateKeyEntry, String> {
    let requested = normalized_fingerprint(fingerprint)?;

    let legacy_dir = ripasso_keys_dir()?;
    let direct_legacy_path = legacy_dir.join(requested.to_ascii_lowercase());
    if direct_legacy_path.exists() {
        return validate_direct_stored_private_key(
            &requested,
            read_password_private_key_entry(&direct_legacy_path)?,
        );
    }

    let hardware_dir = ripasso_keys_v2_dir()?;
    let direct_hardware_dir = hardware_dir.join(requested.to_ascii_lowercase());
    if direct_hardware_dir.exists() {
        return validate_direct_stored_private_key(
            &requested,
            read_hardware_private_key_entry(&direct_hardware_dir)?,
        );
    }

    #[cfg(feature = "fidokey")]
    {
        let fido2_dir = ripasso_fido_keys_dir()?;
        let direct_fido2_path = fido2_dir.join(requested.to_ascii_lowercase());
        if direct_fido2_path.exists() {
            return validate_direct_stored_private_key(
                &requested,
                read_fido2_private_key_entry(&direct_fido2_path)?,
            );
        }
    }

    Err(private_key_not_stored_error())
}

fn connected_smartcard_hardware(
    token: &super::super::hardware::DiscoveredHardwareToken,
) -> ManagedRipassoHardwareKey {
    ManagedRipassoHardwareKey {
        ident: token.ident.clone(),
        signing_fingerprint: token.signing_fingerprint.clone(),
        decryption_fingerprint: token.decryption_fingerprint.clone(),
        reader_hint: token.reader_hint.clone(),
    }
}

fn connected_smartcard_entry_from_token(
    token: super::super::hardware::DiscoveredHardwareToken,
) -> Result<Option<ConnectedSmartcardEntry>, String> {
    let hardware = connected_smartcard_hardware(&token);
    let Some(bytes) = token.cardholder_certificate.as_ref() else {
        return Ok(None);
    };
    let (cert, _) =
        parse_hardware_public_key_bytes(bytes, hardware.clone()).map_err(|err| err.to_string())?;
    if !cert_has_transport_encryption_key(&cert) {
        return Ok(None);
    }

    if let Some(expected) = hardware.decryption_fingerprint.as_ref() {
        let expected = normalized_fingerprint(expected)?;
        if !cert.keys().any(|key| {
            key.key()
                .fingerprint()
                .to_hex()
                .eq_ignore_ascii_case(&expected)
        }) {
            return Ok(None);
        }
    }

    if let Some(expected) = hardware.signing_fingerprint.as_ref() {
        let expected = normalized_fingerprint(expected)?;
        if !cert.keys().any(|key| {
            key.key()
                .fingerprint()
                .to_hex()
                .eq_ignore_ascii_case(&expected)
        }) {
            return Ok(None);
        }
    }

    Ok(Some(ConnectedSmartcardEntry {
        key: connected_smartcard_key_from_cert(&cert, hardware),
        cert,
    }))
}

fn connected_smartcard_entries() -> Result<Vec<ConnectedSmartcardEntry>, String> {
    if !has_smartcard_permission() {
        return Ok(Vec::new());
    }

    let mut entries = Vec::new();
    for token in list_hardware_tokens().map_err(|err| err.to_string())? {
        let ident = token.ident.clone();
        match connected_smartcard_entry_from_token(token) {
            Ok(Some(entry)) => {
                if !entries.iter().any(|existing: &ConnectedSmartcardEntry| {
                    existing
                        .key
                        .fingerprint
                        .eq_ignore_ascii_case(&entry.key.fingerprint)
                }) {
                    entries.push(entry);
                }
            }
            Ok(None) => {}
            Err(err) => {
                log_error(format!(
                    "Failed to inspect connected smartcard '{ident}': {err}"
                ));
            }
        }
    }

    entries.sort_by(|left, right| {
        left.key
            .title()
            .to_ascii_lowercase()
            .cmp(&right.key.title().to_ascii_lowercase())
            .then_with(|| left.key.fingerprint.cmp(&right.key.fingerprint))
    });
    Ok(entries)
}

fn connected_smartcard_entries_for_background_load(context: &str) -> Vec<ConnectedSmartcardEntry> {
    match connected_smartcard_entries() {
        Ok(entries) => entries,
        Err(err) => {
            log_error(format!(
                "Failed to inspect connected smartcards while {context}: {err}"
            ));
            Vec::new()
        }
    }
}

pub fn list_connected_smartcard_keys() -> Result<Vec<ConnectedSmartcardKey>, String> {
    Ok(connected_smartcard_entries()?
        .into_iter()
        .map(|entry| entry.key)
        .collect())
}

pub(in crate::backend::integrated) fn find_connected_smartcard_key(
    fingerprint: &str,
) -> Result<Option<ConnectedSmartcardEntry>, String> {
    let requested = normalized_fingerprint(fingerprint)?;
    Ok(connected_smartcard_entries()?
        .into_iter()
        .find(|entry| entry.key.fingerprint.eq_ignore_ascii_case(&requested)))
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
        let Some(entry) =
            scan_managed_key_entry(&path, "file", || read_password_private_key_entry(&path))?
        else {
            continue;
        };
        let fingerprint = entry.key.fingerprint.clone();
        let Some(entry) = validate_scanned_managed_key_path(
            &path,
            "file",
            &fingerprint,
            entry,
            supports_legacy_compat_features(),
        )?
        else {
            continue;
        };
        let cert = entry
            .cert
            .as_ref()
            .ok_or_else(|| "Missing OpenPGP certificate for stored private key.".to_string())?;
        let fingerprint =
            slice_to_20_bytes(cert.fingerprint().as_bytes()).map_err(|err| err.to_string())?;
        key_ring.insert(fingerprint, Arc::new(cert.clone()));
    }

    for dir in stored_hardware_private_key_dirs(&ripasso_keys_v2_dir()?)? {
        let Some(entry) =
            scan_managed_key_entry(&dir, "folder", || read_hardware_private_key_entry(&dir))?
        else {
            continue;
        };
        let fingerprint = entry.key.fingerprint.clone();
        let Some(entry) = validate_scanned_managed_key_path(
            &dir,
            "folder",
            &fingerprint,
            entry,
            supports_legacy_compat_features(),
        )?
        else {
            continue;
        };
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
        let Some(entry) =
            scan_managed_key_entry(&path, "file", || read_fido2_private_key_entry(&path))?
        else {
            continue;
        };
        let fingerprint = entry.key.fingerprint.clone();
        let Some(entry) = validate_scanned_managed_key_path(
            &path,
            "file",
            &fingerprint,
            entry,
            supports_legacy_compat_features(),
        )?
        else {
            continue;
        };
        let Some(cert) = entry.cert.as_ref() else {
            continue;
        };
        let fingerprint =
            slice_to_20_bytes(cert.fingerprint().as_bytes()).map_err(|err| err.to_string())?;
        key_ring.insert(fingerprint, Arc::new(cert.clone()));
    }

    Ok(key_ring)
}

pub(in crate::backend::integrated) fn load_available_standard_key_ring(
) -> Result<HashMap<[u8; 20], Arc<Cert>>, String> {
    let mut key_ring = load_stored_ripasso_key_ring()?;

    for entry in connected_smartcard_entries_for_background_load("loading the available key ring") {
        let fingerprint = slice_to_20_bytes(entry.cert.fingerprint().as_bytes())
            .map_err(|err| err.to_string())?;
        key_ring
            .entry(fingerprint)
            .or_insert_with(|| Arc::new(entry.cert));
    }

    Ok(key_ring)
}

pub(in crate::backend::integrated) fn load_ripasso_key_ring(
    fingerprint: &str,
) -> Result<HashMap<[u8; 20], Arc<Cert>>, String> {
    let user_key_id = fingerprint_from_string(fingerprint)?;
    let mut key_ring = load_available_standard_key_ring()?;

    if let Some(cert) = borrow_unlocked_ripasso_private_key(fingerprint)? {
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

pub(in crate::backend::integrated) fn available_private_key_fingerprints(
) -> Result<Vec<String>, String> {
    let mut fingerprints = imported_private_key_fingerprints()?;

    for key in connected_smartcard_entries_for_background_load(
        "collecting available private-key fingerprints",
    )
    .into_iter()
    .map(|entry| entry.key)
    {
        if !fingerprints
            .iter()
            .any(|existing| existing.eq_ignore_ascii_case(&key.fingerprint))
        {
            fingerprints.push(key.fingerprint);
        }
    }

    Ok(fingerprints)
}

#[cfg(feature = "audit")]
pub(in crate::backend) fn available_standard_public_certs() -> Result<Vec<Cert>, String> {
    Ok(load_available_standard_key_ring()?
        .into_values()
        .map(|cert| (*cert).clone())
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

#[cfg(feature = "fidokey")]
fn store_fido2_private_key_manifest(
    manifest: Fido2PrivateKeyManifest,
) -> Result<ManagedRipassoPrivateKey, PrivateKeyError> {
    let keys_dir = ripasso_fido_keys_dir().map_err(PrivateKeyError::other)?;
    ensure_private_dir(&keys_dir).map_err(|err| PrivateKeyError::other(err.to_string()))?;
    let (cert, key) =
        validate_fido2_private_key_manifest(&manifest).map_err(PrivateKeyError::other)?;
    if !cert_has_transport_encryption_key(&cert) {
        return Err(PrivateKeyError::incompatible(
            "That private key cannot decrypt password store entries.",
        ));
    }

    let manifest_path = keys_dir.join(key.fingerprint.to_ascii_lowercase());
    let manifest_contents = fido2_private_key_manifest_contents(&manifest)?;
    write_private_file(&manifest_path, manifest_contents.as_bytes())
        .map_err(|err| PrivateKeyError::other(err.to_string()))?;

    Ok(key)
}

#[cfg(feature = "fidokey")]
fn store_fido2_private_key_cert(
    cert: Cert,
    binding_recipient: &str,
) -> Result<ManagedRipassoPrivateKey, PrivateKeyError> {
    let keys_dir = ripasso_fido_keys_dir().map_err(PrivateKeyError::other)?;
    ensure_private_dir(&keys_dir).map_err(|err| PrivateKeyError::other(err.to_string()))?;

    if !cert_can_decrypt_password_entries(&cert) {
        return Err(PrivateKeyError::incompatible(
            "That private key cannot decrypt password store entries.",
        ));
    }

    let binding = super::super::fido2::direct_binding_from_store_recipient(binding_recipient)
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
        super::super::fido2::encrypt_fido2_direct_required_layer(&binding, &private_key_bytes)
            .map_err(PrivateKeyError::other)?,
    )
    .map_err(|err| PrivateKeyError::other(err.to_string()))?;
    let manifest = Fido2PrivateKeyManifest {
        format: 1,
        protection: "fido2-hmac-secret".to_string(),
        fingerprint: key.fingerprint.clone(),
        public_key,
        encrypted_private_key,
    };
    let manifest_path = keys_dir.join(key.fingerprint.to_ascii_lowercase());
    let manifest_contents = fido2_private_key_manifest_contents(&manifest)?;
    write_private_file(&manifest_path, manifest_contents.as_bytes())
        .map_err(|err| PrivateKeyError::other(err.to_string()))?;
    cache_unlocked_ripasso_private_key(cert);
    Ok(key)
}

pub fn list_ripasso_private_keys() -> Result<Vec<ManagedRipassoPrivateKey>, String> {
    let mut keys: Vec<ManagedRipassoPrivateKey> = Vec::new();

    for path in stored_private_key_file_paths(&ripasso_keys_dir()?)? {
        match read_password_private_key_entry(&path) {
            Ok(entry) => {
                let fingerprint = entry.key.fingerprint.clone();
                let Some(entry) =
                    validate_scanned_managed_key_path(&path, "file", &fingerprint, entry, false)?
                else {
                    continue;
                };
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
                let fingerprint = entry.key.fingerprint.clone();
                let Some(entry) =
                    validate_scanned_managed_key_path(&dir, "folder", &fingerprint, entry, false)?
                else {
                    continue;
                };
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
                let fingerprint = entry.key.fingerprint.clone();
                let Some(entry) =
                    validate_scanned_managed_key_path(&path, "file", &fingerprint, entry, false)?
                else {
                    continue;
                };
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
    ensure_private_dir(&keys_dir).map_err(|err| PrivateKeyError::other(err.to_string()))?;

    let (parsed_cert, key) = parse_managed_private_key_bytes(bytes)?;
    let stored_cert = if cert_requires_passphrase(&parsed_cert) {
        parsed_cert
    } else {
        return Err(PrivateKeyError::requires_password_protection(
            "That private key must be password protected before you can import it.",
        ));
    };
    let mut serialized = Vec::new();
    stored_cert
        .as_tsk()
        .serialize(&mut serialized)
        .map_err(|err| PrivateKeyError::other(err.to_string()))?;
    write_private_file(
        &keys_dir.join(key.fingerprint.to_ascii_lowercase()),
        &serialized,
    )
    .map_err(|err| PrivateKeyError::other(err.to_string()))?;

    Ok(key)
}

#[cfg(feature = "hardwarekey")]
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

    let discovered =
        list_hardware_tokens().map_err(private_key_error_from_hardware_transport_error)?;
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

#[cfg(feature = "hardwarekey")]
pub fn store_ripasso_hardware_key_bytes(
    bytes: &[u8],
    hardware: ManagedRipassoHardwareKey,
) -> Result<ManagedRipassoPrivateKey, PrivateKeyError> {
    let keys_dir = ripasso_keys_v2_dir().map_err(PrivateKeyError::other)?;
    ensure_private_dir(&keys_dir).map_err(|err| PrivateKeyError::other(err.to_string()))?;

    let (cert, key) = parse_hardware_public_key_bytes(bytes, hardware.clone())?;
    validate_hardware_key_material(&cert, &hardware)?;
    let dir = keys_dir.join(key.fingerprint.to_ascii_lowercase());
    ensure_private_dir(&dir).map_err(|err| PrivateKeyError::other(err.to_string()))?;
    let manifest = toml::to_string_pretty(&HardwarePrivateKeyManifest::from_key(&key, &hardware))
        .map_err(|err| PrivateKeyError::other(err.to_string()))?;
    let manifest_path = hardware_manifest_path(&dir);
    write_private_file(&manifest_path, manifest.as_bytes())
        .map_err(|err| PrivateKeyError::other(err.to_string()))?;
    let armored = cert
        .armored()
        .to_vec()
        .map_err(|err| PrivateKeyError::other(err.to_string()))?;
    let public_key_path = hardware_public_key_path(&dir);
    write_private_file(&public_key_path, armored)
        .map_err(|err| PrivateKeyError::other(err.to_string()))?;

    Ok(key)
}

#[cfg(feature = "hardwarekey")]
pub fn import_ripasso_hardware_key_bytes(
    bytes: &[u8],
    hardware: ManagedRipassoHardwareKey,
) -> Result<ManagedRipassoPrivateKey, PrivateKeyError> {
    store_ripasso_hardware_key_bytes(bytes, hardware)
}

#[cfg(not(feature = "hardwarekey"))]
pub fn import_ripasso_hardware_key_bytes(
    _bytes: &[u8],
    _hardware: ManagedRipassoHardwareKey,
) -> Result<ManagedRipassoPrivateKey, PrivateKeyError> {
    Err(PrivateKeyError::unsupported_hardware_key(
        HARDWAREKEY_FEATURE_DISABLED_ERROR,
    ))
}

#[cfg(feature = "hardwarekey")]
pub fn discover_ripasso_hardware_keys(
) -> Result<Vec<super::super::hardware::DiscoveredHardwareToken>, String> {
    list_hardware_tokens().map_err(|err| err.to_string())
}

#[cfg(not(feature = "hardwarekey"))]
pub fn discover_ripasso_hardware_keys(
) -> Result<Vec<super::super::hardware::DiscoveredHardwareToken>, String> {
    Err(HARDWAREKEY_FEATURE_DISABLED_ERROR.to_string())
}

#[cfg(feature = "hardwarekey")]
pub fn generate_ripasso_hardware_key(
    ident: &str,
    reader_hint: Option<&str>,
    name: &str,
    email: &str,
    admin_pin: SecretString,
    user_pin: SecretString,
    replace_user_pin: bool,
) -> Result<ManagedRipassoPrivateKey, PrivateKeyError> {
    let name = name.trim();
    if name.is_empty() {
        return Err(PrivateKeyError::other("Enter a name for the hardware key."));
    }

    let email = email.trim();
    if email.is_empty() {
        return Err(PrivateKeyError::other(
            "Enter an email address for the hardware key.",
        ));
    }

    let admin_pin_trimmed = admin_pin.expose_secret().trim();
    if admin_pin_trimmed.is_empty() {
        return Err(PrivateKeyError::hardware_pin_required(
            "Enter the hardware key admin PIN.",
        ));
    }

    let user_pin_trimmed = user_pin.expose_secret().trim();
    if user_pin_trimmed.is_empty() {
        return Err(PrivateKeyError::hardware_pin_required(
            if replace_user_pin {
                "Enter the new hardware key PIN."
            } else {
                "Enter the hardware key PIN."
            },
        ));
    }

    let user_id = format!("{name} <{email}>");
    let (token, public_key_bytes) = generate_hardware_key_material(&HardwareKeyGenerationRequest {
        ident: ident.to_string(),
        cardholder_name: name.to_string(),
        user_id,
        admin_pin: SecretString::from(admin_pin_trimmed),
        user_pin: SecretString::from(user_pin_trimmed),
        replace_user_pin,
    })
    .map_err(private_key_error_from_hardware_transport_error)?;
    let hardware = ManagedRipassoHardwareKey {
        ident: token.ident,
        signing_fingerprint: token.signing_fingerprint,
        decryption_fingerprint: token.decryption_fingerprint,
        reader_hint: token
            .reader_hint
            .or_else(|| reader_hint.map(ToString::to_string)),
    };

    store_ripasso_hardware_key_bytes(&public_key_bytes, hardware)
}

#[cfg(not(feature = "hardwarekey"))]
pub fn generate_ripasso_hardware_key(
    _ident: &str,
    _reader_hint: Option<&str>,
    _name: &str,
    _email: &str,
    _admin_pin: SecretString,
    _user_pin: SecretString,
    _replace_user_pin: bool,
) -> Result<ManagedRipassoPrivateKey, PrivateKeyError> {
    Err(PrivateKeyError::unsupported_hardware_key(
        HARDWAREKEY_FEATURE_DISABLED_ERROR,
    ))
}

#[cfg(feature = "fidokey")]
pub fn generate_fido2_private_key(
    pin: Option<&str>,
) -> Result<ManagedRipassoPrivateKey, PrivateKeyError> {
    let recipient = super::super::fido2::create_fido2_private_key_binding(pin)?;
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
    if let Ok(entry) = find_stored_private_key(fingerprint) {
        return Ok(entry.key.title());
    }

    find_connected_smartcard_key(fingerprint)?
        .map(|entry| entry.key.title())
        .ok_or_else(private_key_not_stored_error)
}

#[cfg(test)]
mod tests {
    #[cfg(not(feature = "legacy-compat"))]
    use super::{
        find_stored_private_key, list_ripasso_private_keys, load_stored_ripasso_key_ring,
        ripasso_keys_dir,
    };
    #[cfg(not(feature = "legacy-compat"))]
    use crate::backend::integrated::keys::cert::parse_managed_private_key_bytes;
    #[cfg(not(feature = "legacy-compat"))]
    use crate::backend::test_support::SystemBackendTestEnv;
    #[cfg(not(feature = "legacy-compat"))]
    use sequoia_openpgp::{cert::CertBuilder, crypto::Password, serialize::Serialize};
    #[cfg(not(feature = "legacy-compat"))]
    use std::fs;

    #[cfg(not(feature = "legacy-compat"))]
    fn protected_cert_bytes(email: &str) -> Vec<u8> {
        let password: Password = "hunter2".into();
        let (cert, _) = CertBuilder::general_purpose(Some(email))
            .set_password(Some(password))
            .generate()
            .expect("generate protected cert");
        let mut bytes = Vec::new();
        cert.as_tsk()
            .serialize(&mut bytes)
            .expect("serialize protected cert");
        bytes
    }

    #[cfg(not(feature = "legacy-compat"))]
    #[test]
    fn invalid_canonical_private_keys_are_skipped_during_scans_without_legacy_compat() {
        let _env = SystemBackendTestEnv::new();
        let valid_bytes = protected_cert_bytes("valid-scan@example.com");
        let broken_bytes = protected_cert_bytes("broken-scan@example.com");
        let (_, valid_key) =
            parse_managed_private_key_bytes(&valid_bytes).expect("parse valid key");
        let (_, broken_key) =
            parse_managed_private_key_bytes(&broken_bytes).expect("parse broken key");
        let keys_dir = ripasso_keys_dir().expect("resolve keys dir");
        fs::create_dir_all(&keys_dir).expect("create keys dir");
        fs::write(
            keys_dir.join(valid_key.fingerprint.to_ascii_lowercase()),
            &valid_bytes,
        )
        .expect("write valid key");
        fs::write(
            keys_dir.join(broken_key.fingerprint.to_ascii_lowercase()),
            b"not a key",
        )
        .expect("write broken key");

        let keys = list_ripasso_private_keys().expect("list keys");
        assert_eq!(keys.len(), 1);
        assert_eq!(keys[0].fingerprint, valid_key.fingerprint);

        let key_ring = load_stored_ripasso_key_ring().expect("load key ring");
        assert_eq!(key_ring.len(), 1);

        assert!(find_stored_private_key(&broken_key.fingerprint).is_err());
    }

    #[cfg(not(feature = "legacy-compat"))]
    #[test]
    fn non_canonical_private_key_paths_are_ignored_without_legacy_compat() {
        let _env = SystemBackendTestEnv::new();
        let bytes = protected_cert_bytes("uppercase-path@example.com");
        let (_, key) = parse_managed_private_key_bytes(&bytes).expect("parse key");
        let keys_dir = ripasso_keys_dir().expect("resolve keys dir");
        fs::create_dir_all(&keys_dir).expect("create keys dir");
        fs::write(keys_dir.join(key.fingerprint.to_ascii_uppercase()), &bytes)
            .expect("write legacy key");

        assert!(list_ripasso_private_keys().expect("list keys").is_empty());
        assert!(load_stored_ripasso_key_ring()
            .expect("load key ring")
            .is_empty());
        assert_eq!(
            find_stored_private_key(&key.fingerprint).expect_err("legacy key should be hidden"),
            "That private key is not stored in the app."
        );
    }

    #[cfg(not(feature = "legacy-compat"))]
    #[test]
    fn mismatched_private_key_paths_are_skipped_without_legacy_compat() {
        let _env = SystemBackendTestEnv::new();
        let bytes = protected_cert_bytes("mismatched-path@example.com");
        let other_bytes = protected_cert_bytes("other-path@example.com");
        let (_, key) = parse_managed_private_key_bytes(&bytes).expect("parse key");
        let (_, other_key) = parse_managed_private_key_bytes(&other_bytes).expect("parse other");
        let keys_dir = ripasso_keys_dir().expect("resolve keys dir");
        fs::create_dir_all(&keys_dir).expect("create keys dir");
        fs::write(
            keys_dir.join(other_key.fingerprint.to_ascii_lowercase()),
            &bytes,
        )
        .expect("write mismatched key");

        assert!(list_ripasso_private_keys().expect("list keys").is_empty());
        assert!(load_stored_ripasso_key_ring()
            .expect("load key ring")
            .is_empty());
        assert_eq!(
            find_stored_private_key(&key.fingerprint).expect_err("mismatched key should be hidden"),
            "That private key is not stored in the app."
        );
        assert!(find_stored_private_key(&other_key.fingerprint).is_err());
    }
}
