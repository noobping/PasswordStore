use super::cert::normalized_fingerprint;
use super::hardware::HardwareSessionPolicy;
use sequoia_openpgp::Cert;
use std::collections::HashMap;
use std::sync::{Arc, OnceLock, RwLock};
#[cfg(feature = "fido")]
use zeroize::Zeroizing;

fn unlocked_ripasso_private_keys() -> &'static RwLock<HashMap<String, Arc<Cert>>> {
    static UNLOCKED_KEYS: OnceLock<RwLock<HashMap<String, Arc<Cert>>>> = OnceLock::new();
    UNLOCKED_KEYS.get_or_init(|| RwLock::new(HashMap::new()))
}

fn unlocked_hardware_private_keys() -> &'static RwLock<HashMap<String, HardwareSessionPolicy>> {
    static UNLOCKED_KEYS: OnceLock<RwLock<HashMap<String, HardwareSessionPolicy>>> =
        OnceLock::new();
    UNLOCKED_KEYS.get_or_init(|| RwLock::new(HashMap::new()))
}

#[cfg(feature = "fido")]
fn cached_fido2_pins() -> &'static RwLock<HashMap<String, Arc<Zeroizing<Vec<u8>>>>> {
    static FIDO2_PINS: OnceLock<RwLock<HashMap<String, Arc<Zeroizing<Vec<u8>>>>>> = OnceLock::new();
    FIDO2_PINS.get_or_init(|| RwLock::new(HashMap::new()))
}

#[cfg(feature = "fido")]
fn pending_fido2_enrollments() -> &'static RwLock<HashMap<String, PendingFido2Enrollment>> {
    static FIDO2_ENROLLMENTS: OnceLock<RwLock<HashMap<String, PendingFido2Enrollment>>> =
        OnceLock::new();
    FIDO2_ENROLLMENTS.get_or_init(|| RwLock::new(HashMap::new()))
}

#[cfg(feature = "fido")]
#[derive(Debug)]
pub(in crate::backend::integrated) struct PendingFido2Enrollment {
    credential_id: Vec<u8>,
    hmac_salt: Zeroizing<Vec<u8>>,
    hmac_secret: Zeroizing<Vec<u8>>,
}

#[cfg(feature = "fido")]
impl PendingFido2Enrollment {
    fn new(
        credential_id: impl AsRef<[u8]>,
        hmac_salt: impl AsRef<[u8]>,
        hmac_secret: impl AsRef<[u8]>,
    ) -> Self {
        Self {
            credential_id: credential_id.as_ref().to_vec(),
            hmac_salt: Zeroizing::new(hmac_salt.as_ref().to_vec()),
            hmac_secret: Zeroizing::new(hmac_secret.as_ref().to_vec()),
        }
    }

    pub(in crate::backend::integrated) fn matches_credential_id(
        &self,
        credential_id: &[u8],
    ) -> bool {
        self.credential_id == credential_id
    }

    pub(in crate::backend::integrated) fn hmac_salt(&self) -> &[u8] {
        self.hmac_salt.as_slice()
    }

    pub(in crate::backend::integrated) fn hmac_secret(&self) -> &[u8] {
        self.hmac_secret.as_slice()
    }
}

#[cfg(feature = "fido")]
impl Clone for PendingFido2Enrollment {
    fn clone(&self) -> Self {
        Self::new(&self.credential_id, self.hmac_salt(), self.hmac_secret())
    }
}

fn with_unlocked_ripasso_keys_read<T>(f: impl FnOnce(&HashMap<String, Arc<Cert>>) -> T) -> T {
    match unlocked_ripasso_private_keys().read() {
        Ok(keys) => f(&keys),
        Err(poisoned) => {
            let keys = poisoned.into_inner();
            f(&keys)
        }
    }
}

fn with_unlocked_ripasso_keys_write<T>(f: impl FnOnce(&mut HashMap<String, Arc<Cert>>) -> T) -> T {
    match unlocked_ripasso_private_keys().write() {
        Ok(mut keys) => f(&mut keys),
        Err(poisoned) => {
            let mut keys = poisoned.into_inner();
            f(&mut keys)
        }
    }
}

fn with_unlocked_hardware_keys_read<T>(
    f: impl FnOnce(&HashMap<String, HardwareSessionPolicy>) -> T,
) -> T {
    match unlocked_hardware_private_keys().read() {
        Ok(keys) => f(&keys),
        Err(poisoned) => {
            let keys = poisoned.into_inner();
            f(&keys)
        }
    }
}

fn with_unlocked_hardware_keys_write<T>(
    f: impl FnOnce(&mut HashMap<String, HardwareSessionPolicy>) -> T,
) -> T {
    match unlocked_hardware_private_keys().write() {
        Ok(mut keys) => f(&mut keys),
        Err(poisoned) => {
            let mut keys = poisoned.into_inner();
            f(&mut keys)
        }
    }
}

#[cfg(feature = "fido")]
fn with_cached_fido2_pins_read<T>(
    f: impl FnOnce(&HashMap<String, Arc<Zeroizing<Vec<u8>>>>) -> T,
) -> T {
    match cached_fido2_pins().read() {
        Ok(pins) => f(&pins),
        Err(poisoned) => {
            let pins = poisoned.into_inner();
            f(&pins)
        }
    }
}

#[cfg(feature = "fido")]
fn with_cached_fido2_pins_write<T>(
    f: impl FnOnce(&mut HashMap<String, Arc<Zeroizing<Vec<u8>>>>) -> T,
) -> T {
    match cached_fido2_pins().write() {
        Ok(mut pins) => f(&mut pins),
        Err(poisoned) => {
            let mut pins = poisoned.into_inner();
            f(&mut pins)
        }
    }
}

#[cfg(feature = "fido")]
fn with_pending_fido2_enrollments_read<T>(
    f: impl FnOnce(&HashMap<String, PendingFido2Enrollment>) -> T,
) -> T {
    match pending_fido2_enrollments().read() {
        Ok(enrollments) => f(&enrollments),
        Err(poisoned) => {
            let enrollments = poisoned.into_inner();
            f(&enrollments)
        }
    }
}

#[cfg(feature = "fido")]
fn with_pending_fido2_enrollments_write<T>(
    f: impl FnOnce(&mut HashMap<String, PendingFido2Enrollment>) -> T,
) -> T {
    match pending_fido2_enrollments().write() {
        Ok(mut enrollments) => f(&mut enrollments),
        Err(poisoned) => {
            let mut enrollments = poisoned.into_inner();
            f(&mut enrollments)
        }
    }
}

pub(in crate::backend::integrated) fn cached_unlocked_ripasso_private_key(
    fingerprint: &str,
) -> Result<Option<Arc<Cert>>, String> {
    let fingerprint = normalized_fingerprint(fingerprint)?;
    Ok(with_unlocked_ripasso_keys_read(|keys| {
        keys.get(&fingerprint).cloned()
    }))
}

pub(in crate::backend::integrated) fn cache_unlocked_ripasso_private_key(cert: Cert) {
    let fingerprint = cert.fingerprint().to_hex();
    with_unlocked_ripasso_keys_write(|keys| {
        keys.insert(fingerprint, Arc::new(cert));
    });
}

pub(in crate::backend::integrated) fn cached_unlocked_hardware_private_key(
    fingerprint: &str,
) -> Result<Option<HardwareSessionPolicy>, String> {
    let fingerprint = normalized_fingerprint(fingerprint)?;
    Ok(with_unlocked_hardware_keys_read(|keys| {
        keys.get(&fingerprint).cloned()
    }))
}

pub(in crate::backend::integrated) fn cache_unlocked_hardware_private_key(
    fingerprint: &str,
    session: HardwareSessionPolicy,
) -> Result<(), String> {
    let fingerprint = normalized_fingerprint(fingerprint)?;
    with_unlocked_hardware_keys_write(|keys| {
        keys.insert(fingerprint, session);
    });
    Ok(())
}

#[cfg(feature = "fido")]
pub(in crate::backend::integrated) fn cached_fido2_pin(
    fingerprint: &str,
) -> Result<Option<Arc<Zeroizing<Vec<u8>>>>, String> {
    let fingerprint = normalized_fingerprint(fingerprint)?;
    Ok(with_cached_fido2_pins_read(|pins| {
        pins.get(&fingerprint).cloned()
    }))
}

#[cfg(feature = "fido")]
pub(in crate::backend::integrated) fn cache_fido2_pin(
    fingerprint: &str,
    pin: impl AsRef<[u8]>,
) -> Result<(), String> {
    let fingerprint = normalized_fingerprint(fingerprint)?;
    with_cached_fido2_pins_write(|pins| {
        pins.insert(fingerprint, Arc::new(Zeroizing::new(pin.as_ref().to_vec())));
    });
    Ok(())
}

#[cfg(feature = "fido")]
pub(in crate::backend::integrated) fn clear_cached_fido2_pin(
    fingerprint: &str,
) -> Result<(), String> {
    let fingerprint = normalized_fingerprint(fingerprint)?;
    with_cached_fido2_pins_write(|pins| {
        pins.remove(&fingerprint);
    });
    Ok(())
}

#[cfg(feature = "fido")]
pub(in crate::backend::integrated) fn cached_pending_fido2_enrollment(
    fingerprint: &str,
) -> Result<Option<PendingFido2Enrollment>, String> {
    let fingerprint = normalized_fingerprint(fingerprint)?;
    Ok(with_pending_fido2_enrollments_read(|enrollments| {
        enrollments.get(&fingerprint).cloned()
    }))
}

#[cfg(feature = "fido")]
pub(in crate::backend::integrated) fn cache_pending_fido2_enrollment(
    fingerprint: &str,
    credential_id: impl AsRef<[u8]>,
    hmac_salt: impl AsRef<[u8]>,
    hmac_secret: impl AsRef<[u8]>,
) -> Result<(), String> {
    let fingerprint = normalized_fingerprint(fingerprint)?;
    let enrollment = PendingFido2Enrollment::new(credential_id, hmac_salt, hmac_secret);
    with_pending_fido2_enrollments_write(|enrollments| {
        enrollments.insert(fingerprint, enrollment);
    });
    Ok(())
}

#[cfg(feature = "fido")]
pub(in crate::backend::integrated) fn clear_pending_fido2_enrollment(
    fingerprint: &str,
) -> Result<(), String> {
    let fingerprint = normalized_fingerprint(fingerprint)?;
    with_pending_fido2_enrollments_write(|enrollments| {
        enrollments.remove(&fingerprint);
    });
    Ok(())
}

#[cfg(not(feature = "fido"))]
pub(in crate::backend::integrated) fn clear_pending_fido2_enrollment(
    _fingerprint: &str,
) -> Result<(), String> {
    Ok(())
}

pub(in crate::backend::integrated) fn remove_cached_unlocked_ripasso_private_key(
    fingerprint: &str,
) -> Result<(), String> {
    let fingerprint = normalized_fingerprint(fingerprint)?;
    with_unlocked_ripasso_keys_write(|keys| {
        keys.remove(&fingerprint);
    });
    with_unlocked_hardware_keys_write(|keys| {
        keys.remove(&fingerprint);
    });
    #[cfg(feature = "fido")]
    with_cached_fido2_pins_write(|pins| {
        pins.remove(&fingerprint);
    });
    #[cfg(feature = "fido")]
    with_pending_fido2_enrollments_write(|enrollments| {
        enrollments.remove(&fingerprint);
    });
    Ok(())
}

#[cfg(test)]
pub(in crate::backend) fn clear_cached_unlocked_ripasso_private_keys() {
    with_unlocked_ripasso_keys_write(std::collections::HashMap::clear);
    with_unlocked_hardware_keys_write(std::collections::HashMap::clear);
    with_cached_fido2_pins_write(std::collections::HashMap::clear);
    with_pending_fido2_enrollments_write(std::collections::HashMap::clear);
}
