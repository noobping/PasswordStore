use super::cert::ManagedRipassoHardwareKey;
use crate::backend::PrivateKeyError;
#[cfg(feature = "hardwarekey")]
use secrecy::SecretString;
use std::fmt::{Display, Formatter};
use std::sync::{Arc, OnceLock, RwLock};

#[cfg(feature = "smartcard")]
mod crypto;
#[cfg(feature = "smartcard")]
mod linux;
#[cfg(not(feature = "smartcard"))]
mod unsupported;

#[cfg(feature = "smartcard")]
use self::linux::RealHardwareTransport;
#[cfg(feature = "smartcard")]
pub(in crate::backend::integrated) use self::linux::{HardwareSessionPolicy, HardwareUnlockMode};
#[cfg(not(feature = "smartcard"))]
use self::unsupported::RealHardwareTransport;
#[cfg(not(feature = "smartcard"))]
pub(in crate::backend::integrated) use self::unsupported::{
    HardwareSessionPolicy, HardwareUnlockMode,
};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DiscoveredHardwareToken {
    pub ident: String,
    pub reader_hint: Option<String>,
    pub cardholder_certificate: Option<Vec<u8>>,
    pub signing_fingerprint: Option<String>,
    pub decryption_fingerprint: Option<String>,
}

#[cfg(feature = "hardwarekey")]
#[derive(Clone, Debug)]
pub struct HardwareKeyGenerationRequest {
    pub ident: String,
    pub cardholder_name: String,
    pub user_id: String,
    pub admin_pin: SecretString,
    pub user_pin: SecretString,
    pub replace_user_pin: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum HardwareTransportError {
    #[cfg(feature = "smartcard")]
    TokenNotPresent(String),
    #[cfg(feature = "smartcard")]
    TokenMismatch(String),
    #[cfg(feature = "smartcard")]
    PinRequired(String),
    #[cfg(feature = "smartcard")]
    IncorrectPin(String),
    #[cfg(feature = "smartcard")]
    PinBlocked(String),
    #[cfg(feature = "smartcard")]
    TokenRemoved(String),
    Unsupported(String),
    Other(String),
}

impl Display for HardwareTransportError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            #[cfg(feature = "smartcard")]
            Self::TokenNotPresent(message)
            | Self::TokenMismatch(message)
            | Self::PinRequired(message)
            | Self::IncorrectPin(message)
            | Self::PinBlocked(message)
            | Self::TokenRemoved(message)
            | Self::Unsupported(message)
            | Self::Other(message) => write!(f, "{message}"),
            #[cfg(not(feature = "smartcard"))]
            Self::Unsupported(message) | Self::Other(message) => write!(f, "{message}"),
        }
    }
}

impl std::error::Error for HardwareTransportError {}

impl From<String> for HardwareTransportError {
    fn from(message: String) -> Self {
        Self::Other(message)
    }
}

impl HardwareUnlockMode {
    pub(super) fn from_pin(pin: &str) -> Self {
        Self::pin_mode(pin)
    }
}

impl HardwareSessionPolicy {
    pub(super) fn from_managed_key(
        key: &ManagedRipassoHardwareKey,
        cert: sequoia_openpgp::Cert,
        mode: HardwareUnlockMode,
    ) -> Self {
        Self::from_key(key, cert, mode)
    }
}

pub trait HardwareTransport: Send + Sync {
    fn list_tokens(&self) -> Result<Vec<DiscoveredHardwareToken>, HardwareTransportError>;
    #[cfg(feature = "hardwarekey")]
    fn generate_key_material(
        &self,
        request: &HardwareKeyGenerationRequest,
    ) -> Result<(DiscoveredHardwareToken, Vec<u8>), HardwareTransportError>;
    fn verify_session(&self, session: &HardwareSessionPolicy)
        -> Result<(), HardwareTransportError>;
    fn decrypt_ciphertext(
        &self,
        session: &HardwareSessionPolicy,
        ciphertext: &[u8],
    ) -> Result<String, HardwareTransportError>;
    fn sign_cleartext(
        &self,
        session: &HardwareSessionPolicy,
        data: &str,
    ) -> Result<String, HardwareTransportError>;
}

fn transport_cell() -> &'static RwLock<Arc<dyn HardwareTransport>> {
    static HARDWARE_TRANSPORT: OnceLock<RwLock<Arc<dyn HardwareTransport>>> = OnceLock::new();
    HARDWARE_TRANSPORT.get_or_init(|| RwLock::new(Arc::new(RealHardwareTransport)))
}

fn with_hardware_transport_read<T>(f: impl FnOnce(&Arc<dyn HardwareTransport>) -> T) -> T {
    match transport_cell().read() {
        Ok(transport) => f(&transport),
        Err(poisoned) => {
            let transport = poisoned.into_inner();
            f(&transport)
        }
    }
}

#[cfg(test)]
pub(in crate::backend::integrated) fn set_hardware_transport_for_tests(
    transport: Arc<dyn HardwareTransport>,
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
pub(in crate::backend::integrated) fn reset_hardware_transport_for_tests() {
    match transport_cell().write() {
        Ok(mut current) => *current = Arc::new(RealHardwareTransport),
        Err(poisoned) => {
            let mut current = poisoned.into_inner();
            *current = Arc::new(RealHardwareTransport);
        }
    }
}

pub(in crate::backend::integrated) fn list_hardware_tokens(
) -> Result<Vec<DiscoveredHardwareToken>, HardwareTransportError> {
    with_hardware_transport_read(|transport| transport.list_tokens())
}

#[cfg(feature = "hardwarekey")]
pub(in crate::backend::integrated) fn generate_hardware_key_material(
    request: &HardwareKeyGenerationRequest,
) -> Result<(DiscoveredHardwareToken, Vec<u8>), HardwareTransportError> {
    with_hardware_transport_read(|transport| transport.generate_key_material(request))
}

pub(in crate::backend::integrated) fn decrypt_with_hardware_session(
    session: &HardwareSessionPolicy,
    ciphertext: &[u8],
) -> Result<String, HardwareTransportError> {
    with_hardware_transport_read(|transport| transport.decrypt_ciphertext(session, ciphertext))
}

pub(in crate::backend::integrated) fn verify_hardware_session(
    session: &HardwareSessionPolicy,
) -> Result<(), HardwareTransportError> {
    with_hardware_transport_read(|transport| transport.verify_session(session))
}

pub(in crate::backend::integrated) fn sign_with_hardware_session(
    session: &HardwareSessionPolicy,
    data: &str,
) -> Result<String, HardwareTransportError> {
    with_hardware_transport_read(|transport| transport.sign_cleartext(session, data))
}

pub(in crate::backend::integrated) fn private_key_error_from_hardware_transport_error(
    err: HardwareTransportError,
) -> PrivateKeyError {
    match err {
        #[cfg(feature = "smartcard")]
        HardwareTransportError::TokenNotPresent(message) => {
            PrivateKeyError::hardware_token_not_present(message)
        }
        #[cfg(feature = "smartcard")]
        HardwareTransportError::TokenMismatch(message) => {
            PrivateKeyError::hardware_token_mismatch(message)
        }
        #[cfg(feature = "smartcard")]
        HardwareTransportError::PinRequired(message) => {
            PrivateKeyError::hardware_pin_required(message)
        }
        #[cfg(feature = "smartcard")]
        HardwareTransportError::IncorrectPin(message) => {
            PrivateKeyError::incorrect_hardware_pin(message)
        }
        #[cfg(feature = "smartcard")]
        HardwareTransportError::PinBlocked(message) => {
            PrivateKeyError::hardware_pin_blocked(message)
        }
        #[cfg(feature = "smartcard")]
        HardwareTransportError::TokenRemoved(message) => {
            PrivateKeyError::hardware_token_removed(message)
        }
        HardwareTransportError::Unsupported(message) => {
            PrivateKeyError::unsupported_hardware_key(message)
        }
        HardwareTransportError::Other(message) => PrivateKeyError::other(message),
    }
}
