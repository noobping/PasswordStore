use super::keys::{
    direct_binding_from_store_recipient, ensure_ripasso_private_key_is_ready,
    fingerprint_from_string, imported_private_key_fingerprints, load_stored_ripasso_key_ring,
    missing_private_key_error, ripasso_private_key_requires_session_unlock,
    selected_ripasso_own_fingerprint, Fido2DirectBinding,
};
use super::paths::recipients_file_for_label;
use crate::backend::{PasswordEntryError, StoreRecipientsPrivateKeyRequirement};
use crate::fido2_recipient::{is_fido2_recipient_string, parse_fido2_recipient_metadata_line};
use sequoia_openpgp::{Cert, KeyHandle};
use std::collections::{HashMap, HashSet};
use std::fs;
use std::sync::Arc;

const REQUIRE_ALL_PRIVATE_KEYS_METADATA: &str = "keycord-private-key-requirement=all";

pub(super) enum ResolvedRecipient<'a> {
    Standard {
        fingerprint: [u8; 20],
        cert: &'a Arc<Cert>,
        requested_id: String,
    },
    Fido2 {
        binding: Fido2DirectBinding,
        requested_id: String,
    },
}

impl ResolvedRecipient<'_> {
    pub(super) fn recipient_id(&self) -> String {
        match self {
            Self::Standard { cert, .. } => cert.fingerprint().to_hex(),
            Self::Fido2 { requested_id, .. } => requested_id.clone(),
        }
    }

    pub(super) fn cert(&self) -> Option<&Arc<Cert>> {
        match self {
            Self::Standard { cert, .. } => Some(cert),
            Self::Fido2 { .. } => None,
        }
    }

    pub(super) fn fido2_binding(&self) -> Option<Fido2DirectBinding> {
        match self {
            Self::Standard { .. } => None,
            Self::Fido2 { binding, .. } => Some(binding.clone()),
        }
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

pub(super) fn resolved_recipients_from_contents<'a>(
    contents: &str,
    key_ring: &'a HashMap<[u8; 20], Arc<Cert>>,
) -> Result<Vec<ResolvedRecipient<'a>>, String> {
    let mut recipients = Vec::new();
    let mut seen_standard = HashSet::new();
    let mut seen_fido2 = HashSet::new();

    for recipient_id in recipient_ids_from_contents(contents)? {
        if is_fido2_recipient_string(&recipient_id) {
            let Some(binding) = direct_binding_from_store_recipient(&recipient_id)? else {
                return Err(format!(
                    "Recipient '{recipient_id}' is not available in the app."
                ));
            };
            if !seen_fido2.insert(binding.fingerprint.clone()) {
                continue;
            }
            recipients.push(ResolvedRecipient::Fido2 {
                binding,
                requested_id: recipient_id,
            });
            continue;
        }

        let Some((fingerprint, cert)) = resolve_recipient_cert(&recipient_id, key_ring) else {
            return Err(format!(
                "Recipient '{recipient_id}' is not available in the app."
            ));
        };
        if !seen_standard.insert(fingerprint) {
            continue;
        }
        recipients.push(ResolvedRecipient::Standard {
            fingerprint,
            cert,
            requested_id: recipient_id,
        });
    }

    Ok(recipients)
}

fn recipient_ids_from_contents(contents: &str) -> Result<Vec<String>, String> {
    let mut recipients = Vec::new();

    for raw_line in contents.lines() {
        if let Some(recipient) = parse_fido2_recipient_metadata_line(raw_line)? {
            recipients.push(recipient);
            continue;
        }

        let line = raw_line
            .split_once('#')
            .map_or(raw_line, |(key, _)| key)
            .trim();
        if line.is_empty() {
            continue;
        }

        recipients.push(line.to_string());
    }

    Ok(recipients)
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
    for recipient in recipients {
        if is_fido2_recipient_string(recipient) {
            lines.push(format!("# {recipient}"));
        } else {
            lines.push(recipient.clone());
        }
    }
    format!("{}\n", lines.join("\n"))
}

pub(super) fn required_private_key_fingerprints_from_contents(
    contents: &str,
    key_ring: &HashMap<[u8; 20], Arc<Cert>>,
) -> Result<Vec<String>, String> {
    Ok(resolved_recipients_from_contents(contents, key_ring)?
        .into_iter()
        .map(|recipient| recipient.recipient_id())
        .collect())
}

pub(super) fn encryption_context_fingerprint_from_contents(
    contents: &str,
    key_ring: &HashMap<[u8; 20], Arc<Cert>>,
) -> Result<String, String> {
    let recipients = resolved_recipients_from_contents(contents, key_ring)?;
    if let Some(selected) = selected_ripasso_own_fingerprint()? {
        if recipients.iter().any(|recipient| {
            recipient
                .cert()
                .is_some_and(|cert| cert.fingerprint().to_hex().eq_ignore_ascii_case(&selected))
        }) {
            return Ok(selected);
        }
    }

    if let Some(fingerprint) = recipients
        .iter()
        .find_map(|recipient| recipient.cert().map(|cert| cert.fingerprint().to_hex()))
    {
        return Ok(fingerprint);
    }

    recipients
        .into_iter()
        .next()
        .map(|recipient| recipient.recipient_id())
        .ok_or_else(|| "No recipients were found for this password entry.".to_string())
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
    let Ok(recipient_ids) = recipient_ids_from_contents(&contents) else {
        return false;
    };
    if recipient_ids.is_empty() {
        return false;
    }

    match private_key_requirement {
        StoreRecipientsPrivateKeyRequirement::AnyManagedKey => {
            recipient_ids.into_iter().any(|id| {
                if is_fido2_recipient_string(&id) {
                    return private_key_is_openable_with_unlock(&id);
                }

                resolve_recipient_cert(&id, &key_ring).is_some_and(|(_, cert)| {
                    private_key_is_openable_with_unlock(&cert.fingerprint().to_hex())
                })
            })
        }
        StoreRecipientsPrivateKeyRequirement::AllManagedKeys => {
            let mut seen = HashSet::new();
            for id in recipient_ids {
                if is_fido2_recipient_string(&id) {
                    if !seen.insert(id.clone()) {
                        continue;
                    }
                    if !private_key_is_openable_with_unlock(&id) {
                        return false;
                    }
                    continue;
                }

                let Some((_, cert)) = resolve_recipient_cert(&id, &key_ring) else {
                    return false;
                };
                if !seen.insert(cert.fingerprint().to_hex()) {
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
    if is_fido2_recipient_string(fingerprint) {
        return true;
    }

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
