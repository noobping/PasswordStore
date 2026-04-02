use super::cert::ManagedRipassoHardwareKey;
#[cfg(target_os = "linux")]
use super::hardware_crypto::{decrypt_with_card_transaction, sign_with_card_transaction};
use crate::backend::PrivateKeyError;
#[cfg(target_os = "linux")]
use card_backend_pcsc::PcscBackend;
#[cfg(target_os = "linux")]
use openpgp_card::ocard::KeyType;
#[cfg(target_os = "linux")]
use openpgp_card::{state, Card};
#[cfg(target_os = "linux")]
use secrecy::SecretString;
use sequoia_openpgp::Cert;
use std::fmt::{Display, Formatter};
use std::sync::{Arc, OnceLock, RwLock};
use zeroize::Zeroizing;

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

#[derive(Clone)]
#[cfg_attr(not(target_os = "linux"), allow(dead_code))]
pub enum HardwareUnlockMode {
    Pin(Arc<Zeroizing<Vec<u8>>>),
    External,
}

#[derive(Clone)]
#[cfg_attr(not(target_os = "linux"), allow(dead_code))]
pub struct HardwareSessionPolicy {
    pub ident: String,
    pub cert: Cert,
    pub signing_fingerprint: Option<String>,
    pub decryption_fingerprint: Option<String>,
    pub mode: HardwareUnlockMode,
}

impl HardwareSessionPolicy {
    pub fn from_key(key: &ManagedRipassoHardwareKey, cert: Cert, mode: HardwareUnlockMode) -> Self {
        Self {
            ident: key.ident.clone(),
            cert,
            signing_fingerprint: key.signing_fingerprint.clone(),
            decryption_fingerprint: key.decryption_fingerprint.clone(),
            mode,
        }
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

#[cfg(target_os = "linux")]
fn pin_string(pin: &Zeroizing<Vec<u8>>) -> Result<String, String> {
    String::from_utf8(pin.as_slice().to_vec()).map_err(|err| err.to_string())
}

#[cfg(target_os = "linux")]
fn fingerprint_matches(actual: Option<&str>, expected: &str) -> bool {
    actual.is_some_and(|actual| actual.eq_ignore_ascii_case(expected))
}

struct RealHardwareTransport;

#[cfg(target_os = "linux")]
impl RealHardwareTransport {
    fn matching_fingerprint(
        tx: &mut Card<state::Transaction<'_>>,
        key_type: KeyType,
    ) -> Result<Option<String>, HardwareTransportError> {
        tx.fingerprint(key_type)
            .map(|value| value.map(|value| value.to_string()))
            .map_err(card_error)
    }

    fn verify_binding(
        tx: &mut Card<state::Transaction<'_>>,
        session: &HardwareSessionPolicy,
    ) -> Result<(), HardwareTransportError> {
        let current_signing = Self::matching_fingerprint(tx, KeyType::Signing)?;
        let current_decryption = Self::matching_fingerprint(tx, KeyType::Decryption)?;

        if session
            .signing_fingerprint
            .as_deref()
            .is_some_and(|expected| !fingerprint_matches(current_signing.as_deref(), expected))
        {
            return Err(HardwareTransportError::TokenMismatch(
                "The connected hardware key does not match the stored signing key.".to_string(),
            ));
        }

        if session
            .decryption_fingerprint
            .as_deref()
            .is_some_and(|expected| !fingerprint_matches(current_decryption.as_deref(), expected))
        {
            return Err(HardwareTransportError::TokenMismatch(
                "The connected hardware key does not match the stored decryption key.".to_string(),
            ));
        }

        Ok(())
    }

    fn open_card(ident: &str) -> Result<Card<state::Open>, HardwareTransportError> {
        let backends = PcscBackend::card_backends(None).map_err(card_error)?;
        Card::<state::Open>::open_by_ident(backends, ident).map_err(card_error)
    }

    fn secret_pin(pin: &Zeroizing<Vec<u8>>) -> Result<SecretString, String> {
        Ok(pin_string(pin)?.into())
    }

    fn verify_user_access(
        tx: &mut Card<state::Transaction<'_>>,
        session: &HardwareSessionPolicy,
    ) -> Result<(), HardwareTransportError> {
        match &session.mode {
            HardwareUnlockMode::Pin(pin) => tx
                .verify_user_pin(Self::secret_pin(pin)?)
                .map_err(card_error),
            HardwareUnlockMode::External => tx.verify_user_pinpad(&|| {}).map_err(card_error),
        }
    }

    fn verify_signing_access(
        tx: &mut Card<state::Transaction<'_>>,
        session: &HardwareSessionPolicy,
    ) -> Result<(), HardwareTransportError> {
        match &session.mode {
            HardwareUnlockMode::Pin(pin) => tx
                .verify_user_signing_pin(Self::secret_pin(pin)?)
                .map_err(card_error),
            HardwareUnlockMode::External => {
                tx.verify_user_signing_pinpad(&|| {}).map_err(card_error)
            }
        }
    }
}

#[cfg(target_os = "linux")]
impl HardwareTransport for RealHardwareTransport {
    fn list_tokens(&self) -> Result<Vec<DiscoveredHardwareToken>, HardwareTransportError> {
        let backends = PcscBackend::cards(None).map_err(card_error)?;
        let mut tokens = Vec::new();

        for backend in backends {
            let backend = backend.map_err(card_error)?;
            let mut card = Card::<state::Open>::new(backend).map_err(card_error)?;
            let mut tx = card.transaction().map_err(card_error)?;
            let ident = tx
                .application_identifier()
                .map_err(card_error)?
                .ident()
                .to_string();
            let cardholder_certificate = tx
                .cardholder_certificate()
                .ok()
                .filter(|value| !value.is_empty());

            tokens.push(DiscoveredHardwareToken {
                ident,
                reader_hint: None,
                cardholder_certificate,
                signing_fingerprint: Self::matching_fingerprint(&mut tx, KeyType::Signing)?,
                decryption_fingerprint: Self::matching_fingerprint(&mut tx, KeyType::Decryption)?,
            });
        }

        Ok(tokens)
    }

    fn verify_session(
        &self,
        session: &HardwareSessionPolicy,
    ) -> Result<(), HardwareTransportError> {
        let mut card = Self::open_card(&session.ident)?;
        let mut tx = card.transaction().map_err(card_error)?;
        Self::verify_binding(&mut tx, session)?;

        if session.decryption_fingerprint.is_some() {
            Self::verify_user_access(&mut tx, session)?;
        }

        if session.signing_fingerprint.is_some() {
            Self::verify_signing_access(&mut tx, session)?;
        }

        Ok(())
    }

    fn decrypt_ciphertext(
        &self,
        session: &HardwareSessionPolicy,
        ciphertext: &[u8],
    ) -> Result<String, HardwareTransportError> {
        let mut card = Self::open_card(&session.ident)?;
        let mut tx = card.transaction().map_err(card_error)?;
        Self::verify_binding(&mut tx, session)?;
        Self::verify_user_access(&mut tx, session)?;
        decrypt_with_card_transaction(
            tx.card(),
            &session.cert,
            session.decryption_fingerprint.as_deref(),
            ciphertext,
        )
        .map_err(card_error)
    }

    fn sign_cleartext(
        &self,
        session: &HardwareSessionPolicy,
        data: &str,
    ) -> Result<String, HardwareTransportError> {
        let mut card = Self::open_card(&session.ident)?;
        let mut tx = card.transaction().map_err(card_error)?;
        Self::verify_binding(&mut tx, session)?;
        Self::verify_signing_access(&mut tx, session)?;
        sign_with_card_transaction(
            tx.card(),
            &session.cert,
            session.signing_fingerprint.as_deref(),
            data,
        )
        .map_err(card_error)
    }
}

#[cfg(not(target_os = "linux"))]
impl HardwareTransport for RealHardwareTransport {
    fn list_tokens(&self) -> Result<Vec<DiscoveredHardwareToken>, HardwareTransportError> {
        Err(HardwareTransportError::Unsupported(
            "Hardware OpenPGP keys are not supported on this platform.".to_string(),
        ))
    }

    fn verify_session(
        &self,
        _session: &HardwareSessionPolicy,
    ) -> Result<(), HardwareTransportError> {
        Err(HardwareTransportError::Unsupported(
            "Hardware OpenPGP keys are not supported on this platform.".to_string(),
        ))
    }

    fn decrypt_ciphertext(
        &self,
        _session: &HardwareSessionPolicy,
        _ciphertext: &[u8],
    ) -> Result<String, HardwareTransportError> {
        Err(HardwareTransportError::Unsupported(
            "Hardware OpenPGP keys are not supported on this platform.".to_string(),
        ))
    }

    fn sign_cleartext(
        &self,
        _session: &HardwareSessionPolicy,
        _data: &str,
    ) -> Result<String, HardwareTransportError> {
        Err(HardwareTransportError::Unsupported(
            "Hardware OpenPGP keys are not supported on this platform.".to_string(),
        ))
    }
}

#[cfg(target_os = "linux")]
fn card_error(err: impl std::fmt::Display) -> HardwareTransportError {
    map_hardware_transport_message(err.to_string())
}

#[cfg(target_os = "linux")]
fn map_hardware_transport_message(message: String) -> HardwareTransportError {
    let lowered = message.to_ascii_lowercase();

    if lowered.contains("couldn't find card")
        || lowered.contains("no smartcard")
        || lowered.contains("reader error")
        || lowered.contains("context error")
    {
        HardwareTransportError::TokenNotPresent(message)
    } else if lowered.contains("password not checked")
        || lowered.contains("authentication method blocked")
        || lowered.contains("security status not satisfied")
        || lowered.contains("pin invalid")
        || lowered.contains("incorrect")
    {
        HardwareTransportError::IncorrectPin(message)
    } else if lowered.contains("pin required") || lowered.contains("enter the hardware key pin") {
        HardwareTransportError::PinRequired(message)
    } else if lowered.contains("not transacted")
        || lowered.contains("reset")
        || lowered.contains("removed")
    {
        HardwareTransportError::TokenRemoved(message)
    } else if lowered.contains("does not support") || lowered.contains("unsupported") {
        HardwareTransportError::Unsupported(message)
    } else {
        HardwareTransportError::Other(message)
    }
}
