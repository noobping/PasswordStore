use super::super::keys::{
    armored_ripasso_private_key, clear_cached_unlocked_ripasso_private_keys,
    ensure_ripasso_private_key_is_ready, generate_ripasso_private_key,
    import_ripasso_private_key_bytes, is_ripasso_private_key_unlocked, list_ripasso_private_keys,
    parse_managed_private_key_bytes, prepare_managed_private_key_bytes, remove_ripasso_private_key,
    resolved_ripasso_own_fingerprint, ripasso_keys_dir, ripasso_private_key_requires_passphrase,
    unlock_ripasso_private_key_for_session,
};
use super::entries::{
    delete_password_entry, read_password_entry, rename_password_entry, save_password_entry,
};
use super::git::{
    git_commit_private_key_requiring_unlock_for_entry,
    git_commit_private_key_requiring_unlock_for_store_recipients,
};
use super::paths::{recipients_file_for_label, secret_entry_relative_path};
use super::store::save_store_recipients;
use crate::backend::{
    test_support::SystemBackendTestEnv, PasswordEntryError, PasswordEntryWriteError,
    PrivateKeyError, StoreRecipientsError,
};
use crate::preferences::Preferences;
use crate::support::git::has_git_repository;
use sequoia_openpgp::{cert::CertBuilder, crypto::Password, parse::Parse, serialize::Serialize};
use std::fs;
use std::path::PathBuf;

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

fn protected_cert(email: &str) -> (sequoia_openpgp::Cert, Vec<u8>) {
    let password: Password = "hunter2".into();
    let (cert, _) = CertBuilder::general_purpose(Some(email))
        .set_password(Some(password))
        .generate()
        .expect("failed to generate password-protected certificate");
    let mut bytes = Vec::new();
    cert.as_tsk()
        .serialize(&mut bytes)
        .expect("failed to serialize protected test certificate");
    (cert, bytes)
}

fn protected_cert_bytes(email: &str) -> Vec<u8> {
    protected_cert(email).1
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
    assert!(matches!(err, PrivateKeyError::MissingPrivateKeyMaterial(_)));
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
fn generated_private_keys_are_stored_and_listed() {
    let env = SystemBackendTestEnv::new();
    env.activate_profile("generated-key");

    let key = generate_ripasso_private_key("Generated User", "generated@example.com", "hunter2")
        .expect("generate private key");

    assert!(is_ripasso_private_key_unlocked(&key.fingerprint).expect("inspect unlocked state"));
    assert!(key
        .user_ids
        .iter()
        .any(|user_id| user_id.contains("Generated User <generated@example.com>")));
    assert!(list_ripasso_private_keys()
        .expect("list generated keys")
        .into_iter()
        .any(|stored| stored.fingerprint == key.fingerprint));
}

#[test]
fn armored_private_keys_can_be_exported() {
    let env = SystemBackendTestEnv::new();
    env.activate_profile("exported-key");

    let key = generate_ripasso_private_key("Export User", "export@example.com", "hunter2")
        .expect("generate private key");
    let armored = armored_ripasso_private_key(&key.fingerprint).expect("export armored key");
    let parsed = sequoia_openpgp::Cert::from_bytes(armored.as_bytes()).expect("parse armored key");

    assert!(armored.starts_with("-----BEGIN PGP PRIVATE KEY BLOCK-----"));
    assert_eq!(parsed.fingerprint().to_hex(), key.fingerprint);
}

#[test]
fn imported_private_keys_stay_encrypted_on_disk() {
    let _env = SystemBackendTestEnv::new();
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
    let _env = SystemBackendTestEnv::new();
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
    assert!(matches!(
        ensure_ripasso_private_key_is_ready(&imported.fingerprint)
            .expect_err("locked key should not be ready"),
        PasswordEntryError::LockedPrivateKey(_)
    ));

    unlock_ripasso_private_key_for_session(&imported.fingerprint, "hunter2")
        .expect("unlock private key for session");
    assert!(is_ripasso_private_key_unlocked(&imported.fingerprint).unwrap());
    assert!(ensure_ripasso_private_key_is_ready(&imported.fingerprint).is_ok());
}

#[test]
fn unprotected_private_keys_are_rejected_for_secure_import() {
    let _env = SystemBackendTestEnv::new();
    let bytes = cert_bytes("Grace Example <grace@example.com>");

    let err = import_ripasso_private_key_bytes(&bytes, None)
        .expect_err("unprotected private keys should be rejected");

    assert!(matches!(
        err,
        PrivateKeyError::RequiresPasswordProtection(_)
    ));
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
    let env = SystemBackendTestEnv::new();
    let primary_store = env.root_dir().join("primary-store");
    let secondary_store = env.root_dir().join("secondary-store");

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
    let env = SystemBackendTestEnv::new();
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

    let primary_store = env.root_dir().join("primary-store");
    let secondary_store = env.root_dir().join("secondary-store");
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
fn duplicate_entry_saves_are_classified_as_already_existing() {
    let env = SystemBackendTestEnv::new();
    let bytes = protected_cert_bytes("Store Example <store@example.com>");
    let imported = import_ripasso_private_key_bytes(&bytes, Some("hunter2"))
        .expect("expected private key import to succeed");

    let store = env.root_dir().join("secondary-store");
    fs::create_dir_all(&store).expect("create secondary store");
    fs::write(store.join(".gpg-id"), format!("{}\n", imported.fingerprint))
        .expect("write recipients");

    save_password_entry(
        store.to_string_lossy().as_ref(),
        "team/service",
        "supersecret\nusername: alice",
        true,
    )
    .expect("save initial entry");

    let err = save_password_entry(
        store.to_string_lossy().as_ref(),
        "team/service",
        "supersecret\nusername: alice",
        false,
    )
    .expect_err("duplicate save should be rejected");

    assert!(matches!(
        err,
        PasswordEntryWriteError::EntryAlreadyExists(_)
    ));
}

#[test]
fn entries_are_encrypted_for_all_selected_private_keys() {
    let env = SystemBackendTestEnv::new();
    let bytes_a = protected_cert_bytes("Key A <a@example.com>");
    let bytes_b = protected_cert_bytes("Key B <b@example.com>");
    let key_a = import_ripasso_private_key_bytes(&bytes_a, Some("hunter2"))
        .expect("import first private key");
    let key_b = import_ripasso_private_key_bytes(&bytes_b, Some("hunter2"))
        .expect("import second private key");

    let store = env.root_dir().join("secondary-store");
    fs::create_dir_all(&store).expect("create secondary store");
    fs::write(
        store.join(".gpg-id"),
        format!("{}\n{}\n", key_a.fingerprint, key_b.fingerprint),
    )
    .expect("write recipients");

    save_password_entry(
        store.to_string_lossy().as_ref(),
        "team/service",
        "supersecret\nusername: alice",
        true,
    )
    .expect("save entry for multiple recipients");

    remove_ripasso_private_key(&key_b.fingerprint).expect("remove second key");
    assert_eq!(
        read_password_entry(store.to_string_lossy().as_ref(), "team/service")
            .expect("read entry with first key only"),
        "supersecret\nusername: alice".to_string()
    );

    import_ripasso_private_key_bytes(&bytes_b, Some("hunter2")).expect("re-import second key");
    remove_ripasso_private_key(&key_a.fingerprint).expect("remove first key");
    assert_eq!(
        read_password_entry(store.to_string_lossy().as_ref(), "team/service")
            .expect("read entry with second key only"),
        "supersecret\nusername: alice".to_string()
    );
}

#[test]
fn missing_entry_renames_and_deletes_are_classified() {
    let env = SystemBackendTestEnv::new();
    let store = env.root_dir().join("secondary-store");
    fs::create_dir_all(&store).expect("create secondary store");

    let rename_err = rename_password_entry(
        store.to_string_lossy().as_ref(),
        "team/missing",
        "team/renamed",
    )
    .expect_err("missing rename should fail");
    assert!(matches!(
        rename_err,
        PasswordEntryWriteError::EntryNotFound(_)
    ));

    let delete_err = delete_password_entry(store.to_string_lossy().as_ref(), "team/missing")
        .expect_err("missing delete should fail");
    assert!(matches!(
        delete_err,
        PasswordEntryWriteError::EntryNotFound(_)
    ));
}

#[test]
fn recipient_saves_reject_non_directory_store_paths() {
    let env = SystemBackendTestEnv::new();
    let file_path = env.root_dir().join("store-file");
    fs::write(&file_path, "not a directory").expect("write store placeholder file");

    let err = save_store_recipients(
        file_path.to_string_lossy().as_ref(),
        &[String::from("alice@example.com")],
    )
    .expect_err("non-directory store paths should fail");

    assert!(matches!(err, StoreRecipientsError::InvalidStorePath(_)));
}

#[test]
fn recipient_saves_initialize_git_for_new_stores() {
    let env = SystemBackendTestEnv::new();
    let bytes = protected_cert_bytes("Store Example <store@example.com>");
    let imported = import_ripasso_private_key_bytes(&bytes, Some("hunter2"))
        .expect("expected private key import to succeed");

    let store = env.root_dir().join("secondary-store");
    save_store_recipients(
        store.to_string_lossy().as_ref(),
        std::slice::from_ref(&imported.fingerprint),
    )
    .expect("save recipients for a new store");

    assert!(has_git_repository(store.to_string_lossy().as_ref()));
}

#[test]
fn new_entries_can_use_email_recipients() {
    let env = SystemBackendTestEnv::new();
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

    let secondary_store = env.root_dir().join("secondary-store");
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
    let env = SystemBackendTestEnv::new();
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

    let store = env.root_dir().join("secondary-store");
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
    let env = SystemBackendTestEnv::new();
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

    let store = env.root_dir().join("secondary-store");
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

#[test]
fn store_recipients_save_can_remove_the_selected_private_key_from_recipients() {
    let env = SystemBackendTestEnv::new();
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

    let store = env.root_dir().join("secondary-store");
    fs::create_dir_all(&store).expect("create store");
    fs::write(
        store.join(".gpg-id"),
        format!("{}\n{}\n", key_a.fingerprint, key_b.fingerprint),
    )
    .expect("write initial recipients");

    save_password_entry(
        store.to_string_lossy().as_ref(),
        "team/service",
        "supersecret\nusername: alice",
        true,
    )
    .expect("save initial entry");

    Preferences::new()
        .set_ripasso_own_fingerprint(Some(&key_a.fingerprint))
        .expect("select first key");

    save_store_recipients(
        store.to_string_lossy().as_ref(),
        std::slice::from_ref(&key_b.fingerprint),
    )
    .expect("re-encrypt store without the selected key");

    assert_eq!(
        read_password_entry(store.to_string_lossy().as_ref(), "team/service")
            .expect("read re-encrypted entry"),
        "supersecret\nusername: alice".to_string()
    );
}

#[test]
fn flatpak_backend_commits_git_backed_store_changes_with_private_key_identity() {
    let env = SystemBackendTestEnv::new();
    let (cert, bytes) = protected_cert("Git Signer <git-flatpak@example.com>");
    let imported =
        import_ripasso_private_key_bytes(&bytes, Some("hunter2")).expect("import private key");
    Preferences::new()
        .set_ripasso_own_fingerprint(Some(&imported.fingerprint))
        .expect("select signing key");

    let mut public_bytes = Vec::new();
    cert.serialize(&mut public_bytes)
        .expect("serialize public certificate");
    env.import_public_key(&public_bytes)
        .expect("import public key for signature verification");
    env.init_store_git_repository()
        .expect("initialize git repository");
    let store_root = env.store_root().to_string_lossy().to_string();

    save_store_recipients(&store_root, std::slice::from_ref(&imported.fingerprint))
        .expect("save store recipients");
    save_password_entry(
        &store_root,
        "team/service",
        "secret-value\nusername: alice",
        true,
    )
    .expect("save password entry");

    let subjects = env
        .store_git_commit_subjects()
        .expect("read commit subjects");
    assert_eq!(subjects.len(), 2);
    assert_eq!(subjects[0], "Add password for team/service");
    assert_eq!(subjects[1], "Update password store recipients");
    assert_eq!(
        env.store_git_head_author().expect("read head author"),
        "Git Signer <git-flatpak@example.com>"
    );
    assert!(env
        .store_head_commit_has_signature()
        .expect("inspect commit headers"));
    env.verify_store_head_commit_signature()
        .expect("verify head commit signature");
}

#[test]
fn flatpak_backend_commits_with_the_entry_private_key_instead_of_an_unrelated_selected_key() {
    let env = SystemBackendTestEnv::new();
    let (cert_a, bytes_a) = protected_cert("Entry Key <entry@example.com>");
    let imported_a =
        import_ripasso_private_key_bytes(&bytes_a, Some("hunter2")).expect("import entry key");
    let (cert_b, bytes_b) = protected_cert("Selected Key <selected@example.com>");
    let imported_b = import_ripasso_private_key_bytes(&bytes_b, Some("hunter2"))
        .expect("import unrelated selected key");
    Preferences::new()
        .set_ripasso_own_fingerprint(Some(&imported_b.fingerprint))
        .expect("select unrelated key");

    let mut public_bytes_a = Vec::new();
    cert_a
        .serialize(&mut public_bytes_a)
        .expect("serialize entry public certificate");
    env.import_public_key(&public_bytes_a)
        .expect("import entry public key for signature verification");

    let mut public_bytes_b = Vec::new();
    cert_b
        .serialize(&mut public_bytes_b)
        .expect("serialize selected public certificate");
    env.import_public_key(&public_bytes_b)
        .expect("import unrelated selected public key for signature verification");
    env.init_store_git_repository()
        .expect("initialize git repository");
    let store_root = env.store_root().to_string_lossy().to_string();

    save_store_recipients(&store_root, std::slice::from_ref(&imported_a.fingerprint))
        .expect("save store recipients");
    save_password_entry(
        &store_root,
        "team/service",
        "secret-value\nusername: alice",
        true,
    )
    .expect("save password entry");

    let subjects = env
        .store_git_commit_subjects()
        .expect("read commit subjects");
    assert_eq!(subjects.len(), 2);
    assert_eq!(
        env.store_git_head_author().expect("read head author"),
        "Entry Key <entry@example.com>"
    );
    assert!(env
        .store_head_commit_has_signature()
        .expect("inspect commit headers"));
    env.verify_store_head_commit_signature()
        .expect("verify head commit signature");
}

#[test]
fn flatpak_backend_commits_without_signature_when_private_key_is_locked() {
    let env = SystemBackendTestEnv::new();
    let bytes = protected_cert_bytes("Locked Signer <locked-flatpak@example.com>");
    let imported =
        import_ripasso_private_key_bytes(&bytes, Some("hunter2")).expect("import private key");
    Preferences::new()
        .set_ripasso_own_fingerprint(Some(&imported.fingerprint))
        .expect("select signing key");
    clear_cached_unlocked_ripasso_private_keys();
    env.init_store_git_repository()
        .expect("initialize git repository");
    let store_root = env.store_root().to_string_lossy().to_string();

    save_store_recipients(&store_root, std::slice::from_ref(&imported.fingerprint))
        .expect("save store recipients");
    save_password_entry(
        &store_root,
        "team/service",
        "secret-value\nusername: alice",
        true,
    )
    .expect("save password entry");

    let subjects = env
        .store_git_commit_subjects()
        .expect("read commit subjects");
    assert_eq!(subjects.len(), 2);
    assert_eq!(
        env.store_git_head_author().expect("read head author"),
        "Locked Signer <locked-flatpak@example.com>"
    );
    assert!(!env
        .store_head_commit_has_signature()
        .expect("inspect commit headers"));
}

#[test]
fn git_commit_unlock_helper_detects_a_locked_entry_signing_key() {
    let env = SystemBackendTestEnv::new();
    let bytes = protected_cert_bytes("Locked Signer <locked-entry@example.com>");
    let imported =
        import_ripasso_private_key_bytes(&bytes, Some("hunter2")).expect("import private key");
    env.init_store_git_repository()
        .expect("initialize git repository");
    let store_root = env.store_root().to_string_lossy().to_string();

    save_store_recipients(&store_root, std::slice::from_ref(&imported.fingerprint))
        .expect("save store recipients");
    save_password_entry(
        &store_root,
        "team/service",
        "secret-value\nusername: alice",
        true,
    )
    .expect("save password entry");
    clear_cached_unlocked_ripasso_private_keys();

    assert_eq!(
        git_commit_private_key_requiring_unlock_for_entry(&store_root, "team/service",)
            .expect("resolve locked signing key"),
        Some(imported.fingerprint)
    );
}

#[test]
fn git_commit_unlock_helper_detects_a_locked_recipients_signing_key() {
    let env = SystemBackendTestEnv::new();
    let bytes = protected_cert_bytes("Locked Signer <locked-store@example.com>");
    let imported =
        import_ripasso_private_key_bytes(&bytes, Some("hunter2")).expect("import private key");
    env.init_store_git_repository()
        .expect("initialize git repository");
    clear_cached_unlocked_ripasso_private_keys();
    let store_root = env.store_root().to_string_lossy().to_string();

    assert_eq!(
        git_commit_private_key_requiring_unlock_for_store_recipients(
            &store_root,
            std::slice::from_ref(&imported.fingerprint),
        )
        .expect("resolve locked signing key"),
        Some(imported.fingerprint)
    );
}
