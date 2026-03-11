use crate::preferences::Preferences;

use std::collections::BTreeMap;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

const USERNAME_KEYS: [&str; 3] = ["login", "username", "user"];

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct CollectItemsOptions {
    pub show_hidden: bool,
    pub show_duplicates: bool,
}

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

    pub fn label_with_updated_path_username(&self, username: &str) -> Option<String> {
        let relative_path = self.relative_path.trim_end_matches('/');
        if relative_path.is_empty() {
            return None;
        }

        let basename = &self.basename;
        let username = username.trim();
        Some(match relative_path.rsplit_once('/') {
            Some((prefix, _)) if username.is_empty() => format!("{prefix}/{basename}"),
            Some((prefix, _)) => format!("{prefix}/{username}/{basename}"),
            None if username.is_empty() => basename.clone(),
            None => format!("{username}/{basename}"),
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum UsernameSource {
    None,
    Path,
    Field,
}

fn username_from_contents_or_path(
    entry: &PassEntry,
    output: &str,
) -> (Option<String>, UsernameSource) {
    if let Some(username) = extract_username_from_contents(output) {
        return (Some(username), UsernameSource::Field);
    }

    match entry.username_from_path() {
        Some(username) => (Some(username), UsernameSource::Path),
        None => (None, UsernameSource::None),
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OpenPassFile {
    pub entry: PassEntry,
    pub username: Option<String>,
    username_source: UsernameSource,
}

impl OpenPassFile {
    pub fn new(entry: PassEntry) -> Self {
        let (username, username_source) = username_from_contents_or_path(&entry, "");
        Self {
            entry,
            username,
            username_source,
        }
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

    pub fn username(&self) -> Option<&str> {
        self.username.as_deref()
    }

    pub fn store_path(&self) -> &str {
        &self.entry.store_path
    }

    pub fn username_is_from_path(&self) -> bool {
        matches!(self.username_source, UsernameSource::Path)
    }

    pub fn refresh_from_contents(&mut self, output: &str) {
        let (username, username_source) = username_from_contents_or_path(&self.entry, output);
        self.username = username;
        self.username_source = username_source;
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

pub fn collect_all_password_items_with_options(
    options: CollectItemsOptions,
) -> io::Result<Vec<PassEntry>> {
    let settings = Preferences::new();
    let roots = settings.paths();
    let mut result: Vec<PassEntry> = Vec::new();

    let mut i = 0;
    let len = roots.len();
    while i < len {
        let base = &roots[i];
        let _ = collect_items_in_dir(base.as_path(), base.as_path(), &mut result, options);
        i += 1;
    }

    result = filter_duplicate_store_entries(result, options.show_duplicates);

    result.sort_by(|left, right| {
        left.store_path
            .cmp(&right.store_path)
            .then_with(|| left.relative_path.cmp(&right.relative_path))
            .then_with(|| left.basename.cmp(&right.basename))
    });
    Ok(result)
}

fn collapse_duplicate_store_entries(items: Vec<PassEntry>) -> Vec<PassEntry> {
    let mut longest_path_items = BTreeMap::<PathBuf, PassEntry>::new();

    for item in items {
        let secret_path = absolute_secret_path(&item);
        match longest_path_items.entry(secret_path) {
            std::collections::btree_map::Entry::Vacant(entry) => {
                entry.insert(item);
            }
            std::collections::btree_map::Entry::Occupied(mut entry) => {
                if should_prefer_duplicate_entry(entry.get(), &item) {
                    entry.insert(item);
                }
            }
        }
    }

    longest_path_items.into_values().collect()
}

fn filter_duplicate_store_entries(items: Vec<PassEntry>, show_duplicates: bool) -> Vec<PassEntry> {
    if show_duplicates {
        items
    } else {
        collapse_duplicate_store_entries(items)
    }
}

fn absolute_secret_path(item: &PassEntry) -> PathBuf {
    Path::new(&item.store_path)
        .join(&item.relative_path)
        .join(format!("{}.gpg", item.basename))
}

fn should_prefer_duplicate_entry(current: &PassEntry, candidate: &PassEntry) -> bool {
    let current_len = current.store_path.len();
    let candidate_len = candidate.store_path.len();
    candidate_len > current_len
        || (candidate_len == current_len && candidate.store_path.cmp(&current.store_path).is_gt())
}

fn is_hidden_name(path: &Path) -> bool {
    path.file_name()
        .and_then(|name| name.to_str())
        .is_some_and(|name| name.starts_with('.'))
}

fn secret_label_from_path(base: &Path, path: &Path) -> Option<String> {
    let relative = path.strip_prefix(base).ok()?;
    let relative = relative.to_string_lossy();
    relative.strip_suffix(".gpg").map(str::to_string)
}

fn collect_items_in_dir(
    root: &Path,
    base: &Path,
    out: &mut Vec<PassEntry>,
    options: CollectItemsOptions,
) -> io::Result<()> {
    if !root.exists() {
        return Ok(());
    }

    let entries = fs::read_dir(root)?;

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
            if !options.show_hidden && is_hidden_name(&path) {
                continue;
            }
            let _ = collect_items_in_dir(path.as_path(), base, out, options);
            continue;
        }

        if !file_type.is_file() || (!options.show_hidden && is_hidden_name(&path)) {
            continue;
        }

        let Some(label) = secret_label_from_path(base, &path) else {
            continue;
        };
        if label.is_empty() {
            continue;
        }

        out.push(PassEntry::from_label(
            base.to_string_lossy().to_string(),
            label,
        ));
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{
        collapse_duplicate_store_entries, collect_items_in_dir, filter_duplicate_store_entries,
        CollectItemsOptions, OpenPassFile, PassEntry,
    };
    use std::fs;
    use std::time::{SystemTime, UNIX_EPOCH};

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
    fn path_usernames_update_only_the_last_directory_segment() {
        let entry = PassEntry::from_label("/tmp/store", "work/alice/github");
        assert_eq!(
            entry.label_with_updated_path_username("bob").as_deref(),
            Some("work/bob/github")
        );
        assert_eq!(
            entry.label_with_updated_path_username("").as_deref(),
            Some("work/github")
        );
    }

    #[test]
    fn explicit_username_beats_directory_fallback() {
        let mut opened = OpenPassFile::from_label("/tmp/store", "work/alice/github");
        opened.refresh_from_contents("secret\nusername: bob\nurl: https://example.com");
        assert_eq!(opened.username.as_deref(), Some("bob"));
        assert!(!opened.username_is_from_path());
    }

    #[test]
    fn blank_username_uses_last_directory_fallback() {
        let mut opened = OpenPassFile::from_label("/tmp/store", "work/alice/github");
        opened.refresh_from_contents("secret\nusername:\nurl: https://example.com");
        assert_eq!(opened.username.as_deref(), Some("alice"));
        assert!(opened.username_is_from_path());
    }

    #[test]
    fn dotted_secret_names_keep_their_full_label() {
        let entry = PassEntry::from_label("/tmp/store", "chat/matrix.org");
        assert_eq!(entry.basename, "matrix.org");
        assert_eq!(entry.label(), "chat/matrix.org");
    }

    #[test]
    fn hidden_entries_are_filtered_by_default() {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock before unix epoch")
            .as_nanos();
        let store = std::env::temp_dir().join(format!("passwordstore-hidden-{nanos}"));
        fs::create_dir_all(store.join(".hidden")).expect("create hidden store dir");
        fs::create_dir_all(store.join("visible")).expect("create visible store dir");
        fs::write(store.join(".top-secret.gpg"), b"x").expect("write hidden secret");
        fs::write(store.join(".hidden").join("inside.gpg"), b"x")
            .expect("write nested hidden secret");
        fs::write(store.join("visible").join("entry.gpg"), b"x").expect("write visible secret");
        fs::write(store.join("notes.txt"), b"x").expect("write non-secret file");

        let mut items = Vec::new();
        collect_items_in_dir(
            &store,
            &store,
            &mut items,
            CollectItemsOptions {
                show_hidden: false,
                show_duplicates: false,
            },
        )
        .expect("collect visible secrets");
        let labels = items
            .into_iter()
            .map(|item| item.label())
            .collect::<Vec<_>>();

        assert_eq!(labels, vec!["visible/entry".to_string()]);

        fs::remove_dir_all(store).expect("remove test store");
    }

    #[test]
    fn hidden_entries_can_be_included() {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock before unix epoch")
            .as_nanos();
        let store = std::env::temp_dir().join(format!("passwordstore-hidden-show-{nanos}"));
        fs::create_dir_all(store.join(".hidden")).expect("create hidden store dir");
        fs::write(store.join(".top-secret.gpg"), b"x").expect("write hidden secret");
        fs::write(store.join(".hidden").join("inside.gpg"), b"x")
            .expect("write nested hidden secret");

        let mut items = Vec::new();
        collect_items_in_dir(
            &store,
            &store,
            &mut items,
            CollectItemsOptions {
                show_hidden: true,
                show_duplicates: false,
            },
        )
        .expect("collect all secrets");
        let mut labels = items
            .into_iter()
            .map(|item| item.label())
            .collect::<Vec<_>>();
        labels.sort();

        assert_eq!(
            labels,
            vec![".hidden/inside".to_string(), ".top-secret".to_string()]
        );

        fs::remove_dir_all(store).expect("remove test store");
    }

    #[test]
    fn duplicate_entries_keep_the_deepest_store_root() {
        let items = vec![
            PassEntry::from_label("/tmp/store", "nested/github"),
            PassEntry::from_label("/tmp/store/nested", "github"),
        ];

        let collapsed = collapse_duplicate_store_entries(items);

        assert_eq!(
            collapsed,
            vec![PassEntry::from_label("/tmp/store/nested", "github")]
        );
    }

    #[test]
    fn duplicate_entries_can_be_left_visible() {
        let items = vec![
            PassEntry::from_label("/tmp/store", "nested/github"),
            PassEntry::from_label("/tmp/store/nested", "github"),
        ];

        assert_eq!(filter_duplicate_store_entries(items.clone(), true), items);
    }
}
