use std::path::{Component, Path, PathBuf};

pub(in crate::backend) fn validated_relative_directory_path(
    relative_dir: &str,
) -> Result<PathBuf, String> {
    let mut relative = PathBuf::new();
    for component in Path::new(relative_dir).components() {
        match component {
            Component::Normal(part) => relative.push(part),
            Component::CurDir => {}
            _ => return Err("Invalid recipients file path.".to_string()),
        }
    }

    Ok(relative)
}

pub(in crate::backend) fn validated_entry_label_path(label: &str) -> Result<PathBuf, String> {
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

#[cfg(test)]
mod tests {
    use super::{validated_entry_label_path, validated_relative_directory_path};
    use std::path::PathBuf;

    #[test]
    fn entry_labels_reject_parent_components() {
        assert_eq!(
            validated_entry_label_path("team/../../escape").unwrap_err(),
            "Invalid password entry path."
        );
    }

    #[test]
    fn entry_labels_reject_empty_names() {
        assert_eq!(
            validated_entry_label_path("./").unwrap_err(),
            "Password entry name is empty."
        );
    }

    #[test]
    fn relative_dirs_reject_parent_components() {
        assert_eq!(
            validated_relative_directory_path("team/../escape").unwrap_err(),
            "Invalid recipients file path."
        );
    }

    #[test]
    fn relative_dirs_keep_normal_components() {
        assert_eq!(
            validated_relative_directory_path("./team/ops").expect("valid recipients path"),
            PathBuf::from("team").join("ops")
        );
    }
}
