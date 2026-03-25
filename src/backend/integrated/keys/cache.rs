use super::cert::normalized_fingerprint;
use super::hardware::HardwareSessionPolicy;
use sequoia_openpgp::Cert;
use std::collections::HashMap;
use std::sync::{Arc, OnceLock, RwLock};

fn unlocked_ripasso_private_keys() -> &'static RwLock<HashMap<String, Arc<Cert>>> {
    static UNLOCKED_KEYS: OnceLock<RwLock<HashMap<String, Arc<Cert>>>> = OnceLock::new();
    UNLOCKED_KEYS.get_or_init(|| RwLock::new(HashMap::new()))
}

fn unlocked_hardware_private_keys() -> &'static RwLock<HashMap<String, HardwareSessionPolicy>> {
    static UNLOCKED_KEYS: OnceLock<RwLock<HashMap<String, HardwareSessionPolicy>>> =
        OnceLock::new();
    UNLOCKED_KEYS.get_or_init(|| RwLock::new(HashMap::new()))
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
    Ok(())
}

#[cfg(test)]
pub(in crate::backend) fn clear_cached_unlocked_ripasso_private_keys() {
    with_unlocked_ripasso_keys_write(std::collections::HashMap::clear);
    with_unlocked_hardware_keys_write(std::collections::HashMap::clear);
}
