#[cfg(not(feature = "flatpak"))]
use ripasso::crypto::CryptoImpl;
#[cfg(feature = "flatpak")]
use ripasso::crypto::{slice_to_20_bytes, Crypto, Sequoia};
#[cfg(not(feature = "flatpak"))]
use ripasso::pass::PasswordStore;
#[cfg(feature = "flatpak")]
use ripasso::pass::{Comment, KeyRingStatus, OwnerTrustLevel, Recipient};

#[cfg(feature = "flatpak")]
use crate::logging::log_error;
#[cfg(feature = "flatpak")]
use crate::preferences::Preferences;
#[cfg(feature = "flatpak")]
use sequoia_openpgp::{
    cert::amalgamation::key::PrimaryKey, crypto::Password, parse::Parse, serialize::Serialize,
    Cert, Fingerprint, KeyHandle, Packet,
};
#[cfg(feature = "flatpak")]
use std::collections::{HashMap, HashSet};
use std::fs;
#[cfg(feature = "flatpak")]
use std::fs::File;
#[cfg(feature = "flatpak")]
use std::path::{Component, Path, PathBuf};
#[cfg(not(feature = "flatpak"))]
use std::path::PathBuf;
#[cfg(feature = "flatpak")]
use std::sync::{Arc, OnceLock, RwLock};
#[cfg(feature = "flatpak")]
use walkdir::WalkDir;

#[cfg(feature = "flatpak")]
const PRIVATE_KEY_NOT_STORED_ERROR: &str = "That private key is not stored in the app.";
#[cfg(feature = "flatpak")]
const MISSING_PRIVATE_KEY_ERROR: &str =
    "Import a private key in Preferences before using the password store.";
#[cfg(feature = "flatpak")]
const LOCKED_PRIVATE_KEY_ERROR: &str =
    "The selected private key is locked. Unlock it in Preferences and enter its password.";
#[cfg(feature = "flatpak")]
const INCOMPATIBLE_PRIVATE_KEY_ERROR: &str =
    "The selected private key cannot decrypt password store entries.";

#[cfg(feature = "flatpak")]
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ManagedRipassoPrivateKey {
    pub fingerprint: String,
    pub user_ids: Vec<String>,
}

#[cfg(feature = "flatpak")]
impl ManagedRipassoPrivateKey {
    pub fn title(&self) -> String {
        self.user_ids
            .first()
            .cloned()
            .filter(|value| !value.trim().is_empty())
            .unwrap_or_else(|| "Unnamed private key".to_string())
    }
}

fn user_home() -> Option<PathBuf> {
    dirs_next::home_dir()
}

#[cfg(not(feature = "flatpak"))]
fn open_store(store_root: &str) -> Result<PasswordStore, String> {
    let own_fingerprint = None;
    let crypto_impl = CryptoImpl::GpgMe;

    PasswordStore::new(
        "default",
        &Some(PathBuf::from(store_root)),
        &None,
        &user_home(),
        &None,
        &crypto_impl,
        &own_fingerprint,
    )
    .map_err(|err| err.to_string())
}

#[cfg(not(feature = "flatpak"))]
fn load_store_entry(
    store_root: &str,
    label: &str,
) -> Result<(PasswordStore, ripasso::pass::PasswordEntry), String> {
    let mut store = open_store(store_root)?;
    store
        .reload_password_list()
        .map_err(|err| err.to_string())?;
    let entry = store
        .passwords
        .iter()
        .find(|entry| entry.name == label)
        .cloned()
        .ok_or_else(|| format!("Password entry '{label}' was not found."))?;
    Ok((store, entry))
}

#[cfg(feature = "flatpak")]
fn ripasso_keys_dir() -> Result<PathBuf, String> {
    let data_dir = dirs_next::data_local_dir()
        .ok_or_else(|| "Could not determine the data folder.".to_string())?;
    Ok(data_dir.join(env!("CARGO_PKG_NAME")).join("keys"))
}

#[cfg(feature = "flatpak")]
fn unlocked_ripasso_private_keys() -> &'static RwLock<HashMap<String, Arc<Cert>>> {
    static UNLOCKED_KEYS: OnceLock<RwLock<HashMap<String, Arc<Cert>>>> = OnceLock::new();
    UNLOCKED_KEYS.get_or_init(|| RwLock::new(HashMap::new()))
}

#[cfg(feature = "flatpak")]
fn with_unlocked_ripasso_keys_read<T>(f: impl FnOnce(&HashMap<String, Arc<Cert>>) -> T) -> T {
    match unlocked_ripasso_private_keys().read() {
        Ok(keys) => f(&keys),
        Err(poisoned) => {
            let keys = poisoned.into_inner();
            f(&keys)
        }
    }
}

#[cfg(feature = "flatpak")]
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

#[cfg(feature = "flatpak")]
fn fingerprint_from_string(value: &str) -> Result<[u8; 20], String> {
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

#[cfg(feature = "flatpak")]
fn normalized_fingerprint(value: &str) -> Result<String, String> {
    Ok(Fingerprint::from_hex(value)
        .map_err(|err| format!("Invalid private key fingerprint '{value}': {err}"))?
        .to_hex())
}

#[cfg(feature = "flatpak")]
fn cached_unlocked_ripasso_private_key(fingerprint: &str) -> Result<Option<Arc<Cert>>, String> {
    let fingerprint = normalized_fingerprint(fingerprint)?;
    Ok(with_unlocked_ripasso_keys_read(|keys| {
        keys.get(&fingerprint).cloned()
    }))
}

#[cfg(feature = "flatpak")]
fn cache_unlocked_ripasso_private_key(cert: Cert) {
    let fingerprint = cert.fingerprint().to_hex();
    with_unlocked_ripasso_keys_write(|keys| {
        keys.insert(fingerprint, Arc::new(cert));
    });
}

#[cfg(feature = "flatpak")]
fn remove_cached_unlocked_ripasso_private_key(fingerprint: &str) -> Result<(), String> {
    let fingerprint = normalized_fingerprint(fingerprint)?;
    with_unlocked_ripasso_keys_write(|keys| {
        keys.remove(&fingerprint);
    });
    Ok(())
}

#[cfg(all(feature = "flatpak", test))]
fn clear_cached_unlocked_ripasso_private_keys() {
    with_unlocked_ripasso_keys_write(|keys| keys.clear());
}

#[cfg(feature = "flatpak")]
fn missing_private_key_error() -> String {
    MISSING_PRIVATE_KEY_ERROR.to_string()
}

#[cfg(feature = "flatpak")]
fn locked_private_key_error() -> String {
    LOCKED_PRIVATE_KEY_ERROR.to_string()
}

#[cfg(feature = "flatpak")]
fn incompatible_private_key_error() -> String {
    INCOMPATIBLE_PRIVATE_KEY_ERROR.to_string()
}

#[cfg(feature = "flatpak")]
fn private_key_not_stored_error() -> String {
    PRIVATE_KEY_NOT_STORED_ERROR.to_string()
}

#[cfg(feature = "flatpak")]
fn parse_managed_private_key_bytes(
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

#[cfg(all(feature = "flatpak", debug_assertions))]
fn debug_flatpak_save(message: impl AsRef<str>) {
    let message = message.as_ref();
    eprintln!("[flatpak-save] {message}");
    log_error(format!("Flatpak save: {message}"));
}

#[cfg(all(feature = "flatpak", not(debug_assertions)))]
fn debug_flatpak_save(_message: impl AsRef<str>) {}

#[cfg(feature = "flatpak")]
fn cert_requires_passphrase(cert: &Cert) -> bool {
    cert.keys()
        .secret()
        .any(|key_amalgamation| !key_amalgamation.key().has_unencrypted_secret())
}

#[cfg(feature = "flatpak")]
fn cert_can_decrypt_password_entries(cert: &Cert) -> bool {
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

#[cfg(feature = "flatpak")]
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

#[cfg(feature = "flatpak")]
fn prepare_managed_private_key_bytes(
    bytes: &[u8],
    passphrase: Option<&str>,
) -> Result<(Cert, ManagedRipassoPrivateKey), String> {
    let (parsed_cert, key) = parse_managed_private_key_bytes(bytes)?;
    let cert = if cert_requires_passphrase(&parsed_cert) {
        let passphrase = passphrase.ok_or_else(|| "This private key is password protected.".to_string())?;
        unlock_managed_private_key_cert(&parsed_cert, passphrase)?
    } else {
        parsed_cert
    };

    if !cert_can_decrypt_password_entries(&cert) {
        return Err("That private key cannot decrypt password store entries.".to_string());
    }

    Ok((cert, key))
}

#[cfg(feature = "flatpak")]
fn read_ripasso_private_key_cert(path: &Path) -> Result<(Cert, ManagedRipassoPrivateKey), String> {
    let data = fs::read(path).map_err(|err| err.to_string())?;
    parse_managed_private_key_bytes(&data)
}

#[cfg(feature = "flatpak")]
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

#[cfg(feature = "flatpak")]
fn build_ripasso_crypto() -> Result<Sequoia, String> {
    let fingerprint = resolved_ripasso_own_fingerprint()?;
    let key_ring = load_ripasso_key_ring(&fingerprint)?;
    build_ripasso_crypto_from_key_ring(&fingerprint, key_ring)
}

#[cfg(feature = "flatpak")]
fn build_ripasso_crypto_from_key_ring(
    fingerprint: &str,
    key_ring: HashMap<[u8; 20], Arc<Cert>>,
) -> Result<Sequoia, String> {
    let user_key_id = fingerprint_from_string(fingerprint)?;
    let home = user_home().ok_or_else(|| "Could not determine the home folder.".to_string())?;
    Ok(Sequoia::from_values(user_key_id, key_ring, &home))
}

#[cfg(feature = "flatpak")]
fn load_ripasso_key_ring(fingerprint: &str) -> Result<HashMap<[u8; 20], Arc<Cert>>, String> {
    let user_key_id = fingerprint_from_string(fingerprint)?;
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

    if let Some(cert) = cached_unlocked_ripasso_private_key(&fingerprint)? {
        key_ring.insert(user_key_id, cert);
    }

    Ok(key_ring)
}

#[cfg(feature = "flatpak")]
fn available_unlocked_private_key_fingerprints(preferred: &str) -> Vec<String> {
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

#[cfg(feature = "flatpak")]
fn describe_key_ring(key_ring: &HashMap<[u8; 20], Arc<Cert>>) -> String {
    let mut entries = key_ring
        .iter()
        .map(|(fingerprint, cert)| {
            let user_ids = cert
                .userids()
                .map(|user_id| user_id.userid().to_string())
                .filter(|value| !value.trim().is_empty())
                .collect::<Vec<_>>();
            format!(
                "{} [{}]",
                uppercase_fingerprint_hex(fingerprint),
                if user_ids.is_empty() {
                    "?".to_string()
                } else {
                    user_ids.join(" | ")
                }
            )
        })
        .collect::<Vec<_>>();
    entries.sort();
    entries.join(", ")
}

#[cfg(feature = "flatpak")]
fn describe_recipients(recipients: &[Recipient]) -> String {
    recipients
        .iter()
        .map(|recipient| {
            let fingerprint = recipient
                .fingerprint
                .map(|fingerprint| uppercase_fingerprint_hex(&fingerprint))
                .unwrap_or_else(|| "?".to_string());
            format!("{} => {}", recipient.key_id, fingerprint)
        })
        .collect::<Vec<_>>()
        .join(", ")
}

#[cfg(feature = "flatpak")]
fn uppercase_fingerprint_hex(fingerprint: &[u8; 20]) -> String {
    fingerprint
        .iter()
        .map(|byte| format!("{byte:02X}"))
        .collect::<String>()
}

#[cfg(feature = "flatpak")]
fn validated_entry_label_path(label: &str) -> Result<PathBuf, String> {
    let mut relative = PathBuf::new();
    for component in Path::new(label).components() {
        match component {
            Component::Normal(part) => relative.push(part),
            Component::CurDir => {}
            _ => return Err("Invalid password entry path.".to_string()),
        }
    }

    if relative.as_os_str().is_empty() {
        return Err("Password entry name is empty.".to_string());
    }

    Ok(relative)
}

#[cfg(feature = "flatpak")]
fn secret_entry_relative_path(label: &str) -> Result<PathBuf, String> {
    let mut relative = validated_entry_label_path(label)?;
    let file_name = relative
        .file_name()
        .and_then(|value| value.to_str())
        .ok_or_else(|| "Invalid password entry path.".to_string())?;
    relative.set_file_name(format!("{file_name}.gpg"));
    Ok(relative)
}

#[cfg(feature = "flatpak")]
fn entry_file_path(store_root: &str, label: &str) -> Result<PathBuf, String> {
    let mut path = PathBuf::from(store_root);
    path.push(secret_entry_relative_path(label)?);
    Ok(path)
}

#[cfg(feature = "flatpak")]
fn recipients_file_for_label(store_root: &str, label: &str) -> Result<PathBuf, String> {
    let relative = validated_entry_label_path(label)?;
    let mut current = Some(relative.parent().map(PathBuf::from).unwrap_or_default());

    while let Some(dir) = current {
        let candidate = PathBuf::from(store_root).join(&dir).join(".gpg-id");
        if candidate.is_file() {
            return Ok(candidate);
        }
        current = dir.parent().map(PathBuf::from);
    }

    Err("No recipients were found for this password entry.".to_string())
}

#[cfg(feature = "flatpak")]
fn label_from_entry_path(store_root: &Path, entry_path: &Path) -> Result<String, String> {
    let relative = entry_path
        .strip_prefix(store_root)
        .map_err(|_| "Invalid password entry path.".to_string())?;
    let mut label = relative.to_path_buf();
    if label.extension().and_then(|value| value.to_str()) != Some("gpg") {
        return Err("Invalid password entry path.".to_string());
    }
    label.set_extension("");
    Ok(label.to_string_lossy().to_string())
}

#[cfg(feature = "flatpak")]
fn read_entry_ciphertext(entry_path: &Path) -> Result<Vec<u8>, String> {
    let metadata = fs::metadata(entry_path).map_err(|err| err.to_string())?;
    if metadata.len() == 0 {
        return Err("empty password file".to_string());
    }
    fs::read(entry_path).map_err(|err| err.to_string())
}

#[cfg(feature = "flatpak")]
fn decrypt_password_entry_with_crypto(
    crypto: &Sequoia,
    entry_path: &Path,
) -> Result<String, String> {
    let ciphertext = read_entry_ciphertext(entry_path)?;
    crypto
        .decrypt_string(&ciphertext)
        .map_err(|err| err.to_string())
}

#[cfg(feature = "flatpak")]
fn decrypt_password_entry_with_any_available_key(
    preferred_fingerprint: &str,
    entry_path: &Path,
) -> Result<String, String> {
    let mut last_error = None;
    for fingerprint in available_unlocked_private_key_fingerprints(preferred_fingerprint) {
        let key_ring = load_ripasso_key_ring(&fingerprint)?;
        let crypto = build_ripasso_crypto_from_key_ring(&fingerprint, key_ring)?;
        match decrypt_password_entry_with_crypto(&crypto, entry_path) {
            Ok(secret) => {
                debug_flatpak_save(format!(
                    "decrypted {} using {}",
                    entry_path.display(),
                    fingerprint
                ));
                return Ok(secret);
            }
            Err(err) => {
                debug_flatpak_save(format!(
                    "failed to decrypt {} using {}: {}",
                    entry_path.display(),
                    fingerprint,
                    err
                ));
                last_error = Some(err);
            }
        }
    }

    Err(last_error.unwrap_or_else(|| LOCKED_PRIVATE_KEY_ERROR.to_string()))
}

#[cfg(feature = "flatpak")]
fn encrypt_password_entry_with_crypto(
    crypto: &Sequoia,
    recipients: &[Recipient],
    contents: &str,
) -> Result<Vec<u8>, String> {
    crypto
        .encrypt_string(contents, recipients)
        .map_err(|err| err.to_string())
}

#[cfg(feature = "flatpak")]
fn recipients_for_encryption(
    recipients_file: &Path,
    key_ring: &HashMap<[u8; 20], Arc<Cert>>,
) -> Result<Vec<Recipient>, String> {
    let contents = fs::read_to_string(recipients_file).map_err(|err| err.to_string())?;
    debug_flatpak_save(format!(
        "recipients file {} contains: {:?}",
        recipients_file.display(),
        contents
    ));
    let mut recipients = Vec::new();
    let mut seen = HashSet::new();

    for raw_line in contents.lines() {
        let line = raw_line
            .split_once('#')
            .map(|(key, _)| key)
            .unwrap_or(raw_line)
            .trim();
        if line.is_empty() {
            continue;
        }

        let Some((fingerprint, cert)) = resolve_recipient_cert(line, key_ring) else {
            debug_flatpak_save(format!(
                "recipient '{line}' could not be resolved; available keys: {}",
                describe_key_ring(key_ring)
            ));
            return Err(format!("Recipient '{line}' is not available in the app."));
        };
        if !seen.insert(fingerprint) {
            continue;
        }

        let name = cert
            .userids()
            .map(|user_id| user_id.userid().to_string())
            .find(|value| !value.trim().is_empty())
            .unwrap_or_else(|| line.to_string());

        recipients.push(Recipient {
            name,
            comment: Comment {
                pre_comment: None,
                post_comment: None,
            },
            key_id: cert.fingerprint().to_hex(),
            fingerprint: Some(fingerprint),
            key_ring_status: KeyRingStatus::InKeyRing,
            trust_level: OwnerTrustLevel::Ultimate,
            not_usable: false,
        });
        debug_flatpak_save(format!(
            "resolved recipient '{line}' to {}",
            uppercase_fingerprint_hex(&fingerprint)
        ));
    }

    Ok(recipients)
}

#[cfg(feature = "flatpak")]
fn resolve_recipient_cert<'a>(
    recipient_id: &str,
    key_ring: &'a HashMap<[u8; 20], Arc<Cert>>,
) -> Option<([u8; 20], &'a Arc<Cert>)> {
    if let Ok(fingerprint) = fingerprint_from_string(recipient_id) {
        if let Some(cert) = key_ring.get(&fingerprint) {
            return Some((fingerprint, cert));
        }
    }

    if let Ok(handle) = recipient_id.parse::<KeyHandle>() {
        for (fingerprint, cert) in key_ring {
            if cert.key_handle().aliases(&handle) {
                return Some((*fingerprint, cert));
            }
        }
    }

    let needle = recipient_id.trim().to_ascii_lowercase();
    if needle.is_empty() {
        return None;
    }

    for (fingerprint, cert) in key_ring {
        if cert.userids().any(|user_id| {
            let user_id = user_id.userid().to_string();
            let user_id = user_id.trim().to_ascii_lowercase();
            user_id == needle || user_id.contains(&format!("<{needle}>"))
        }) {
            return Some((*fingerprint, cert));
        }
    }

    None
}

#[cfg(feature = "flatpak")]
fn cleanup_empty_store_dirs(store_root: &str, entry_path: &Path) -> Result<(), String> {
    let root = PathBuf::from(store_root);
    let mut current = entry_path.parent().map(PathBuf::from);

    while let Some(dir) = current {
        if dir == root {
            break;
        }

        match fs::remove_dir(&dir) {
            Ok(()) => current = dir.parent().map(PathBuf::from),
            Err(err)
                if matches!(
                    err.kind(),
                    std::io::ErrorKind::DirectoryNotEmpty | std::io::ErrorKind::NotFound
                ) =>
            {
                break;
            }
            Err(err) => return Err(err.to_string()),
        }
    }

    Ok(())
}

#[cfg(feature = "flatpak")]
fn collect_password_entry_files(store_root: &Path) -> Result<Vec<PathBuf>, String> {
    if !store_root.exists() {
        return Ok(Vec::new());
    }

    let mut entries = Vec::new();
    for entry in WalkDir::new(store_root) {
        let entry = entry.map_err(|err| err.to_string())?;
        if !entry.file_type().is_file() {
            continue;
        }
        if entry.path().extension().and_then(|value| value.to_str()) == Some("gpg") {
            entries.push(entry.into_path());
        }
    }

    Ok(entries)
}

#[cfg(not(feature = "flatpak"))]
pub(super) fn read_password_entry(store_root: &str, label: &str) -> Result<String, String> {
    let (store, entry) = load_store_entry(store_root, label)?;
    entry.secret(&store).map_err(|err| err.to_string())
}

#[cfg(feature = "flatpak")]
pub(super) fn read_password_entry(store_root: &str, label: &str) -> Result<String, String> {
    let fingerprint = resolved_ripasso_own_fingerprint()?;
    ensure_ripasso_private_key_is_ready(&fingerprint)?;
    let crypto = build_ripasso_crypto()?;
    let entry_path = entry_file_path(store_root, label)?;
    decrypt_password_entry_with_crypto(&crypto, &entry_path)
}

#[cfg(not(feature = "flatpak"))]
pub(super) fn read_password_line(store_root: &str, label: &str) -> Result<String, String> {
    let (store, entry) = load_store_entry(store_root, label)?;
    entry.password(&store).map_err(|err| err.to_string())
}

#[cfg(feature = "flatpak")]
pub(super) fn read_password_line(store_root: &str, label: &str) -> Result<String, String> {
    let secret = read_password_entry(store_root, label)?;
    Ok(secret.lines().next().unwrap_or_default().to_string())
}

#[cfg(not(feature = "flatpak"))]
pub(super) fn save_password_entry(
    store_root: &str,
    label: &str,
    contents: &str,
    overwrite: bool,
) -> Result<(), String> {
    let mut store = open_store(store_root)?;
    store
        .reload_password_list()
        .map_err(|err| err.to_string())?;
    if let Some(entry) = store.passwords.iter().find(|entry| entry.name == label).cloned() {
        if !overwrite {
            return Err("That password entry already exists.".to_string());
        }
        entry
            .update(contents.to_string(), &store)
            .map_err(|err| err.to_string())
    } else {
        store
            .new_password_file(label, contents)
            .map(|_| ())
            .map_err(|err| err.to_string())
    }
}

#[cfg(feature = "flatpak")]
pub(super) fn save_password_entry(
    store_root: &str,
    label: &str,
    contents: &str,
    overwrite: bool,
) -> Result<(), String> {
    debug_flatpak_save(format!(
        "save start store_root='{}' label='{}' overwrite={overwrite}",
        store_root, label
    ));
    let entry_path = entry_file_path(store_root, label)?;
    if entry_path.exists() && !overwrite {
        return Err("That password entry already exists.".to_string());
    }

    let fingerprint = resolved_ripasso_own_fingerprint()?;
    let key_ring = load_ripasso_key_ring(&fingerprint)?;
    debug_flatpak_save(format!(
        "selected private key: {}; available keys: {}",
        fingerprint,
        describe_key_ring(&key_ring)
    ));
    let crypto = build_ripasso_crypto_from_key_ring(&fingerprint, key_ring.clone())?;
    let recipients_file = recipients_file_for_label(store_root, label)?;
    let recipients = recipients_for_encryption(&recipients_file, &key_ring)?;
    debug_flatpak_save(format!(
        "resolved recipients for {}: {}",
        label,
        describe_recipients(&recipients)
    ));
    let ciphertext = encrypt_password_entry_with_crypto(&crypto, &recipients, contents)?;
    if let Some(parent) = entry_path.parent() {
        fs::create_dir_all(parent).map_err(|err| err.to_string())?;
    }
    debug_flatpak_save(format!("writing encrypted entry to {}", entry_path.display()));
    fs::write(entry_path, ciphertext).map_err(|err| err.to_string())
}

#[cfg(not(feature = "flatpak"))]
pub(super) fn rename_password_entry(
    store_root: &str,
    old_label: &str,
    new_label: &str,
) -> Result<(), String> {
    let mut store = open_store(store_root)?;
    store
        .reload_password_list()
        .map_err(|err| err.to_string())?;
    store
        .rename_file(old_label, new_label)
        .map(|_| ())
        .map_err(|err| err.to_string())
}

#[cfg(feature = "flatpak")]
pub(super) fn rename_password_entry(
    store_root: &str,
    old_label: &str,
    new_label: &str,
) -> Result<(), String> {
    let old_path = entry_file_path(store_root, old_label)?;
    let new_path = entry_file_path(store_root, new_label)?;
    if !old_path.exists() {
        return Err(format!("Password entry '{old_label}' was not found."));
    }
    if new_path.exists() {
        return Err("That password entry already exists.".to_string());
    }
    if let Some(parent) = new_path.parent() {
        fs::create_dir_all(parent).map_err(|err| err.to_string())?;
    }
    fs::rename(&old_path, &new_path).map_err(|err| err.to_string())?;
    cleanup_empty_store_dirs(store_root, &old_path)
}

#[cfg(not(feature = "flatpak"))]
pub(super) fn delete_password_entry(store_root: &str, label: &str) -> Result<(), String> {
    let (store, entry) = load_store_entry(store_root, label)?;
    entry.delete_file(&store).map_err(|err| err.to_string())
}

#[cfg(feature = "flatpak")]
pub(super) fn delete_password_entry(store_root: &str, label: &str) -> Result<(), String> {
    let entry_path = entry_file_path(store_root, label)?;
    fs::remove_file(&entry_path).map_err(|err| err.to_string())?;
    cleanup_empty_store_dirs(store_root, &entry_path)
}

#[cfg(not(feature = "flatpak"))]
pub(super) fn save_store_recipients(
    store_root: &str,
    recipients: &[String],
) -> Result<(), String> {
    let store_dir = PathBuf::from(store_root);
    if store_dir.exists() {
        if !store_dir.is_dir() {
            return Err("The selected password store path is not a folder.".to_string());
        }
    } else {
        fs::create_dir_all(&store_dir).map_err(|err| err.to_string())?;
    }

    let recipients_path = store_dir.join(".gpg-id");
    let previous_recipients = fs::read_to_string(&recipients_path).ok();
    let contents = format!("{}\n", recipients.join("\n"));

    fs::write(&recipients_path, contents).map_err(|err| err.to_string())?;

    let result = (|| {
        let store = open_store(store_root)?;
        let entries = store.all_passwords().map_err(|err| err.to_string())?;
        for entry in entries {
            let secret = entry.secret(&store).map_err(|err| err.to_string())?;
            entry.update(secret, &store).map_err(|err| err.to_string())?;
        }
        Ok(())
    })();

    if let Err(err) = result {
        match previous_recipients {
            Some(previous) => {
                let _ = fs::write(&recipients_path, previous);
            }
            None => {
                let _ = fs::remove_file(&recipients_path);
            }
        }
        return Err(err);
    }

    Ok(())
}

#[cfg(feature = "flatpak")]
pub(super) fn save_store_recipients(
    store_root: &str,
    recipients: &[String],
) -> Result<(), String> {
    let fingerprint = resolved_ripasso_own_fingerprint()?;

    let store_dir = PathBuf::from(store_root);
    if store_dir.exists() {
        if !store_dir.is_dir() {
            return Err("The selected password store path is not a folder.".to_string());
        }
    } else {
        fs::create_dir_all(&store_dir).map_err(|err| err.to_string())?;
    }

    let key_ring = load_ripasso_key_ring(&fingerprint)?;
    let crypto = build_ripasso_crypto_from_key_ring(&fingerprint, key_ring.clone())?;
    let recipients_path = store_dir.join(".gpg-id");
    let previous_recipients = fs::read_to_string(&recipients_path).ok();
    let contents = format!("{}\n", recipients.join("\n"));

    fs::write(&recipients_path, contents).map_err(|err| err.to_string())?;

    let result = (|| {
        let key_ring = load_ripasso_key_ring(&fingerprint)?;
        for entry_path in collect_password_entry_files(&store_dir)? {
            let secret = decrypt_password_entry_with_any_available_key(&fingerprint, &entry_path)?;
            let label = label_from_entry_path(&store_dir, &entry_path)?;
            let recipients_file = recipients_file_for_label(store_root, &label)?;
            let recipients = recipients_for_encryption(&recipients_file, &key_ring)?;
            let ciphertext = encrypt_password_entry_with_crypto(&crypto, &recipients, &secret)?;
            fs::write(&entry_path, ciphertext).map_err(|err| err.to_string())?;
        }
        Ok(())
    })();

    if let Err(err) = result {
        match previous_recipients {
            Some(previous) => {
                let _ = fs::write(&recipients_path, previous);
            }
            None => {
                let _ = fs::remove_file(&recipients_path);
            }
        }
        return Err(err);
    }

    Ok(())
}

#[cfg(feature = "flatpak")]
pub fn is_ripasso_private_key_unlocked(fingerprint: &str) -> Result<bool, String> {
    Ok(cached_unlocked_ripasso_private_key(fingerprint)?.is_some())
}

#[cfg(feature = "flatpak")]
pub fn ripasso_private_key_requires_session_unlock(fingerprint: &str) -> Result<bool, String> {
    if cached_unlocked_ripasso_private_key(fingerprint)?.is_some() {
        return Ok(false);
    }

    let (_, cert, _) = find_ripasso_private_key_cert(fingerprint)?;
    Ok(cert_requires_passphrase(&cert))
}

#[cfg(feature = "flatpak")]
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

#[cfg(feature = "flatpak")]
fn ensure_ripasso_private_key_is_ready(fingerprint: &str) -> Result<(), String> {
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

#[cfg(feature = "flatpak")]
pub fn ripasso_private_key_requires_passphrase(bytes: &[u8]) -> Result<bool, String> {
    let (cert, _) = parse_managed_private_key_bytes(bytes)?;
    Ok(cert_requires_passphrase(&cert))
}

#[cfg(feature = "flatpak")]
pub fn list_ripasso_private_keys() -> Result<Vec<ManagedRipassoPrivateKey>, String> {
    let keys_dir = ripasso_keys_dir()?;
    if !keys_dir.exists() {
        return Ok(Vec::new());
    }

    let mut keys = Vec::new();
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

    keys.sort_by(|left, right| {
        left.title()
            .to_ascii_lowercase()
            .cmp(&right.title().to_ascii_lowercase())
            .then_with(|| left.fingerprint.cmp(&right.fingerprint))
    });
    Ok(keys)
}

#[cfg(feature = "flatpak")]
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

#[cfg(feature = "flatpak")]
pub fn remove_ripasso_private_key(fingerprint: &str) -> Result<(), String> {
    let (path, _, _) = find_ripasso_private_key_cert(fingerprint)?;
    fs::remove_file(path).map_err(|err| err.to_string())?;
    remove_cached_unlocked_ripasso_private_key(fingerprint)?;
    Ok(())
}

#[cfg(feature = "flatpak")]
pub fn resolved_ripasso_own_fingerprint() -> Result<String, String> {
    let settings = Preferences::new();
    let configured = settings.ripasso_own_fingerprint();
    let keys = list_ripasso_private_keys()?;

    let resolved = configured
        .as_deref()
        .and_then(|fingerprint| {
            keys.iter()
                .find(|key| key.fingerprint.eq_ignore_ascii_case(fingerprint))
                .map(|key| key.fingerprint.clone())
        })
        .or_else(|| keys.first().map(|key| key.fingerprint.clone()))
        .ok_or_else(missing_private_key_error)?;

    if configured.as_deref() != Some(resolved.as_str()) {
        let _ = settings.set_ripasso_own_fingerprint(Some(&resolved));
    }

    Ok(resolved)
}

#[cfg(feature = "flatpak")]
pub fn ripasso_private_key_title(fingerprint: &str) -> Result<String, String> {
    let (_, _, key) = find_ripasso_private_key_cert(fingerprint)?;
    Ok(key.title())
}

#[cfg(all(test, feature = "flatpak"))]
mod tests {
    use super::{
        clear_cached_unlocked_ripasso_private_keys, ensure_ripasso_private_key_is_ready,
        import_ripasso_private_key_bytes, is_ripasso_private_key_unlocked,
        parse_managed_private_key_bytes, prepare_managed_private_key_bytes, ripasso_keys_dir,
        read_password_entry, recipients_file_for_label, ripasso_private_key_requires_passphrase,
        save_password_entry, save_store_recipients, secret_entry_relative_path,
        unlock_ripasso_private_key_for_session,
    };
    use crate::preferences::Preferences;
    use sequoia_openpgp::{cert::CertBuilder, crypto::Password, serialize::Serialize};
    use std::env;
    use std::ffi::OsString;
    use std::fs;
    use std::path::PathBuf;
    use std::sync::{Mutex, OnceLock};
    use std::time::{SystemTime, UNIX_EPOCH};

    fn test_lock() -> &'static Mutex<()> {
        static TEST_LOCK: OnceLock<Mutex<()>> = OnceLock::new();
        TEST_LOCK.get_or_init(|| Mutex::new(()))
    }

    struct TestHome {
        original_home: Option<OsString>,
        path: std::path::PathBuf,
    }

    impl TestHome {
        fn new() -> Self {
            let nanos = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .expect("system time before unix epoch")
                .as_nanos();
            let path = std::env::temp_dir().join(format!("passwordstore-flatpak-test-{nanos}"));
            fs::create_dir_all(&path).expect("create temporary HOME");
            let original_home = env::var_os("HOME");
            env::set_var("HOME", &path);
            clear_cached_unlocked_ripasso_private_keys();
            Self {
                original_home,
                path,
            }
        }
    }

    impl Drop for TestHome {
        fn drop(&mut self) {
            clear_cached_unlocked_ripasso_private_keys();
            if let Some(original_home) = self.original_home.as_ref() {
                env::set_var("HOME", original_home);
            } else {
                env::remove_var("HOME");
            }
            let _ = fs::remove_dir_all(&self.path);
        }
    }

    fn cert_bytes(email: &str) -> Vec<u8> {
        let (cert, _) = CertBuilder::general_purpose(Some(email))
            .generate()
            .expect("failed to generate test certificate");
        let mut bytes = Vec::new();
        cert.as_tsk()
            .serialize(&mut bytes)
            .expect("failed to serialize test certificate");
        bytes
    }

    #[test]
    fn ripasso_private_key_parser_reads_secret_keys() {
        let bytes = cert_bytes("Alice Example <alice@example.com>");

        let (_, key) = parse_managed_private_key_bytes(&bytes)
            .expect("expected secret key to parse as a managed private key");

        assert_eq!(key.fingerprint.len(), 40);
        assert!(key
            .user_ids
            .iter()
            .any(|user_id| user_id.contains("alice@example.com")));
    }

    #[test]
    fn ripasso_private_key_parser_rejects_public_only_keys() {
        let (cert, _) = CertBuilder::general_purpose(Some("Bob Example <bob@example.com>"))
            .generate()
            .expect("failed to generate test certificate");
        let public_only = cert.strip_secret_key_material();
        let mut bytes = Vec::new();
        public_only
            .serialize(&mut bytes)
            .expect("failed to serialize public test certificate");

        let err = parse_managed_private_key_bytes(&bytes)
            .expect_err("public-only keys should not be accepted as managed private keys");
        assert!(err.contains("does not include a private key"));
    }

    #[test]
    fn encrypted_private_keys_report_that_a_passphrase_is_required() {
        let password: Password = "hunter2".into();
        let (cert, _) = CertBuilder::general_purpose(Some("Carol Example <carol@example.com>"))
            .set_password(Some(password))
            .generate()
            .expect("failed to generate password-protected certificate");
        let mut bytes = Vec::new();
        cert.as_tsk()
            .serialize(&mut bytes)
            .expect("failed to serialize protected test certificate");

        assert!(
            ripasso_private_key_requires_passphrase(&bytes)
                .expect("expected password inspection to work")
        );
    }

    #[test]
    fn protected_private_keys_can_be_unlocked_for_ripasso_storage() {
        let password: Password = "hunter2".into();
        let (cert, _) = CertBuilder::general_purpose(Some("Dana Example <dana@example.com>"))
            .set_password(Some(password.clone()))
            .generate()
            .expect("failed to generate password-protected certificate");
        let mut bytes = Vec::new();
        cert.as_tsk()
            .serialize(&mut bytes)
            .expect("failed to serialize protected test certificate");

        let (unlocked, key) = prepare_managed_private_key_bytes(&bytes, Some("hunter2"))
            .expect("expected protected key to unlock successfully");

        assert_eq!(key.fingerprint.len(), 40);
        assert!(unlocked.keys().all(|key| key.key().has_unencrypted_secret()));
    }

    #[test]
    fn imported_private_keys_stay_encrypted_on_disk() {
        let _guard = test_lock().lock().expect("test lock poisoned");
        let _home = TestHome::new();
        let password: Password = "hunter2".into();
        let (cert, _) = CertBuilder::general_purpose(Some("Eve Example <eve@example.com>"))
            .set_password(Some(password.clone()))
            .generate()
            .expect("failed to generate password-protected certificate");
        let mut bytes = Vec::new();
        cert.as_tsk()
            .serialize(&mut bytes)
            .expect("failed to serialize protected test certificate");

        let imported = import_ripasso_private_key_bytes(&bytes, Some("hunter2"))
            .expect("expected private key import to succeed");
        let stored_path = ripasso_keys_dir()
            .expect("expected keys dir")
            .join(imported.fingerprint.to_ascii_lowercase());
        let stored_bytes = fs::read(stored_path).expect("read stored key");
        let (stored_cert, _) =
            parse_managed_private_key_bytes(&stored_bytes).expect("parse stored key");

        assert!(ripasso_private_key_requires_passphrase(&stored_bytes).unwrap());
        assert!(stored_cert.keys().any(|key| !key.key().has_unencrypted_secret()));
        assert!(is_ripasso_private_key_unlocked(&imported.fingerprint).unwrap());
    }

    #[test]
    fn encrypted_private_keys_unlock_for_the_current_session_only() {
        let _guard = test_lock().lock().expect("test lock poisoned");
        let _home = TestHome::new();
        let password: Password = "hunter2".into();
        let (cert, _) = CertBuilder::general_purpose(Some("Frank Example <frank@example.com>"))
            .set_password(Some(password.clone()))
            .generate()
            .expect("failed to generate password-protected certificate");
        let mut bytes = Vec::new();
        cert.as_tsk()
            .serialize(&mut bytes)
            .expect("failed to serialize protected test certificate");

        let imported = import_ripasso_private_key_bytes(&bytes, Some("hunter2"))
            .expect("expected private key import to succeed");
        assert!(ensure_ripasso_private_key_is_ready(&imported.fingerprint).is_ok());

        clear_cached_unlocked_ripasso_private_keys();
        assert!(!is_ripasso_private_key_unlocked(&imported.fingerprint).unwrap());
        assert!(ensure_ripasso_private_key_is_ready(&imported.fingerprint)
            .expect_err("locked key should not be ready")
            .contains("locked"));

        unlock_ripasso_private_key_for_session(&imported.fingerprint, "hunter2")
            .expect("unlock private key for session");
        assert!(is_ripasso_private_key_unlocked(&imported.fingerprint).unwrap());
        assert!(ensure_ripasso_private_key_is_ready(&imported.fingerprint).is_ok());
    }

    #[test]
    fn unprotected_private_keys_are_rejected_for_secure_import() {
        let _guard = test_lock().lock().expect("test lock poisoned");
        let _home = TestHome::new();
        let bytes = cert_bytes("Grace Example <grace@example.com>");

        let err = import_ripasso_private_key_bytes(&bytes, None)
            .expect_err("unprotected private keys should be rejected");

        assert!(err.contains("must be password protected"));
    }

    #[test]
    fn dotted_entry_labels_keep_their_full_name() {
        assert_eq!(
            secret_entry_relative_path("chat/matrix.org").unwrap(),
            PathBuf::from("chat/matrix.org.gpg")
        );
    }

    #[test]
    fn recipients_file_lookup_stays_inside_the_selected_store() {
        let _guard = test_lock().lock().expect("test lock poisoned");
        let _home = TestHome::new();
        let primary_store = PathBuf::from("/tmp/primary-store");
        let secondary_store = PathBuf::from("/tmp/secondary-store");

        fs::create_dir_all(primary_store.join("team")).expect("create primary store");
        fs::create_dir_all(secondary_store.join("team")).expect("create secondary store");
        fs::write(primary_store.join(".gpg-id"), "primary@example.com\n")
            .expect("write primary recipients");
        fs::write(secondary_store.join(".gpg-id"), "secondary@example.com\n")
            .expect("write secondary recipients");

        assert_eq!(
            recipients_file_for_label(secondary_store.to_string_lossy().as_ref(), "team/chat")
                .expect("resolve recipients file"),
            secondary_store.join(".gpg-id")
        );
    }

    #[test]
    fn new_entries_can_be_saved_in_a_secondary_store() {
        let _guard = test_lock().lock().expect("test lock poisoned");
        let home = TestHome::new();
        let password: Password = "hunter2".into();
        let (cert, _) = CertBuilder::general_purpose(Some("Store Example <store@example.com>"))
            .set_password(Some(password.clone()))
            .generate()
            .expect("failed to generate password-protected certificate");
        let mut bytes = Vec::new();
        cert.as_tsk()
            .serialize(&mut bytes)
            .expect("failed to serialize protected test certificate");
        let imported = import_ripasso_private_key_bytes(&bytes, Some("hunter2"))
            .expect("expected private key import to succeed");

        let primary_store = home.path.join("primary-store");
        let secondary_store = home.path.join("secondary-store");
        fs::create_dir_all(&primary_store).expect("create primary store");
        fs::create_dir_all(&secondary_store).expect("create secondary store");
        fs::write(primary_store.join(".gpg-id"), format!("{}\n", imported.fingerprint))
            .expect("write primary recipients");
        fs::write(
            secondary_store.join(".gpg-id"),
            format!("{}\n", imported.fingerprint),
        )
        .expect("write secondary recipients");

        save_password_entry(
            secondary_store.to_string_lossy().as_ref(),
            "team/service",
            "supersecret\nusername: alice",
            true,
        )
        .expect("save entry in secondary store");

        assert!(secondary_store.join("team/service.gpg").is_file());
        assert_eq!(
            read_password_entry(secondary_store.to_string_lossy().as_ref(), "team/service")
                .expect("read saved entry"),
            "supersecret\nusername: alice".to_string()
        );
    }

    #[test]
    fn new_entries_can_use_email_recipients() {
        let _guard = test_lock().lock().expect("test lock poisoned");
        let home = TestHome::new();
        let password: Password = "hunter2".into();
        let (cert, _) = CertBuilder::general_purpose(Some("Store Example <store@example.com>"))
            .set_password(Some(password.clone()))
            .generate()
            .expect("failed to generate password-protected certificate");
        let mut bytes = Vec::new();
        cert.as_tsk()
            .serialize(&mut bytes)
            .expect("failed to serialize protected test certificate");
        let imported = import_ripasso_private_key_bytes(&bytes, Some("hunter2"))
            .expect("expected private key import to succeed");

        let secondary_store = home.path.join("secondary-store");
        fs::create_dir_all(&secondary_store).expect("create secondary store");
        fs::write(
            secondary_store.join(".gpg-id"),
            "store@example.com\n",
        )
        .expect("write recipients");

        save_password_entry(
            secondary_store.to_string_lossy().as_ref(),
            "team/service",
            "supersecret\nusername: alice",
            true,
        )
        .expect("save entry with email recipient");

        assert!(secondary_store.join("team/service.gpg").is_file());
        assert_eq!(
            read_password_entry(secondary_store.to_string_lossy().as_ref(), "team/service")
                .expect("read saved entry"),
            "supersecret\nusername: alice".to_string()
        );
        assert_eq!(imported.fingerprint.len(), 40);
    }

    #[test]
    fn store_recipients_save_can_decrypt_with_a_non_selected_imported_key() {
        let _guard = test_lock().lock().expect("test lock poisoned");
        let home = TestHome::new();
        let password: Password = "hunter2".into();

        let (cert_a, _) = CertBuilder::general_purpose(Some("Key A <a@example.com>"))
            .set_password(Some(password.clone()))
            .generate()
            .expect("generate first certificate");
        let mut bytes_a = Vec::new();
        cert_a
            .as_tsk()
            .serialize(&mut bytes_a)
            .expect("serialize first certificate");
        let key_a = import_ripasso_private_key_bytes(&bytes_a, Some("hunter2"))
            .expect("import first private key");

        let (cert_b, _) = CertBuilder::general_purpose(Some("Key B <b@example.com>"))
            .set_password(Some(password.clone()))
            .generate()
            .expect("generate second certificate");
        let mut bytes_b = Vec::new();
        cert_b
            .as_tsk()
            .serialize(&mut bytes_b)
            .expect("serialize second certificate");
        let key_b = import_ripasso_private_key_bytes(&bytes_b, Some("hunter2"))
            .expect("import second private key");

        let store = home.path.join("secondary-store");
        fs::create_dir_all(&store).expect("create store");
        fs::write(store.join(".gpg-id"), format!("{}\n", key_a.fingerprint))
            .expect("write initial recipients");

        save_password_entry(
            store.to_string_lossy().as_ref(),
            "team/service",
            "supersecret\nusername: alice",
            true,
        )
        .expect("save initial entry");

        Preferences::new()
            .set_ripasso_own_fingerprint(Some(&key_b.fingerprint))
            .expect("select second key");

        save_store_recipients(store.to_string_lossy().as_ref(), std::slice::from_ref(&key_b.fingerprint))
            .expect("re-encrypt store with second key");

        assert_eq!(
            read_password_entry(store.to_string_lossy().as_ref(), "team/service")
                .expect("read re-encrypted entry"),
            "supersecret\nusername: alice".to_string()
        );
    }
}
