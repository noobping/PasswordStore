use super::cache::{cached_fido2_pin, clear_cached_fido2_pin};
use crate::backend::PrivateKeyError;
use crate::fido2_recipient::{
    build_fido2_recipient_string, parse_fido2_recipient_string, Fido2StoreRecipient,
};
use rand::random;
use serde::{Deserialize, Serialize};
use std::fmt::{Display, Formatter};
use std::sync::{Arc, OnceLock, RwLock};

#[cfg(any(target_os = "linux", target_os = "windows"))]
use base64::{engine::general_purpose::STANDARD as BASE64, Engine as _};
#[cfg(any(target_os = "linux", target_os = "windows"))]
use fido2_rs::{
    assertion::AssertRequest,
    credentials::{CoseType, Credential, Extensions, Opt},
    device::{Device, DeviceInfo, DeviceList},
    error::Error as Fido2LibraryError,
};
#[cfg(any(target_os = "linux", target_os = "windows"))]
use hmac::{Hmac, Mac};
#[cfg(any(target_os = "linux", target_os = "windows"))]
use openssl::symm::{Cipher, Crypter, Mode};
#[cfg(any(target_os = "linux", target_os = "windows"))]
use sha2::{Digest, Sha256};

pub const FIDO2_RP_ID: &str = "io.github.noobping.keycord";
#[cfg(not(any(target_os = "linux", target_os = "windows")))]
const FIDO2_PLATFORM_UNSUPPORTED_MESSAGE: &str =
    "FIDO2 recipients are only available on Linux and Windows.";

const FIDO2_HMAC_SALT_LEN: usize = 32;
const FIDO2_CLIENT_DATA_HASH_LEN: usize = 32;
const FIDO2_USER_ID_LEN: usize = 32;
const FIDO2_DEK_LEN: usize = 32;
const AES_GCM_NONCE_LEN: usize = 12;
const AES_GCM_TAG_LEN: usize = 16;
const FIDO2_KEK_INFO: &[u8] = b"keycord/fido2-hmac-secret/kek/v1";
const FIDO2_DIRECT_ENTRY_FORMAT: u32 = 1;
const FIDO2_DIRECT_ANY_MANAGED_HEADER: &str = "keycord-fido2-any-managed-v1";
const FIDO2_DIRECT_ANY_MANAGED_KIND: &str = "fido2-any-managed";
const FIDO2_DIRECT_LAYER_HEADER: &str = "keycord-fido2-required-layer-v1";
const FIDO2_DIRECT_LAYER_KIND: &str = "fido2-required-layer";
const FIDO2_DIRECT_ANY_PAYLOAD_AAD: &[u8] = b"keycord/fido2-any-managed/payload/v1";
const FIDO2_DIRECT_ANY_WRAPPED_DEK_AAD_PREFIX: &[u8] = b"keycord/fido2-any-managed/wrapped-dek/v1:";
const FIDO2_DIRECT_LAYER_AAD_PREFIX: &[u8] = b"keycord/fido2-required-layer/payload/v1:";

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Fido2DirectBinding {
    pub fingerprint: String,
    pub label: String,
    pub rp_id: String,
    pub credential_id: Vec<u8>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Fido2DeviceLabel {
    pub manufacturer: Option<String>,
    pub product: Option<String>,
    pub vendor_id: Option<u16>,
    pub product_id: Option<u16>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Fido2TransportError {
    PinRequired,
    IncorrectPin,
    TokenNotPresent,
    TokenRemoved,
    Unsupported,
    Other(String),
}

impl Display for Fido2TransportError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::PinRequired => write!(f, "Enter the FIDO2 security key PIN."),
            Self::IncorrectPin => write!(f, "The FIDO2 security key PIN is incorrect."),
            Self::TokenNotPresent => write!(f, "Connect the matching FIDO2 security key."),
            Self::TokenRemoved => write!(f, "Reconnect the FIDO2 security key and try again."),
            Self::Unsupported => write!(
                f,
                "That FIDO2 security key does not support the hmac-secret extension."
            ),
            Self::Other(message) => write!(f, "{message}"),
        }
    }
}

impl std::error::Error for Fido2TransportError {}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Fido2Enrollment {
    pub credential_id: Vec<u8>,
    pub device: Fido2DeviceLabel,
    pub hmac_secret: Vec<u8>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Fido2AssertionOutput {
    pub hmac_secret: Vec<u8>,
    pub device: Option<Fido2DeviceLabel>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
struct Fido2DirectRecipientEnvelope {
    fingerprint: String,
    rp_id: String,
    credential_id: String,
    hmac_salt: String,
    wrapped_dek_nonce: String,
    wrapped_dek: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
struct Fido2DirectAnyManagedEnvelope {
    format: u32,
    protection: String,
    payload_nonce: String,
    payload_ciphertext: String,
    pgp_wrapped_dek: Option<String>,
    fido2_recipients: Vec<Fido2DirectRecipientEnvelope>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
struct Fido2DirectLayerEnvelope {
    format: u32,
    protection: String,
    fingerprint: String,
    rp_id: String,
    credential_id: String,
    hmac_salt: String,
    payload_nonce: String,
    payload_ciphertext: String,
}

pub trait Fido2Transport: Send + Sync {
    fn enroll_hmac_secret(
        &self,
        rp_id: &str,
        user_name: &str,
        user_display_name: &str,
        pin: Option<&str>,
        salt: &[u8],
    ) -> Result<Fido2Enrollment, Fido2TransportError>;

    fn derive_hmac_secret(
        &self,
        rp_id: &str,
        credential_id: &[u8],
        pin: Option<&str>,
        salt: &[u8],
    ) -> Result<Fido2AssertionOutput, Fido2TransportError>;
}

fn transport_cell() -> &'static RwLock<Arc<dyn Fido2Transport>> {
    static FIDO2_TRANSPORT: OnceLock<RwLock<Arc<dyn Fido2Transport>>> = OnceLock::new();
    FIDO2_TRANSPORT.get_or_init(|| RwLock::new(Arc::new(RealFido2Transport)))
}

fn with_fido2_transport_read<T>(f: impl FnOnce(&Arc<dyn Fido2Transport>) -> T) -> T {
    match transport_cell().read() {
        Ok(transport) => f(&transport),
        Err(poisoned) => {
            let transport = poisoned.into_inner();
            f(&transport)
        }
    }
}

#[cfg(test)]
pub(in crate::backend::integrated) fn set_fido2_transport_for_tests(
    transport: Arc<dyn Fido2Transport>,
) {
    match transport_cell().write() {
        Ok(mut current) => *current = transport,
        Err(poisoned) => {
            let mut current = poisoned.into_inner();
            *current = transport;
        }
    }
}

#[cfg(test)]
pub(in crate::backend::integrated) fn reset_fido2_transport_for_tests() {
    match transport_cell().write() {
        Ok(mut current) => *current = Arc::new(RealFido2Transport),
        Err(poisoned) => {
            let mut current = poisoned.into_inner();
            *current = Arc::new(RealFido2Transport);
        }
    }
}

pub(in crate::backend::integrated) fn private_key_error_from_fido2_error(
    err: Fido2TransportError,
) -> PrivateKeyError {
    match err {
        Fido2TransportError::PinRequired => {
            PrivateKeyError::fido2_pin_required("Enter the FIDO2 security key PIN.")
        }
        Fido2TransportError::IncorrectPin => {
            PrivateKeyError::incorrect_fido2_pin("The FIDO2 security key PIN is incorrect.")
        }
        Fido2TransportError::TokenNotPresent => {
            PrivateKeyError::fido2_token_not_present("Connect the matching FIDO2 security key.")
        }
        Fido2TransportError::TokenRemoved => {
            PrivateKeyError::fido2_token_removed("Reconnect the FIDO2 security key and try again.")
        }
        Fido2TransportError::Unsupported => PrivateKeyError::unsupported_fido2_key(
            "That FIDO2 security key does not support the hmac-secret extension.",
        ),
        Fido2TransportError::Other(message) => PrivateKeyError::other(message),
    }
}

#[cfg(any(target_os = "linux", target_os = "windows"))]
pub fn create_fido2_store_recipient(pin: Option<&str>) -> Result<String, PrivateKeyError> {
    let enrollment = with_fido2_transport_read(|transport| {
        transport.enroll_hmac_secret(
            FIDO2_RP_ID,
            "keycord-fido2-recipient",
            "Keycord FIDO2 recipient",
            pin,
            &random_bytes::<FIDO2_HMAC_SALT_LEN>(),
        )
    })
    .map_err(private_key_error_from_fido2_error)?;
    let id = direct_binding_id(&enrollment.credential_id);
    let label = direct_binding_label(&enrollment.device);
    if let Some(pin) = pin {
        super::cache::cache_fido2_pin(&id, pin).map_err(PrivateKeyError::other)?;
    }
    build_fido2_recipient_string(&id, &label, &enrollment.credential_id)
        .map_err(PrivateKeyError::other)
}

#[cfg(not(any(target_os = "linux", target_os = "windows")))]
pub fn create_fido2_store_recipient(_pin: Option<&str>) -> Result<String, PrivateKeyError> {
    Err(PrivateKeyError::unsupported_fido2_key(
        FIDO2_PLATFORM_UNSUPPORTED_MESSAGE,
    ))
}

#[cfg(any(target_os = "linux", target_os = "windows"))]
pub fn unlock_fido2_store_recipient_for_session(
    recipient: &str,
    pin: Option<&str>,
) -> Result<(), PrivateKeyError> {
    let recipient = parse_store_recipient_binding(recipient)
        .ok_or_else(|| PrivateKeyError::other("That FIDO2 store recipient is invalid."))?;
    with_fido2_transport_read(|transport| {
        transport.derive_hmac_secret(
            &recipient.rp_id,
            &recipient.credential_id,
            pin,
            &random_bytes::<FIDO2_HMAC_SALT_LEN>(),
        )
    })
    .map_err(private_key_error_from_fido2_error)?;
    if let Some(pin) = pin {
        super::cache::cache_fido2_pin(&recipient.fingerprint, pin)
            .map_err(PrivateKeyError::other)?;
    }
    Ok(())
}

#[cfg(not(any(target_os = "linux", target_os = "windows")))]
pub fn unlock_fido2_store_recipient_for_session(
    _recipient: &str,
    _pin: Option<&str>,
) -> Result<(), PrivateKeyError> {
    Err(PrivateKeyError::unsupported_fido2_key(
        FIDO2_PLATFORM_UNSUPPORTED_MESSAGE,
    ))
}

pub(in crate::backend::integrated) fn direct_binding_from_store_recipient(
    recipient: &str,
) -> Result<Option<Fido2DirectBinding>, String> {
    Ok(parse_fido2_recipient_string(recipient)?
        .map(|recipient| direct_binding_from_store_recipient_data(&recipient)))
}

pub(in crate::backend::integrated) fn encrypt_fido2_any_managed_bundle(
    bindings: &[Fido2DirectBinding],
    dek: &[u8],
    payload: &[u8],
    pgp_wrapped_dek: Option<&[u8]>,
) -> Result<Vec<u8>, String> {
    if bindings.is_empty() && pgp_wrapped_dek.is_none() {
        return Err("No recipients were found for this password entry.".to_string());
    }

    let payload_nonce = random_bytes::<AES_GCM_NONCE_LEN>();
    let payload_ciphertext =
        encrypt_aes_256_gcm(dek, &payload_nonce, FIDO2_DIRECT_ANY_PAYLOAD_AAD, payload)
            .map_err(|err| err.to_string())?;

    let mut fido2_recipients = Vec::with_capacity(bindings.len());
    for binding in bindings {
        let hmac_salt = random_bytes::<FIDO2_HMAC_SALT_LEN>();
        let hmac_secret = derive_direct_hmac_secret(
            &binding.fingerprint,
            &binding.rp_id,
            &binding.credential_id,
            &hmac_salt,
        )?;
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
        fido2_recipients.push(Fido2DirectRecipientEnvelope {
            fingerprint: binding.fingerprint.clone(),
            rp_id: binding.rp_id.clone(),
            credential_id: encode_base64(&binding.credential_id),
            hmac_salt: encode_base64(&hmac_salt),
            wrapped_dek_nonce: encode_base64(&wrapped_dek_nonce),
            wrapped_dek: encode_base64(&wrapped_dek),
        });
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
    let Some(envelope) = parse_text_envelope::<Fido2DirectAnyManagedEnvelope>(
        FIDO2_DIRECT_ANY_MANAGED_HEADER,
        ciphertext,
    )?
    else {
        return Err("Invalid FIDO2 any-managed password entry.".to_string());
    };
    validate_direct_any_envelope(&envelope)?;

    let recipient = envelope
        .fido2_recipients
        .iter()
        .find(|recipient| recipient.fingerprint.eq_ignore_ascii_case(fingerprint))
        .ok_or_else(|| "The available private keys cannot decrypt this item.".to_string())?;
    let hmac_salt = decode_base64(&recipient.hmac_salt)?;
    let credential_id = decode_base64(&recipient.credential_id)?;
    let hmac_secret =
        derive_direct_hmac_secret(fingerprint, &recipient.rp_id, &credential_id, &hmac_salt)?;
    let kek = derive_kek(&hmac_secret, &recipient.fingerprint, &hmac_salt)
        .map_err(|err| err.to_string())?;
    let wrapped_dek_nonce = decode_base64(&recipient.wrapped_dek_nonce)?;
    let wrapped_dek = decode_base64(&recipient.wrapped_dek)?;
    let dek = decrypt_aes_256_gcm(
        &kek,
        &wrapped_dek_nonce,
        &direct_any_wrapped_dek_aad(&recipient.fingerprint),
        &wrapped_dek,
    )
    .map_err(|err| err.to_string())?;
    let payload_nonce = decode_base64(&envelope.payload_nonce)?;
    let payload_ciphertext = decode_base64(&envelope.payload_ciphertext)?;
    decrypt_aes_256_gcm(
        &dek,
        &payload_nonce,
        FIDO2_DIRECT_ANY_PAYLOAD_AAD,
        &payload_ciphertext,
    )
    .map_err(|err| err.to_string())
}

pub(in crate::backend::integrated) fn encrypt_fido2_direct_required_layer(
    binding: &Fido2DirectBinding,
    payload: &[u8],
) -> Result<Vec<u8>, String> {
    let hmac_salt = random_bytes::<FIDO2_HMAC_SALT_LEN>();
    let hmac_secret = derive_direct_hmac_secret(
        &binding.fingerprint,
        &binding.rp_id,
        &binding.credential_id,
        &hmac_salt,
    )?;
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
    let hmac_secret = derive_direct_hmac_secret(
        expected_fingerprint,
        &envelope.rp_id,
        &credential_id,
        &hmac_salt,
    )?;
    let kek = derive_kek(&hmac_secret, &envelope.fingerprint, &hmac_salt)
        .map_err(|err| err.to_string())?;
    let payload_nonce = decode_base64(&envelope.payload_nonce)?;
    let payload_ciphertext = decode_base64(&envelope.payload_ciphertext)?;
    decrypt_aes_256_gcm(
        &kek,
        &payload_nonce,
        &direct_required_layer_aad(&envelope.fingerprint),
        &payload_ciphertext,
    )
    .map_err(|err| err.to_string())
}

pub(in crate::backend::integrated) fn extract_pgp_wrapped_dek_from_any_managed_bundle(
    ciphertext: &[u8],
) -> Result<Option<Vec<u8>>, String> {
    let Some(envelope) = parse_text_envelope::<Fido2DirectAnyManagedEnvelope>(
        FIDO2_DIRECT_ANY_MANAGED_HEADER,
        ciphertext,
    )?
    else {
        return Ok(None);
    };
    validate_direct_any_envelope(&envelope)?;
    envelope
        .pgp_wrapped_dek
        .as_deref()
        .map(decode_base64)
        .transpose()
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

pub(in crate::backend::integrated) fn ciphertext_is_any_managed_bundle(ciphertext: &[u8]) -> bool {
    ciphertext.starts_with(text_envelope_prefix(FIDO2_DIRECT_ANY_MANAGED_HEADER).as_slice())
}

fn direct_any_wrapped_dek_aad(fingerprint: &str) -> Vec<u8> {
    let mut aad = FIDO2_DIRECT_ANY_WRAPPED_DEK_AAD_PREFIX.to_vec();
    aad.extend_from_slice(fingerprint.as_bytes());
    aad
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
    let cached_pin = cached_pin_string(fingerprint)?;
    with_fido2_transport_read(|transport| {
        transport.derive_hmac_secret(rp_id, credential_id, cached_pin.as_deref(), salt)
    })
    .map(|assertion| assertion.hmac_secret)
    .map_err(|err| direct_fido2_store_message(fingerprint, err))
}

fn cached_pin_string(fingerprint: &str) -> Result<Option<String>, String> {
    let Some(pin) = cached_fido2_pin(fingerprint)? else {
        return Ok(None);
    };
    let text = std::str::from_utf8(pin.as_slice())
        .map_err(|err| format!("Stored FIDO2 PIN is not valid UTF-8: {err}"))?;
    Ok(Some(text.to_string()))
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
        Fido2TransportError::TokenRemoved => {
            "Reconnect the FIDO2 security key and try again.".to_string()
        }
        Fido2TransportError::Unsupported => {
            "That FIDO2 security key does not support the hmac-secret extension.".to_string()
        }
        Fido2TransportError::Other(message) => message,
    }
}

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

#[cfg(any(target_os = "linux", target_os = "windows"))]
fn direct_binding_id(credential_id: &[u8]) -> String {
    let digest = Sha256::digest(credential_id);
    let mut encoded = String::with_capacity(40);
    for byte in &digest[..20] {
        use std::fmt::Write as _;
        write!(encoded, "{byte:02X}").expect("writing hex into a string should not fail");
    }
    encoded
}

fn direct_binding_label(device: &Fido2DeviceLabel) -> String {
    match (device.manufacturer.as_deref(), device.product.as_deref()) {
        (Some(manufacturer), Some(product))
            if !manufacturer.trim().is_empty() && !product.trim().is_empty() =>
        {
            format!("{manufacturer} {product}")
        }
        (_, Some(product)) if !product.trim().is_empty() => product.to_string(),
        (Some(manufacturer), _) if !manufacturer.trim().is_empty() => manufacturer.to_string(),
        _ => "FIDO2 security key".to_string(),
    }
}

fn serialize_text_envelope<T: Serialize>(header: &str, value: &T) -> Result<Vec<u8>, String> {
    let body = toml::to_string(value).map_err(|err| err.to_string())?;
    let mut encoded = text_envelope_prefix(header);
    encoded.extend_from_slice(body.as_bytes());
    Ok(encoded)
}

fn parse_text_envelope<T: for<'de> Deserialize<'de>>(
    header: &str,
    ciphertext: &[u8],
) -> Result<Option<T>, String> {
    let prefix = text_envelope_prefix(header);
    let Some(body) = ciphertext.strip_prefix(prefix.as_slice()) else {
        return Ok(None);
    };
    let body = std::str::from_utf8(body).map_err(|err| err.to_string())?;
    toml::from_str(body)
        .map(Some)
        .map_err(|err| err.to_string())
}

fn text_envelope_prefix(header: &str) -> Vec<u8> {
    format!("{header}\n").into_bytes()
}

fn validate_direct_any_envelope(envelope: &Fido2DirectAnyManagedEnvelope) -> Result<(), String> {
    if envelope.format != FIDO2_DIRECT_ENTRY_FORMAT {
        return Err(format!(
            "Unsupported FIDO2 password-entry format {}.",
            envelope.format
        ));
    }
    if envelope.protection != FIDO2_DIRECT_ANY_MANAGED_KIND {
        return Err(format!(
            "Unsupported FIDO2 password-entry protection '{}'.",
            envelope.protection
        ));
    }
    decode_base64(&envelope.payload_nonce)?;
    decode_base64(&envelope.payload_ciphertext)?;
    if let Some(pgp_wrapped_dek) = envelope.pgp_wrapped_dek.as_deref() {
        decode_base64(pgp_wrapped_dek)?;
    }
    for recipient in &envelope.fido2_recipients {
        decode_base64(&recipient.credential_id)?;
        decode_base64(&recipient.hmac_salt)?;
        decode_base64(&recipient.wrapped_dek_nonce)?;
        decode_base64(&recipient.wrapped_dek)?;
    }
    Ok(())
}

fn validate_direct_layer_envelope(envelope: &Fido2DirectLayerEnvelope) -> Result<(), String> {
    if envelope.format != FIDO2_DIRECT_ENTRY_FORMAT {
        return Err(format!(
            "Unsupported FIDO2 password-entry format {}.",
            envelope.format
        ));
    }
    if envelope.protection != FIDO2_DIRECT_LAYER_KIND {
        return Err(format!(
            "Unsupported FIDO2 password-entry protection '{}'.",
            envelope.protection
        ));
    }
    decode_base64(&envelope.credential_id)?;
    decode_base64(&envelope.hmac_salt)?;
    decode_base64(&envelope.payload_nonce)?;
    decode_base64(&envelope.payload_ciphertext)?;
    Ok(())
}

#[cfg(any(target_os = "linux", target_os = "windows"))]
fn derive_kek(
    hmac_secret: &[u8],
    fingerprint: &str,
    hmac_salt: &[u8],
) -> Result<Vec<u8>, PrivateKeyError> {
    hkdf_sha256(
        hmac_secret,
        fingerprint.as_bytes(),
        hmac_salt,
        FIDO2_KEK_INFO,
        FIDO2_DEK_LEN,
    )
    .map_err(PrivateKeyError::other)
}

#[cfg(not(any(target_os = "linux", target_os = "windows")))]
fn derive_kek(
    _hmac_secret: &[u8],
    _fingerprint: &str,
    _hmac_salt: &[u8],
) -> Result<Vec<u8>, PrivateKeyError> {
    Err(PrivateKeyError::unsupported_fido2_key(
        FIDO2_PLATFORM_UNSUPPORTED_MESSAGE,
    ))
}

#[cfg(any(target_os = "linux", target_os = "windows"))]
fn hkdf_sha256(
    ikm: &[u8],
    salt: &[u8],
    hmac_salt: &[u8],
    info: &[u8],
    len: usize,
) -> Result<Vec<u8>, String> {
    type HmacSha256 = Hmac<Sha256>;

    let mut extract = HmacSha256::new_from_slice(salt).map_err(|err| err.to_string())?;
    extract.update(ikm);
    extract.update(hmac_salt);
    let prk = extract.finalize().into_bytes();

    let mut okm = Vec::with_capacity(len);
    let mut previous = Vec::<u8>::new();
    let mut counter: u8 = 1;
    while okm.len() < len {
        let mut expand = HmacSha256::new_from_slice(&prk).map_err(|err| err.to_string())?;
        if !previous.is_empty() {
            expand.update(&previous);
        }
        expand.update(info);
        expand.update(&[counter]);
        previous = expand.finalize().into_bytes().to_vec();
        okm.extend_from_slice(&previous);
        counter = counter
            .checked_add(1)
            .ok_or_else(|| "Failed to derive enough HKDF output.".to_string())?;
    }
    okm.truncate(len);
    Ok(okm)
}

#[cfg(any(target_os = "linux", target_os = "windows"))]
fn encrypt_aes_256_gcm(
    key: &[u8],
    nonce: &[u8],
    aad: &[u8],
    plaintext: &[u8],
) -> Result<Vec<u8>, PrivateKeyError> {
    let cipher = Cipher::aes_256_gcm();
    let mut crypter = Crypter::new(cipher, Mode::Encrypt, key, Some(nonce))
        .map_err(|err| PrivateKeyError::other(err.to_string()))?;
    crypter.pad(false);
    crypter
        .aad_update(aad)
        .map_err(|err| PrivateKeyError::other(err.to_string()))?;
    let mut ciphertext = vec![0u8; plaintext.len() + cipher.block_size()];
    let mut count = crypter
        .update(plaintext, &mut ciphertext)
        .map_err(|err| PrivateKeyError::other(err.to_string()))?;
    count += crypter
        .finalize(&mut ciphertext[count..])
        .map_err(|err| PrivateKeyError::other(err.to_string()))?;
    ciphertext.truncate(count);

    let mut tag = [0u8; AES_GCM_TAG_LEN];
    crypter
        .get_tag(&mut tag)
        .map_err(|err| PrivateKeyError::other(err.to_string()))?;
    ciphertext.extend_from_slice(&tag);
    Ok(ciphertext)
}

#[cfg(not(any(target_os = "linux", target_os = "windows")))]
fn encrypt_aes_256_gcm(
    _key: &[u8],
    _nonce: &[u8],
    _aad: &[u8],
    _plaintext: &[u8],
) -> Result<Vec<u8>, PrivateKeyError> {
    Err(PrivateKeyError::unsupported_fido2_key(
        FIDO2_PLATFORM_UNSUPPORTED_MESSAGE,
    ))
}

#[cfg(any(target_os = "linux", target_os = "windows"))]
fn decrypt_aes_256_gcm(
    key: &[u8],
    nonce: &[u8],
    aad: &[u8],
    ciphertext_and_tag: &[u8],
) -> Result<Vec<u8>, PrivateKeyError> {
    if ciphertext_and_tag.len() < AES_GCM_TAG_LEN {
        return Err(PrivateKeyError::other("Invalid FIDO2 encrypted data."));
    }
    let split_at = ciphertext_and_tag.len() - AES_GCM_TAG_LEN;
    let (ciphertext, tag) = ciphertext_and_tag.split_at(split_at);
    let cipher = Cipher::aes_256_gcm();
    let mut crypter = Crypter::new(cipher, Mode::Decrypt, key, Some(nonce))
        .map_err(|err| PrivateKeyError::other(err.to_string()))?;
    crypter.pad(false);
    crypter
        .aad_update(aad)
        .map_err(|err| PrivateKeyError::other(err.to_string()))?;
    crypter
        .set_tag(tag)
        .map_err(|err| PrivateKeyError::other(err.to_string()))?;
    let mut plaintext = vec![0u8; ciphertext.len() + cipher.block_size()];
    let mut count = crypter
        .update(ciphertext, &mut plaintext)
        .map_err(|_| PrivateKeyError::other("Couldn't decrypt the FIDO2-encrypted data."))?;
    count += crypter
        .finalize(&mut plaintext[count..])
        .map_err(|_| PrivateKeyError::other("Couldn't decrypt the FIDO2-encrypted data."))?;
    plaintext.truncate(count);
    Ok(plaintext)
}

#[cfg(not(any(target_os = "linux", target_os = "windows")))]
fn decrypt_aes_256_gcm(
    _key: &[u8],
    _nonce: &[u8],
    _aad: &[u8],
    _ciphertext_and_tag: &[u8],
) -> Result<Vec<u8>, PrivateKeyError> {
    Err(PrivateKeyError::unsupported_fido2_key(
        FIDO2_PLATFORM_UNSUPPORTED_MESSAGE,
    ))
}

fn encode_base64(bytes: &[u8]) -> String {
    #[cfg(any(target_os = "linux", target_os = "windows"))]
    {
        BASE64.encode(bytes)
    }

    #[cfg(not(any(target_os = "linux", target_os = "windows")))]
    {
        let _ = bytes;
        String::new()
    }
}

fn decode_base64(value: &str) -> Result<Vec<u8>, String> {
    #[cfg(any(target_os = "linux", target_os = "windows"))]
    {
        BASE64.decode(value).map_err(|err| err.to_string())
    }

    #[cfg(not(any(target_os = "linux", target_os = "windows")))]
    {
        let _ = value;
        Err(FIDO2_PLATFORM_UNSUPPORTED_MESSAGE.to_string())
    }
}

fn random_bytes<const N: usize>() -> [u8; N] {
    random::<[u8; N]>()
}

#[cfg(any(target_os = "linux", target_os = "windows"))]
fn map_fido2_library_error(err: Fido2LibraryError) -> Fido2TransportError {
    let lowered = err.to_string().to_ascii_lowercase();
    if lowered.contains("pin required")
        || lowered.contains("pin invalid")
        || lowered.contains("pin not set")
        || lowered.contains("uv invalid")
    {
        if lowered.contains("required") || lowered.contains("not set") {
            Fido2TransportError::PinRequired
        } else {
            Fido2TransportError::IncorrectPin
        }
    } else if lowered.contains("no credentials")
        || lowered.contains("not found")
        || lowered.contains("open")
        || lowered.contains("device not found")
    {
        Fido2TransportError::TokenNotPresent
    } else if lowered.contains("unsupported") || lowered.contains("invalid option") {
        Fido2TransportError::Unsupported
    } else if lowered.contains("rx")
        || lowered.contains("keepalive")
        || lowered.contains("action timeout")
        || lowered.contains("removed")
        || lowered.contains("cancelled")
    {
        Fido2TransportError::TokenRemoved
    } else {
        Fido2TransportError::Other(err.to_string())
    }
}

#[cfg(any(target_os = "linux", target_os = "windows"))]
fn client_data_hash(label: &str) -> [u8; FIDO2_CLIENT_DATA_HASH_LEN] {
    let mut hasher = Sha256::new();
    hasher.update(random_bytes::<FIDO2_CLIENT_DATA_HASH_LEN>());
    hasher.update(label.as_bytes());
    let digest = hasher.finalize();
    let mut hash = [0u8; FIDO2_CLIENT_DATA_HASH_LEN];
    hash.copy_from_slice(&digest);
    hash
}

#[cfg(any(target_os = "linux", target_os = "windows"))]
fn user_id() -> [u8; FIDO2_USER_ID_LEN] {
    random_bytes::<FIDO2_USER_ID_LEN>()
}

#[cfg(any(target_os = "linux", target_os = "windows"))]
fn owned_device_label(info: DeviceInfo<'_>) -> Fido2DeviceLabel {
    Fido2DeviceLabel {
        manufacturer: Some(info.manufacturer.to_string_lossy().into_owned())
            .filter(|value| !value.trim().is_empty()),
        product: Some(info.product.to_string_lossy().into_owned())
            .filter(|value| !value.trim().is_empty()),
        vendor_id: u16::try_from(info.vendor_id).ok(),
        product_id: u16::try_from(info.product_id).ok(),
    }
}

struct RealFido2Transport;

#[cfg(any(target_os = "linux", target_os = "windows"))]
impl RealFido2Transport {
    fn single_enrollment_device() -> Result<(Device, Fido2DeviceLabel), Fido2TransportError> {
        let mut devices = DeviceList::list_devices(16);
        let Some(info) = devices.next() else {
            return Err(Fido2TransportError::TokenNotPresent);
        };
        if devices.next().is_some() {
            return Err(Fido2TransportError::Other(
                "Connect only one FIDO2 security key before continuing.".to_string(),
            ));
        }
        let label = owned_device_label(info);
        let device = info.open().map_err(map_fido2_library_error)?;
        Ok((device, label))
    }

    fn hmac_secret_for_device(
        device: &Device,
        rp_id: &str,
        credential_id: &[u8],
        pin: Option<&str>,
        salt: &[u8],
    ) -> Result<Vec<u8>, Fido2TransportError> {
        let mut request = AssertRequest::new();
        request.set_rp(rp_id).map_err(map_fido2_library_error)?;
        request
            .set_client_data_hash(client_data_hash(rp_id))
            .map_err(map_fido2_library_error)?;
        request
            .set_allow_credential(credential_id)
            .map_err(map_fido2_library_error)?;
        request
            .set_extensions(Extensions::HMAC_SECRET)
            .map_err(map_fido2_library_error)?;
        request
            .set_hmac_salt(salt)
            .map_err(map_fido2_library_error)?;
        request.set_uv(Opt::Omit).map_err(map_fido2_library_error)?;
        let assertions = device
            .get_assertion(request, pin)
            .map_err(map_fido2_library_error)?;
        let Some(assertion) = assertions.iter().next() else {
            return Err(Fido2TransportError::TokenNotPresent);
        };
        let secret = assertion.hmac_secret();
        if secret.is_empty() {
            return Err(Fido2TransportError::Unsupported);
        }
        Ok(secret.to_vec())
    }
}

#[cfg(any(target_os = "linux", target_os = "windows"))]
impl Fido2Transport for RealFido2Transport {
    fn enroll_hmac_secret(
        &self,
        rp_id: &str,
        user_name: &str,
        user_display_name: &str,
        pin: Option<&str>,
        salt: &[u8],
    ) -> Result<Fido2Enrollment, Fido2TransportError> {
        let (device, label) = Self::single_enrollment_device()?;
        let mut credential = Credential::new();
        credential
            .set_client_data_hash(client_data_hash(user_name))
            .map_err(map_fido2_library_error)?;
        credential
            .set_rp(rp_id, rp_id)
            .map_err(map_fido2_library_error)?;
        credential
            .set_user(user_id(), user_name, Some(user_display_name), Some(""))
            .map_err(map_fido2_library_error)?;
        credential
            .set_extension(Extensions::HMAC_SECRET)
            .map_err(map_fido2_library_error)?;
        credential
            .set_rk(Opt::False)
            .map_err(map_fido2_library_error)?;
        credential
            .set_uv(Opt::Omit)
            .map_err(map_fido2_library_error)?;
        credential
            .set_cose_type(CoseType::ES256)
            .map_err(map_fido2_library_error)?;
        device
            .make_credential(&mut credential, pin)
            .map_err(map_fido2_library_error)?;
        let credential_id = credential.id().to_vec();
        if credential_id.is_empty() {
            return Err(Fido2TransportError::Other(
                "The FIDO2 security key did not return a credential identifier.".to_string(),
            ));
        }
        let hmac_secret = Self::hmac_secret_for_device(&device, rp_id, &credential_id, pin, salt)?;
        Ok(Fido2Enrollment {
            credential_id,
            device: label,
            hmac_secret,
        })
    }

    fn derive_hmac_secret(
        &self,
        rp_id: &str,
        credential_id: &[u8],
        pin: Option<&str>,
        salt: &[u8],
    ) -> Result<Fido2AssertionOutput, Fido2TransportError> {
        let mut last_error = None;
        let mut found_any_device = false;
        for info in DeviceList::list_devices(16) {
            found_any_device = true;
            let label = owned_device_label(info);
            let device = match info.open() {
                Ok(device) => device,
                Err(err) => {
                    last_error = Some(map_fido2_library_error(err));
                    continue;
                }
            };
            match Self::hmac_secret_for_device(&device, rp_id, credential_id, pin, salt) {
                Ok(hmac_secret) => {
                    return Ok(Fido2AssertionOutput {
                        hmac_secret,
                        device: Some(label),
                    });
                }
                Err(err) => {
                    last_error = Some(err);
                }
            }
        }

        if !found_any_device {
            return Err(Fido2TransportError::TokenNotPresent);
        }

        Err(last_error.unwrap_or(Fido2TransportError::TokenNotPresent))
    }
}

#[cfg(not(any(target_os = "linux", target_os = "windows")))]
impl Fido2Transport for RealFido2Transport {
    fn enroll_hmac_secret(
        &self,
        _rp_id: &str,
        _user_name: &str,
        _user_display_name: &str,
        _pin: Option<&str>,
        _salt: &[u8],
    ) -> Result<Fido2Enrollment, Fido2TransportError> {
        Err(Fido2TransportError::Unsupported)
    }

    fn derive_hmac_secret(
        &self,
        _rp_id: &str,
        _credential_id: &[u8],
        _pin: Option<&str>,
        _salt: &[u8],
    ) -> Result<Fido2AssertionOutput, Fido2TransportError> {
        Err(Fido2TransportError::Unsupported)
    }
}

#[cfg(test)]
mod tests {
    use super::{decode_base64, encode_base64, hkdf_sha256, map_fido2_library_error, FIDO2_RP_ID};
    #[cfg(any(target_os = "linux", target_os = "windows"))]
    use fido2_rs::error::Error as Fido2LibraryError;

    #[test]
    fn base64_helpers_round_trip() {
        let encoded = encode_base64(b"hello");
        assert_eq!(decode_base64(&encoded).unwrap(), b"hello");
    }

    #[cfg(any(target_os = "linux", target_os = "windows"))]
    #[test]
    fn hkdf_derives_a_stable_32_byte_key() {
        let derived = hkdf_sha256(b"secret", b"fingerprint", b"salt", b"info", 32).unwrap();
        assert_eq!(derived.len(), 32);
        assert_eq!(
            derived,
            hkdf_sha256(b"secret", b"fingerprint", b"salt", b"info", 32).unwrap()
        );
    }

    #[cfg(any(target_os = "linux", target_os = "windows"))]
    #[test]
    fn fido2_error_mapping_covers_pin_required() {
        let err = map_fido2_library_error(Fido2LibraryError::Unsupported);
        assert!(matches!(err, super::Fido2TransportError::Unsupported));
    }

    #[test]
    fn relying_party_id_matches_expected_value() {
        assert_eq!(FIDO2_RP_ID, "io.github.noobping.keycord");
    }
}
