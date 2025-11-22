use std::ffi::OsStr;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone)]
pub struct PassEntry {
    pub basename: String,
    pub relative_path: String,
    pub store_path: String,
}

impl PassEntry {
    pub fn label(&self) -> String {
        let name = &self.basename;
        let dir = &self.relative_path;
        format!("{dir}{name}")
    }
}

pub fn collect_all_password_items(roots: &[PathBuf]) -> io::Result<Vec<PassEntry>> {
    let mut result: Vec<PassEntry> = Vec::new();

    let mut i = 0;
    let len = roots.len();
    while i < len {
        let base = &roots[i];
        let _ = collect_items_in_dir(base.as_path(), base.as_path(), &mut result);
        i += 1;
    }

    Ok(result)
}

fn collect_items_in_dir(root: &Path, base: &Path, out: &mut Vec<PassEntry>) -> io::Result<()> {
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
            // Path relative to store root
            let rel = match path.strip_prefix(base) {
                Ok(r) => r,
                Err(_) => path.as_path(),
            };

            // Get the file stem (filename without extension)
            let basename = rel
                .file_stem()
                .and_then(|s| s.to_str())
                .unwrap_or_default()
                .to_string();

            // Extract the directory path (may be empty)
            let relative_path = rel
                .parent()
                .map(|p| {
                    let mut s = p.to_string_lossy().to_string();
                    if !s.is_empty() && !s.ends_with('/') {
                        s.push('/');
                    }
                    s
                })
                .unwrap_or_else(|| "".to_string());

            let store_path = base.to_string_lossy().to_string();

            out.push(PassEntry {
                basename,
                relative_path,
                store_path,
            });
        }
    }

    Ok(())
}
