use std::fs;
use std::path::{Component, Path, PathBuf};
use walkdir::WalkDir;

use crate::fido2_recipient::FIDO2_RECIPIENTS_FILE_NAME;
use crate::password::entry_files::{
    is_password_entry_file, label_from_password_entry_path, password_entry_extension,
};
use crate::support::secure_fs::write_atomic_file;

pub(super) fn validated_entry_label_path(label: &str) -> Result<PathBuf, String> {
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

fn secret_entry_relative_path_with_extension(
    label: &str,
    uses_fido2: bool,
) -> Result<PathBuf, String> {
    let mut relative = validated_entry_label_path(label)?;
    let file_name = relative
        .file_name()
        .and_then(|value| value.to_str())
        .ok_or_else(|| "Invalid password entry path.".to_string())?;
    relative.set_file_name(format!(
        "{file_name}.{}",
        password_entry_extension(uses_fido2)
    ));
    Ok(relative)
}

#[cfg(test)]
pub(super) fn secret_entry_relative_path(label: &str) -> Result<PathBuf, String> {
    secret_entry_relative_path_with_extension(label, false)
}

fn entry_file_path_with_extension(
    store_root: &str,
    label: &str,
    uses_fido2: bool,
) -> Result<PathBuf, String> {
    let mut path = PathBuf::from(store_root);
    path.push(secret_entry_relative_path_with_extension(
        label, uses_fido2,
    )?);
    Ok(path)
}

pub(super) fn existing_entry_file_path(
    store_root: &str,
    label: &str,
) -> Result<Option<PathBuf>, String> {
    let fido2_path = entry_file_path_with_extension(store_root, label, true)?;
    if fido2_path.is_file() {
        return Ok(Some(fido2_path));
    }

    let standard_path = entry_file_path_with_extension(store_root, label, false)?;
    if standard_path.is_file() {
        return Ok(Some(standard_path));
    }

    Ok(None)
}

fn label_uses_fido2_recipients(store_root: &str, label: &str) -> Result<bool, String> {
    let recipients_path = recipients_file_for_label(store_root, label)?;
    let fido2_recipients_path = fido2_recipients_file_for_recipients_path(&recipients_path);
    match fs::read_to_string(fido2_recipients_path) {
        Ok(contents) => Ok(contents.lines().any(|line| !line.trim().is_empty())),
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => Ok(false),
        Err(err) => Err(err.to_string()),
    }
}

pub(super) fn desired_entry_file_path(store_root: &str, label: &str) -> Result<PathBuf, String> {
    entry_file_path_with_extension(
        store_root,
        label,
        label_uses_fido2_recipients(store_root, label)?,
    )
}

pub(super) fn entry_file_path(store_root: &str, label: &str) -> Result<PathBuf, String> {
    if let Some(path) = existing_entry_file_path(store_root, label)? {
        Ok(path)
    } else {
        desired_entry_file_path(store_root, label)
    }
}

pub(super) fn recipients_file_for_label(store_root: &str, label: &str) -> Result<PathBuf, String> {
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

pub(super) fn fido2_recipients_file_for_recipients_path(recipients_path: &Path) -> PathBuf {
    recipients_path.with_file_name(FIDO2_RECIPIENTS_FILE_NAME)
}

pub(super) fn label_from_entry_path(
    store_root: &Path,
    entry_path: &Path,
) -> Result<String, String> {
    label_from_password_entry_path(store_root, entry_path)
        .ok_or_else(|| "Invalid password entry path.".to_string())
}

pub(super) fn ensure_store_directory(store_root: &str) -> Result<PathBuf, String> {
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

fn write_optional_text_file(path: &Path, contents: &str) -> Result<(), String> {
    if contents.trim().is_empty() {
        match fs::remove_file(path) {
            Ok(()) => Ok(()),
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(err) => Err(err.to_string()),
        }
    } else {
        write_atomic_file(path, contents.as_bytes()).map_err(|err| err.to_string())
    }
}

pub(super) fn with_updated_recipient_files<T>(
    recipients_path: &Path,
    recipients_contents: &str,
    fido2_recipients_path: &Path,
    fido2_recipients_contents: &str,
    f: impl FnOnce() -> Result<T, String>,
) -> Result<T, String> {
    let previous_contents = fs::read_to_string(recipients_path).ok();
    let previous_fido2_contents = fs::read_to_string(fido2_recipients_path).ok();
    write_atomic_file(recipients_path, recipients_contents.as_bytes())
        .map_err(|err| err.to_string())?;
    if let Err(err) = write_optional_text_file(fido2_recipients_path, fido2_recipients_contents) {
        match previous_contents {
            Some(previous) => {
                let _ = write_atomic_file(recipients_path, previous.as_bytes());
            }
            None => {
                let _ = fs::remove_file(recipients_path);
            }
        }
        return Err(err);
    }

    match f() {
        Ok(value) => Ok(value),
        Err(err) => {
            match previous_contents {
                Some(previous) => {
                    let _ = write_atomic_file(recipients_path, previous.as_bytes());
                }
                None => {
                    let _ = fs::remove_file(recipients_path);
                }
            }
            match previous_fido2_contents {
                Some(previous) => {
                    let _ = write_atomic_file(fido2_recipients_path, previous.as_bytes());
                }
                None => {
                    let _ = fs::remove_file(fido2_recipients_path);
                }
            }
            Err(err)
        }
    }
}

pub(super) fn cleanup_empty_store_dirs(store_root: &str, entry_path: &Path) -> Result<(), String> {
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

pub(super) fn collect_password_entry_files(store_root: &Path) -> Result<Vec<PathBuf>, String> {
    if !store_root.exists() {
        return Ok(Vec::new());
    }

    let mut entries = Vec::new();
    for entry in WalkDir::new(store_root) {
        let entry = entry.map_err(|err| err.to_string())?;
        if !entry.file_type().is_file() {
            continue;
        }
        if is_password_entry_file(entry.path()) {
            entries.push(entry.into_path());
        }
    }

    Ok(entries)
}

#[cfg(test)]
mod tests {
    use super::{
        collect_password_entry_files, desired_entry_file_path, entry_file_path,
        existing_entry_file_path, label_from_entry_path,
    };
    use std::fs;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn temp_store(prefix: &str) -> std::path::PathBuf {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock before unix epoch")
            .as_nanos();
        std::env::temp_dir().join(format!("{prefix}-{nanos}"))
    }

    #[test]
    fn entry_paths_choose_keycord_extension_for_fido2_recipients() {
        let store = temp_store("keycord-paths-fido2");
        fs::create_dir_all(store.join("team")).expect("create store");
        fs::write(store.join(".gpg-id"), "user@example.com\n").expect("write recipients");
        fs::write(
            store.join(crate::fido2_recipient::FIDO2_RECIPIENTS_FILE_NAME),
            "keycord-fido2-recipient-v1=0123456789abcdef0123456789abcdef01234567:4465736b204b6579:63726564\n",
        )
        .expect("write fido2 recipients");

        let desired = desired_entry_file_path(store.to_string_lossy().as_ref(), "team/service")
            .expect("resolve desired path");
        assert_eq!(desired, store.join("team/service.keycord"));
        assert_eq!(
            entry_file_path(store.to_string_lossy().as_ref(), "team/service")
                .expect("resolve entry path"),
            store.join("team/service.keycord")
        );

        fs::remove_dir_all(store).expect("remove store");
    }

    #[test]
    fn existing_standard_entry_paths_still_resolve_for_legacy_fido2_items() {
        let store = temp_store("keycord-paths-legacy");
        fs::create_dir_all(store.join("team")).expect("create store");
        fs::write(store.join(".gpg-id"), "user@example.com\n").expect("write recipients");
        fs::write(
            store.join(crate::fido2_recipient::FIDO2_RECIPIENTS_FILE_NAME),
            "keycord-fido2-recipient-v1=0123456789abcdef0123456789abcdef01234567:4465736b204b6579:63726564\n",
        )
        .expect("write fido2 recipients");
        fs::write(store.join("team/service.gpg"), b"x").expect("write legacy entry");

        assert_eq!(
            existing_entry_file_path(store.to_string_lossy().as_ref(), "team/service")
                .expect("resolve existing path"),
            Some(store.join("team/service.gpg"))
        );
        assert_eq!(
            entry_file_path(store.to_string_lossy().as_ref(), "team/service")
                .expect("resolve entry path"),
            store.join("team/service.gpg")
        );

        fs::remove_dir_all(store).expect("remove store");
    }

    #[test]
    fn entry_path_helpers_understand_both_supported_extensions() {
        let store = temp_store("keycord-paths-collect");
        fs::create_dir_all(store.join("team")).expect("create store");
        fs::write(store.join("team/service.gpg"), b"x").expect("write standard entry");
        fs::write(store.join("team/key.keycord"), b"x").expect("write fido2 entry");

        let mut entries = collect_password_entry_files(&store).expect("collect entries");
        entries.sort();

        assert_eq!(
            entries,
            vec![
                store.join("team/key.keycord"),
                store.join("team/service.gpg"),
            ]
        );
        assert_eq!(
            label_from_entry_path(&store, &store.join("team/key.keycord"))
                .expect("decode keycord label"),
            "team/key".to_string()
        );
        assert_eq!(
            label_from_entry_path(&store, &store.join("team/service.gpg"))
                .expect("decode gpg label"),
            "team/service".to_string()
        );

        fs::remove_dir_all(store).expect("remove store");
    }
}
