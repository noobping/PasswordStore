use std::path::{Component, Path};

pub const STANDARD_PASSWORD_ENTRY_EXTENSION: &str = "gpg";
pub const FIDO2_PASSWORD_ENTRY_EXTENSION: &str = "keycord";

pub const fn password_entry_extension(uses_fido2: bool) -> &'static str {
    if uses_fido2 {
        FIDO2_PASSWORD_ENTRY_EXTENSION
    } else {
        STANDARD_PASSWORD_ENTRY_EXTENSION
    }
}

pub fn is_password_entry_file(path: &Path) -> bool {
    path.extension()
        .and_then(|value| value.to_str())
        .is_some_and(is_password_entry_extension)
}

pub fn is_password_entry_extension(extension: &str) -> bool {
    matches!(
        extension,
        STANDARD_PASSWORD_ENTRY_EXTENSION | FIDO2_PASSWORD_ENTRY_EXTENSION
    )
}

pub fn normalize_password_entry_label(label: &str) -> Result<String, &'static str> {
    let label = label.trim();
    if label.is_empty() {
        return Err("Enter a name.");
    }
    if label.contains('\\') {
        return Err("Use a path inside the password store.");
    }

    let mut parts = Vec::new();
    for component in Path::new(label).components() {
        match component {
            Component::Normal(part) => parts.push(part.to_string_lossy().into_owned()),
            Component::CurDir => {}
            Component::Prefix(_) | Component::RootDir | Component::ParentDir => {
                return Err("Use a path inside the password store.");
            }
        }
    }

    if parts.is_empty() {
        return Err("Enter a name.");
    }

    Ok(parts.join("/"))
}

pub fn label_from_password_entry_path(store_root: &Path, entry_path: &Path) -> Option<String> {
    let relative = entry_path.strip_prefix(store_root).ok()?;
    label_from_password_entry_relative_path(relative)
}

pub fn label_from_password_entry_relative_path(relative: &Path) -> Option<String> {
    let extension = relative.extension().and_then(|value| value.to_str())?;
    if !is_password_entry_extension(extension) {
        return None;
    }

    let mut label = relative.to_path_buf();
    label.set_extension("");
    Some(label.to_string_lossy().to_string())
}

#[cfg(test)]
mod tests {
    use super::{
        is_password_entry_file, label_from_password_entry_path,
        label_from_password_entry_relative_path, normalize_password_entry_label,
        password_entry_extension, FIDO2_PASSWORD_ENTRY_EXTENSION,
        STANDARD_PASSWORD_ENTRY_EXTENSION,
    };
    use std::path::Path;

    #[test]
    fn password_entry_extensions_distinguish_standard_and_fido2_entries() {
        assert_eq!(
            password_entry_extension(false),
            STANDARD_PASSWORD_ENTRY_EXTENSION
        );
        assert_eq!(
            password_entry_extension(true),
            FIDO2_PASSWORD_ENTRY_EXTENSION
        );
    }

    #[test]
    fn supported_entry_paths_round_trip_back_to_labels() {
        assert_eq!(
            label_from_password_entry_relative_path(Path::new("team/service.gpg")).as_deref(),
            Some("team/service")
        );
        assert_eq!(
            label_from_password_entry_relative_path(Path::new("team/service.keycord")).as_deref(),
            Some("team/service")
        );
        assert_eq!(
            label_from_password_entry_path(
                Path::new("/tmp/store"),
                Path::new("/tmp/store/team/service.keycord"),
            )
            .as_deref(),
            Some("team/service")
        );
    }

    #[test]
    fn unsupported_files_are_not_treated_as_password_entries() {
        assert!(is_password_entry_file(Path::new("team/service.gpg")));
        assert!(is_password_entry_file(Path::new("team/service.keycord")));
        assert!(!is_password_entry_file(Path::new("team/service.txt")));
    }

    #[test]
    fn password_entry_labels_are_normalized() {
        assert_eq!(
            normalize_password_entry_label(" team//alice/github "),
            Ok("team/alice/github".to_string())
        );
    }

    #[test]
    fn password_entry_labels_reject_parent_traversal() {
        assert_eq!(
            normalize_password_entry_label("../alice/github"),
            Err("Use a path inside the password store.")
        );
    }

    #[test]
    fn password_entry_labels_reject_absolute_paths() {
        assert_eq!(
            normalize_password_entry_label("/alice/github"),
            Err("Use a path inside the password store.")
        );
    }

    #[test]
    fn password_entry_labels_reject_backslashes() {
        assert_eq!(
            normalize_password_entry_label(r"team\alice\github"),
            Err("Use a path inside the password store.")
        );
    }
}
