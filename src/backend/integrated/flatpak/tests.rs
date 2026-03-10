use super::super::keys::{
    clear_cached_unlocked_ripasso_private_keys, ensure_ripasso_private_key_is_ready,
    import_ripasso_private_key_bytes, is_ripasso_private_key_unlocked,
    parse_managed_private_key_bytes, prepare_managed_private_key_bytes,
    resolved_ripasso_own_fingerprint, ripasso_keys_dir, ripasso_private_key_requires_passphrase,
    unlock_ripasso_private_key_for_session,
};
use super::entries::{read_password_entry, save_password_entry};
use super::paths::{recipients_file_for_label, secret_entry_relative_path};
use super::store::save_store_recipients;
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
    path: PathBuf,
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

    assert!(ripasso_private_key_requires_passphrase(&bytes)
        .expect("expected password inspection to work"));
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
    assert!(unlocked
        .keys()
        .all(|key| key.key().has_unencrypted_secret()));
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
    assert!(stored_cert
        .keys()
        .any(|key| !key.key().has_unencrypted_secret()));
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
    fs::write(
        primary_store.join(".gpg-id"),
        format!("{}\n", imported.fingerprint),
    )
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
