mod command;

use self::command::{ensure_success, run_store_command_output, run_store_command_with_input};
use crate::backend::{PasswordEntryError, PasswordEntryWriteError, StoreRecipientsError};
use crate::logging::CommandLogOptions;
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
) -> Result<(), StoreRecipientsError> {
    let output = run_store_command_output(
        store_root,
        "Save password store recipients",
        CommandLogOptions::DEFAULT,
        |cmd| {
            cmd.arg("init").args(recipients);
        },
    )
    .map_err(StoreRecipientsError::from_store_message)?;
    ensure_success(output, "pass init failed")
        .map(|_| ())
        .map_err(StoreRecipientsError::from_store_message)
}

#[cfg(all(test, not(feature = "flatpak")))]
mod tests {
    use super::{save_password_entry, save_store_recipients};
    use crate::backend::test_support::assert_entry_is_encrypted_for_each_recipient;

    #[test]
    fn host_backend_encrypts_entries_for_all_store_recipients() {
        assert_entry_is_encrypted_for_each_recipient(
            |store_root, recipients| {
                save_store_recipients(store_root, recipients).map_err(|err| err.to_string())
            },
            |store_root, label, contents| {
                save_password_entry(store_root, label, contents, true)
                    .map_err(|err| err.to_string())
            },
        );
    }
}
