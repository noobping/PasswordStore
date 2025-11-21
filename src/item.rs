use std::ffi::OsStr;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone)]
pub struct PasswordItem {
    path: PathBuf, // full path to the .gpg file
    label: String, // what you show in the list (relative path without .gpg)
}

/// Recursively walk a pass root and collect all *.gpg files.
pub fn scan_pass_root(root: &Path) -> std::io::Result<Vec<PasswordItem>> {
    let mut items = Vec::new();

    if !root.exists() {
        return Ok(items);
    }

    // If you’re okay with an extra dep, this is super nice:
    // walkdir = "2"
    for entry in walkdir::WalkDir::new(root)
        .into_iter()
        .filter_map(Result::ok)
    {
        if !entry.file_type().is_file() {
            continue;
        }
        if entry.path().extension() != Some(OsStr::new("gpg")) {
            continue;
        }

        let rel = entry.path().strip_prefix(root).unwrap_or(entry.path());
        let mut label = rel.to_string_lossy().to_string();
        if let Some(stripped) = label.strip_suffix(".gpg") {
            label = stripped.to_string();
        }

        items.push(PasswordItem {
            path: entry.path().to_path_buf(),
            label,
        });
    }

    Ok(items)
}
