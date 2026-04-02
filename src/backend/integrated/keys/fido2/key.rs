use super::common::{
    cached_pin_string, create_fido2_binding, decode_base64, decrypt_aes_256_gcm,
    derive_direct_hmac_assertion_with_pin, derive_kek, parse_text_envelope,
    private_key_error_from_fido2_error, validate_direct_layer_envelope, Fido2DirectLayerEnvelope,
    FIDO2_DIRECT_LAYER_AAD_PREFIX, FIDO2_DIRECT_LAYER_HEADER,
};
use crate::backend::PrivateKeyError;
use secrecy::ExposeSecret;

pub(in crate::backend::integrated) fn create_fido2_private_key_binding(
    pin: Option<&str>,
) -> Result<String, PrivateKeyError> {
    create_fido2_binding(pin)
}

pub(in crate::backend::integrated) fn unlock_fido2_private_key_material_for_session(
    ciphertext: &[u8],
    pin: Option<&str>,
) -> Result<Vec<u8>, PrivateKeyError> {
    let Some(envelope) =
        parse_text_envelope::<Fido2DirectLayerEnvelope>(FIDO2_DIRECT_LAYER_HEADER, ciphertext)
            .map_err(PrivateKeyError::other)?
    else {
        return Err(PrivateKeyError::other(
            "That FIDO2-protected key data is invalid.",
        ));
    };
    validate_direct_layer_envelope(&envelope).map_err(PrivateKeyError::other)?;

    let resolved_pin = match pin {
        Some(pin) => {
            let trimmed = pin.trim();
            if trimmed.is_empty() {
                return Err(PrivateKeyError::fido2_pin_required(
                    "Enter the FIDO2 security key PIN.",
                ));
            }
            Some(secrecy::SecretString::from(trimmed))
        }
        None => cached_pin_string(&envelope.fingerprint).map_err(PrivateKeyError::other)?,
    };
    let hmac_salt = decode_base64(&envelope.hmac_salt).map_err(PrivateKeyError::other)?;
    let credential_id = decode_base64(&envelope.credential_id).map_err(PrivateKeyError::other)?;
    let payload_nonce = decode_base64(&envelope.payload_nonce).map_err(PrivateKeyError::other)?;
    let payload_ciphertext =
        decode_base64(&envelope.payload_ciphertext).map_err(PrivateKeyError::other)?;
    let mut excluded_devices = Vec::new();

    loop {
        let assertion = derive_direct_hmac_assertion_with_pin(
            &envelope.fingerprint,
            &envelope.rp_id,
            &credential_id,
            &hmac_salt,
            &excluded_devices,
            resolved_pin.as_ref().map(|pin| pin.expose_secret()),
        )
        .map_err(private_key_error_from_fido2_error)?;
        let kek = derive_kek(&assertion.hmac_secret, &envelope.fingerprint, &hmac_salt)?;

        match decrypt_aes_256_gcm(
            &kek,
            &payload_nonce,
            &direct_required_layer_aad(&envelope.fingerprint),
            &payload_ciphertext,
        ) {
            Ok(plaintext) => {
                if let Some(pin) = resolved_pin.as_ref().map(|pin| pin.expose_secret()) {
                    super::super::cache::cache_fido2_pin(&envelope.fingerprint, pin)
                        .map_err(PrivateKeyError::other)?;
                }
                return Ok(plaintext);
            }
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

fn direct_required_layer_aad(fingerprint: &str) -> Vec<u8> {
    let mut aad = FIDO2_DIRECT_LAYER_AAD_PREFIX.to_vec();
    aad.extend_from_slice(fingerprint.as_bytes());
    aad
}
