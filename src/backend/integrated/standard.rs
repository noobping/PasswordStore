mod git;
mod store;

use self::git::maybe_commit_git_paths;
use self::store::{load_store_entry, open_store, password_entry_git_path};
use crate::backend::{PasswordEntryError, PasswordEntryWriteError, StoreRecipientsError};
use std::fs;
use std::path::PathBuf;

pub(crate) fn read_password_entry(
    store_root: &str,
    label: &str,
) -> Result<String, PasswordEntryError> {
    let (store, entry) =
        load_store_entry(store_root, label).map_err(PasswordEntryError::from_store_message)?;
    entry
        .secret(&store)
        .map_err(|err| PasswordEntryError::from_store_message(err.to_string()))
}

pub(crate) fn read_password_line(
    store_root: &str,
    label: &str,
) -> Result<String, PasswordEntryError> {
    let (store, entry) =
        load_store_entry(store_root, label).map_err(PasswordEntryError::from_store_message)?;
    entry
        .password(&store)
        .map_err(|err| PasswordEntryError::from_store_message(err.to_string()))
}

pub(crate) fn save_password_entry(
    store_root: &str,
    label: &str,
    contents: &str,
    overwrite: bool,
) -> Result<(), PasswordEntryWriteError> {
    let mut store = open_store(store_root).map_err(PasswordEntryWriteError::from_store_message)?;
    store
        .reload_password_list()
        .map_err(|err| PasswordEntryWriteError::from_store_message(err.to_string()))?;
    let git_message;
    let result = if let Some(entry) = store
        .passwords
        .iter()
        .find(|entry| entry.name == label)
        .cloned()
    {
        if !overwrite {
            return Err(PasswordEntryWriteError::already_exists(
                "That password entry already exists.",
            ));
        }
        git_message = format!("Update password for {label}");
        entry
            .update(contents.to_string(), &store)
            .map_err(|err| PasswordEntryWriteError::from_store_message(err.to_string()))
    } else {
        git_message = format!("Add password for {label}");
        store
            .new_password_file(label, contents)
            .map(|_| ())
            .map_err(|err| PasswordEntryWriteError::from_store_message(err.to_string()))
    };
    if result.is_ok() {
        maybe_commit_git_paths(store_root, &git_message, [password_entry_git_path(label)]);
    }
    result
}

pub(crate) fn rename_password_entry(
    store_root: &str,
    old_label: &str,
    new_label: &str,
) -> Result<(), PasswordEntryWriteError> {
    let mut store = open_store(store_root).map_err(PasswordEntryWriteError::from_store_message)?;
    store
        .reload_password_list()
        .map_err(|err| PasswordEntryWriteError::from_store_message(err.to_string()))?;
    let result = store
        .rename_file(old_label, new_label)
        .map(|_| ())
        .map_err(|err| PasswordEntryWriteError::from_store_message(err.to_string()));
    if result.is_ok() {
        maybe_commit_git_paths(
            store_root,
            &format!("Rename password from {old_label} to {new_label}"),
            [
                password_entry_git_path(old_label),
                password_entry_git_path(new_label),
            ],
        );
    }
    result
}

pub(crate) fn delete_password_entry(
    store_root: &str,
    label: &str,
) -> Result<(), PasswordEntryWriteError> {
    let (store, entry) =
        load_store_entry(store_root, label).map_err(PasswordEntryWriteError::from_store_message)?;
    let result = entry
        .delete_file(&store)
        .map_err(|err| PasswordEntryWriteError::from_store_message(err.to_string()));
    if result.is_ok() {
        maybe_commit_git_paths(
            store_root,
            &format!("Remove password for {label}"),
            [password_entry_git_path(label)],
        );
    }
    result
}

pub(crate) fn save_store_recipients(
    store_root: &str,
    recipients: &[String],
) -> Result<(), StoreRecipientsError> {
    let store_dir = PathBuf::from(store_root);
    if store_dir.exists() {
        if !store_dir.is_dir() {
            return Err(StoreRecipientsError::invalid_store_path(
                "The selected password store path is not a folder.",
            ));
        }
    } else {
        fs::create_dir_all(&store_dir)
            .map_err(|err| StoreRecipientsError::other(err.to_string()))?;
    }

    let recipients_path = store_dir.join(".gpg-id");
    let previous_recipients = fs::read_to_string(&recipients_path).ok();
    let contents = format!("{}\n", recipients.join("\n"));

    fs::write(&recipients_path, contents)
        .map_err(|err| StoreRecipientsError::other(err.to_string()))?;

    let result = (|| {
        let store = open_store(store_root).map_err(StoreRecipientsError::from_store_message)?;
        let entries = store
            .all_passwords()
            .map_err(|err| StoreRecipientsError::from_store_message(err.to_string()))?;
        let entry_labels = entries
            .iter()
            .map(|entry| entry.name.clone())
            .collect::<Vec<_>>();
        for entry in entries {
            let secret = entry
                .secret(&store)
                .map_err(|err| StoreRecipientsError::from_store_message(err.to_string()))?;
            entry
                .update(secret, &store)
                .map_err(|err| StoreRecipientsError::from_store_message(err.to_string()))?;
        }
        Ok(entry_labels)
    })();

    let entry_labels = match result {
        Ok(entry_labels) => entry_labels,
        Err(err) => {
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
    };

    maybe_commit_git_paths(
        store_root,
        "Update password store recipients",
        std::iter::once(".gpg-id".to_string()).chain(
            entry_labels
                .into_iter()
                .map(|label| password_entry_git_path(&label)),
        ),
    );

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{save_password_entry, save_store_recipients};
    use crate::backend::test_support::{
        assert_entry_is_encrypted_for_each_recipient, SystemBackendTestEnv,
    };

    #[test]
    fn integrated_standard_backend_encrypts_entries_for_all_store_recipients() {
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

    #[test]
    fn integrated_standard_backend_commits_git_backed_store_changes() {
        let env = SystemBackendTestEnv::new();
        env.init_store_git_repository()
            .expect("initialize git repository");

        let key = env
            .generate_secret_key("Recipient <git-integrated@example.com>")
            .expect("generate git recipient key");
        env.import_public_key(&key.public_key_bytes)
            .expect("import git recipient key");
        env.trust_public_key(&key.fingerprint_hex)
            .expect("trust git recipient key");

        let store_root = env.store_root().to_string_lossy().to_string();
        save_store_recipients(&store_root, std::slice::from_ref(&key.fingerprint_hex))
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
            .expect("read store git commit subjects");
        assert_eq!(subjects.len(), 2);
        assert!(subjects[0].contains("Add password for team/service"));
        assert_eq!(subjects[1], "Update password store recipients");
    }
}
