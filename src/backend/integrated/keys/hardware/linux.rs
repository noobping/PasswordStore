use super::crypto::{
    build_public_cert, decrypt_with_card_transaction, public_key_material_and_fp_to_key,
    public_to_fingerprint, sign_with_card_transaction,
};
use super::{
    DiscoveredHardwareToken, HardwareKeyGenerationRequest, HardwareTransport,
    HardwareTransportError,
};
use crate::backend::integrated::keys::cert::ManagedRipassoHardwareKey;
use card_backend_pcsc::PcscBackend;
use openpgp_card::ocard::KeyType;
use openpgp_card::{state, Card};
use secrecy::SecretString;
use sequoia_openpgp::serialize::Serialize;
use sequoia_openpgp::Cert;
use std::sync::Arc;
use zeroize::Zeroizing;

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

    fn generated_public_cert(
        request: &HardwareKeyGenerationRequest,
    ) -> Result<(DiscoveredHardwareToken, Vec<u8>), HardwareTransportError> {
        let mut card = Self::open_card(&request.ident)?;
        let mut transaction = card.transaction().map_err(card_error)?;
        let mut admin = transaction
            .to_admin_card(SecretString::from(request.admin_pin.clone()))
            .map_err(card_error)?;
        admin
            .set_cardholder_name(&request.cardholder_name)
            .map_err(card_error)?;
        let (signing_material, signing_time) = admin
            .generate_key(public_to_fingerprint, KeyType::Signing)
            .map_err(card_error)?;
        let (decryption_material, decryption_time) = admin
            .generate_key(public_to_fingerprint, KeyType::Decryption)
            .map_err(card_error)?;
        if request.replace_user_pin {
            admin
                .reset_user_pin(SecretString::from(request.user_pin.clone()))
                .map_err(card_error)?;
        }
        let transaction = admin.as_transaction();
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
        let cert = build_public_cert(
            transaction,
            signing_key,
            Some(decryption_key),
            None,
            Some(request.user_pin.as_str()),
            &|| {},
            &|| {},
            std::slice::from_ref(&request.user_id),
        )
        .map_err(|err| map_hardware_transport_message(err.to_string()))?
        .strip_secret_key_material();
        let mut bytes = Vec::new();
        cert.serialize(&mut bytes)
            .map_err(|err| HardwareTransportError::Other(err.to_string()))?;

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
