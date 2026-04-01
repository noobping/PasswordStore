use crate::backend::PrivateKeyError;
use crate::fido2_recipient::{parse_fido2_recipient_string, Fido2StoreRecipient};
#[cfg(test)]
use std::sync::Arc;

const FIDO2_FEATURE_DISABLED_MESSAGE: &str = "FIDO2 support is disabled in this build of Keycord.";
const FIDO2_DIRECT_ANY_MANAGED_HEADER: &str = "keycord-fido2-any-managed-v1";

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

#[cfg(test)]
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Fido2DeviceLabel {
    pub manufacturer: Option<String>,
    pub product: Option<String>,
    pub vendor_id: Option<u16>,
    pub product_id: Option<u16>,
}

#[cfg(test)]
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Fido2TransportError {
    Unsupported,
}

#[cfg(test)]
impl std::fmt::Display for Fido2TransportError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{FIDO2_FEATURE_DISABLED_MESSAGE}")
    }
}

#[cfg(test)]
impl std::error::Error for Fido2TransportError {}

#[cfg(test)]
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Fido2Enrollment {
    pub credential_id: Vec<u8>,
    pub device: Fido2DeviceLabel,
    pub hmac_secret: Vec<u8>,
}

#[cfg(test)]
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Fido2AssertionOutput {
    pub hmac_secret: Vec<u8>,
    pub device: Option<Fido2DeviceLabel>,
}

#[cfg(test)]
pub trait Fido2Transport: Send + Sync {}

#[cfg(test)]
pub(in crate::backend::integrated) fn set_fido2_transport_for_tests(
    _transport: Arc<dyn Fido2Transport>,
) {
}

#[cfg(test)]
pub(in crate::backend::integrated) fn reset_fido2_transport_for_tests() {}

pub fn create_fido2_store_recipient(_pin: Option<&str>) -> Result<String, PrivateKeyError> {
    Err(PrivateKeyError::unsupported_fido2_key(
        FIDO2_FEATURE_DISABLED_MESSAGE,
    ))
}

#[cfg_attr(not(feature = "fidokey"), allow(dead_code))]
pub(in crate::backend::integrated) fn create_fido2_private_key_binding(
    _pin: Option<&str>,
) -> Result<String, PrivateKeyError> {
    Err(PrivateKeyError::unsupported_fido2_key(
        FIDO2_FEATURE_DISABLED_MESSAGE,
    ))
}

pub fn unlock_fido2_store_recipient_for_session(
    _recipient: &str,
    _pin: Option<&str>,
) -> Result<(), PrivateKeyError> {
    Err(PrivateKeyError::unsupported_fido2_key(
        FIDO2_FEATURE_DISABLED_MESSAGE,
    ))
}

pub(in crate::backend::integrated) fn direct_binding_from_store_recipient(
    recipient: &str,
) -> Result<Option<Fido2DirectBinding>, String> {
    match parse_fido2_recipient_string(recipient)? {
        Some(parsed) => Err(disabled_store_error(&parsed)),
        None => Ok(None),
    }
}

pub(in crate::backend::integrated) fn encrypt_fido2_any_managed_bundle_with_progress(
    _bindings: &[Fido2DirectBinding],
    _dek: &[u8],
    _payload: &[u8],
    _pgp_wrapped_dek: Option<&[u8]>,
    _report_progress: Option<&mut dyn FnMut(Fido2WriteProgress)>,
) -> Result<Vec<u8>, String> {
    Err(FIDO2_FEATURE_DISABLED_MESSAGE.to_string())
}

pub(in crate::backend::integrated) fn reencrypt_fido2_any_managed_bundle_with_progress(
    _bindings: &[Fido2DirectBinding],
    _dek: &[u8],
    _payload: &[u8],
    _pgp_wrapped_dek: Option<&[u8]>,
    _previous_ciphertext: &[u8],
    _report_progress: Option<&mut dyn FnMut(Fido2WriteProgress)>,
) -> Result<Vec<u8>, String> {
    Err(FIDO2_FEATURE_DISABLED_MESSAGE.to_string())
}

pub(in crate::backend::integrated) fn decrypt_fido2_any_managed_bundle_for_fingerprint(
    _fingerprint: &str,
    _ciphertext: &[u8],
) -> Result<Vec<u8>, String> {
    Err(FIDO2_FEATURE_DISABLED_MESSAGE.to_string())
}

pub(in crate::backend::integrated) fn decrypt_fido2_any_managed_bundle_dek_for_fingerprint(
    _fingerprint: &str,
    _ciphertext: &[u8],
) -> Result<Vec<u8>, String> {
    Err(FIDO2_FEATURE_DISABLED_MESSAGE.to_string())
}

pub(in crate::backend::integrated) fn decrypt_fido2_any_managed_bundle_dek_for_bindings(
    _bindings: &[Fido2DirectBinding],
    _ciphertext: &[u8],
) -> Result<Vec<u8>, String> {
    Err(FIDO2_FEATURE_DISABLED_MESSAGE.to_string())
}

pub(in crate::backend::integrated) fn encrypt_fido2_direct_required_layer(
    _binding: &Fido2DirectBinding,
    _payload: &[u8],
) -> Result<Vec<u8>, String> {
    Err(FIDO2_FEATURE_DISABLED_MESSAGE.to_string())
}

pub(in crate::backend::integrated) fn decrypt_fido2_direct_required_layer(
    _expected_fingerprint: &str,
    _ciphertext: &[u8],
) -> Result<Vec<u8>, String> {
    Err(FIDO2_FEATURE_DISABLED_MESSAGE.to_string())
}

pub(in crate::backend::integrated) fn extract_pgp_wrapped_dek_from_any_managed_bundle(
    ciphertext: &[u8],
) -> Result<Option<Vec<u8>>, String> {
    if ciphertext_is_any_managed_bundle(ciphertext) {
        return Err(FIDO2_FEATURE_DISABLED_MESSAGE.to_string());
    }

    Ok(None)
}

pub(in crate::backend::integrated) fn decrypt_payload_from_any_managed_bundle(
    _ciphertext: &[u8],
    _dek: &[u8],
) -> Result<Vec<u8>, String> {
    Err(FIDO2_FEATURE_DISABLED_MESSAGE.to_string())
}

pub(in crate::backend::integrated) fn ciphertext_is_any_managed_bundle(ciphertext: &[u8]) -> bool {
    ciphertext.starts_with(format!("{FIDO2_DIRECT_ANY_MANAGED_HEADER}\n").as_bytes())
}

fn disabled_store_error(recipient: &Fido2StoreRecipient) -> String {
    format!(
        "FIDO2 recipient '{}' requires a build with FIDO2 support enabled.",
        recipient.id
    )
}
