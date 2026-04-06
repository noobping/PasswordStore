#[cfg(feature = "hardwarekey")]
use super::crypto::{build_public_cert, public_key_material_and_fp_to_key, public_to_fingerprint};
use super::crypto::{decrypt_with_card_transaction, sign_with_card_transaction};
#[cfg(feature = "hardwarekey")]
use super::HardwareKeyGenerationRequest;
use super::{DiscoveredHardwareToken, HardwareTransport, HardwareTransportError};
use crate::backend::integrated::keys::cert::ManagedRipassoHardwareKey;
use card_backend_pcsc::PcscBackend;
use openpgp_card::ocard::KeyType;
#[cfg(feature = "hardwarekey")]
use openpgp_card::ocard::StatusBytes;
#[cfg(feature = "hardwarekey")]
use openpgp_card::Error as OpenPgpCardError;
use openpgp_card::{state, Card};
use secrecy::SecretString;
#[cfg(feature = "hardwarekey")]
use sequoia_openpgp::serialize::Serialize;
use sequoia_openpgp::Cert;
use std::sync::Arc;
use zeroize::Zeroizing;

#[cfg(feature = "hardwarekey")]
const DEFAULT_OPENPGP_CARD_USER_PIN: &str = "123456";

#[derive(Clone)]
pub(in crate::backend::integrated) enum HardwareUnlockMode {
    Pin(Arc<Zeroizing<Vec<u8>>>),
    External,
}

impl HardwareUnlockMode {
    pub(super) fn pin_mode(pin: &str) -> Self {
        Self::Pin(Arc::new(Zeroizing::new(pin.as_bytes().to_vec())))
    }
}

#[derive(Clone)]
pub(in crate::backend::integrated) struct HardwareSessionPolicy {
    ident: String,
    cert: Cert,
    signing_fingerprint: Option<String>,
    decryption_fingerprint: Option<String>,
    mode: HardwareUnlockMode,
}

impl HardwareSessionPolicy {
    pub(super) fn from_key(
        key: &ManagedRipassoHardwareKey,
        cert: Cert,
        mode: HardwareUnlockMode,
    ) -> Self {
        Self {
            ident: key.ident.clone(),
            cert,
            signing_fingerprint: key.signing_fingerprint.clone(),
            decryption_fingerprint: key.decryption_fingerprint.clone(),
            mode,
        }
    }
}

pub(super) struct RealHardwareTransport;

fn pin_string(pin: &Zeroizing<Vec<u8>>) -> Result<String, String> {
    String::from_utf8(pin.as_slice().to_vec()).map_err(|err| err.to_string())
}

fn fingerprint_matches(actual: Option<&str>, expected: &str) -> bool {
    actual.is_some_and(|actual| actual.eq_ignore_ascii_case(expected))
}

impl RealHardwareTransport {
    #[cfg(feature = "hardwarekey")]
    fn step_error(step: &str, err: impl std::fmt::Display) -> HardwareTransportError {
        map_hardware_transport_message(format!("{step}: {err}"))
    }

    #[cfg(feature = "hardwarekey")]
    fn should_retry_user_pin_change(err: &OpenPgpCardError) -> bool {
        matches!(
            err,
            OpenPgpCardError::CardStatus(
                StatusBytes::SecurityStatusNotSatisfied | StatusBytes::ConditionOfUseNotSatisfied
            )
        )
    }

    #[cfg(feature = "hardwarekey")]
    fn set_requested_user_pin(
        admin: &mut Card<state::Admin<'_, '_>>,
        request: &HardwareKeyGenerationRequest,
    ) -> Result<(), HardwareTransportError> {
        if !request.replace_user_pin {
            return Ok(());
        }

        let new_pin = SecretString::from(request.user_pin.clone());
        match admin.reset_user_pin(new_pin.clone()) {
            Ok(()) => Ok(()),
            Err(err) if Self::should_retry_user_pin_change(&err) => admin
                .as_transaction()
                .change_user_pin(SecretString::from(DEFAULT_OPENPGP_CARD_USER_PIN), new_pin)
                .map_err(card_error),
            Err(err) => Err(card_error(err)),
        }
    }

    #[cfg(feature = "hardwarekey")]
    fn should_retry_generated_cert_signing(message: &str) -> bool {
        let lowered = message.to_ascii_lowercase();
        lowered.contains("security status not satisfied")
            || lowered.contains("condition of use not satisfied")
            || lowered.contains("password not checked")
    }

    #[cfg(feature = "hardwarekey")]
    fn build_generated_public_cert(
        card: &mut Card<state::Open>,
        signing_key: sequoia_openpgp::packet::key::Key<
            sequoia_openpgp::packet::key::PublicParts,
            sequoia_openpgp::packet::key::UnspecifiedRole,
        >,
        decryption_key: sequoia_openpgp::packet::key::Key<
            sequoia_openpgp::packet::key::PublicParts,
            sequoia_openpgp::packet::key::UnspecifiedRole,
        >,
        request: &HardwareKeyGenerationRequest,
    ) -> Result<Cert, HardwareTransportError> {
        let mut candidate_pins = vec![request.user_pin.as_str()];
        if request.replace_user_pin && request.user_pin != DEFAULT_OPENPGP_CARD_USER_PIN {
            candidate_pins.push(DEFAULT_OPENPGP_CARD_USER_PIN);
        }

        let mut last_error = None;
        for (index, pin) in candidate_pins.iter().enumerate() {
            let mut transaction = card.transaction().map_err(card_error)?;
            match build_public_cert(
                &mut transaction,
                signing_key.clone(),
                Some(decryption_key.clone()),
                None,
                Some(pin),
                &|| {},
                &|| {},
                std::slice::from_ref(&request.user_id),
            ) {
                Ok(cert) => return Ok(cert.strip_secret_key_material()),
                Err(err) => {
                    let message = err.to_string();
                    let can_retry = index + 1 < candidate_pins.len()
                        && Self::should_retry_generated_cert_signing(&message);
                    last_error = Some(map_hardware_transport_message(message));
                    if can_retry {
                        continue;
                    }
                    break;
                }
            }
        }

        Err(last_error.unwrap_or_else(|| {
            HardwareTransportError::Other(
                "Couldn't create the public key certificate on the hardware key.".to_string(),
            )
        }))
    }

    #[cfg(feature = "hardwarekey")]
    fn requested_user_pin_is_active(
        card: &mut Card<state::Open>,
        request: &HardwareKeyGenerationRequest,
    ) -> Result<bool, HardwareTransportError> {
        let mut transaction = card.transaction().map_err(card_error)?;
        match transaction.verify_user_pin(SecretString::from(request.user_pin.clone())) {
            Ok(()) => Ok(true),
            Err(err) if Self::should_retry_user_pin_change(&err) => Ok(false),
            Err(err) => Err(card_error(err)),
        }
    }

    #[cfg(feature = "hardwarekey")]
    fn ensure_requested_user_pin_is_active(
        card: &mut Card<state::Open>,
        request: &HardwareKeyGenerationRequest,
    ) -> Result<(), HardwareTransportError> {
        if !request.replace_user_pin {
            return Ok(());
        }

        if Self::requested_user_pin_is_active(card, request)? {
            return Ok(());
        }

        {
            let mut transaction = card.transaction().map_err(card_error)?;
            let mut admin = transaction
                .to_admin_card(SecretString::from(request.admin_pin.clone()))
                .map_err(card_error)?;
            Self::set_requested_user_pin(&mut admin, request)?;
        }

        if Self::requested_user_pin_is_active(card, request)? {
            Ok(())
        } else {
            Err(HardwareTransportError::IncorrectPin(
                "The hardware key did not accept the requested new PIN.".to_string(),
            ))
        }
    }

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

    #[cfg(feature = "hardwarekey")]
    fn generated_public_cert(
        request: &HardwareKeyGenerationRequest,
    ) -> Result<(DiscoveredHardwareToken, Vec<u8>), HardwareTransportError> {
        let mut card = Self::open_card(&request.ident)?;
        let requested_pin_active = Self::requested_user_pin_is_active(&mut card, request)?;
        {
            let mut transaction = card.transaction().map_err(card_error)?;
            let mut admin = transaction
                .to_admin_card(SecretString::from(request.admin_pin.clone()))
                .map_err(|err| {
                    Self::step_error("Couldn't verify the hardware key admin PIN", err)
                })?;
            admin
                .set_cardholder_name(&request.cardholder_name)
                .map_err(|err| {
                    Self::step_error("Couldn't set the hardware key cardholder name", err)
                })?;
            if !requested_pin_active {
                Self::set_requested_user_pin(&mut admin, request).map_err(|err| {
                    Self::step_error("Couldn't set the hardware key user PIN", err)
                })?;
            }
        }

        let (signing_material, signing_time, decryption_material, decryption_time) = {
            let mut transaction = card.transaction().map_err(card_error)?;
            let mut admin = transaction
                .to_admin_card(SecretString::from(request.admin_pin.clone()))
                .map_err(|err| {
                    Self::step_error("Couldn't reopen the hardware key admin session", err)
                })?;
            let (signing_material, signing_time) = admin
                .generate_key(public_to_fingerprint, KeyType::Signing)
                .map_err(|err| {
                    Self::step_error("Couldn't generate the hardware signing key", err)
                })?;
            let (decryption_material, decryption_time) = admin
                .generate_key(public_to_fingerprint, KeyType::Decryption)
                .map_err(|err| {
                    Self::step_error("Couldn't generate the hardware decryption key", err)
                })?;
            (
                signing_material,
                signing_time,
                decryption_material,
                decryption_time,
            )
        };

        let (signing_fingerprint, decryption_fingerprint, signing_key, decryption_key) = {
            let mut transaction = card.transaction().map_err(card_error)?;
            let signing_fingerprint = transaction
                .fingerprint(KeyType::Signing)
                .map_err(card_error)?
                .ok_or_else(|| {
                    HardwareTransportError::Other(
                        "The hardware key did not return a signing fingerprint.".to_string(),
                    )
                })?;
            let decryption_fingerprint = transaction
                .fingerprint(KeyType::Decryption)
                .map_err(card_error)?
                .ok_or_else(|| {
                    HardwareTransportError::Other(
                        "The hardware key did not return a decryption fingerprint.".to_string(),
                    )
                })?;
            let signing_key = public_key_material_and_fp_to_key(
                &signing_material,
                KeyType::Signing,
                &signing_time,
                &signing_fingerprint,
            )
            .map_err(|err| HardwareTransportError::Other(err.to_string()))?;
            let decryption_key = public_key_material_and_fp_to_key(
                &decryption_material,
                KeyType::Decryption,
                &decryption_time,
                &decryption_fingerprint,
            )
            .map_err(|err| HardwareTransportError::Other(err.to_string()))?;
            (
                signing_fingerprint,
                decryption_fingerprint,
                signing_key,
                decryption_key,
            )
        };
        let cert =
            Self::build_generated_public_cert(&mut card, signing_key, decryption_key, request)
                .map_err(|err| {
                    Self::step_error("Couldn't certify the generated hardware key", err)
                })?;
        Self::ensure_requested_user_pin_is_active(&mut card, request).map_err(|err| {
            Self::step_error("Couldn't verify the requested hardware key PIN", err)
        })?;
        let mut bytes = Vec::new();
        cert.serialize(&mut bytes).map_err(|err| {
            Self::step_error("Couldn't serialize the generated hardware public key", err)
        })?;

        Ok((
            DiscoveredHardwareToken {
                ident: request.ident.clone(),
                reader_hint: None,
                cardholder_certificate: Some(bytes.clone()),
                signing_fingerprint: Some(signing_fingerprint.to_string()),
                decryption_fingerprint: Some(decryption_fingerprint.to_string()),
            },
            bytes,
        ))
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

#[cfg(all(test, feature = "hardwarekey"))]
mod tests {
    use super::RealHardwareTransport;
    use openpgp_card::ocard::StatusBytes;
    use openpgp_card::Error as OpenPgpCardError;

    #[test]
    fn security_status_retry_is_enabled_for_blank_card_pin_setup() {
        assert!(RealHardwareTransport::should_retry_user_pin_change(
            &OpenPgpCardError::CardStatus(StatusBytes::SecurityStatusNotSatisfied),
        ));
        assert!(RealHardwareTransport::should_retry_user_pin_change(
            &OpenPgpCardError::CardStatus(StatusBytes::ConditionOfUseNotSatisfied),
        ));
        assert!(!RealHardwareTransport::should_retry_user_pin_change(
            &OpenPgpCardError::CardStatus(StatusBytes::AuthenticationMethodBlocked),
        ));
    }

    #[test]
    fn generated_cert_signing_retry_is_enabled_for_auth_state_errors() {
        assert!(RealHardwareTransport::should_retry_generated_cert_signing(
            "OpenPGP card error status: Security status not satisfied",
        ));
        assert!(RealHardwareTransport::should_retry_generated_cert_signing(
            "OpenPGP card error status: Password not checked, 3 allowed retries",
        ));
        assert!(!RealHardwareTransport::should_retry_generated_cert_signing(
            "OpenPGP card error status: Authentication method blocked",
        ));
    }
}

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

    #[cfg(feature = "hardwarekey")]
    fn generate_key_material(
        &self,
        request: &HardwareKeyGenerationRequest,
    ) -> Result<(DiscoveredHardwareToken, Vec<u8>), HardwareTransportError> {
        if request.user_pin.trim().is_empty() {
            return Err(HardwareTransportError::PinRequired(
                if request.replace_user_pin {
                    "Enter the new hardware key PIN."
                } else {
                    "Enter the hardware key PIN."
                }
                .to_string(),
            ));
        }
        Self::generated_public_cert(request)
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

fn card_error(err: impl std::fmt::Display) -> HardwareTransportError {
    map_hardware_transport_message(err.to_string())
}

fn map_hardware_transport_message(message: String) -> HardwareTransportError {
    let lowered = message.to_ascii_lowercase();

    if lowered.contains("couldn't find card")
        || lowered.contains("no smartcard")
        || lowered.contains("reader error")
        || lowered.contains("context error")
    {
        HardwareTransportError::TokenNotPresent(message)
    } else if lowered.contains("authentication method blocked") {
        HardwareTransportError::PinBlocked(message)
    } else if lowered.contains("password not checked")
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
