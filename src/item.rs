use std::ffi::OsStr;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone)]
pub struct PasswordItem {
    pub path: PathBuf, // full path to the .gpg
    pub label: String, // relative path without .gpg
    pub base: String,
}

pub fn collect_all_password_items(roots: &[PathBuf]) -> io::Result<Vec<PasswordItem>> {
    let mut result: Vec<PasswordItem> = Vec::new();

    let mut i = 0;
    let len = roots.len();
    while i < len {
        let base = &roots[i];
        let _ = collect_items_in_dir(base.as_path(), base.as_path(), &mut result);
        i += 1;
    }

    Ok(result)
}

fn collect_items_in_dir(root: &Path, base: &Path, out: &mut Vec<PasswordItem>) -> io::Result<()> {
    if !root.exists() {
        return Ok(());
    }

    let entries = match fs::read_dir(root) {
        Ok(e) => e,
        Err(err) => return Err(err),
    };

    for entry_result in entries {
        let entry = match entry_result {
            Ok(e) => e,
            Err(_) => continue,
        };

        let path = entry.path();
        let file_type = match entry.file_type() {
            Ok(t) => t,
            Err(_) => continue,
        };

        if file_type.is_dir() {
            let _ = collect_items_in_dir(path.as_path(), base, out);
        } else if file_type.is_file() && path.extension() == Some(OsStr::new("gpg")) {
            // relative to the pass root
            let rel = match path.strip_prefix(base) {
                Ok(r) => r,
                Err(_) => path.as_path(),
            };

            let rel_str = rel.to_string_lossy().to_string();
            let label = match rel_str.strip_suffix(".gpg") {
                Some(s) => s.to_string(),
                None => rel_str,
            };

            out.push(PasswordItem {
                path: path.clone(),
                label,
                base: base.to_string_lossy().to_string(),
            });
        }
    }

    Ok(())
}
