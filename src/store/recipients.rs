use crate::preferences::Preferences;
use std::fs;
#[cfg(any(test, not(feature = "flatpak")))]
use std::{cell::RefCell, rc::Rc};

pub(crate) fn read_store_gpg_recipients(store_root: &str) -> Vec<String> {
    let path = std::path::Path::new(store_root).join(".gpg-id");
    let Ok(contents) = fs::read_to_string(path) else {
        return Vec::new();
    };

    parse_gpg_recipients(&contents)
}

pub(crate) fn store_gpg_recipients_subtitle(store_root: &str) -> String {
    let recipients = read_store_gpg_recipients(store_root);
    match recipients.len() {
        0 => "No recipients set".to_string(),
        1 => "1 recipient".to_string(),
        count => format!("{count} recipients"),
    }
}

pub(crate) fn suggested_gpg_recipients(settings: &Preferences) -> Vec<String> {
    for root in settings.paths() {
        let recipients = read_store_gpg_recipients(root.to_string_lossy().as_ref());
        if !recipients.is_empty() {
            return recipients;
        }
    }

    Vec::new()
}

#[cfg(any(test, not(feature = "flatpak")))]
pub(crate) fn append_gpg_recipients(recipients: &Rc<RefCell<Vec<String>>>, input: &str) -> bool {
    let parsed = parse_gpg_recipients(input);
    if parsed.is_empty() {
        return false;
    }

    let mut values = recipients.borrow_mut();
    let original_len = values.len();
    for recipient in parsed {
        if !values.iter().any(|existing| existing == &recipient) {
            values.push(recipient);
        }
    }

    values.len() > original_len
}

pub(crate) fn parse_gpg_recipients(value: &str) -> Vec<String> {
    let mut recipients = Vec::new();
    for recipient in value.split([',', ';', '\n']) {
        let recipient = normalize_gpg_recipient(recipient);
        if recipient.is_empty() || recipients.iter().any(|existing| existing == &recipient) {
            continue;
        }
        recipients.push(recipient);
    }
    recipients
}

pub(crate) fn normalize_gpg_recipient(value: &str) -> String {
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

pub(crate) fn stores_with_preferred_first(stores: &[String], preferred: &str) -> Vec<String> {
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
        append_gpg_recipients, normalize_gpg_recipient, parse_gpg_recipients,
        stores_with_preferred_first,
    };
    use std::{cell::RefCell, rc::Rc};

    #[test]
    fn gpg_recipients_are_trimmed_and_deduplicated() {
        assert_eq!(
            parse_gpg_recipients("alice@example.com; bob@example.com,\nalice@example.com"),
            vec![
                "alice@example.com".to_string(),
                "bob@example.com".to_string()
            ]
        );
    }

    #[test]
    fn gpg_fingerprints_drop_internal_spaces() {
        assert_eq!(
            normalize_gpg_recipient("7D FF 03 8D EE 12 AB 34"),
            "7DFF038DEE12AB34".to_string()
        );
    }

    #[test]
    fn gpg_user_ids_keep_internal_spaces() {
        assert_eq!(
            normalize_gpg_recipient("Alice Example <alice@example.com>"),
            "Alice Example <alice@example.com>".to_string()
        );
    }

    #[test]
    fn gpg_recipient_input_appends_unique_values() {
        let recipients = Rc::new(RefCell::new(vec!["alice@example.com".to_string()]));

        assert!(append_gpg_recipients(
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
}
