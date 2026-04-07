use crate::backend::{StoreRecipients, StoreRecipientsPrivateKeyRequirement};
use crate::fido2_recipient::{
    build_fido2_recipient_string, is_fido2_recipient_string, parse_fido2_recipient_metadata_line,
    parse_fido2_recipient_string, FIDO2_RECIPIENTS_FILE_NAME,
};
use crate::i18n::gettext;
use crate::support::runtime::{supports_fidostore_features, supports_nested_recipients_features};
use std::fs;
use std::path::{Component, Path, PathBuf};
#[cfg(test)]
use std::{cell::RefCell, rc::Rc};
use walkdir::WalkDir;

const REQUIRE_ALL_PRIVATE_KEYS_METADATA: &str = "keycord-private-key-requirement=all";
pub const UNSUPPORTED_FIDOSTORE_MESSAGE: &str = "This build doesn't support FIDO2-backed stores.";
pub const ROOT_STORE_RECIPIENTS_SCOPE: &str = ".";

fn normalized_store_recipients_scope(scope: &str) -> String {
    if !supports_nested_recipients_features() {
        return ROOT_STORE_RECIPIENTS_SCOPE.to_string();
    }

    let trimmed = scope.trim();
    if trimmed.is_empty() || trimmed == ROOT_STORE_RECIPIENTS_SCOPE {
        return ROOT_STORE_RECIPIENTS_SCOPE.to_string();
    }

    let mut relative = PathBuf::new();
    for component in Path::new(trimmed).components() {
        match component {
            Component::Normal(part) => relative.push(part),
            Component::CurDir => {}
            _ => return ROOT_STORE_RECIPIENTS_SCOPE.to_string(),
        }
    }

    if relative.as_os_str().is_empty() {
        ROOT_STORE_RECIPIENTS_SCOPE.to_string()
    } else {
        relative.to_string_lossy().to_string()
    }
}

fn store_recipients_scope_directory(store_root: &str, scope: &str) -> PathBuf {
    let normalized = normalized_store_recipients_scope(scope);
    let mut path = PathBuf::from(store_root);
    if normalized != ROOT_STORE_RECIPIENTS_SCOPE {
        path.push(normalized);
    }
    path
}

fn standard_recipients_path_for_scope(store_root: &str, scope: &str) -> PathBuf {
    store_recipients_scope_directory(store_root, scope).join(".gpg-id")
}

fn fido2_recipients_path_for_scope(store_root: &str, scope: &str) -> PathBuf {
    store_recipients_scope_directory(store_root, scope).join(FIDO2_RECIPIENTS_FILE_NAME)
}

pub fn read_store_standard_recipients(store_root: &str) -> Vec<String> {
    read_store_standard_recipients_for_scope(store_root, ROOT_STORE_RECIPIENTS_SCOPE)
}

pub fn read_store_standard_recipients_for_scope(store_root: &str, scope: &str) -> Vec<String> {
    let path = standard_recipients_path_for_scope(store_root, scope);
    let Ok(contents) = fs::read_to_string(path) else {
        return Vec::new();
    };

    parse_standard_recipients(&contents)
}

pub fn read_store_fido2_recipients(store_root: &str) -> Vec<String> {
    read_store_fido2_recipients_for_scope(store_root, ROOT_STORE_RECIPIENTS_SCOPE)
}

pub fn read_store_fido2_recipients_for_scope(store_root: &str, scope: &str) -> Vec<String> {
    let path = fido2_recipients_path_for_scope(store_root, scope);
    let Ok(contents) = fs::read_to_string(path) else {
        return Vec::new();
    };

    parse_fido2_recipients(&contents)
}

pub fn read_store_recipients(store_root: &str) -> Vec<String> {
    let mut recipients = read_store_standard_recipients(store_root);
    recipients.extend(read_store_fido2_recipients(store_root));
    recipients
}

pub fn read_store_recipients_for_scope(store_root: &str, scope: &str) -> Vec<String> {
    let mut recipients = read_store_standard_recipients_for_scope(store_root, scope);
    recipients.extend(read_store_fido2_recipients_for_scope(store_root, scope));
    recipients
}

pub fn store_uses_fido2_recipients(store_root: &str) -> bool {
    !read_store_fido2_recipients(store_root).is_empty()
}

pub fn store_is_supported_in_current_build(store_root: &str) -> bool {
    supports_fidostore_features() || !store_uses_fido2_recipients(store_root)
}

pub fn read_store_private_key_requirement(
    store_root: &str,
) -> StoreRecipientsPrivateKeyRequirement {
    read_store_private_key_requirement_for_scope(store_root, ROOT_STORE_RECIPIENTS_SCOPE)
}

pub fn read_store_private_key_requirement_for_scope(
    store_root: &str,
    scope: &str,
) -> StoreRecipientsPrivateKeyRequirement {
    let path = standard_recipients_path_for_scope(store_root, scope);
    let Ok(contents) = fs::read_to_string(path) else {
        return StoreRecipientsPrivateKeyRequirement::AnyManagedKey;
    };

    for line in contents.lines() {
        if line
            .trim()
            .strip_prefix('#')
            .map(str::trim)
            .is_some_and(|value| value.eq_ignore_ascii_case(REQUIRE_ALL_PRIVATE_KEYS_METADATA))
        {
            return StoreRecipientsPrivateKeyRequirement::AllManagedKeys;
        }
    }

    StoreRecipientsPrivateKeyRequirement::AnyManagedKey
}

fn root_store_standard_recipients_contents(store_root: &str) -> String {
    fs::read_to_string(standard_recipients_path_for_scope(
        store_root,
        ROOT_STORE_RECIPIENTS_SCOPE,
    ))
    .unwrap_or_default()
}

fn store_recipients_scope_from_path(store_root: &Path, recipients_path: &Path) -> Option<String> {
    let directory = recipients_path.parent()?;
    let relative = directory.strip_prefix(store_root).ok()?;
    if relative.as_os_str().is_empty() {
        Some(ROOT_STORE_RECIPIENTS_SCOPE.to_string())
    } else {
        Some(relative.to_string_lossy().to_string())
    }
}

pub fn relevant_store_recipient_scopes(store_root: &str) -> Vec<String> {
    let store_root = Path::new(store_root);
    let root_path = store_root.join(".gpg-id");
    if !supports_nested_recipients_features() {
        return if root_path.is_file() {
            vec![ROOT_STORE_RECIPIENTS_SCOPE.to_string()]
        } else {
            Vec::new()
        };
    }

    let root_contents =
        root_store_standard_recipients_contents(store_root.to_string_lossy().as_ref());
    let mut scopes = Vec::new();

    if root_path.is_file() {
        scopes.push(ROOT_STORE_RECIPIENTS_SCOPE.to_string());
    }

    for entry in WalkDir::new(store_root).into_iter().filter_map(Result::ok) {
        if !entry.file_type().is_file() || entry.file_name() != ".gpg-id" {
            continue;
        }
        if entry.path() == root_path {
            continue;
        }
        let Ok(contents) = fs::read_to_string(entry.path()) else {
            continue;
        };
        if contents == root_contents {
            continue;
        }
        let Some(scope) = store_recipients_scope_from_path(store_root, entry.path()) else {
            continue;
        };
        if scopes.iter().any(|existing| existing == &scope) {
            continue;
        }
        scopes.push(scope);
    }

    scopes.sort_by(|left, right| {
        if left == ROOT_STORE_RECIPIENTS_SCOPE {
            std::cmp::Ordering::Less
        } else if right == ROOT_STORE_RECIPIENTS_SCOPE {
            std::cmp::Ordering::Greater
        } else {
            left.cmp(right)
        }
    });
    scopes
}

pub fn store_recipients_subtitle(store_root: &str) -> String {
    if !store_is_supported_in_current_build(store_root) {
        return gettext(UNSUPPORTED_FIDOSTORE_MESSAGE);
    }

    let recipients = read_store_recipients(store_root);
    match recipients.len() {
        0 => gettext("No recipients set"),
        1 => gettext("1 recipient"),
        count => gettext("{count} recipients").replace("{count}", &count.to_string()),
    }
}

fn push_unique_recipient(recipients: &mut Vec<String>, recipient: String) {
    if recipient.is_empty() || recipients.iter().any(|existing| existing == &recipient) {
        return;
    }

    recipients.push(recipient);
}

pub fn split_store_recipients(recipients: &[String]) -> StoreRecipients {
    let mut standard = Vec::new();
    let mut fido2 = Vec::new();

    for recipient in recipients {
        if is_fido2_recipient_string(recipient) {
            push_unique_recipient(&mut fido2, recipient.clone());
        } else {
            push_unique_recipient(&mut standard, recipient.clone());
        }
    }

    StoreRecipients::new(standard, fido2)
}

#[cfg(test)]
pub fn append_standard_recipients(recipients: &Rc<RefCell<Vec<String>>>, input: &str) -> bool {
    let parsed = parse_standard_recipients(input);
    if parsed.is_empty() {
        return false;
    }

    let mut values = recipients.borrow_mut();
    let original_len = values.len();
    for recipient in parsed {
        push_unique_recipient(&mut values, recipient);
    }

    values.len() > original_len
}

pub fn parse_standard_recipients(value: &str) -> Vec<String> {
    let mut recipients = Vec::new();

    for line in value.lines() {
        for recipient in line.split([',', ';']) {
            let recipient = recipient
                .split_once('#')
                .map_or(recipient, |(value, _)| value);
            let recipient = recipient.trim();
            if is_fido2_recipient_string(recipient) {
                continue;
            }
            let recipient = normalize_standard_recipient(recipient);
            push_unique_recipient(&mut recipients, recipient);
        }
    }

    recipients
}

pub fn parse_fido2_recipients(value: &str) -> Vec<String> {
    let mut recipients = Vec::new();

    for raw_line in value.lines() {
        let line = raw_line.trim();
        if line.is_empty() {
            continue;
        }

        if let Ok(Some(recipient)) = parse_fido2_recipient_metadata_line(line) {
            push_unique_recipient(&mut recipients, recipient);
            continue;
        }

        if let Ok(Some(parsed)) = parse_fido2_recipient_string(line) {
            if let Ok(normalized) =
                build_fido2_recipient_string(&parsed.id, &parsed.label, &parsed.credential_id)
            {
                push_unique_recipient(&mut recipients, normalized);
            }
        }
    }

    recipients
}

pub fn normalize_standard_recipient(value: &str) -> String {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return String::new();
    }

    let compact = trimmed
        .chars()
        .filter(|c| !c.is_ascii_whitespace())
        .collect::<String>();
    if trimmed.contains(char::is_whitespace) && compact.chars().all(|c| c.is_ascii_hexdigit()) {
        compact
    } else {
        trimmed.to_string()
    }
}

pub fn stores_with_preferred_first(stores: &[String], preferred: &str) -> Vec<String> {
    let mut ordered = vec![preferred.to_string()];
    for store in stores {
        if store != preferred {
            ordered.push(store.clone());
        }
    }
    ordered
}

#[cfg(test)]
mod tests {
    use super::{
        append_standard_recipients, normalize_standard_recipient, parse_fido2_recipients,
        parse_standard_recipients, read_store_private_key_requirement_for_scope,
        read_store_recipients_for_scope, relevant_store_recipient_scopes, split_store_recipients,
        store_is_supported_in_current_build, store_recipients_subtitle,
        store_uses_fido2_recipients, stores_with_preferred_first, ROOT_STORE_RECIPIENTS_SCOPE,
        UNSUPPORTED_FIDOSTORE_MESSAGE,
    };
    use crate::backend::{StoreRecipients, StoreRecipientsPrivateKeyRequirement};
    use crate::fido2_recipient::{
        build_fido2_recipient_string, derived_fido2_recipient_id, FIDO2_RECIPIENTS_FILE_NAME,
    };
    use crate::i18n::gettext;
    use std::{
        cell::RefCell,
        fs,
        rc::Rc,
        time::{SystemTime, UNIX_EPOCH},
    };

    fn test_fido2_recipient(label: &str, credential_id: &[u8]) -> String {
        build_fido2_recipient_string(
            &derived_fido2_recipient_id(credential_id),
            label,
            credential_id,
        )
        .expect("build recipient")
    }

    #[test]
    fn standard_recipients_are_trimmed_and_deduplicated() {
        assert_eq!(
            parse_standard_recipients("alice@example.com; bob@example.com,\nalice@example.com"),
            vec![
                "alice@example.com".to_string(),
                "bob@example.com".to_string()
            ]
        );
    }

    #[test]
    fn standard_fingerprints_drop_internal_spaces() {
        assert_eq!(
            normalize_standard_recipient("7D FF 03 8D EE 12 AB 34"),
            "7DFF038DEE12AB34".to_string()
        );
    }

    #[test]
    fn standard_user_ids_keep_internal_spaces() {
        assert_eq!(
            normalize_standard_recipient("Alice Example <alice@example.com>"),
            "Alice Example <alice@example.com>".to_string()
        );
    }

    #[test]
    fn standard_recipient_comments_are_ignored() {
        assert_eq!(
            parse_standard_recipients(
                "# keycord-private-key-requirement=all\nalice@example.com # preferred\nbob@example.com"
            ),
            vec![
                "alice@example.com".to_string(),
                "bob@example.com".to_string()
            ]
        );
    }

    #[test]
    fn scope_reads_use_the_requested_relative_directory() {
        let timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system time before unix epoch")
            .as_nanos();
        let store = std::env::temp_dir().join(format!("keycord-store-scope-{timestamp}"));
        let fido2_recipient = test_fido2_recipient("Desk Key", b"cred");
        fs::create_dir_all(store.join("team")).expect("create store directories");
        fs::write(store.join(".gpg-id"), "root@example.com\n").expect("write root recipients");
        fs::write(
            store.join("team/.gpg-id"),
            "# keycord-private-key-requirement=all\nnested@example.com\n",
        )
        .expect("write nested recipients");
        fs::write(
            store.join("team").join(FIDO2_RECIPIENTS_FILE_NAME),
            format!("{fido2_recipient}\n"),
        )
        .expect("write nested FIDO2 recipients");

        assert_eq!(
            read_store_recipients_for_scope(
                store.to_string_lossy().as_ref(),
                ROOT_STORE_RECIPIENTS_SCOPE
            ),
            vec!["root@example.com".to_string()]
        );
        assert_eq!(
            read_store_recipients_for_scope(store.to_string_lossy().as_ref(), "team"),
            vec!["nested@example.com".to_string(), fido2_recipient]
        );
        assert_eq!(
            read_store_private_key_requirement_for_scope(store.to_string_lossy().as_ref(), "team"),
            StoreRecipientsPrivateKeyRequirement::AllManagedKeys
        );

        fs::remove_dir_all(store).expect("remove temporary store");
    }

    #[test]
    fn relevant_scopes_include_only_root_and_nested_files_that_differ_from_root() {
        let timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system time before unix epoch")
            .as_nanos();
        let store = std::env::temp_dir().join(format!("keycord-store-relevant-scopes-{timestamp}"));
        fs::create_dir_all(store.join("team/shared")).expect("create nested store directories");
        fs::create_dir_all(store.join("team/custom"))
            .expect("create second nested store directory");
        fs::write(store.join(".gpg-id"), "root@example.com\n").expect("write root recipients");
        fs::write(store.join("team/shared/.gpg-id"), "root@example.com\n")
            .expect("write identical nested recipients");
        fs::write(store.join("team/custom/.gpg-id"), "custom@example.com\n")
            .expect("write custom nested recipients");

        assert_eq!(
            relevant_store_recipient_scopes(store.to_string_lossy().as_ref()),
            vec![
                ROOT_STORE_RECIPIENTS_SCOPE.to_string(),
                "team/custom".to_string()
            ]
        );

        fs::remove_dir_all(store).expect("remove temporary store");
    }

    #[test]
    fn fido2_recipient_metadata_lines_are_preserved() {
        let recipient =
            build_fido2_recipient_string(&derived_fido2_recipient_id(b"cred"), "Desk Key", b"cred")
                .expect("build recipient");
        let value = format!("# {recipient}");
        assert_eq!(parse_fido2_recipients(&value), vec![recipient]);
    }

    #[test]
    fn standard_recipient_input_appends_unique_values() {
        let recipients = Rc::new(RefCell::new(vec!["alice@example.com".to_string()]));

        assert!(append_standard_recipients(
            &recipients,
            "alice@example.com; bob@example.com, carol@example.com"
        ));
        assert_eq!(
            recipients.borrow().clone(),
            vec![
                "alice@example.com".to_string(),
                "bob@example.com".to_string(),
                "carol@example.com".to_string()
            ]
        );
    }

    #[test]
    fn store_recipients_are_split_by_type() {
        let recipient = test_fido2_recipient("Desk Key", b"cred");
        let recipients = vec!["alice@example.com".to_string(), recipient.clone()];

        assert_eq!(
            split_store_recipients(&recipients),
            StoreRecipients::new(vec!["alice@example.com".to_string()], vec![recipient])
        );
    }

    #[test]
    fn preferred_store_moves_to_the_front_once() {
        let stores = vec![
            "/tmp/one".to_string(),
            "/tmp/two".to_string(),
            "/tmp/three".to_string(),
        ];
        assert_eq!(
            stores_with_preferred_first(&stores, "/tmp/two"),
            vec![
                "/tmp/two".to_string(),
                "/tmp/one".to_string(),
                "/tmp/three".to_string()
            ]
        );
    }

    #[test]
    fn store_fido2_usage_detection_follows_the_sidecar_file() {
        let recipient = test_fido2_recipient("Desk Key", b"cred");
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock should be after epoch")
            .as_nanos();
        let store_root = std::env::temp_dir().join(format!("keycord-store-recipients-{unique}"));
        fs::create_dir_all(&store_root).expect("store root should be created");

        assert!(!store_uses_fido2_recipients(
            store_root.to_str().expect("utf8 temp path")
        ));

        fs::write(
            store_root.join(crate::fido2_recipient::FIDO2_RECIPIENTS_FILE_NAME),
            format!("{recipient}\n"),
        )
        .expect("fido2 recipients file should be written");

        assert!(store_uses_fido2_recipients(
            store_root.to_str().expect("utf8 temp path")
        ));

        let _ = fs::remove_dir_all(store_root);
    }

    #[test]
    fn store_support_matches_the_fidostore_feature_flag() {
        let recipient = test_fido2_recipient("Desk Key", b"cred");
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock should be after epoch")
            .as_nanos();
        let store_root = std::env::temp_dir().join(format!("keycord-store-support-{unique}"));
        fs::create_dir_all(&store_root).expect("store root should be created");
        fs::write(
            store_root.join(crate::fido2_recipient::FIDO2_RECIPIENTS_FILE_NAME),
            format!("{recipient}\n"),
        )
        .expect("fido2 recipients file should be written");

        assert_eq!(
            store_is_supported_in_current_build(store_root.to_str().expect("utf8 temp path")),
            crate::support::runtime::supports_fidostore_features()
        );

        let _ = fs::remove_dir_all(store_root);
    }

    #[test]
    fn unsupported_fido_store_subtitle_explains_the_build_limit() {
        let recipient = test_fido2_recipient("Desk Key", b"cred");
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock should be after epoch")
            .as_nanos();
        let store_root =
            std::env::temp_dir().join(format!("keycord-store-subtitle-unsupported-{unique}"));
        fs::create_dir_all(&store_root).expect("store root should be created");
        fs::write(
            store_root.join(crate::fido2_recipient::FIDO2_RECIPIENTS_FILE_NAME),
            format!("{recipient}\n"),
        )
        .expect("fido2 recipients file should be written");

        if crate::support::runtime::supports_fidostore_features() {
            assert_ne!(
                store_recipients_subtitle(store_root.to_str().expect("utf8 temp path")),
                gettext(UNSUPPORTED_FIDOSTORE_MESSAGE)
            );
        } else {
            assert_eq!(
                store_recipients_subtitle(store_root.to_str().expect("utf8 temp path")),
                gettext(UNSUPPORTED_FIDOSTORE_MESSAGE)
            );
        }

        let _ = fs::remove_dir_all(store_root);
    }
}
