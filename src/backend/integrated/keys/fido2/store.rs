use super::super::cache::{borrow_pending_fido2_enrollment, clear_cached_fido2_pin};
#[cfg(not(feature = "fidostore"))]
use super::common::FIDO2_STORE_FEATURE_DISABLED_MESSAGE;
use super::common::{
    cached_pin_string, decode_base64, decrypt_aes_256_gcm, derive_direct_hmac_assertion_with_pin,
    derive_kek, encode_base64, encrypt_aes_256_gcm, parse_text_envelope, random_bytes,
    serialize_text_envelope, validate_direct_any_envelope, validate_direct_layer_envelope,
    Fido2AssertionOutput, Fido2DeviceLabel, Fido2DirectAnyManagedEnvelope, Fido2DirectBinding,
    Fido2DirectLayerEnvelope, Fido2DirectRecipientEnvelope, Fido2TransportError,
    Fido2WriteProgress, FIDO2_DIRECT_ANY_MANAGED_HEADER, FIDO2_DIRECT_ANY_MANAGED_KIND,
    FIDO2_DIRECT_ANY_PAYLOAD_AAD, FIDO2_DIRECT_ANY_WRAPPED_DEK_AAD_PREFIX,
    FIDO2_DIRECT_ENTRY_FORMAT, FIDO2_DIRECT_LAYER_AAD_PREFIX, FIDO2_DIRECT_LAYER_HEADER,
    FIDO2_DIRECT_LAYER_KIND, FIDO2_HMAC_SALT_LEN, FIDO2_RP_ID,
};
use crate::backend::PrivateKeyError;
use crate::fido2_recipient::{parse_fido2_recipient_string, Fido2StoreRecipient};
use secrecy::ExposeSecret;
use std::sync::mpsc;
use std::sync::{Arc, Barrier};
use std::thread;

const PASSWORD_ENTRY_CANDIDATE_MISMATCH: &str =
    "The available private keys cannot decrypt this item.";
const AES_GCM_NONCE_LEN: usize = 12;

#[derive(Clone, Debug, PartialEq, Eq)]
struct DirectAnyRecipientCandidate {
    fingerprint: String,
    rp_id: String,
    credential_id: Vec<u8>,
    hmac_salt: Vec<u8>,
    wrapped_dek_nonce: Vec<u8>,
    wrapped_dek: Vec<u8>,
}

#[cfg(feature = "fidostore")]
pub fn create_fido2_store_recipient(pin: Option<&str>) -> Result<String, PrivateKeyError> {
    super::common::create_fido2_binding(pin)
}

#[cfg(not(feature = "fidostore"))]
pub fn create_fido2_store_recipient(_pin: Option<&str>) -> Result<String, PrivateKeyError> {
    Err(PrivateKeyError::unsupported_fido2_key(
        FIDO2_STORE_FEATURE_DISABLED_MESSAGE,
    ))
}

#[cfg(all(feature = "fidostore", any(target_os = "linux", target_os = "windows")))]
fn unlock_fido2_binding_for_session(
    recipient: &str,
    pin: Option<&str>,
) -> Result<(), PrivateKeyError> {
    let recipient = parse_store_recipient_binding(recipient)
        .ok_or_else(|| PrivateKeyError::other("That FIDO2 store recipient is invalid."))?;
    let salt = random_bytes::<FIDO2_HMAC_SALT_LEN>();
    derive_direct_hmac_assertion_with_pin(
        &recipient.fingerprint,
        &recipient.rp_id,
        &recipient.credential_id,
        &salt,
        &[],
        pin,
    )
    .map_err(super::common::private_key_error_from_fido2_error)?;
    if let Some(pin) = pin {
        super::super::cache::cache_fido2_pin(&recipient.fingerprint, pin)
            .map_err(PrivateKeyError::other)?;
    }
    Ok(())
}

#[cfg(all(
    feature = "fidostore",
    not(any(target_os = "linux", target_os = "windows"))
))]
fn unlock_fido2_binding_for_session(
    _recipient: &str,
    _pin: Option<&str>,
) -> Result<(), PrivateKeyError> {
    Err(PrivateKeyError::unsupported_fido2_key(
        super::common::FIDO2_PLATFORM_UNSUPPORTED_MESSAGE,
    ))
}

#[cfg(feature = "fidostore")]
pub fn unlock_fido2_store_recipient_for_session(
    recipient: &str,
    pin: Option<&str>,
) -> Result<(), PrivateKeyError> {
    unlock_fido2_binding_for_session(recipient, pin)
}

#[cfg(not(feature = "fidostore"))]
pub fn unlock_fido2_store_recipient_for_session(
    _recipient: &str,
    _pin: Option<&str>,
) -> Result<(), PrivateKeyError> {
    Err(PrivateKeyError::unsupported_fido2_key(
        FIDO2_STORE_FEATURE_DISABLED_MESSAGE,
    ))
}

pub(in crate::backend::integrated) fn direct_binding_from_store_recipient(
    recipient: &str,
) -> Result<Option<Fido2DirectBinding>, String> {
    Ok(parse_fido2_recipient_string(recipient)?
        .map(|recipient| direct_binding_from_store_recipient_data(&recipient)))
}

pub(in crate::backend::integrated) fn encrypt_fido2_any_managed_bundle_with_progress(
    bindings: &[Fido2DirectBinding],
    dek: &[u8],
    payload: &[u8],
    pgp_wrapped_dek: Option<&[u8]>,
    mut report_progress: Option<&mut dyn FnMut(Fido2WriteProgress)>,
) -> Result<Vec<u8>, String> {
    if bindings.is_empty() && pgp_wrapped_dek.is_none() {
        return Err("No recipients were found for this password entry.".to_string());
    }

    let payload_nonce = random_bytes::<AES_GCM_NONCE_LEN>();
    let payload_ciphertext =
        encrypt_aes_256_gcm(dek, &payload_nonce, FIDO2_DIRECT_ANY_PAYLOAD_AAD, payload)
            .map_err(|err| err.to_string())?;

    let mut fido2_recipients = Vec::with_capacity(bindings.len());
    for (index, binding) in bindings.iter().enumerate() {
        if let Some(report_progress) = report_progress.as_deref_mut() {
            report_progress(Fido2WriteProgress {
                current_step: index + 1,
                total_steps: bindings.len(),
            });
        }
        fido2_recipients.push(build_direct_any_recipient_envelope(binding, dek)?);
    }

    serialize_text_envelope(
        FIDO2_DIRECT_ANY_MANAGED_HEADER,
        &Fido2DirectAnyManagedEnvelope {
            format: FIDO2_DIRECT_ENTRY_FORMAT,
            protection: FIDO2_DIRECT_ANY_MANAGED_KIND.to_string(),
            payload_nonce: encode_base64(&payload_nonce),
            payload_ciphertext: encode_base64(&payload_ciphertext),
            pgp_wrapped_dek: pgp_wrapped_dek.map(encode_base64),
            fido2_recipients,
        },
    )
}

pub(in crate::backend::integrated) fn reencrypt_fido2_any_managed_bundle_with_progress(
    bindings: &[Fido2DirectBinding],
    dek: &[u8],
    payload: &[u8],
    pgp_wrapped_dek: Option<&[u8]>,
    previous_ciphertext: &[u8],
    mut report_progress: Option<&mut dyn FnMut(Fido2WriteProgress)>,
) -> Result<Vec<u8>, String> {
    if bindings.is_empty() && pgp_wrapped_dek.is_none() {
        return Err("No recipients were found for this password entry.".to_string());
    }

    let Some(previous) = parse_text_envelope::<Fido2DirectAnyManagedEnvelope>(
        FIDO2_DIRECT_ANY_MANAGED_HEADER,
        previous_ciphertext,
    )?
    else {
        return Err("Invalid FIDO2 any-managed password entry.".to_string());
    };
    validate_direct_any_envelope(&previous)?;

    let payload_nonce = random_bytes::<AES_GCM_NONCE_LEN>();
    let payload_ciphertext =
        encrypt_aes_256_gcm(dek, &payload_nonce, FIDO2_DIRECT_ANY_PAYLOAD_AAD, payload)
            .map_err(|err| err.to_string())?;

    let mut preserved = previous
        .fido2_recipients
        .into_iter()
        .map(|recipient| (recipient.fingerprint.to_ascii_lowercase(), recipient))
        .collect::<std::collections::HashMap<_, _>>();
    let total_steps = bindings.iter().try_fold(0usize, |count, binding| {
        let needs_rewrap = match preserved.get(&binding.fingerprint.to_ascii_lowercase()) {
            Some(existing) => !direct_any_recipient_matches_binding(existing, binding)?,
            None => true,
        };
        Ok::<usize, String>(count + usize::from(needs_rewrap))
    })?;
    let mut fido2_recipients = Vec::with_capacity(bindings.len());
    let mut current_step = 0usize;
    for binding in bindings {
        if let Some(existing) = preserved.remove(&binding.fingerprint.to_ascii_lowercase()) {
            if direct_any_recipient_matches_binding(&existing, binding)? {
                fido2_recipients.push(existing);
                continue;
            }
        }
        current_step += 1;
        if let Some(report_progress) = report_progress.as_deref_mut() {
            report_progress(Fido2WriteProgress {
                current_step,
                total_steps,
            });
        }
        fido2_recipients.push(build_direct_any_recipient_envelope(binding, dek)?);
    }

    serialize_text_envelope(
        FIDO2_DIRECT_ANY_MANAGED_HEADER,
        &Fido2DirectAnyManagedEnvelope {
            format: FIDO2_DIRECT_ENTRY_FORMAT,
            protection: FIDO2_DIRECT_ANY_MANAGED_KIND.to_string(),
            payload_nonce: encode_base64(&payload_nonce),
            payload_ciphertext: encode_base64(&payload_ciphertext),
            pgp_wrapped_dek: pgp_wrapped_dek.map(encode_base64),
            fido2_recipients,
        },
    )
}

pub(in crate::backend::integrated) fn decrypt_fido2_any_managed_bundle_for_fingerprint(
    fingerprint: &str,
    ciphertext: &[u8],
) -> Result<Vec<u8>, String> {
    let dek = decrypt_fido2_any_managed_bundle_dek_for_fingerprint(fingerprint, ciphertext)?;
    let payload = decrypt_payload_from_any_managed_bundle(ciphertext, &dek)?;
    Ok(payload)
}

pub(in crate::backend::integrated) fn decrypt_fido2_any_managed_bundle_dek_for_fingerprint(
    fingerprint: &str,
    ciphertext: &[u8],
) -> Result<Vec<u8>, String> {
    let Some(envelope) = parse_text_envelope::<Fido2DirectAnyManagedEnvelope>(
        FIDO2_DIRECT_ANY_MANAGED_HEADER,
        ciphertext,
    )?
    else {
        return Err("Invalid FIDO2 any-managed password entry.".to_string());
    };
    validate_direct_any_envelope(&envelope)?;

    let recipient = direct_any_recipient_candidate_for_fingerprint(&envelope, fingerprint)?;
    decrypt_direct_any_recipient_candidate(&recipient)
}

pub(in crate::backend::integrated) fn decrypt_fido2_any_managed_bundle_dek_for_bindings(
    bindings: &[Fido2DirectBinding],
    ciphertext: &[u8],
) -> Result<Vec<u8>, String> {
    let Some(envelope) = parse_text_envelope::<Fido2DirectAnyManagedEnvelope>(
        FIDO2_DIRECT_ANY_MANAGED_HEADER,
        ciphertext,
    )?
    else {
        return Err("Invalid FIDO2 any-managed password entry.".to_string());
    };
    validate_direct_any_envelope(&envelope)?;

    let candidates = direct_any_recipient_candidates_for_bindings(&envelope, bindings)?;
    let Some((first_candidate, remaining_candidates)) = candidates.split_first() else {
        return Err(PASSWORD_ENTRY_CANDIDATE_MISMATCH.to_string());
    };
    if remaining_candidates.is_empty() {
        return decrypt_direct_any_recipient_candidate(first_candidate);
    }

    let (send, recv) = mpsc::channel();
    let start = Arc::new(Barrier::new(candidates.len()));
    for candidate in candidates {
        let send = send.clone();
        let start = start.clone();
        thread::spawn(move || {
            start.wait();
            let _ = send.send(decrypt_direct_any_recipient_candidate(&candidate));
        });
    }
    drop(send);

    let mut best_error = None;
    for result in recv {
        match result {
            Ok(dek) => return Ok(dek),
            Err(err) => best_error = prefer_direct_any_candidate_error(best_error, err),
        }
    }

    Err(best_error.unwrap_or_else(|| PASSWORD_ENTRY_CANDIDATE_MISMATCH.to_string()))
}

pub(in crate::backend::integrated) fn encrypt_fido2_direct_required_layer(
    binding: &Fido2DirectBinding,
    payload: &[u8],
) -> Result<Vec<u8>, String> {
    let (hmac_salt, hmac_secret) = direct_hmac_material_for_binding(binding)?;
    let kek = derive_kek(&hmac_secret, &binding.fingerprint, &hmac_salt)
        .map_err(|err| err.to_string())?;
    let payload_nonce = random_bytes::<AES_GCM_NONCE_LEN>();
    let payload_ciphertext = encrypt_aes_256_gcm(
        &kek,
        &payload_nonce,
        &direct_required_layer_aad(&binding.fingerprint),
        payload,
    )
    .map_err(|err| err.to_string())?;

    serialize_text_envelope(
        FIDO2_DIRECT_LAYER_HEADER,
        &Fido2DirectLayerEnvelope {
            format: FIDO2_DIRECT_ENTRY_FORMAT,
            protection: FIDO2_DIRECT_LAYER_KIND.to_string(),
            fingerprint: binding.fingerprint.clone(),
            rp_id: binding.rp_id.clone(),
            credential_id: encode_base64(&binding.credential_id),
            hmac_salt: encode_base64(&hmac_salt),
            payload_nonce: encode_base64(&payload_nonce),
            payload_ciphertext: encode_base64(&payload_ciphertext),
        },
    )
}

pub(in crate::backend::integrated) fn decrypt_fido2_direct_required_layer(
    expected_fingerprint: &str,
    ciphertext: &[u8],
) -> Result<Vec<u8>, String> {
    let Some(envelope) =
        parse_text_envelope::<Fido2DirectLayerEnvelope>(FIDO2_DIRECT_LAYER_HEADER, ciphertext)?
    else {
        return Err("Invalid FIDO2 required password-entry layer.".to_string());
    };
    validate_direct_layer_envelope(&envelope)?;
    if !envelope
        .fingerprint
        .eq_ignore_ascii_case(expected_fingerprint)
    {
        return Err("The available private keys cannot decrypt this item.".to_string());
    }

    let hmac_salt = decode_base64(&envelope.hmac_salt)?;
    let credential_id = decode_base64(&envelope.credential_id)?;
    let payload_nonce = decode_base64(&envelope.payload_nonce)?;
    let payload_ciphertext = decode_base64(&envelope.payload_ciphertext)?;
    decrypt_with_direct_hmac_secret_candidates(
        expected_fingerprint,
        &envelope.rp_id,
        &credential_id,
        &hmac_salt,
        |hmac_secret| {
            let kek = derive_kek(hmac_secret, &envelope.fingerprint, &hmac_salt)
                .map_err(|err| err.to_string())?;
            decrypt_aes_256_gcm(
                &kek,
                &payload_nonce,
                &direct_required_layer_aad(&envelope.fingerprint),
                &payload_ciphertext,
            )
            .map_err(|err| err.to_string())
        },
    )
}

pub(in crate::backend::integrated) fn decrypt_payload_from_any_managed_bundle(
    ciphertext: &[u8],
    dek: &[u8],
) -> Result<Vec<u8>, String> {
    let Some(envelope) = parse_text_envelope::<Fido2DirectAnyManagedEnvelope>(
        FIDO2_DIRECT_ANY_MANAGED_HEADER,
        ciphertext,
    )?
    else {
        return Err("Invalid FIDO2 any-managed password entry.".to_string());
    };
    validate_direct_any_envelope(&envelope)?;
    let payload_nonce = decode_base64(&envelope.payload_nonce)?;
    let payload_ciphertext = decode_base64(&envelope.payload_ciphertext)?;
    decrypt_aes_256_gcm(
        dek,
        &payload_nonce,
        FIDO2_DIRECT_ANY_PAYLOAD_AAD,
        &payload_ciphertext,
    )
    .map_err(|err| err.to_string())
}

fn direct_any_wrapped_dek_aad(fingerprint: &str) -> Vec<u8> {
    let mut aad = FIDO2_DIRECT_ANY_WRAPPED_DEK_AAD_PREFIX.to_vec();
    aad.extend_from_slice(fingerprint.as_bytes());
    aad
}

fn build_direct_any_recipient_envelope(
    binding: &Fido2DirectBinding,
    dek: &[u8],
) -> Result<Fido2DirectRecipientEnvelope, String> {
    let (hmac_salt, hmac_secret) = direct_hmac_material_for_binding(binding)?;
    let kek = derive_kek(&hmac_secret, &binding.fingerprint, &hmac_salt)
        .map_err(|err| err.to_string())?;
    let wrapped_dek_nonce = random_bytes::<AES_GCM_NONCE_LEN>();
    let wrapped_dek = encrypt_aes_256_gcm(
        &kek,
        &wrapped_dek_nonce,
        &direct_any_wrapped_dek_aad(&binding.fingerprint),
        dek,
    )
    .map_err(|err| err.to_string())?;
    Ok(Fido2DirectRecipientEnvelope {
        fingerprint: binding.fingerprint.clone(),
        rp_id: binding.rp_id.clone(),
        credential_id: encode_base64(&binding.credential_id),
        hmac_salt: encode_base64(&hmac_salt),
        wrapped_dek_nonce: encode_base64(&wrapped_dek_nonce),
        wrapped_dek: encode_base64(&wrapped_dek),
    })
}

fn direct_any_recipient_candidate_for_fingerprint(
    envelope: &Fido2DirectAnyManagedEnvelope,
    fingerprint: &str,
) -> Result<DirectAnyRecipientCandidate, String> {
    let recipient = envelope
        .fido2_recipients
        .iter()
        .find(|recipient| recipient.fingerprint.eq_ignore_ascii_case(fingerprint))
        .ok_or_else(|| PASSWORD_ENTRY_CANDIDATE_MISMATCH.to_string())?;
    direct_any_recipient_candidate(recipient)
}

fn direct_any_recipient_candidates_for_bindings(
    envelope: &Fido2DirectAnyManagedEnvelope,
    bindings: &[Fido2DirectBinding],
) -> Result<Vec<DirectAnyRecipientCandidate>, String> {
    let mut candidates = Vec::new();

    for binding in bindings {
        let Some(recipient) = envelope.fido2_recipients.iter().find(|recipient| {
            recipient
                .fingerprint
                .eq_ignore_ascii_case(&binding.fingerprint)
        }) else {
            continue;
        };

        if !direct_any_recipient_matches_binding(recipient, binding)? {
            continue;
        }

        candidates.push(direct_any_recipient_candidate(recipient)?);
    }

    Ok(candidates)
}

fn direct_any_recipient_candidate(
    recipient: &Fido2DirectRecipientEnvelope,
) -> Result<DirectAnyRecipientCandidate, String> {
    Ok(DirectAnyRecipientCandidate {
        fingerprint: recipient.fingerprint.clone(),
        rp_id: recipient.rp_id.clone(),
        credential_id: decode_base64(&recipient.credential_id)?,
        hmac_salt: decode_base64(&recipient.hmac_salt)?,
        wrapped_dek_nonce: decode_base64(&recipient.wrapped_dek_nonce)?,
        wrapped_dek: decode_base64(&recipient.wrapped_dek)?,
    })
}

fn direct_hmac_material_for_binding(
    binding: &Fido2DirectBinding,
) -> Result<(Vec<u8>, Vec<u8>), String> {
    if let Some(enrollment) = borrow_pending_fido2_enrollment(&binding.fingerprint)?
        .filter(|enrollment| enrollment.matches_credential_id(&binding.credential_id))
    {
        return Ok((
            enrollment.hmac_salt().to_vec(),
            enrollment.hmac_secret().to_vec(),
        ));
    }

    let hmac_salt = random_bytes::<{ FIDO2_HMAC_SALT_LEN }>().to_vec();
    let hmac_secret = derive_direct_hmac_secret(
        &binding.fingerprint,
        &binding.rp_id,
        &binding.credential_id,
        &hmac_salt,
    )?;
    Ok((hmac_salt, hmac_secret))
}

fn decrypt_direct_any_recipient_candidate(
    recipient: &DirectAnyRecipientCandidate,
) -> Result<Vec<u8>, String> {
    decrypt_with_direct_hmac_secret_candidates(
        &recipient.fingerprint,
        &recipient.rp_id,
        &recipient.credential_id,
        &recipient.hmac_salt,
        |hmac_secret| {
            let kek = derive_kek(hmac_secret, &recipient.fingerprint, &recipient.hmac_salt)
                .map_err(|err| err.to_string())?;
            decrypt_aes_256_gcm(
                &kek,
                &recipient.wrapped_dek_nonce,
                &direct_any_wrapped_dek_aad(&recipient.fingerprint),
                &recipient.wrapped_dek,
            )
            .map_err(|err| err.to_string())
        },
    )
}

fn direct_any_candidate_error_rank(message: &str) -> usize {
    if message.contains("Touch the FIDO2 security key") {
        0
    } else if message.contains("Enter the FIDO2 security key PIN")
        || message.contains("incorrect")
        || message.contains("locked")
    {
        1
    } else if message.contains("Reconnect the FIDO2 security key") {
        2
    } else if message.contains("Connect the matching FIDO2 security key") {
        3
    } else if message.contains("does not support the hmac-secret extension") {
        4
    } else if message == PASSWORD_ENTRY_CANDIDATE_MISMATCH {
        6
    } else {
        5
    }
}

fn prefer_direct_any_candidate_error(current: Option<String>, candidate: String) -> Option<String> {
    match current {
        None => Some(candidate),
        Some(current) => {
            if direct_any_candidate_error_rank(&candidate)
                < direct_any_candidate_error_rank(&current)
            {
                Some(candidate)
            } else {
                Some(current)
            }
        }
    }
}

fn direct_any_recipient_matches_binding(
    recipient: &Fido2DirectRecipientEnvelope,
    binding: &Fido2DirectBinding,
) -> Result<bool, String> {
    Ok(recipient
        .fingerprint
        .eq_ignore_ascii_case(&binding.fingerprint)
        && recipient.rp_id == binding.rp_id
        && decode_base64(&recipient.credential_id)? == binding.credential_id)
}

fn direct_required_layer_aad(fingerprint: &str) -> Vec<u8> {
    let mut aad = FIDO2_DIRECT_LAYER_AAD_PREFIX.to_vec();
    aad.extend_from_slice(fingerprint.as_bytes());
    aad
}

fn derive_direct_hmac_secret(
    fingerprint: &str,
    rp_id: &str,
    credential_id: &[u8],
    salt: &[u8],
) -> Result<Vec<u8>, String> {
    derive_direct_hmac_assertion(fingerprint, rp_id, credential_id, salt, &[])
        .map(|assertion| assertion.hmac_secret)
}

fn decrypt_with_direct_hmac_secret_candidates<T>(
    fingerprint: &str,
    rp_id: &str,
    credential_id: &[u8],
    salt: &[u8],
    mut try_decrypt: impl FnMut(&[u8]) -> Result<T, String>,
) -> Result<T, String> {
    let mut excluded_devices = Vec::new();

    loop {
        let assertion = derive_direct_hmac_assertion(
            fingerprint,
            rp_id,
            credential_id,
            salt,
            &excluded_devices,
        )?;

        match try_decrypt(&assertion.hmac_secret) {
            Ok(value) => return Ok(value),
            Err(err) if assertion.device.is_some() => {
                let device = assertion.device.expect("checked above");
                if excluded_devices.iter().any(|excluded| excluded == &device) {
                    return Err(err);
                }
                excluded_devices.push(device);
            }
            Err(err) => return Err(err),
        }
    }
}

fn derive_direct_hmac_assertion(
    fingerprint: &str,
    rp_id: &str,
    credential_id: &[u8],
    salt: &[u8],
    excluded_devices: &[Fido2DeviceLabel],
) -> Result<Fido2AssertionOutput, String> {
    let cached_pin = cached_pin_string(fingerprint)?;
    derive_direct_hmac_assertion_with_pin(
        fingerprint,
        rp_id,
        credential_id,
        salt,
        excluded_devices,
        cached_pin.as_ref().map(|pin| pin.expose_secret()),
    )
    .map_err(|err| direct_fido2_store_message(fingerprint, err))
}

fn direct_fido2_store_message(fingerprint: &str, err: Fido2TransportError) -> String {
    match err {
        Fido2TransportError::PinRequired | Fido2TransportError::IncorrectPin => {
            let _ = clear_cached_fido2_pin(fingerprint);
            "A FIDO2 security key for this item is locked. Unlock it in Preferences.".to_string()
        }
        Fido2TransportError::TokenNotPresent => {
            "Connect the matching FIDO2 security key.".to_string()
        }
        Fido2TransportError::UserActionTimeout => {
            "Touch the FIDO2 security key and try again.".to_string()
        }
        Fido2TransportError::TokenRemoved => {
            "Reconnect the FIDO2 security key and try again.".to_string()
        }
        Fido2TransportError::Unsupported => {
            "That FIDO2 security key does not support the hmac-secret extension.".to_string()
        }
        Fido2TransportError::Other(message) => message,
    }
}

#[cfg(feature = "fidostore")]
fn parse_store_recipient_binding(recipient: &str) -> Option<Fido2DirectBinding> {
    parse_fido2_recipient_string(recipient)
        .ok()
        .flatten()
        .map(|recipient| direct_binding_from_store_recipient_data(&recipient))
}

fn direct_binding_from_store_recipient_data(recipient: &Fido2StoreRecipient) -> Fido2DirectBinding {
    Fido2DirectBinding {
        fingerprint: recipient.id.clone(),
        label: recipient.label.clone(),
        rp_id: FIDO2_RP_ID.to_string(),
        credential_id: recipient.credential_id.clone(),
    }
}
