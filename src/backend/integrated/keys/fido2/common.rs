use super::super::cache::{
    borrow_cached_fido2_pin, cache_fido2_pin, cache_pending_fido2_enrollment,
};
use crate::backend::PrivateKeyError;
use crate::fido2_recipient::build_fido2_recipient_string;
use rand::random;
use secrecy::SecretString;
use serde::{Deserialize, Serialize};
use std::fmt::{Display, Formatter};
use std::sync::{Arc, OnceLock, RwLock};
use std::thread;
use std::time::{Duration, Instant};

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
use hmac::{digest::KeyInit, Hmac, Mac};
#[cfg(any(target_os = "linux", target_os = "windows"))]
use openssl::symm::{Cipher, Crypter, Mode};
#[cfg(any(target_os = "linux", target_os = "windows"))]
use sha2::{Digest, Sha256};

pub const FIDO2_RP_ID: &str = "io.github.noobping.keycord";
#[cfg(not(any(target_os = "linux", target_os = "windows")))]
pub(super) const FIDO2_PLATFORM_UNSUPPORTED_MESSAGE: &str =
    "FIDO2 recipients are only available on Linux and Windows.";
#[cfg(not(feature = "fidostore"))]
pub(super) const FIDO2_STORE_FEATURE_DISABLED_MESSAGE: &str =
    "FIDO store support is disabled in this build of Keycord.";

pub(super) const FIDO2_HMAC_SALT_LEN: usize = 32;
const FIDO2_CLIENT_DATA_HASH_LEN: usize = 32;
const FIDO2_USER_ID_LEN: usize = 32;
const FIDO2_DEK_LEN: usize = 32;
const AES_GCM_TAG_LEN: usize = 16;
const FIDO2_KEK_INFO: &[u8] = b"keycord/fido2-hmac-secret/kek/v1";
const FIDO2_MATCHING_KEY_RETRY_WINDOW: Duration = Duration::from_secs(4);
const FIDO2_MATCHING_KEY_RETRY_INTERVAL: Duration = Duration::from_millis(150);

pub(super) const FIDO2_DIRECT_ENTRY_FORMAT: u32 = 1;
pub(super) const FIDO2_DIRECT_ANY_MANAGED_HEADER: &str = "keycord-fido2-any-managed-v1";
pub(super) const FIDO2_DIRECT_ANY_MANAGED_KIND: &str = "fido2-any-managed";
pub(super) const FIDO2_DIRECT_LAYER_HEADER: &str = "keycord-fido2-required-layer-v1";
pub(super) const FIDO2_DIRECT_LAYER_KIND: &str = "fido2-required-layer";
pub(super) const FIDO2_DIRECT_ANY_PAYLOAD_AAD: &[u8] = b"keycord/fido2-any-managed/payload/v1";
pub(super) const FIDO2_DIRECT_ANY_WRAPPED_DEK_AAD_PREFIX: &[u8] =
    b"keycord/fido2-any-managed/wrapped-dek/v1:";
pub(super) const FIDO2_DIRECT_LAYER_AAD_PREFIX: &[u8] = b"keycord/fido2-required-layer/payload/v1:";

#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::backend::integrated) struct Fido2Progress {
    pub current_step: usize,
    pub total_steps: usize,
}

pub(in crate::backend::integrated) type Fido2ReadProgress = Fido2Progress;
pub(in crate::backend::integrated) type Fido2WriteProgress = Fido2Progress;

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
    UserActionTimeout,
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
            Self::UserActionTimeout => write!(f, "Touch the FIDO2 security key and try again."),
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
pub(super) struct Fido2DirectRecipientEnvelope {
    pub(super) fingerprint: String,
    pub(super) rp_id: String,
    pub(super) credential_id: String,
    pub(super) hmac_salt: String,
    pub(super) wrapped_dek_nonce: String,
    pub(super) wrapped_dek: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub(super) struct Fido2DirectAnyManagedEnvelope {
    pub(super) format: u32,
    pub(super) protection: String,
    pub(super) payload_nonce: String,
    pub(super) payload_ciphertext: String,
    pub(super) pgp_wrapped_dek: Option<String>,
    pub(super) fido2_recipients: Vec<Fido2DirectRecipientEnvelope>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub(super) struct Fido2DirectLayerEnvelope {
    pub(super) format: u32,
    pub(super) protection: String,
    pub(super) fingerprint: String,
    pub(super) rp_id: String,
    pub(super) credential_id: String,
    pub(super) hmac_salt: String,
    pub(super) payload_nonce: String,
    pub(super) payload_ciphertext: String,
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
        excluded_devices: &[Fido2DeviceLabel],
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

pub(super) fn private_key_error_from_fido2_error(err: Fido2TransportError) -> PrivateKeyError {
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
        Fido2TransportError::UserActionTimeout => PrivateKeyError::fido2_user_action_timeout(
            "Touch the FIDO2 security key and try again.",
        ),
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
pub(super) fn create_fido2_binding(pin: Option<&str>) -> Result<String, PrivateKeyError> {
    let enrollment_salt = random_bytes::<FIDO2_HMAC_SALT_LEN>();
    let enrollment = with_fido2_transport_read(|transport| {
        transport.enroll_hmac_secret(
            FIDO2_RP_ID,
            "keycord-fido2-recipient",
            "Keycord FIDO2 recipient",
            pin,
            &enrollment_salt,
        )
    })
    .map_err(private_key_error_from_fido2_error)?;
    let id = direct_binding_id(&enrollment.credential_id);
    let label = direct_binding_label(&enrollment.device);
    cache_pending_fido2_enrollment(
        &id,
        &enrollment.credential_id,
        enrollment_salt,
        &enrollment.hmac_secret,
    )
    .map_err(PrivateKeyError::other)?;
    if let Some(pin) = pin {
        cache_fido2_pin(&id, pin).map_err(PrivateKeyError::other)?;
    }
    build_fido2_recipient_string(&id, &label, &enrollment.credential_id)
        .map_err(PrivateKeyError::other)
}

#[cfg(not(any(target_os = "linux", target_os = "windows")))]
pub(super) fn create_fido2_binding(_pin: Option<&str>) -> Result<String, PrivateKeyError> {
    Err(PrivateKeyError::unsupported_fido2_key(
        FIDO2_PLATFORM_UNSUPPORTED_MESSAGE,
    ))
}

pub(super) fn cached_pin_string(fingerprint: &str) -> Result<Option<SecretString>, String> {
    let Some(pin) = borrow_cached_fido2_pin(fingerprint)? else {
        return Ok(None);
    };
    let text = std::str::from_utf8(pin.as_slice())
        .map_err(|err| format!("Stored FIDO2 PIN is not valid UTF-8: {err}"))?;
    Ok(Some(SecretString::from(text)))
}

pub(super) fn serialize_text_envelope<T: Serialize>(
    header: &str,
    value: &T,
) -> Result<Vec<u8>, String> {
    let body = toml::to_string(value).map_err(|err| err.to_string())?;
    let mut encoded = text_envelope_prefix(header);
    encoded.extend_from_slice(body.as_bytes());
    Ok(encoded)
}

pub(super) fn parse_text_envelope<T: for<'de> Deserialize<'de>>(
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

pub(super) fn validate_direct_any_envelope(
    envelope: &Fido2DirectAnyManagedEnvelope,
) -> Result<(), String> {
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

pub(super) fn validate_direct_layer_envelope(
    envelope: &Fido2DirectLayerEnvelope,
) -> Result<(), String> {
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
pub(super) fn derive_kek(
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
pub(super) fn derive_kek(
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
pub(super) fn encrypt_aes_256_gcm(
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
pub(super) fn encrypt_aes_256_gcm(
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
pub(super) fn decrypt_aes_256_gcm(
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
pub(super) fn decrypt_aes_256_gcm(
    _key: &[u8],
    _nonce: &[u8],
    _aad: &[u8],
    _ciphertext_and_tag: &[u8],
) -> Result<Vec<u8>, PrivateKeyError> {
    Err(PrivateKeyError::unsupported_fido2_key(
        FIDO2_PLATFORM_UNSUPPORTED_MESSAGE,
    ))
}

pub(super) fn encode_base64(bytes: &[u8]) -> String {
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

pub(super) fn decode_base64(value: &str) -> Result<Vec<u8>, String> {
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

pub(super) fn random_bytes<const N: usize>() -> [u8; N] {
    random::<[u8; N]>()
}

pub(in crate::backend::integrated) fn ciphertext_is_any_managed_bundle(ciphertext: &[u8]) -> bool {
    ciphertext.starts_with(text_envelope_prefix(FIDO2_DIRECT_ANY_MANAGED_HEADER).as_slice())
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

fn text_envelope_prefix(header: &str) -> Vec<u8> {
    format!("{header}\n").into_bytes()
}

#[cfg(any(target_os = "linux", target_os = "windows"))]
fn enroll_with_passkey_fallback(
    mut enroll: impl FnMut(bool) -> Result<Fido2Enrollment, Fido2TransportError>,
) -> Result<Fido2Enrollment, Fido2TransportError> {
    match enroll(true) {
        Ok(enrollment) => Ok(enrollment),
        Err(Fido2TransportError::Unsupported) => enroll(false),
        Err(err) => Err(err),
    }
}

#[cfg(any(target_os = "linux", target_os = "windows"))]
fn map_fido2_library_error(err: Fido2LibraryError) -> Fido2TransportError {
    map_fido2_error_message(&err.to_string())
}

fn map_fido2_error_message(message: &str) -> Fido2TransportError {
    let lowered = message.to_ascii_lowercase();
    let normalized = lowered.replace('_', " ");
    if normalized.contains("pin required")
        || normalized.contains("pin not set")
        || normalized.contains("uv invalid")
    {
        Fido2TransportError::PinRequired
    } else if normalized.contains("pin invalid")
        || normalized.contains("pin auth invalid")
        || normalized.contains("pin auth blocked")
    {
        Fido2TransportError::IncorrectPin
    } else if normalized.contains("no credentials")
        || normalized.contains("not found")
        || normalized.contains("open")
        || normalized.contains("device not found")
    {
        Fido2TransportError::TokenNotPresent
    } else if normalized.contains("unsupported") || normalized.contains("invalid option") {
        Fido2TransportError::Unsupported
    } else if normalized.contains("action timeout") || normalized.contains("operation denied") {
        Fido2TransportError::UserActionTimeout
    } else if normalized.contains("rx")
        || normalized.contains("keepalive")
        || normalized.contains("removed")
        || normalized.contains("cancelled")
    {
        Fido2TransportError::TokenRemoved
    } else {
        Fido2TransportError::Other(message.to_string())
    }
}

fn transport_error_rank(err: &Fido2TransportError) -> usize {
    match err {
        Fido2TransportError::PinRequired => 0,
        Fido2TransportError::IncorrectPin => 1,
        Fido2TransportError::UserActionTimeout => 2,
        Fido2TransportError::TokenRemoved => 3,
        Fido2TransportError::Unsupported => 4,
        Fido2TransportError::Other(_) => 5,
        Fido2TransportError::TokenNotPresent => 6,
    }
}

fn prefer_transport_error(
    current: Option<Fido2TransportError>,
    candidate: Fido2TransportError,
) -> Option<Fido2TransportError> {
    match current {
        None => Some(candidate),
        Some(current) => {
            if transport_error_rank(&candidate) < transport_error_rank(&current) {
                Some(candidate)
            } else {
                Some(current)
            }
        }
    }
}

fn select_matching_hmac_secret<'a>(
    assertions: impl IntoIterator<Item = (&'a [u8], &'a [u8])>,
    assertion_count: usize,
    credential_id: &[u8],
) -> Result<Vec<u8>, Fido2TransportError> {
    let mut unnamed_secret = None;

    for (assertion_id, secret) in assertions {
        if assertion_id == credential_id {
            if secret.is_empty() {
                return Err(Fido2TransportError::Unsupported);
            }
            return Ok(secret.to_vec());
        }

        if assertion_count == 1 && assertion_id.is_empty() {
            unnamed_secret = Some(secret.to_vec());
        }
    }

    match unnamed_secret {
        Some(secret) if secret.is_empty() => Err(Fido2TransportError::Unsupported),
        Some(secret) => Ok(secret),
        None => Err(Fido2TransportError::TokenNotPresent),
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
fn client_data(label: &str) -> Vec<u8> {
    let mut data = random_bytes::<FIDO2_CLIENT_DATA_HASH_LEN>().to_vec();
    data.extend_from_slice(label.as_bytes());
    data
}

#[cfg(any(target_os = "linux", target_os = "windows"))]
fn set_assert_client_data(
    device: &Device,
    request: &mut AssertRequest,
    label: &str,
) -> Result<(), Fido2TransportError> {
    if device.is_winhello() {
        request
            .set_client_data(client_data(label))
            .map_err(map_fido2_library_error)
    } else {
        request
            .set_client_data_hash(client_data_hash(label))
            .map_err(map_fido2_library_error)
    }
}

#[cfg(any(target_os = "linux", target_os = "windows"))]
fn set_credential_client_data(
    device: &Device,
    credential: &mut Credential,
    label: &str,
) -> Result<(), Fido2TransportError> {
    if device.is_winhello() {
        credential
            .set_client_data(client_data(label))
            .map_err(map_fido2_library_error)
    } else {
        credential
            .set_client_data_hash(client_data_hash(label))
            .map_err(map_fido2_library_error)
    }
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

struct EnrollmentRequest<'a> {
    label: &'a Fido2DeviceLabel,
    rp_id: &'a str,
    user_name: &'a str,
    user_display_name: &'a str,
    pin: Option<&'a str>,
    salt: &'a [u8],
    discoverable: bool,
}

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
        set_assert_client_data(device, &mut request, rp_id)?;
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
        let assertion_count = assertions.count();
        let candidates: Vec<(Vec<u8>, Vec<u8>)> = assertions
            .iter()
            .map(|assertion| (assertion.id().to_vec(), assertion.hmac_secret().to_vec()))
            .collect();
        select_matching_hmac_secret(
            candidates
                .iter()
                .map(|(assertion_id, secret)| (assertion_id.as_slice(), secret.as_slice())),
            assertion_count,
            credential_id,
        )
    }

    fn enroll_hmac_secret_on_device(
        device: &Device,
        request: EnrollmentRequest<'_>,
    ) -> Result<Fido2Enrollment, Fido2TransportError> {
        let mut credential = Credential::new();
        set_credential_client_data(device, &mut credential, request.user_name)?;
        credential
            .set_rp(request.rp_id, request.rp_id)
            .map_err(map_fido2_library_error)?;
        credential
            .set_user(
                user_id(),
                request.user_name,
                Some(request.user_display_name),
                Some(""),
            )
            .map_err(map_fido2_library_error)?;
        credential
            .set_extension(Extensions::HMAC_SECRET)
            .map_err(map_fido2_library_error)?;
        credential
            .set_rk(if request.discoverable {
                Opt::True
            } else {
                Opt::False
            })
            .map_err(map_fido2_library_error)?;
        credential
            .set_uv(Opt::Omit)
            .map_err(map_fido2_library_error)?;
        credential
            .set_cose_type(CoseType::ES256)
            .map_err(map_fido2_library_error)?;
        device
            .make_credential(&mut credential, request.pin)
            .map_err(map_fido2_library_error)?;
        let credential_id = credential.id().to_vec();
        if credential_id.is_empty() {
            return Err(Fido2TransportError::Other(
                "The FIDO2 security key did not return a credential identifier.".to_string(),
            ));
        }
        let hmac_secret = Self::hmac_secret_for_device(
            device,
            request.rp_id,
            &credential_id,
            request.pin,
            request.salt,
        )?;
        Ok(Fido2Enrollment {
            credential_id,
            device: request.label.clone(),
            hmac_secret,
        })
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
        enroll_with_passkey_fallback(|discoverable| {
            Self::enroll_hmac_secret_on_device(
                &device,
                EnrollmentRequest {
                    label: &label,
                    rp_id,
                    user_name,
                    user_display_name,
                    pin,
                    salt,
                    discoverable,
                },
            )
        })
    }

    fn derive_hmac_secret(
        &self,
        rp_id: &str,
        credential_id: &[u8],
        pin: Option<&str>,
        salt: &[u8],
        excluded_devices: &[Fido2DeviceLabel],
    ) -> Result<Fido2AssertionOutput, Fido2TransportError> {
        let mut last_error = None;
        let mut found_any_device = false;
        for info in DeviceList::list_devices(16) {
            found_any_device = true;
            let label = owned_device_label(info);
            if excluded_devices.iter().any(|excluded| excluded == &label) {
                continue;
            }
            let device = match info.open() {
                Ok(device) => device,
                Err(err) => {
                    last_error = prefer_transport_error(last_error, map_fido2_library_error(err));
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
                    last_error = prefer_transport_error(last_error, err);
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
        _excluded_devices: &[Fido2DeviceLabel],
    ) -> Result<Fido2AssertionOutput, Fido2TransportError> {
        Err(Fido2TransportError::Unsupported)
    }
}

pub(super) fn derive_direct_hmac_assertion_with_pin(
    _fingerprint: &str,
    rp_id: &str,
    credential_id: &[u8],
    salt: &[u8],
    excluded_devices: &[Fido2DeviceLabel],
    pin: Option<&str>,
) -> Result<Fido2AssertionOutput, Fido2TransportError> {
    let retry_deadline = Instant::now() + FIDO2_MATCHING_KEY_RETRY_WINDOW;

    loop {
        match with_fido2_transport_read(|transport| {
            transport.derive_hmac_secret(rp_id, credential_id, pin, salt, excluded_devices)
        }) {
            Ok(assertion) => return Ok(assertion),
            Err(err) if should_retry_direct_hmac_error(&err) && Instant::now() < retry_deadline => {
                thread::sleep(FIDO2_MATCHING_KEY_RETRY_INTERVAL);
            }
            Err(err) => return Err(err),
        }
    }
}

fn should_retry_direct_hmac_error(err: &Fido2TransportError) -> bool {
    matches!(
        err,
        Fido2TransportError::TokenNotPresent
            | Fido2TransportError::UserActionTimeout
            | Fido2TransportError::TokenRemoved
    )
}

fn direct_binding_id(credential_id: &[u8]) -> String {
    #[cfg(any(target_os = "linux", target_os = "windows"))]
    {
        let digest = Sha256::digest(credential_id);
        let mut encoded = String::with_capacity(40);
        for byte in &digest[..20] {
            use std::fmt::Write as _;
            write!(encoded, "{byte:02X}").expect("writing hex into a string should not fail");
        }
        encoded
    }

    #[cfg(not(any(target_os = "linux", target_os = "windows")))]
    {
        let _ = credential_id;
        String::new()
    }
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

#[cfg(test)]
mod tests {
    use super::{
        decode_base64, encode_base64, enroll_with_passkey_fallback, hkdf_sha256,
        map_fido2_error_message, map_fido2_library_error, prefer_transport_error,
        select_matching_hmac_secret, Fido2DeviceLabel, Fido2Enrollment, Fido2TransportError,
        FIDO2_RP_ID,
    };
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
    fn fido2_error_mapping_understands_libfido2_pin_required_code_strings() {
        let err = map_fido2_error_message(
            "libfido2: Error { code: 54, message: \"FIDO_ERR_PIN_REQUIRED\" }",
        );
        assert!(matches!(err, Fido2TransportError::PinRequired));
    }

    #[test]
    fn fido2_error_mapping_understands_action_timeout_strings() {
        let err = map_fido2_error_message(
            "libfido2: Error { code: 47, message: \"FIDO_ERR_USER_ACTION_TIMEOUT\" }",
        );
        assert!(matches!(err, Fido2TransportError::UserActionTimeout));
    }

    #[test]
    fn fido2_error_mapping_understands_operation_denied_strings() {
        let err = map_fido2_error_message(
            "libfido2: Error { code: 39, message: \"FIDO_ERR_OPERATION_DENIED\" }",
        );
        assert!(matches!(err, Fido2TransportError::UserActionTimeout));
    }

    #[test]
    fn transport_error_preference_keeps_pin_required_over_token_not_present() {
        let preferred = prefer_transport_error(
            Some(Fido2TransportError::PinRequired),
            Fido2TransportError::TokenNotPresent,
        )
        .expect("preferred error");
        assert!(matches!(preferred, Fido2TransportError::PinRequired));
    }

    #[test]
    fn transport_error_preference_keeps_touch_timeout_over_token_not_present() {
        let preferred = prefer_transport_error(
            Some(Fido2TransportError::UserActionTimeout),
            Fido2TransportError::TokenNotPresent,
        )
        .expect("preferred error");
        assert!(matches!(preferred, Fido2TransportError::UserActionTimeout));
    }

    #[test]
    fn select_matching_hmac_secret_accepts_a_single_unnamed_assertion() {
        let secret = select_matching_hmac_secret(
            [(b"".as_slice(), b"derived-secret".as_slice())],
            1,
            b"expected-credential",
        )
        .expect("selected secret");
        assert_eq!(secret, b"derived-secret");
    }

    #[test]
    fn select_matching_hmac_secret_rejects_non_matching_named_assertions() {
        let err = select_matching_hmac_secret(
            [(b"other-credential".as_slice(), b"derived-secret".as_slice())],
            1,
            b"expected-credential",
        )
        .expect_err("non-matching assertion should fail");
        assert!(matches!(err, Fido2TransportError::TokenNotPresent));
    }

    #[cfg(any(target_os = "linux", target_os = "windows"))]
    #[test]
    fn passkey_enrollment_falls_back_when_discoverable_credentials_are_unsupported() {
        let mut attempts = Vec::new();
        let enrollment = enroll_with_passkey_fallback(|discoverable| {
            attempts.push(discoverable);
            if discoverable {
                Err(Fido2TransportError::Unsupported)
            } else {
                Ok(Fido2Enrollment {
                    credential_id: b"cred".to_vec(),
                    device: Fido2DeviceLabel {
                        manufacturer: None,
                        product: Some("Security Key".to_string()),
                        vendor_id: None,
                        product_id: None,
                    },
                    hmac_secret: b"secret".to_vec(),
                })
            }
        })
        .expect("fallback enrollment");

        assert_eq!(attempts, [true, false]);
        assert_eq!(enrollment.credential_id, b"cred");
    }

    #[cfg(any(target_os = "linux", target_os = "windows"))]
    #[test]
    fn passkey_enrollment_does_not_retry_after_non_capability_errors() {
        let mut attempts = Vec::new();
        let err = enroll_with_passkey_fallback(|discoverable| {
            attempts.push(discoverable);
            Err(Fido2TransportError::TokenRemoved)
        })
        .expect_err("non-capability error should stop immediately");

        assert_eq!(attempts, [true]);
        assert!(matches!(err, Fido2TransportError::TokenRemoved));
    }

    #[test]
    fn relying_party_id_matches_expected_value() {
        assert_eq!(FIDO2_RP_ID, "io.github.noobping.keycord");
    }
}
