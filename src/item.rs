use std::ffi::OsStr;
use std::path::{Path, PathBuf};

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

    // You can use walkdir here if you want; this is a stub.
    for entry_result in std::fs::read_dir(root)? {
        let entry = match entry_result {
            Ok(e) => e,
            Err(_) => continue,
        };
        let path = entry.path();
        if path.is_dir() {
            // TODO: recurse if you want full tree
            continue;
        }
        if path.extension() != Some(OsStr::new("gpg")) {
            continue;
        }

        let rel = match path.strip_prefix(root) {
            Ok(r) => r,
            Err(_) => path.as_path(),
        };

        let rel_str = rel.to_string_lossy().to_string();
        let label = match rel_str.strip_suffix(".gpg") {
            Some(s) => s.to_string(),
            None => rel_str,
        };

        items.push(PasswordItem { path, label });
    }

    Ok(items)
}
