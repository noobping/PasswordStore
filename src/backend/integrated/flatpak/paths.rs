use std::fs;
use std::path::{Component, Path, PathBuf};
use walkdir::WalkDir;

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

pub(super) fn secret_entry_relative_path(label: &str) -> Result<PathBuf, String> {
    let mut relative = validated_entry_label_path(label)?;
    let file_name = relative
        .file_name()
        .and_then(|value| value.to_str())
        .ok_or_else(|| "Invalid password entry path.".to_string())?;
    relative.set_file_name(format!("{file_name}.gpg"));
    Ok(relative)
}

pub(super) fn entry_file_path(store_root: &str, label: &str) -> Result<PathBuf, String> {
    let mut path = PathBuf::from(store_root);
    path.push(secret_entry_relative_path(label)?);
    Ok(path)
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

pub(super) fn label_from_entry_path(
    store_root: &Path,
    entry_path: &Path,
) -> Result<String, String> {
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

pub(super) fn with_updated_recipients_file<T>(
    recipients_path: &Path,
    contents: &str,
    f: impl FnOnce() -> Result<T, String>,
) -> Result<T, String> {
    let previous_contents = fs::read_to_string(recipients_path).ok();
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
        if entry.path().extension().and_then(|value| value.to_str()) == Some("gpg") {
            entries.push(entry.into_path());
        }
    }

    Ok(entries)
}
