use super::keys::{
    ensure_ripasso_private_key_is_ready, fingerprint_from_string,
    imported_private_key_fingerprints, load_stored_ripasso_key_ring, missing_private_key_error,
    ripasso_private_key_requires_session_unlock, selected_ripasso_own_fingerprint,
};
use super::paths::recipients_file_for_label;
use crate::backend::{PasswordEntryError, StoreRecipientsPrivateKeyRequirement};
use ripasso::pass::{Comment, KeyRingStatus, OwnerTrustLevel, Recipient};
use sequoia_openpgp::{Cert, KeyHandle};
use std::collections::{HashMap, HashSet};
use std::fs;
use std::sync::Arc;

const REQUIRE_ALL_PRIVATE_KEYS_METADATA: &str = "keycord-private-key-requirement=all";

pub(super) struct ResolvedRecipient<'a> {
    pub(super) fingerprint: [u8; 20],
    pub(super) cert: &'a Arc<Cert>,
    pub(super) requested_id: String,
}

impl ResolvedRecipient<'_> {
    fn fingerprint_hex(&self) -> String {
        self.cert.fingerprint().to_hex()
    }
}

fn resolve_recipient_cert<'a>(
    recipient_id: &str,
    key_ring: &'a HashMap<[u8; 20], Arc<Cert>>,
) -> Option<([u8; 20], &'a Arc<Cert>)> {
    if let Ok(fingerprint) = fingerprint_from_string(recipient_id) {
        if let Some(cert) = key_ring.get(&fingerprint) {
            return Some((fingerprint, cert));
        }
    }

    if let Ok(handle) = recipient_id.parse::<KeyHandle>() {
        for (fingerprint, cert) in key_ring {
            if cert.key_handle().aliases(&handle) {
                return Some((*fingerprint, cert));
            }
        }
    }

    let needle = recipient_id.trim().to_ascii_lowercase();
    if needle.is_empty() {
        return None;
    }

    for (fingerprint, cert) in key_ring {
        if cert.userids().any(|user_id| {
            let user_id = user_id.userid().to_string();
            let user_id = user_id.trim().to_ascii_lowercase();
            user_id == needle || user_id.contains(&format!("<{needle}>"))
        }) {
            return Some((*fingerprint, cert));
        }
    }

    None
}

fn resolved_recipients_from_contents<'a>(
    contents: &str,
    key_ring: &'a HashMap<[u8; 20], Arc<Cert>>,
) -> Result<Vec<ResolvedRecipient<'a>>, String> {
    let mut recipients = Vec::new();
    let mut seen = HashSet::new();

    for line in recipient_ids_from_contents(contents) {
        let Some((fingerprint, cert)) = resolve_recipient_cert(&line, key_ring) else {
            return Err(format!("Recipient '{line}' is not available in the app."));
        };
        if !seen.insert(fingerprint) {
            continue;
        }

        recipients.push(ResolvedRecipient {
            fingerprint,
            cert,
            requested_id: line,
        });
    }

    Ok(recipients)
}

fn recipient_ids_from_contents(contents: &str) -> Vec<String> {
    let mut recipients = Vec::new();

    for raw_line in contents.lines() {
        let line = raw_line
            .split_once('#')
            .map_or(raw_line, |(key, _)| key)
            .trim();
        if line.is_empty() {
            continue;
        }

        recipients.push(line.to_string());
    }

    recipients
}

fn metadata_line_matches(line: &str, expected: &str) -> bool {
    line.trim()
        .strip_prefix('#')
        .map(str::trim)
        .is_some_and(|value| value.eq_ignore_ascii_case(expected))
}

pub(super) fn private_key_requirement_from_contents(
    contents: &str,
) -> StoreRecipientsPrivateKeyRequirement {
    for line in contents.lines() {
        if metadata_line_matches(line, REQUIRE_ALL_PRIVATE_KEYS_METADATA) {
            return StoreRecipientsPrivateKeyRequirement::AllManagedKeys;
        }
    }

    StoreRecipientsPrivateKeyRequirement::AnyManagedKey
}

pub(super) fn recipient_contents(
    recipients: &[String],
    private_key_requirement: StoreRecipientsPrivateKeyRequirement,
) -> String {
    let mut lines = Vec::with_capacity(recipients.len() + 1);
    if matches!(
        private_key_requirement,
        StoreRecipientsPrivateKeyRequirement::AllManagedKeys
    ) {
        lines.push(format!("# {REQUIRE_ALL_PRIVATE_KEYS_METADATA}"));
    }
    lines.extend(recipients.iter().cloned());
    format!("{}\n", lines.join("\n"))
}

pub(super) fn required_private_key_fingerprints_from_contents(
    contents: &str,
    key_ring: &HashMap<[u8; 20], Arc<Cert>>,
) -> Result<Vec<String>, String> {
    Ok(resolved_recipients_from_contents(contents, key_ring)?
        .into_iter()
        .map(|recipient| recipient.fingerprint_hex())
        .collect())
}

pub(super) fn encryption_context_fingerprint_from_contents(
    contents: &str,
    key_ring: &HashMap<[u8; 20], Arc<Cert>>,
) -> Result<String, String> {
    let recipients = resolved_recipients_from_contents(contents, key_ring)?;
    if let Some(selected) = selected_ripasso_own_fingerprint()? {
        if recipients
            .iter()
            .any(|recipient| recipient.fingerprint_hex().eq_ignore_ascii_case(&selected))
        {
            return Ok(selected);
        }
    }

    recipients
        .into_iter()
        .next()
        .map(|recipient| recipient.fingerprint_hex())
        .ok_or_else(|| "No recipients were found for this password entry.".to_string())
}

pub(super) fn recipients_for_encryption_from_contents(
    contents: &str,
    key_ring: &HashMap<[u8; 20], Arc<Cert>>,
) -> Result<Vec<Recipient>, String> {
    let mut recipients = Vec::new();

    for recipient in resolved_recipients_from_contents(contents, key_ring)? {
        let name = recipient
            .cert
            .userids()
            .map(|user_id| user_id.userid().to_string())
            .find(|value| !value.trim().is_empty())
            .unwrap_or_else(|| recipient.requested_id.clone());

        recipients.push(Recipient {
            name,
            comment: Comment {
                pre_comment: None,
                post_comment: None,
            },
            key_id: recipient.cert.fingerprint().to_hex(),
            fingerprint: Some(recipient.fingerprint),
            key_ring_status: KeyRingStatus::InKeyRing,
            trust_level: OwnerTrustLevel::Ultimate,
            not_usable: false,
        });
    }

    Ok(recipients)
}

fn push_unique_fingerprint(fingerprints: &mut Vec<String>, candidate: String) {
    if fingerprints
        .iter()
        .any(|existing| existing.eq_ignore_ascii_case(&candidate))
    {
        return;
    }

    fingerprints.push(candidate);
}

fn recipient_fingerprints_for_label(store_root: &str, label: &str) -> Result<Vec<String>, String> {
    let recipients_file = recipients_file_for_label(store_root, label)?;
    let contents = fs::read_to_string(recipients_file).map_err(|err| err.to_string())?;
    let key_ring = load_stored_ripasso_key_ring()?;

    required_private_key_fingerprints_from_contents(&contents, &key_ring)
}

pub(super) fn private_key_requirement_for_label(
    store_root: &str,
    label: &str,
) -> Result<StoreRecipientsPrivateKeyRequirement, String> {
    let recipients_file = recipients_file_for_label(store_root, label)?;
    let contents = fs::read_to_string(recipients_file).map_err(|err| err.to_string())?;
    Ok(private_key_requirement_from_contents(&contents))
}

pub(super) fn required_private_key_fingerprints_for_label(
    store_root: &str,
    label: &str,
) -> Result<Vec<String>, String> {
    recipient_fingerprints_for_label(store_root, label)
}

pub fn password_entry_is_readable(store_root: &str, label: &str) -> bool {
    let Ok(recipients_file) = recipients_file_for_label(store_root, label) else {
        return false;
    };
    let Ok(contents) = fs::read_to_string(recipients_file) else {
        return false;
    };
    let private_key_requirement = private_key_requirement_from_contents(&contents);
    let Ok(key_ring) = load_stored_ripasso_key_ring() else {
        return false;
    };

    let recipient_ids = recipient_ids_from_contents(&contents);
    if recipient_ids.is_empty() {
        return false;
    }

    match private_key_requirement {
        StoreRecipientsPrivateKeyRequirement::AnyManagedKey => {
            recipient_ids.into_iter().any(|id| {
                resolve_recipient_cert(&id, &key_ring).is_some_and(|(_, cert)| {
                    private_key_is_openable_with_unlock(&cert.fingerprint().to_hex())
                })
            })
        }
        StoreRecipientsPrivateKeyRequirement::AllManagedKeys => {
            let mut seen = HashSet::new();
            for id in recipient_ids {
                let Some((fingerprint, cert)) = resolve_recipient_cert(&id, &key_ring) else {
                    return false;
                };
                if !seen.insert(fingerprint) {
                    continue;
                }
                if !private_key_is_openable_with_unlock(&cert.fingerprint().to_hex()) {
                    return false;
                }
            }
            true
        }
    }
}

fn private_key_is_openable_with_unlock(fingerprint: &str) -> bool {
    matches!(
        ensure_ripasso_private_key_is_ready(fingerprint),
        Ok(()) | Err(PasswordEntryError::LockedPrivateKey(_))
    )
}

pub(super) fn decryption_candidate_fingerprints_for_entry(
    store_root: &str,
    label: &str,
) -> Result<Vec<String>, String> {
    if matches!(
        private_key_requirement_for_label(store_root, label),
        Ok(StoreRecipientsPrivateKeyRequirement::AllManagedKeys)
    ) {
        return required_private_key_fingerprints_for_label(store_root, label);
    }

    let mut candidates = Vec::new();

    if let Ok(fingerprints) = recipient_fingerprints_for_label(store_root, label) {
        for fingerprint in fingerprints {
            push_unique_fingerprint(&mut candidates, fingerprint);
        }
    }

    if let Some(fingerprint) = selected_ripasso_own_fingerprint()? {
        push_unique_fingerprint(&mut candidates, fingerprint);
    }

    for fingerprint in imported_private_key_fingerprints()? {
        push_unique_fingerprint(&mut candidates, fingerprint);
    }

    Ok(candidates)
}

pub fn preferred_ripasso_private_key_fingerprint_for_entry(
    store_root: &str,
    label: &str,
) -> Result<String, String> {
    let candidates = decryption_candidate_fingerprints_for_entry(store_root, label)?;
    for fingerprint in &candidates {
        if matches!(
            ripasso_private_key_requires_session_unlock(fingerprint),
            Ok(true)
        ) {
            return Ok(fingerprint.clone());
        }
    }

    candidates
        .into_iter()
        .next()
        .ok_or_else(missing_private_key_error)
}
