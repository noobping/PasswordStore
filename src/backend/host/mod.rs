mod command;

use self::command::{ensure_success, run_store_command_output, run_store_command_with_input};
use crate::backend::{
    PasswordEntryError, PasswordEntryWriteError, StoreRecipientsError,
    StoreRecipientsPrivateKeyRequirement,
};
use crate::logging::CommandLogOptions;
use crate::support::git::{ensure_store_git_repository, has_git_repository};
use std::path::Path;
use std::process::Output;

fn read_entry_output(store_root: &str, label: &str, action: &str) -> Result<Output, String> {
    let output =
        run_store_command_output(store_root, action, CommandLogOptions::SENSITIVE, |cmd| {
            cmd.arg(label);
        })?;
    ensure_success(output, "pass failed")
}

pub(super) fn read_password_entry(
    store_root: &str,
    label: &str,
) -> Result<String, PasswordEntryError> {
    let output = read_entry_output(store_root, label, "Read password entry")
        .map_err(PasswordEntryError::from_store_message)?;
    Ok(String::from_utf8_lossy(&output.stdout).to_string())
}

pub(super) fn read_password_line(
    store_root: &str,
    label: &str,
) -> Result<String, PasswordEntryError> {
    let output = read_entry_output(store_root, label, "Read password entry for clipboard copy")
        .map_err(PasswordEntryError::from_store_message)?;
    Ok(String::from_utf8_lossy(&output.stdout)
        .lines()
        .next()
        .unwrap_or_default()
        .to_string())
}

pub(super) fn save_password_entry(
    store_root: &str,
    label: &str,
    contents: &str,
    overwrite: bool,
) -> Result<(), PasswordEntryWriteError> {
    let output = run_store_command_with_input(
        store_root,
        "Save password entry",
        contents,
        CommandLogOptions::SENSITIVE,
        |cmd| {
            cmd.arg("insert").arg("-m");
            if overwrite {
                cmd.arg("-f");
            }
            cmd.arg(label);
        },
    )
    .map_err(PasswordEntryWriteError::from_store_message)?;
    ensure_success(output, "pass insert failed")
        .map(|_| ())
        .map_err(PasswordEntryWriteError::from_store_message)
}

pub(super) fn rename_password_entry(
    store_root: &str,
    old_label: &str,
    new_label: &str,
) -> Result<(), PasswordEntryWriteError> {
    let output = run_store_command_output(
        store_root,
        "Rename password entry",
        CommandLogOptions::DEFAULT,
        |cmd| {
            cmd.arg("mv").arg(old_label).arg(new_label);
        },
    )
    .map_err(PasswordEntryWriteError::from_store_message)?;
    ensure_success(output, "pass mv failed")
        .map(|_| ())
        .map_err(PasswordEntryWriteError::from_store_message)
}

pub(super) fn delete_password_entry(
    store_root: &str,
    label: &str,
) -> Result<(), PasswordEntryWriteError> {
    let output = run_store_command_output(
        store_root,
        "Delete password entry",
        CommandLogOptions::DEFAULT,
        |cmd| {
            cmd.arg("rm").arg("-rf").arg(label);
        },
    )
    .map_err(PasswordEntryWriteError::from_store_message)?;
    ensure_success(output, "pass rm failed")
        .map(|_| ())
        .map_err(PasswordEntryWriteError::from_store_message)
}

pub(super) fn save_store_recipients(
    store_root: &str,
    recipients: &[String],
    _private_key_requirement: StoreRecipientsPrivateKeyRequirement,
) -> Result<(), StoreRecipientsError> {
    let should_initialize_git =
        !Path::new(store_root).join(".gpg-id").exists() && !has_git_repository(store_root);
    let output = run_store_command_output(
        store_root,
        "Save password store recipients",
        CommandLogOptions::DEFAULT,
        |cmd| {
            cmd.arg("init").args(recipients);
        },
    )
    .map_err(StoreRecipientsError::from_store_message)?;
    ensure_success(output, "pass init failed").map_err(StoreRecipientsError::from_store_message)?;
    if should_initialize_git {
        ensure_store_git_repository(store_root)
            .map_err(StoreRecipientsError::from_store_message)?;
    }
    Ok(())
}

#[cfg(all(test, keycord_standard_linux))]
mod tests {
    use super::{save_password_entry, save_store_recipients};
    use crate::backend::test_support::assert_entry_is_encrypted_for_each_recipient;
    use crate::backend::test_support::SystemBackendTestEnv;
    use crate::backend::StoreRecipientsPrivateKeyRequirement;
    use crate::support::git::has_git_repository;

    #[test]
    fn host_backend_encrypts_entries_for_all_store_recipients() {
        assert_entry_is_encrypted_for_each_recipient(
            |store_root, recipients| {
                save_store_recipients(
                    store_root,
                    recipients,
                    StoreRecipientsPrivateKeyRequirement::AnyManagedKey,
                )
                .map_err(|err| err.to_string())
            },
            |store_root, label, contents| {
                save_password_entry(store_root, label, contents, true)
                    .map_err(|err| err.to_string())
            },
        );
    }

    #[test]
    fn host_backend_initializes_git_for_new_stores() {
        let env = SystemBackendTestEnv::new();

        let key = env
            .generate_secret_key("Recipient <host-create@example.com>")
            .expect("generate host recipient key");
        env.import_public_key(&key.public_key_bytes)
            .expect("import host recipient key");
        env.trust_public_key(&key.fingerprint_hex)
            .expect("trust host recipient key");

        let store_root = env.store_root().to_string_lossy().to_string();
        save_store_recipients(
            &store_root,
            std::slice::from_ref(&key.fingerprint_hex),
            StoreRecipientsPrivateKeyRequirement::AnyManagedKey,
        )
        .expect("save store recipients");

        assert!(has_git_repository(&store_root));
    }
}
