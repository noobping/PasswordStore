use crate::backend::{PasswordEntryError, PasswordEntryWriteError, StoreRecipientsError};
use std::process::Output;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum HostStoreAction {
    ReadEntry,
    ReadLine,
    SaveEntry,
    RenameEntry,
    DeleteEntry,
    SaveRecipients,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct HostCommandFailure {
    action: HostStoreAction,
    message: String,
}

impl HostCommandFailure {
    pub(super) fn from_output(
        action: HostStoreAction,
        output: Output,
        fallback_prefix: &str,
    ) -> Self {
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
        let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
        let message = if !stderr.is_empty() {
            stderr
        } else if !stdout.is_empty() && !action.stdout_may_contain_secret_data() {
            stdout
        } else {
            format!("{fallback_prefix}: {}", output.status)
        };

        Self { action, message }
    }

    fn message(&self) -> &str {
        &self.message
    }
}

impl HostStoreAction {
    const fn stdout_may_contain_secret_data(self) -> bool {
        matches!(self, Self::ReadEntry | Self::ReadLine | Self::SaveEntry)
    }
}

fn message_contains_any(lowered: &str, patterns: &[&str]) -> bool {
    patterns.iter().any(|pattern| lowered.contains(pattern))
}

fn host_failure_is_entry_not_found(action: HostStoreAction, lowered: &str) -> bool {
    matches!(
        action,
        HostStoreAction::ReadEntry
            | HostStoreAction::ReadLine
            | HostStoreAction::RenameEntry
            | HostStoreAction::DeleteEntry
    ) && message_contains_any(
        lowered,
        &[
            "not in the password store",
            "was not found",
            "no such file or directory",
        ],
    )
}

fn host_failure_is_already_exists(action: HostStoreAction, lowered: &str) -> bool {
    matches!(
        action,
        HostStoreAction::SaveEntry | HostStoreAction::RenameEntry
    ) && lowered.contains("already exists")
}

fn host_failure_is_missing_private_key(message: &str) -> bool {
    message.contains("Import a private key in Preferences")
}

fn host_failure_is_locked_private_key(message: &str) -> bool {
    message.contains("A private key for this item is locked.")
}

fn host_failure_is_incompatible_private_key(message: &str) -> bool {
    message.contains("cannot decrypt password store entries")
        || message.contains("available private keys cannot decrypt")
        || message.contains("no pkesks managed to decrypt the ciphertext")
        || message.contains("no pkesk managed to decrypt the ciphertext")
}

fn host_failure_is_invalid_store_path(action: HostStoreAction, lowered: &str) -> bool {
    action == HostStoreAction::SaveRecipients
        && lowered.contains("selected password store path is not a folder")
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum HostStoreFailureKind {
    EntryNotFound,
    EntryAlreadyExists,
    MissingPrivateKey,
    LockedPrivateKey,
    IncompatiblePrivateKey,
    InvalidStorePath,
    Other,
}

fn classify_host_store_failure(failure: &HostCommandFailure) -> HostStoreFailureKind {
    let lowered = failure.message().to_ascii_lowercase();
    if host_failure_is_entry_not_found(failure.action, &lowered) {
        HostStoreFailureKind::EntryNotFound
    } else if host_failure_is_already_exists(failure.action, &lowered) {
        HostStoreFailureKind::EntryAlreadyExists
    } else if host_failure_is_missing_private_key(failure.message()) {
        HostStoreFailureKind::MissingPrivateKey
    } else if host_failure_is_locked_private_key(failure.message()) {
        HostStoreFailureKind::LockedPrivateKey
    } else if host_failure_is_incompatible_private_key(failure.message()) {
        HostStoreFailureKind::IncompatiblePrivateKey
    } else if host_failure_is_invalid_store_path(failure.action, &lowered) {
        HostStoreFailureKind::InvalidStorePath
    } else {
        HostStoreFailureKind::Other
    }
}

pub(super) fn ensure_host_command_success(
    action: HostStoreAction,
    output: Output,
    fallback_prefix: &str,
) -> Result<Output, HostCommandFailure> {
    if output.status.success() {
        Ok(output)
    } else {
        Err(HostCommandFailure::from_output(
            action,
            output,
            fallback_prefix,
        ))
    }
}

pub(super) fn password_entry_error_from_host_launch(
    message: impl Into<String>,
) -> PasswordEntryError {
    PasswordEntryError::other(message.into())
}

pub(super) fn password_entry_error_from_host_failure(
    failure: HostCommandFailure,
) -> PasswordEntryError {
    let message = failure.message;
    match classify_host_store_failure(&HostCommandFailure {
        action: failure.action,
        message: message.clone(),
    }) {
        HostStoreFailureKind::EntryNotFound => PasswordEntryError::EntryNotFound(message),
        HostStoreFailureKind::MissingPrivateKey => PasswordEntryError::MissingPrivateKey(message),
        HostStoreFailureKind::LockedPrivateKey => PasswordEntryError::LockedPrivateKey(message),
        HostStoreFailureKind::IncompatiblePrivateKey => {
            PasswordEntryError::IncompatiblePrivateKey(message)
        }
        HostStoreFailureKind::EntryAlreadyExists
        | HostStoreFailureKind::InvalidStorePath
        | HostStoreFailureKind::Other => PasswordEntryError::other(message),
    }
}

pub(super) fn password_entry_write_error_from_host_launch(
    message: impl Into<String>,
) -> PasswordEntryWriteError {
    PasswordEntryWriteError::other(message.into())
}

pub(super) fn password_entry_write_error_from_host_failure(
    failure: HostCommandFailure,
) -> PasswordEntryWriteError {
    let message = failure.message;
    match classify_host_store_failure(&HostCommandFailure {
        action: failure.action,
        message: message.clone(),
    }) {
        HostStoreFailureKind::EntryAlreadyExists => {
            PasswordEntryWriteError::already_exists(message)
        }
        HostStoreFailureKind::EntryNotFound => PasswordEntryWriteError::entry_not_found(message),
        HostStoreFailureKind::MissingPrivateKey => {
            PasswordEntryWriteError::MissingPrivateKey(message)
        }
        HostStoreFailureKind::LockedPrivateKey => {
            PasswordEntryWriteError::LockedPrivateKey(message)
        }
        HostStoreFailureKind::IncompatiblePrivateKey => {
            PasswordEntryWriteError::IncompatiblePrivateKey(message)
        }
        HostStoreFailureKind::InvalidStorePath | HostStoreFailureKind::Other => {
            PasswordEntryWriteError::other(message)
        }
    }
}

pub(super) fn store_recipients_error_from_host_launch(
    message: impl Into<String>,
) -> StoreRecipientsError {
    StoreRecipientsError::other(message.into())
}

pub(super) fn store_recipients_error_from_host_failure(
    failure: HostCommandFailure,
) -> StoreRecipientsError {
    let message = failure.message;
    match classify_host_store_failure(&HostCommandFailure {
        action: failure.action,
        message: message.clone(),
    }) {
        HostStoreFailureKind::InvalidStorePath => StoreRecipientsError::invalid_store_path(message),
        HostStoreFailureKind::MissingPrivateKey => StoreRecipientsError::MissingPrivateKey(message),
        HostStoreFailureKind::LockedPrivateKey => StoreRecipientsError::LockedPrivateKey(message),
        HostStoreFailureKind::IncompatiblePrivateKey => {
            StoreRecipientsError::IncompatiblePrivateKey(message)
        }
        HostStoreFailureKind::EntryNotFound
        | HostStoreFailureKind::EntryAlreadyExists
        | HostStoreFailureKind::Other => StoreRecipientsError::other(message),
    }
}

#[cfg(test)]
pub(super) fn password_entry_write_error_from_host_io(
    err: &std::io::Error,
) -> PasswordEntryWriteError {
    match err.kind() {
        std::io::ErrorKind::AlreadyExists => {
            PasswordEntryWriteError::already_exists(err.to_string())
        }
        std::io::ErrorKind::NotFound => PasswordEntryWriteError::entry_not_found(err.to_string()),
        _ => PasswordEntryWriteError::other(err.to_string()),
    }
}

#[cfg(test)]
mod tests {
    use super::{
        password_entry_error_from_host_failure, password_entry_write_error_from_host_failure,
        password_entry_write_error_from_host_io, password_entry_write_error_from_host_launch,
        store_recipients_error_from_host_failure, HostCommandFailure, HostStoreAction,
    };
    use crate::backend::{PasswordEntryError, PasswordEntryWriteError, StoreRecipientsError};
    use std::io;
    use std::process::{ExitStatus, Output};

    #[cfg(unix)]
    use std::os::unix::process::ExitStatusExt;

    #[cfg(windows)]
    use std::os::windows::process::ExitStatusExt;

    fn failed_output(stderr: &str) -> Output {
        Output {
            status: ExitStatus::from_raw(1 << 8),
            stdout: Vec::new(),
            stderr: stderr.as_bytes().to_vec(),
        }
    }

    fn failed_output_with_stdout(stdout: &str) -> Output {
        Output {
            status: ExitStatus::from_raw(1 << 8),
            stdout: stdout.as_bytes().to_vec(),
            stderr: Vec::new(),
        }
    }

    #[test]
    fn host_write_errors_classify_existing_and_missing_entries() {
        assert!(matches!(
            password_entry_write_error_from_host_failure(HostCommandFailure::from_output(
                HostStoreAction::SaveEntry,
                failed_output("That password entry already exists."),
                "pass insert failed",
            )),
            PasswordEntryWriteError::EntryAlreadyExists(_)
        ));
        assert!(matches!(
            password_entry_write_error_from_host_failure(HostCommandFailure::from_output(
                HostStoreAction::RenameEntry,
                failed_output("Password entry 'team/demo' was not found."),
                "pass mv failed",
            )),
            PasswordEntryWriteError::EntryNotFound(_)
        ));
    }

    #[test]
    fn host_write_io_errors_classify_by_io_kind_without_english_matching() {
        assert!(matches!(
            password_entry_write_error_from_host_io(&io::Error::from(io::ErrorKind::NotFound)),
            PasswordEntryWriteError::EntryNotFound(_)
        ));
        assert!(matches!(
            password_entry_write_error_from_host_io(&io::Error::from(io::ErrorKind::AlreadyExists)),
            PasswordEntryWriteError::EntryAlreadyExists(_)
        ));
    }

    #[test]
    fn host_launch_errors_fall_back_to_generic_write_failures() {
        assert!(matches!(
            password_entry_write_error_from_host_launch("Failed to run the host backend command"),
            PasswordEntryWriteError::Other(_)
        ));
    }

    #[test]
    fn host_store_recipient_errors_map_to_specific_variants() {
        assert!(matches!(
            store_recipients_error_from_host_failure(HostCommandFailure::from_output(
                HostStoreAction::SaveRecipients,
                failed_output(
                    "Import a private key in Preferences before using the password store."
                ),
                "pass init failed",
            )),
            StoreRecipientsError::MissingPrivateKey(_)
        ));
        assert!(matches!(
            store_recipients_error_from_host_failure(HostCommandFailure::from_output(
                HostStoreAction::SaveRecipients,
                failed_output("The selected password store path is not a folder."),
                "pass init failed",
            )),
            StoreRecipientsError::InvalidStorePath(_)
        ));
    }

    #[test]
    fn host_read_errors_classify_pkesks_failures_as_incompatible_private_keys() {
        assert!(matches!(
            password_entry_error_from_host_failure(HostCommandFailure::from_output(
                HostStoreAction::ReadEntry,
                failed_output("no pkesks managed to decrypt the ciphertext"),
                "pass failed",
            )),
            PasswordEntryError::IncompatiblePrivateKey(_)
        ));
    }

    #[test]
    fn host_sensitive_read_failures_do_not_surface_stdout_contents() {
        let error = password_entry_error_from_host_failure(HostCommandFailure::from_output(
            HostStoreAction::ReadEntry,
            failed_output_with_stdout("supersecret\nusername: alice"),
            "pass failed",
        ));

        match error {
            PasswordEntryError::Other(message) => {
                assert!(message.contains("pass failed"));
                assert!(!message.contains("supersecret"));
                assert!(!message.contains("username: alice"));
            }
            other => panic!("unexpected host read error: {other:?}"),
        }
    }
}
