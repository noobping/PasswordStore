use super::super::keys::{
    fingerprint_from_string, imported_private_key_fingerprints, load_stored_ripasso_key_ring,
    missing_private_key_error, selected_ripasso_own_fingerprint,
};
use super::paths::recipients_file_for_label;
use ripasso::pass::{Comment, KeyRingStatus, OwnerTrustLevel, Recipient};
use sequoia_openpgp::{Cert, KeyHandle};
use std::collections::{HashMap, HashSet};
use std::fs;
use std::sync::Arc;

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

    for raw_line in contents.lines() {
        let line = raw_line
            .split_once('#')
            .map(|(key, _)| key)
            .unwrap_or(raw_line)
            .trim();
        if line.is_empty() {
            continue;
        }

        let Some((fingerprint, cert)) = resolve_recipient_cert(line, key_ring) else {
            return Err(format!("Recipient '{line}' is not available in the app."));
        };
        if !seen.insert(fingerprint) {
            continue;
        }

        recipients.push(ResolvedRecipient {
            fingerprint,
            cert,
            requested_id: line.to_string(),
        });
    }

    Ok(recipients)
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

    Ok(resolved_recipients_from_contents(&contents, &key_ring)?
        .into_iter()
        .map(|recipient| recipient.fingerprint_hex())
        .collect())
}

pub(super) fn decryption_candidate_fingerprints_for_entry(
    store_root: &str,
    label: &str,
) -> Result<Vec<String>, String> {
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

pub(crate) fn preferred_ripasso_private_key_fingerprint_for_entry(
    store_root: &str,
    label: &str,
) -> Result<String, String> {
    decryption_candidate_fingerprints_for_entry(store_root, label)?
        .into_iter()
        .next()
        .ok_or_else(missing_private_key_error)
}
