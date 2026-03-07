use crate::preferences::Preferences;

use std::ffi::OsStr;
use std::fs;
use std::io;
use std::path::Path;

const USERNAME_KEYS: [&str; 3] = ["login", "username", "user"];

#[derive(Debug, Clone, PartialEq, Eq)]
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

    pub fn from_label(store_path: impl Into<String>, label: impl AsRef<str>) -> Self {
        let label = label.as_ref();
        let (relative_path, basename) = match label.rsplit_once('/') {
            Some((dir, name)) => (format!("{dir}/"), name.to_string()),
            None => (String::new(), label.to_string()),
        };

        Self {
            basename,
            relative_path,
            store_path: store_path.into(),
        }
    }

    pub fn username_from_path(&self) -> Option<String> {
        self.relative_path
            .trim_end_matches('/')
            .rsplit('/')
            .next()
            .filter(|segment| !segment.is_empty())
            .map(str::to_string)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OpenPassFile {
    pub entry: PassEntry,
    pub username: Option<String>,
}

impl OpenPassFile {
    pub fn new(entry: PassEntry) -> Self {
        let username = entry.username_from_path();
        Self { entry, username }
    }

    pub fn from_label(store_path: impl Into<String>, label: impl AsRef<str>) -> Self {
        Self::new(PassEntry::from_label(store_path, label))
    }

    pub fn label(&self) -> String {
        self.entry.label()
    }

    pub fn title(&self) -> &str {
        &self.entry.basename
    }

    pub fn store_path(&self) -> &str {
        &self.entry.store_path
    }

    pub fn refresh_from_contents(&mut self, output: &str) {
        self.username =
            extract_username_from_contents(output).or_else(|| self.entry.username_from_path());
    }
}

fn extract_username_from_contents(output: &str) -> Option<String> {
    output.lines().skip(1).find_map(|line| {
        let (key, value) = line.split_once(':')?;
        let key = key.trim().to_ascii_lowercase();
        if !USERNAME_KEYS.contains(&key.as_str()) {
            return None;
        }

        let value = value.trim();
        if value.is_empty() {
            None
        } else {
            Some(value.to_string())
        }
    })
}

pub fn collect_all_password_items() -> io::Result<Vec<PassEntry>> {
    let settings = Preferences::new();
    let roots = settings.paths();
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

#[cfg(test)]
mod tests {
    use super::{OpenPassFile, PassEntry};

    #[test]
    fn root_level_entries_do_not_invent_a_username() {
        let entry = PassEntry::from_label("/tmp/store", "github");
        assert_eq!(entry.username_from_path(), None);
    }

    #[test]
    fn username_falls_back_to_last_directory_only() {
        let entry = PassEntry::from_label("/tmp/store", "work/email/alice/github");
        assert_eq!(entry.username_from_path().as_deref(), Some("alice"));
    }

    #[test]
    fn explicit_username_beats_directory_fallback() {
        let mut opened = OpenPassFile::from_label("/tmp/store", "work/alice/github");
        opened.refresh_from_contents("secret\nusername: bob\nurl: https://example.com");
        assert_eq!(opened.username.as_deref(), Some("bob"));
    }

    #[test]
    fn blank_username_uses_last_directory_fallback() {
        let mut opened = OpenPassFile::from_label("/tmp/store", "work/alice/github");
        opened.refresh_from_contents("secret\nusername:\nurl: https://example.com");
        assert_eq!(opened.username.as_deref(), Some("alice"));
    }
}
