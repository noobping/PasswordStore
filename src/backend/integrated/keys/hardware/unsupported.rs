use super::{
    DiscoveredHardwareToken, HardwareKeyGenerationRequest, HardwareTransport,
    HardwareTransportError,
};
use crate::backend::integrated::keys::cert::ManagedRipassoHardwareKey;
use sequoia_openpgp::Cert;

const UNSUPPORTED_MESSAGE: &str = "Hardware OpenPGP keys are not supported on this platform.";

#[derive(Clone)]
pub(in crate::backend::integrated) enum HardwareUnlockMode {
    Pin,
    External,
}

impl HardwareUnlockMode {
    pub(super) fn pin_mode(_pin: &str) -> Self {
        Self::Pin
    }
}

#[derive(Clone)]
pub(in crate::backend::integrated) struct HardwareSessionPolicy;

impl HardwareSessionPolicy {
    pub(super) fn from_key(
        _key: &ManagedRipassoHardwareKey,
        _cert: Cert,
        _mode: HardwareUnlockMode,
    ) -> Self {
        Self
    }
}

pub(super) struct RealHardwareTransport;

impl HardwareTransport for RealHardwareTransport {
    fn list_tokens(&self) -> Result<Vec<DiscoveredHardwareToken>, HardwareTransportError> {
        Err(HardwareTransportError::Unsupported(
            UNSUPPORTED_MESSAGE.to_string(),
        ))
    }

    fn generate_key_material(
        &self,
        _request: &HardwareKeyGenerationRequest,
    ) -> Result<(DiscoveredHardwareToken, Vec<u8>), HardwareTransportError> {
        Err(HardwareTransportError::Unsupported(
            UNSUPPORTED_MESSAGE.to_string(),
        ))
    }

    fn verify_session(
        &self,
        _session: &HardwareSessionPolicy,
    ) -> Result<(), HardwareTransportError> {
        Err(HardwareTransportError::Unsupported(
            UNSUPPORTED_MESSAGE.to_string(),
        ))
    }

    fn decrypt_ciphertext(
        &self,
        _session: &HardwareSessionPolicy,
        _ciphertext: &[u8],
    ) -> Result<String, HardwareTransportError> {
        Err(HardwareTransportError::Unsupported(
            UNSUPPORTED_MESSAGE.to_string(),
        ))
    }

    fn sign_cleartext(
        &self,
        _session: &HardwareSessionPolicy,
        _data: &str,
    ) -> Result<String, HardwareTransportError> {
        Err(HardwareTransportError::Unsupported(
            UNSUPPORTED_MESSAGE.to_string(),
        ))
    }
}
