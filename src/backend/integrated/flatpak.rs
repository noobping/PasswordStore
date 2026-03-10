use ripasso::crypto::{Crypto, Sequoia};
use ripasso::pass::{Comment, KeyRingStatus, OwnerTrustLevel, Recipient};
use sequoia_openpgp::{Cert, KeyHandle};
use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::{Component, Path, PathBuf};
use std::sync::Arc;
use walkdir::WalkDir;

pub use super::keys::{
    import_ripasso_private_key_bytes, is_ripasso_private_key_unlocked, list_ripasso_private_keys,
    ManagedRipassoPrivateKey, remove_ripasso_private_key, ripasso_private_key_requires_passphrase,
    ripasso_private_key_requires_session_unlock, ripasso_private_key_title,
    unlock_ripasso_private_key_for_session,
};
use super::keys::{
    available_unlocked_private_key_fingerprints, build_ripasso_crypto_from_key_ring,
    ensure_ripasso_private_key_is_ready, fingerprint_from_string, imported_private_key_fingerprints,
    incompatible_private_key_error, load_ripasso_key_ring, load_stored_ripasso_key_ring,
    locked_private_key_error, missing_private_key_error, selected_ripasso_own_fingerprint,
};
#[cfg(test)]
use super::keys::{
    clear_cached_unlocked_ripasso_private_keys, parse_managed_private_key_bytes,
    prepare_managed_private_key_bytes, ripasso_keys_dir,
};

#[cfg_attr(not(test), allow(dead_code))]
pub(crate) fn resolved_ripasso_own_fingerprint() -> Result<String, String> {
    super::keys::resolved_ripasso_own_fingerprint()
}

struct FlatpakCryptoContext {
    key_ring: HashMap<[u8; 20], Arc<Cert>>,
    crypto: Sequoia,
}

impl FlatpakCryptoContext {
    fn load_for_fingerprint(fingerprint: &str) -> Result<Self, String> {
        let key_ring = load_ripasso_key_ring(fingerprint)?;
        let crypto = build_ripasso_crypto_from_key_ring(fingerprint, key_ring.clone())?;
        Ok(Self { key_ring, crypto })
    }

    fn load_for_label(store_root: &str, label: &str) -> Result<Self, String> {
        let recipients_file = recipients_file_for_label(store_root, label)?;
        Self::load_for_recipients_file(&recipients_file)
    }

    fn load_for_recipients_file(recipients_file: &Path) -> Result<Self, String> {
        let contents = fs::read_to_string(recipients_file).map_err(|err| err.to_string())?;
        Self::load_for_recipient_contents(&contents)
    }

    fn load_for_recipient_contents(contents: &str) -> Result<Self, String> {
        let key_ring = load_stored_ripasso_key_ring()?;
        let fingerprint = preferred_context_fingerprint_from_contents(contents, &key_ring)?;
        let crypto = build_ripasso_crypto_from_key_ring(&fingerprint, key_ring.clone())?;
        Ok(Self { key_ring, crypto })
    }

    fn decrypt_entry(&self, entry_path: &Path) -> Result<String, String> {
        decrypt_password_entry_with_crypto(&self.crypto, entry_path)
    }

    fn encrypt_contents_for_label(
        &self,
        store_root: &str,
        label: &str,
        contents: &str,
    ) -> Result<Vec<u8>, String> {
        let recipients_file = recipients_file_for_label(store_root, label)?;
        let recipients = recipients_for_encryption(&recipients_file, &self.key_ring)?;
        encrypt_password_entry_with_crypto(&self.crypto, &recipients, contents)
    }
}

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

fn secret_entry_relative_path(label: &str) -> Result<PathBuf, String> {
    let mut relative = validated_entry_label_path(label)?;
    let file_name = relative
        .file_name()
        .and_then(|value| value.to_str())
        .ok_or_else(|| "Invalid password entry path.".to_string())?;
    relative.set_file_name(format!("{file_name}.gpg"));
    Ok(relative)
}

fn entry_file_path(store_root: &str, label: &str) -> Result<PathBuf, String> {
    let mut path = PathBuf::from(store_root);
    path.push(secret_entry_relative_path(label)?);
    Ok(path)
}

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

fn decrypt_password_entry_with_any_available_key(
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

fn recipients_for_encryption(
    recipients_file: &Path,
    key_ring: &HashMap<[u8; 20], Arc<Cert>>,
) -> Result<Vec<Recipient>, String> {
    let contents = fs::read_to_string(recipients_file).map_err(|err| err.to_string())?;
    let mut recipients = Vec::new();

    for recipient in resolved_recipients_from_contents(&contents, key_ring)? {
        let name = recipient
            .cert
            .userids()
            .map(|user_id| user_id.userid().to_string())
            .find(|value| !value.trim().is_empty())
            .unwrap_or_else(|| recipient.requested_id.clone());

        recipients.push(Recipient {
            name,
            comment: Comment {
                pre_comment: None,
                post_comment: None,
            },
            key_id: recipient.cert.fingerprint().to_hex(),
            fingerprint: Some(recipient.fingerprint),
            key_ring_status: KeyRingStatus::InKeyRing,
            trust_level: OwnerTrustLevel::Ultimate,
            not_usable: false,
        });
    }

    Ok(recipients)
}

struct ResolvedRecipient<'a> {
    fingerprint: [u8; 20],
    cert: &'a Arc<Cert>,
    requested_id: String,
}

impl ResolvedRecipient<'_> {
    fn fingerprint_hex(&self) -> String {
        self.cert.fingerprint().to_hex()
    }
}

fn resolved_recipients_from_contents<'a>(
    contents: &str,
    key_ring: &'a HashMap<[u8; 20], Arc<Cert>>,
) -> Result<Vec<ResolvedRecipient<'a>>, String> {
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
            return Err(format!("Recipient '{line}' is not available in the app."));
        };
        if !seen.insert(fingerprint) {
            continue;
        }

        recipients.push(ResolvedRecipient {
            fingerprint,
            cert,
            requested_id: line.to_string(),
        });
    }

    Ok(recipients)
}

fn preferred_context_fingerprint_from_contents(
    contents: &str,
    key_ring: &HashMap<[u8; 20], Arc<Cert>>,
) -> Result<String, String> {
    resolved_recipients_from_contents(contents, key_ring)?
        .into_iter()
        .next()
        .map(|recipient| recipient.fingerprint_hex())
        .ok_or_else(|| "No recipients were found for this password entry.".to_string())
}

fn ensure_store_directory(store_root: &str) -> Result<PathBuf, String> {
    let store_dir = PathBuf::from(store_root);
    if store_dir.exists() {
        if !store_dir.is_dir() {
            return Err("The selected password store path is not a folder.".to_string());
        }
    } else {
        fs::create_dir_all(&store_dir).map_err(|err| err.to_string())?;
    }
    Ok(store_dir)
}

fn with_updated_recipients_file<T>(
    recipients_path: &Path,
    recipients: &[String],
    f: impl FnOnce() -> Result<T, String>,
) -> Result<T, String> {
    let previous_contents = fs::read_to_string(recipients_path).ok();
    let contents = format!("{}\n", recipients.join("\n"));
    fs::write(recipients_path, contents).map_err(|err| err.to_string())?;

    match f() {
        Ok(value) => Ok(value),
        Err(err) => {
            match previous_contents {
                Some(previous) => {
                    let _ = fs::write(recipients_path, previous);
                }
                None => {
                    let _ = fs::remove_file(recipients_path);
                }
            }
            Err(err)
        }
    }
}

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

fn push_unique_fingerprint(fingerprints: &mut Vec<String>, candidate: String) {
    if fingerprints
        .iter()
        .any(|existing| existing.eq_ignore_ascii_case(&candidate))
    {
        return;
    }

    fingerprints.push(candidate);
}

fn recipient_fingerprints_for_label(store_root: &str, label: &str) -> Result<Vec<String>, String> {
    let recipients_file = recipients_file_for_label(store_root, label)?;
    let contents = fs::read_to_string(recipients_file).map_err(|err| err.to_string())?;
    let key_ring = load_stored_ripasso_key_ring()?;

    Ok(resolved_recipients_from_contents(&contents, &key_ring)?
        .into_iter()
        .map(|recipient| recipient.fingerprint_hex())
        .collect())
}

fn decryption_candidate_fingerprints_for_entry(
    store_root: &str,
    label: &str,
) -> Result<Vec<String>, String> {
    let mut candidates = Vec::new();

    if let Ok(fingerprints) = recipient_fingerprints_for_label(store_root, label) {
        for fingerprint in fingerprints {
            push_unique_fingerprint(&mut candidates, fingerprint);
        }
    }

    if let Some(fingerprint) = selected_ripasso_own_fingerprint()? {
        push_unique_fingerprint(&mut candidates, fingerprint);
    }

    for fingerprint in imported_private_key_fingerprints()? {
        push_unique_fingerprint(&mut candidates, fingerprint);
    }

    Ok(candidates)
}

pub fn preferred_ripasso_private_key_fingerprint_for_entry(
    store_root: &str,
    label: &str,
) -> Result<String, String> {
    decryption_candidate_fingerprints_for_entry(store_root, label)?
        .into_iter()
        .next()
        .ok_or_else(missing_private_key_error)
}

pub(crate) fn read_password_entry(store_root: &str, label: &str) -> Result<String, String> {
    let entry_path = entry_file_path(store_root, label)?;
    let mut saw_locked_key = false;
    let mut saw_incompatible_key = false;
    let mut last_error = None;

    for fingerprint in decryption_candidate_fingerprints_for_entry(store_root, label)? {
        match ensure_ripasso_private_key_is_ready(&fingerprint) {
            Ok(()) => {}
            Err(err) if err.contains(&locked_private_key_error()) => {
                saw_locked_key = true;
                continue;
            }
            Err(err) if err.contains(&incompatible_private_key_error()) => {
                saw_incompatible_key = true;
                last_error = Some(err);
                continue;
            }
            Err(err) => {
                last_error = Some(err);
                continue;
            }
        }

        match FlatpakCryptoContext::load_for_fingerprint(&fingerprint)
            .and_then(|context| context.decrypt_entry(&entry_path))
        {
            Ok(secret) => return Ok(secret),
            Err(err) => last_error = Some(err),
        }
    }

    if saw_locked_key {
        return Err(locked_private_key_error());
    }
    if saw_incompatible_key {
        return Err(incompatible_private_key_error());
    }

    Err(last_error.unwrap_or_else(missing_private_key_error))
}

pub(crate) fn read_password_line(store_root: &str, label: &str) -> Result<String, String> {
    let secret = read_password_entry(store_root, label)?;
    Ok(secret.lines().next().unwrap_or_default().to_string())
}

pub(crate) fn save_password_entry(
    store_root: &str,
    label: &str,
    contents: &str,
    overwrite: bool,
) -> Result<(), String> {
    let entry_path = entry_file_path(store_root, label)?;
    if entry_path.exists() && !overwrite {
        return Err("That password entry already exists.".to_string());
    }

    let context = FlatpakCryptoContext::load_for_label(store_root, label)?;
    let ciphertext = context.encrypt_contents_for_label(store_root, label, contents)?;
    if let Some(parent) = entry_path.parent() {
        fs::create_dir_all(parent).map_err(|err| err.to_string())?;
    }
    fs::write(entry_path, ciphertext).map_err(|err| err.to_string())
}

pub(crate) fn rename_password_entry(
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

pub(crate) fn delete_password_entry(store_root: &str, label: &str) -> Result<(), String> {
    let entry_path = entry_file_path(store_root, label)?;
    fs::remove_file(&entry_path).map_err(|err| err.to_string())?;
    cleanup_empty_store_dirs(store_root, &entry_path)
}

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

#[cfg(test)]
mod tests {
    use super::{
        clear_cached_unlocked_ripasso_private_keys, ensure_ripasso_private_key_is_ready,
        import_ripasso_private_key_bytes, is_ripasso_private_key_unlocked,
        parse_managed_private_key_bytes, prepare_managed_private_key_bytes, read_password_entry,
        recipients_file_for_label, resolved_ripasso_own_fingerprint,
        ripasso_private_key_requires_passphrase, ripasso_keys_dir, save_password_entry,
        save_store_recipients, secret_entry_relative_path, unlock_ripasso_private_key_for_session,
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
        fs::write(secondary_store.join(".gpg-id"), "store@example.com\n").expect("write recipients");

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
    fn store_recipients_work_without_a_selected_default_key() {
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

        let store = home.path.join("secondary-store");
        fs::create_dir_all(&store).expect("create store");
        fs::write(store.join(".gpg-id"), format!("{}\n", imported.fingerprint))
            .expect("write recipients");

        Preferences::new()
            .set_ripasso_own_fingerprint(None)
            .expect("clear selected fingerprint");
        assert!(resolved_ripasso_own_fingerprint().is_err());

        save_password_entry(
            store.to_string_lossy().as_ref(),
            "team/service",
            "supersecret\nusername: alice",
            true,
        )
        .expect("save entry with store recipients only");

        assert_eq!(
            read_password_entry(store.to_string_lossy().as_ref(), "team/service")
                .expect("read saved entry"),
            "supersecret\nusername: alice".to_string()
        );
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

        save_store_recipients(
            store.to_string_lossy().as_ref(),
            std::slice::from_ref(&key_b.fingerprint),
        )
        .expect("re-encrypt store with second key");

        assert_eq!(
            read_password_entry(store.to_string_lossy().as_ref(), "team/service")
                .expect("read re-encrypted entry"),
            "supersecret\nusername: alice".to_string()
        );
    }
}
