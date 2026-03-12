use ripasso::crypto::CryptoImpl;
use ripasso::pass::PasswordStore;
use std::path::PathBuf;

fn user_home() -> Option<PathBuf> {
    dirs_next::home_dir()
}

pub(super) fn open_store(store_root: &str) -> Result<PasswordStore, String> {
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

pub(super) fn load_store_entry(
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

pub(super) fn password_entry_git_path(label: &str) -> String {
    format!("{label}.gpg")
}
