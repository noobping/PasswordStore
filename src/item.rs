use std::path::{Path, PathBuf};
use std::ffi::OsStr;

#[derive(Debug, Clone)]
pub struct PasswordItem {
    pub path: PathBuf,
    pub label: String,
}

pub fn scan_pass_root(root: &Path) -> std::io::Result<Vec<PasswordItem>> {
    let mut items: Vec<PasswordItem> = Vec::new();

    if !root.exists() {
        return Ok(items);
    }

    // Using walkdir is convenient but optional:
    // walkdir = "2" in Cargo.toml
    for entry_result in walkdir::WalkDir::new(root) {
        let entry = match entry_result {
            Ok(e) => e,
            Err(_) => continue,
        };

        if !entry.file_type().is_file() {
            continue;
        }

        if entry.path().extension() != Some(OsStr::new("gpg")) {
            continue;
        }

        let rel = match entry.path().strip_prefix(root) {
            Ok(r) => r,
            Err(_) => entry.path(),
        };

        let rel_str = rel.to_string_lossy().to_string();
        let label = match rel_str.strip_suffix(".gpg") {
            Some(s) => s.to_string(),
            None => rel_str,
        };

        items.push(PasswordItem {
            path: entry.path().to_path_buf(),
            label,
        });
    }

    Ok(items)
}
