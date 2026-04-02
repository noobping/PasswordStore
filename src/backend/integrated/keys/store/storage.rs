#[cfg(feature = "fidokey")]
use super::super::cache::clear_cached_fido2_pin;
use super::super::cache::{
    borrow_unlocked_ripasso_private_key, cache_unlocked_ripasso_private_key,
    remove_cached_unlocked_ripasso_private_key,
};
#[cfg(feature = "fidokey")]
use super::super::cert::cert_can_decrypt_password_entries;
#[cfg(feature = "fidokey")]
use super::super::cert::parse_fido2_public_key_bytes;
use super::super::cert::{
    cert_has_transport_encryption_key, cert_requires_passphrase, fingerprint_from_string,
    normalized_fingerprint, parse_hardware_public_key_bytes, parse_managed_private_key_bytes,
    prepare_managed_private_key_bytes, ManagedRipassoHardwareKey, ManagedRipassoPrivateKey,
    ManagedRipassoPrivateKeyProtection,
};
use super::super::hardware::{
    list_hardware_tokens, private_key_error_from_hardware_transport_error,
};
#[cfg(feature = "fidokey")]
use super::manifest::{
    fido2_private_key_manifest_contents, managed_fido2_private_key_from_cert,
    parse_fido2_private_key_manifest, parse_fido2_private_key_manifest_bytes,
    read_fido2_private_key_manifest_entry, Fido2PrivateKeyManifest,
};
use super::manifest::{
    read_hardware_private_key_manifest, read_hardware_private_key_manifest_entry,
    HardwarePrivateKeyManifest,
};
#[cfg(test)]
use super::missing_private_key_error;
#[cfg(feature = "fidokey")]
use super::paths::ripasso_fido_keys_dir;
use super::paths::{
    hardware_manifest_path, hardware_public_key_path, ripasso_keys_dir, ripasso_keys_v2_dir,
};
use super::private_key_not_stored_error;
#[cfg(not(feature = "fidokey"))]
use super::FIDO2_PRIVATE_KEY_FEATURE_DISABLED_ERROR;
use crate::backend::PrivateKeyError;
#[cfg(feature = "fidokey")]
use crate::fido2_recipient::parse_fido2_recipient_string;
use crate::logging::log_error;
use crate::preferences::Preferences;
use crate::support::secure_fs::{ensure_private_dir, write_private_file};
use ripasso::crypto::{slice_to_20_bytes, Sequoia};
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
        if path.is_file() {
            paths.push(path);
        }
    }
    Ok(paths)
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

pub fn import_ripasso_hardware_key_bytes(
    bytes: &[u8],
    hardware: ManagedRipassoHardwareKey,
) -> Result<ManagedRipassoPrivateKey, PrivateKeyError> {
    store_ripasso_hardware_key_bytes(bytes, hardware)
}

pub fn discover_ripasso_hardware_keys(
) -> Result<Vec<super::super::hardware::DiscoveredHardwareToken>, String> {
    list_hardware_tokens().map_err(|err| err.to_string())
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
    Ok(find_stored_private_key(fingerprint)?.key.title())
}
