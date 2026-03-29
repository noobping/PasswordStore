use super::cert::normalized_fingerprint;
use super::hardware::HardwareSessionPolicy;
use sequoia_openpgp::Cert;
use std::collections::HashMap;
use std::sync::{Arc, OnceLock, RwLock};
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

fn cached_fido2_pins() -> &'static RwLock<HashMap<String, Arc<Zeroizing<Vec<u8>>>>> {
    static FIDO2_PINS: OnceLock<RwLock<HashMap<String, Arc<Zeroizing<Vec<u8>>>>>> = OnceLock::new();
    FIDO2_PINS.get_or_init(|| RwLock::new(HashMap::new()))
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

pub(in crate::backend::integrated) fn cached_fido2_pin(
    fingerprint: &str,
) -> Result<Option<Arc<Zeroizing<Vec<u8>>>>, String> {
    let fingerprint = normalized_fingerprint(fingerprint)?;
    Ok(with_cached_fido2_pins_read(|pins| {
        pins.get(&fingerprint).cloned()
    }))
}

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

pub(in crate::backend::integrated) fn clear_cached_fido2_pin(
    fingerprint: &str,
) -> Result<(), String> {
    let fingerprint = normalized_fingerprint(fingerprint)?;
    with_cached_fido2_pins_write(|pins| {
        pins.remove(&fingerprint);
    });
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
    with_cached_fido2_pins_write(|pins| {
        pins.remove(&fingerprint);
    });
    Ok(())
}

#[cfg(test)]
pub(in crate::backend) fn clear_cached_unlocked_ripasso_private_keys() {
    with_unlocked_ripasso_keys_write(std::collections::HashMap::clear);
    with_unlocked_hardware_keys_write(std::collections::HashMap::clear);
    with_cached_fido2_pins_write(std::collections::HashMap::clear);
}
