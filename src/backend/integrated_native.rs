use ripasso::crypto::CryptoImpl;
use ripasso::pass::PasswordStore;
use std::fs;
use std::path::PathBuf;

fn user_home() -> Option<PathBuf> {
    dirs_next::home_dir()
}

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

pub(crate) fn read_password_entry(store_root: &str, label: &str) -> Result<String, String> {
    let (store, entry) = load_store_entry(store_root, label)?;
    entry.secret(&store).map_err(|err| err.to_string())
}

pub(crate) fn read_password_line(store_root: &str, label: &str) -> Result<String, String> {
    let (store, entry) = load_store_entry(store_root, label)?;
    entry.password(&store).map_err(|err| err.to_string())
}

pub(crate) fn save_password_entry(
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

pub(crate) fn rename_password_entry(
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

pub(crate) fn delete_password_entry(store_root: &str, label: &str) -> Result<(), String> {
    let (store, entry) = load_store_entry(store_root, label)?;
    entry.delete_file(&store).map_err(|err| err.to_string())
}

pub(crate) fn save_store_recipients(
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
