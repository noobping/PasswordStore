use super::cert::ManagedRipassoHardwareKey;
use crate::backend::PrivateKeyError;
use std::fmt::{Display, Formatter};
use std::sync::{Arc, OnceLock, RwLock};

#[cfg(target_os = "linux")]
mod crypto;
#[cfg(target_os = "linux")]
mod linux;
#[cfg(not(target_os = "linux"))]
mod unsupported;

#[cfg(target_os = "linux")]
use self::linux::RealHardwareTransport;
#[cfg(target_os = "linux")]
pub(in crate::backend::integrated) use self::linux::{HardwareSessionPolicy, HardwareUnlockMode};
#[cfg(not(target_os = "linux"))]
use self::unsupported::RealHardwareTransport;
#[cfg(not(target_os = "linux"))]
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

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum HardwareTransportError {
    TokenNotPresent(String),
    TokenMismatch(String),
    PinRequired(String),
    IncorrectPin(String),
    TokenRemoved(String),
    Unsupported(String),
    Other(String),
}

impl Display for HardwareTransportError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::TokenNotPresent(message)
            | Self::TokenMismatch(message)
            | Self::PinRequired(message)
            | Self::IncorrectPin(message)
            | Self::TokenRemoved(message)
            | Self::Unsupported(message)
            | Self::Other(message) => write!(f, "{message}"),
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
        HardwareTransportError::TokenNotPresent(message) => {
            PrivateKeyError::hardware_token_not_present(message)
        }
        HardwareTransportError::TokenMismatch(message) => {
            PrivateKeyError::hardware_token_mismatch(message)
        }
        HardwareTransportError::PinRequired(message) => {
            PrivateKeyError::hardware_pin_required(message)
        }
        HardwareTransportError::IncorrectPin(message) => {
            PrivateKeyError::incorrect_hardware_pin(message)
        }
        HardwareTransportError::TokenRemoved(message) => {
            PrivateKeyError::hardware_token_removed(message)
        }
        HardwareTransportError::Unsupported(message) => {
            PrivateKeyError::unsupported_hardware_key(message)
        }
        HardwareTransportError::Other(message) => PrivateKeyError::other(message),
    }
}
