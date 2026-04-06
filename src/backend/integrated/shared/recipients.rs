use super::keys::{
    available_private_key_fingerprints, direct_binding_from_store_recipient,
    ensure_ripasso_private_key_is_ready, fingerprint_from_string, load_available_standard_key_ring,
    missing_private_key_error, selected_ripasso_own_fingerprint, Fido2DirectBinding,
};
use super::paths::{fido2_recipients_file_for_recipients_path, recipients_file_for_label};
use crate::backend::{PasswordEntryError, StoreRecipientsPrivateKeyRequirement};
use crate::fido2_recipient::{
    build_fido2_recipient_string, is_fido2_recipient_string, parse_fido2_recipient_metadata_line,
    parse_fido2_recipient_string,
};
use sequoia_openpgp::{Cert, KeyHandle};
use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::Path;
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

type StandardRecipientMatch<'a> = ([u8; 20], &'a Arc<Cert>);

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
) -> Result<Option<StandardRecipientMatch<'a>>, String> {
    if let Ok(fingerprint) = fingerprint_from_string(recipient_id) {
        if let Some(cert) = key_ring.get(&fingerprint) {
            return Ok(Some((fingerprint, cert)));
        }
    }

    if let Ok(handle) = recipient_id.parse::<KeyHandle>() {
        if let Some(resolved) = resolve_unique_standard_recipient_match(
            recipient_id,
            key_ring
                .iter()
                .filter(|(_, cert)| cert.key_handle().aliases(&handle))
                .map(|(fingerprint, cert)| (*fingerprint, cert)),
        )? {
            return Ok(Some(resolved));
        }
    }

    let Some(needle) = normalized_standard_recipient_lookup(recipient_id) else {
        return Ok(None);
    };

    resolve_unique_standard_recipient_match(
        recipient_id,
        key_ring
            .iter()
            .filter(|(_, cert)| {
                cert.userids().any(|user_id| {
                    standard_recipient_matches_user_id(&needle, &user_id.userid().to_string())
                })
            })
            .map(|(fingerprint, cert)| (*fingerprint, cert)),
    )
}

fn resolve_unique_standard_recipient_match<'a>(
    recipient_id: &str,
    mut matches: impl Iterator<Item = StandardRecipientMatch<'a>>,
) -> Result<Option<StandardRecipientMatch<'a>>, String> {
    let Some(first) = matches.next() else {
        return Ok(None);
    };
    if matches.next().is_some() {
        return Err(format!(
            "Recipient '{recipient_id}' matches multiple keys in the app. Use a fingerprint instead."
        ));
    }

    Ok(Some(first))
}

fn normalized_standard_recipient_lookup(value: &str) -> Option<String> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed.to_ascii_lowercase())
    }
}

fn extracted_user_id_email(user_id: &str) -> Option<&str> {
    let trimmed = user_id.trim();
    let start = trimmed.rfind('<')?;
    let after_start = &trimmed[start + 1..];
    let end = after_start.find('>')?;
    let remainder = &after_start[end + 1..];
    if !remainder.trim().is_empty() {
        return None;
    }

    let email = after_start[..end].trim();
    if email.is_empty() {
        None
    } else {
        Some(email)
    }
}

fn standard_recipient_matches_user_id(needle: &str, user_id: &str) -> bool {
    normalized_standard_recipient_lookup(user_id).is_some_and(|candidate| candidate == needle)
        || extracted_user_id_email(user_id)
            .and_then(normalized_standard_recipient_lookup)
            .is_some_and(|email| email == needle)
}

fn resolved_standard_recipients_from_contents<'a>(
    contents: &str,
    key_ring: &'a HashMap<[u8; 20], Arc<Cert>>,
) -> Result<Vec<ResolvedRecipient<'a>>, String> {
    let mut recipients = Vec::new();
    let mut seen_standard = HashSet::new();

    for recipient_id in standard_recipient_ids_from_contents(contents) {
        let Some((fingerprint, cert)) = resolve_recipient_cert(&recipient_id, key_ring)? else {
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

fn resolved_fido2_recipients_from_contents<'a>(
    contents: &str,
) -> Result<Vec<ResolvedRecipient<'a>>, String> {
    let mut recipients = Vec::new();
    let mut seen_fido2 = HashSet::new();

    for recipient_id in parse_fido2_recipient_file_contents(contents)? {
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
    }

    Ok(recipients)
}

pub(super) fn resolved_recipients_from_contents<'a>(
    standard_contents: &str,
    fido2_contents: &str,
    key_ring: &'a HashMap<[u8; 20], Arc<Cert>>,
) -> Result<Vec<ResolvedRecipient<'a>>, String> {
    let mut recipients = resolved_standard_recipients_from_contents(standard_contents, key_ring)?;
    recipients.extend(resolved_fido2_recipients_from_contents(fido2_contents)?);
    Ok(recipients)
}

fn standard_recipient_ids_from_contents(contents: &str) -> Vec<String> {
    let mut recipients = Vec::new();

    for raw_line in contents.lines() {
        let line = raw_line
            .split_once('#')
            .map_or(raw_line, |(key, _)| key)
            .trim();
        if line.is_empty() || recipients.iter().any(|existing| existing == line) {
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

fn parse_fido2_recipient_file_contents(contents: &str) -> Result<Vec<String>, String> {
    let mut recipients = Vec::new();

    for raw_line in contents.lines() {
        let line = raw_line.trim();
        if line.is_empty() {
            continue;
        }

        if let Some(recipient) = parse_fido2_recipient_metadata_line(line)? {
            if !recipients.iter().any(|existing| existing == &recipient) {
                recipients.push(recipient);
            }
            continue;
        }

        let Some(parsed) = parse_fido2_recipient_string(line)? else {
            return Err("Invalid FIDO2 recipient file.".to_string());
        };
        let normalized =
            build_fido2_recipient_string(&parsed.id, &parsed.label, &parsed.credential_id)?;
        if !recipients.iter().any(|existing| existing == &normalized) {
            recipients.push(normalized);
        }
    }

    Ok(recipients)
}

pub(super) fn standard_recipient_file_contents(
    standard_recipients: &[String],
    private_key_requirement: StoreRecipientsPrivateKeyRequirement,
) -> String {
    let mut lines = Vec::with_capacity(standard_recipients.len() + 1);
    if matches!(
        private_key_requirement,
        StoreRecipientsPrivateKeyRequirement::AllManagedKeys
    ) {
        lines.push(format!("# {REQUIRE_ALL_PRIVATE_KEYS_METADATA}"));
    }
    for recipient in standard_recipients {
        lines.push(recipient.clone());
    }
    format!("{}\n", lines.join("\n"))
}

pub(super) fn fido2_recipient_file_contents(fido2_recipients: &[String]) -> String {
    if fido2_recipients.is_empty() {
        return String::new();
    }

    format!("{}\n", fido2_recipients.join("\n"))
}

fn read_standard_recipient_file_contents(recipients_file: &Path) -> Result<String, String> {
    fs::read_to_string(recipients_file).map_err(|err| err.to_string())
}

fn read_fido2_recipient_file_contents(recipients_file: &Path) -> Result<String, String> {
    let fido2_recipients_path = fido2_recipients_file_for_recipients_path(recipients_file);
    match fs::read_to_string(fido2_recipients_path) {
        Ok(contents) => Ok(contents),
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => Ok(String::new()),
        Err(err) => Err(err.to_string()),
    }
}

pub(super) fn read_store_recipient_file_contents(
    recipients_file: &Path,
) -> Result<(String, String), String> {
    Ok((
        read_standard_recipient_file_contents(recipients_file)?,
        read_fido2_recipient_file_contents(recipients_file)?,
    ))
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

pub(super) fn effective_private_key_requirement(
    configured_requirement: StoreRecipientsPrivateKeyRequirement,
    standard_recipient_count: usize,
    fido2_recipient_count: usize,
) -> StoreRecipientsPrivateKeyRequirement {
    if matches!(
        configured_requirement,
        StoreRecipientsPrivateKeyRequirement::AllManagedKeys
    ) || (standard_recipient_count == 0 && fido2_recipient_count > 1)
    {
        StoreRecipientsPrivateKeyRequirement::AllManagedKeys
    } else {
        StoreRecipientsPrivateKeyRequirement::AnyManagedKey
    }
}

fn effective_private_key_requirement_from_contents(
    standard_contents: &str,
    fido2_contents: &str,
) -> Result<StoreRecipientsPrivateKeyRequirement, String> {
    Ok(effective_private_key_requirement(
        private_key_requirement_from_contents(standard_contents),
        standard_recipient_ids_from_contents(standard_contents).len(),
        parse_fido2_recipient_file_contents(fido2_contents)?.len(),
    ))
}

pub(super) fn required_private_key_fingerprints_from_contents(
    standard_contents: &str,
    fido2_contents: &str,
    key_ring: &HashMap<[u8; 20], Arc<Cert>>,
) -> Result<Vec<String>, String> {
    Ok(
        resolved_recipients_from_contents(standard_contents, fido2_contents, key_ring)?
            .into_iter()
            .map(|recipient| recipient.recipient_id())
            .collect(),
    )
}

pub(super) fn encryption_context_fingerprint_from_contents(
    standard_contents: &str,
    fido2_contents: &str,
    key_ring: &HashMap<[u8; 20], Arc<Cert>>,
) -> Result<String, String> {
    let recipients =
        resolved_recipients_from_contents(standard_contents, fido2_contents, key_ring)?;
    let standard_fingerprints = recipients
        .iter()
        .filter_map(|recipient| recipient.cert().map(|cert| cert.fingerprint().to_hex()))
        .collect::<Vec<_>>();
    let mut preferred_standard_fingerprints =
        Vec::with_capacity(standard_fingerprints.len().saturating_add(1));
    if let Some(selected) = selected_ripasso_own_fingerprint()?.filter(|selected| {
        standard_fingerprints
            .iter()
            .any(|fingerprint| fingerprint.eq_ignore_ascii_case(selected))
    }) {
        preferred_standard_fingerprints.push(selected);
    }
    preferred_standard_fingerprints.extend(standard_fingerprints);

    if let Some(fingerprint) = prioritized_unique_fingerprints(preferred_standard_fingerprints)
        .into_iter()
        .next()
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

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum PrivateKeyUsePriority {
    Ready,
    Unlockable,
    Unavailable,
}

fn private_key_use_priority(fingerprint: &str) -> PrivateKeyUsePriority {
    if is_fido2_recipient_string(fingerprint) {
        return PrivateKeyUsePriority::Unlockable;
    }

    match ensure_ripasso_private_key_is_ready(fingerprint) {
        Ok(()) => PrivateKeyUsePriority::Ready,
        Err(PasswordEntryError::LockedPrivateKey(_)) => PrivateKeyUsePriority::Unlockable,
        Err(_) => PrivateKeyUsePriority::Unavailable,
    }
}

fn prioritized_unique_fingerprints(candidates: impl IntoIterator<Item = String>) -> Vec<String> {
    let mut unique = Vec::new();
    for candidate in candidates {
        push_unique_fingerprint(&mut unique, candidate);
    }

    let mut ready = Vec::new();
    let mut unlockable = Vec::new();
    let mut unavailable = Vec::new();

    for candidate in unique {
        match private_key_use_priority(&candidate) {
            PrivateKeyUsePriority::Ready => ready.push(candidate),
            PrivateKeyUsePriority::Unlockable => unlockable.push(candidate),
            PrivateKeyUsePriority::Unavailable => unavailable.push(candidate),
        }
    }

    ready.extend(unlockable);
    ready.extend(unavailable);
    ready
}

fn recipient_fingerprints_for_label(store_root: &str, label: &str) -> Result<Vec<String>, String> {
    let recipients_file = recipients_file_for_label(store_root, label)?;
    let (standard_contents, fido2_contents) = read_store_recipient_file_contents(&recipients_file)?;
    let key_ring = load_available_standard_key_ring()?;

    required_private_key_fingerprints_from_contents(&standard_contents, &fido2_contents, &key_ring)
}

pub(super) fn private_key_requirement_for_label(
    store_root: &str,
    label: &str,
) -> Result<StoreRecipientsPrivateKeyRequirement, String> {
    let recipients_file = recipients_file_for_label(store_root, label)?;
    let (standard_contents, fido2_contents) = read_store_recipient_file_contents(&recipients_file)?;
    effective_private_key_requirement_from_contents(&standard_contents, &fido2_contents)
}

pub fn required_private_key_fingerprints_for_entry(
    store_root: &str,
    label: &str,
) -> Result<Vec<String>, String> {
    recipient_fingerprints_for_label(store_root, label)
}

pub(super) fn password_entry_fido2_recipient_count(
    store_root: &str,
    label: &str,
) -> Result<usize, String> {
    let recipients_file = recipients_file_for_label(store_root, label)?;
    let (_, fido2_contents) = read_store_recipient_file_contents(&recipients_file)?;
    Ok(parse_fido2_recipient_file_contents(&fido2_contents)?.len())
}

pub fn password_entry_is_readable(store_root: &str, label: &str) -> bool {
    let Ok(recipients_file) = recipients_file_for_label(store_root, label) else {
        return false;
    };
    let Ok((standard_contents, fido2_contents)) =
        read_store_recipient_file_contents(&recipients_file)
    else {
        return false;
    };
    let Ok(private_key_requirement) =
        effective_private_key_requirement_from_contents(&standard_contents, &fido2_contents)
    else {
        return false;
    };
    let Ok(key_ring) = load_available_standard_key_ring() else {
        return false;
    };
    let standard_recipient_ids = standard_recipient_ids_from_contents(&standard_contents);
    let Ok(fido2_recipient_ids) = parse_fido2_recipient_file_contents(&fido2_contents) else {
        return false;
    };
    if standard_recipient_ids.is_empty() && fido2_recipient_ids.is_empty() {
        return false;
    }

    match private_key_requirement {
        StoreRecipientsPrivateKeyRequirement::AnyManagedKey => {
            standard_recipient_ids.into_iter().any(|id| {
                resolve_recipient_cert(&id, &key_ring)
                    .ok()
                    .flatten()
                    .is_some_and(|(_, cert)| {
                        private_key_is_openable_with_unlock(&cert.fingerprint().to_hex())
                    })
            }) || fido2_recipient_ids
                .into_iter()
                .any(|id| private_key_is_openable_with_unlock(&id))
        }
        StoreRecipientsPrivateKeyRequirement::AllManagedKeys => {
            let mut seen_standard = HashSet::new();
            for id in standard_recipient_ids {
                let Ok(Some((_, cert))) = resolve_recipient_cert(&id, &key_ring) else {
                    return false;
                };
                if !seen_standard.insert(cert.fingerprint().to_hex()) {
                    continue;
                }
                if !private_key_is_openable_with_unlock(&cert.fingerprint().to_hex()) {
                    return false;
                }
            }

            let mut seen_fido2 = HashSet::new();
            for id in fido2_recipient_ids {
                if !seen_fido2.insert(id.clone()) {
                    continue;
                }
                if !private_key_is_openable_with_unlock(&id) {
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
        return required_private_key_fingerprints_for_entry(store_root, label);
    }

    let recipient_fingerprints =
        recipient_fingerprints_for_label(store_root, label).unwrap_or_default();
    let selected_fingerprint = selected_ripasso_own_fingerprint()?;
    let available_fingerprints = available_private_key_fingerprints()?;
    let mut candidates = Vec::with_capacity(
        recipient_fingerprints
            .len()
            .saturating_add(available_fingerprints.len())
            .saturating_add(2),
    );

    if let Some(selected) = selected_fingerprint.as_ref().filter(|selected| {
        recipient_fingerprints
            .iter()
            .any(|fingerprint| fingerprint.eq_ignore_ascii_case(selected))
    }) {
        candidates.push(selected.clone());
    }
    candidates.extend(recipient_fingerprints);
    if let Some(selected) = selected_fingerprint {
        candidates.push(selected);
    }
    candidates.extend(available_fingerprints);

    Ok(prioritized_unique_fingerprints(candidates))
}

pub fn preferred_ripasso_private_key_fingerprint_for_entry(
    store_root: &str,
    label: &str,
) -> Result<String, String> {
    decryption_candidate_fingerprints_for_entry(store_root, label)?
        .into_iter()
        .next()
        .ok_or_else(missing_private_key_error)
}

#[cfg(test)]
mod tests {
    use super::{
        effective_private_key_requirement, resolve_recipient_cert,
        resolved_standard_recipients_from_contents,
    };
    use crate::backend::StoreRecipientsPrivateKeyRequirement;
    use sequoia_openpgp::{cert::CertBuilder, Cert};
    use std::collections::HashMap;
    use std::sync::Arc;

    fn test_key_ring(user_ids: &[&str]) -> HashMap<[u8; 20], Arc<Cert>> {
        user_ids
            .iter()
            .map(|user_id| {
                let (cert, _) = CertBuilder::general_purpose(Some(*user_id))
                    .generate()
                    .expect("generate test certificate");
                let fingerprint = crate::backend::integrated::keys::fingerprint_from_string(
                    &cert.fingerprint().to_hex(),
                )
                .expect("parse fingerprint");
                (fingerprint, Arc::new(cert))
            })
            .collect()
    }

    #[test]
    fn pure_multi_fido2_stores_effectively_require_all_keys() {
        assert_eq!(
            effective_private_key_requirement(
                StoreRecipientsPrivateKeyRequirement::AnyManagedKey,
                0,
                2,
            ),
            StoreRecipientsPrivateKeyRequirement::AllManagedKeys
        );
        assert_eq!(
            effective_private_key_requirement(
                StoreRecipientsPrivateKeyRequirement::AnyManagedKey,
                1,
                2,
            ),
            StoreRecipientsPrivateKeyRequirement::AnyManagedKey
        );
        assert_eq!(
            effective_private_key_requirement(
                StoreRecipientsPrivateKeyRequirement::AllManagedKeys,
                0,
                1,
            ),
            StoreRecipientsPrivateKeyRequirement::AllManagedKeys
        );
    }

    #[test]
    fn exact_email_matches_only_a_unique_cert() {
        let key_ring = test_key_ring(&["Alice Example <alice@example.com>"]);
        let resolved = resolve_recipient_cert("alice@example.com", &key_ring)
            .expect("resolve recipient")
            .expect("expected a matching certificate");

        assert_eq!(resolved.1.fingerprint().to_hex().len(), 40);
    }

    #[test]
    fn ambiguous_email_matches_are_rejected() {
        let key_ring = test_key_ring(&[
            "Alice One <shared@example.com>",
            "Alice Two <shared@example.com>",
        ]);

        assert_eq!(
            resolve_recipient_cert("shared@example.com", &key_ring).unwrap_err(),
            "Recipient 'shared@example.com' matches multiple keys in the app. Use a fingerprint instead."
                .to_string()
        );
        assert_eq!(
            resolved_standard_recipients_from_contents("shared@example.com\n", &key_ring)
                .err()
                .expect("expected ambiguity error"),
            "Recipient 'shared@example.com' matches multiple keys in the app. Use a fingerprint instead."
                .to_string()
        );
    }

    #[test]
    fn user_id_fragments_do_not_match_by_substring() {
        let key_ring = test_key_ring(&["Alice Example <alice@example.com>"]);

        assert!(resolve_recipient_cert("example.com", &key_ring)
            .expect("resolve recipient")
            .is_none());
    }
}
