use super::cert::normalized_fingerprint;
use super::hardware::HardwareSessionPolicy;
use sequoia_openpgp::Cert;
use std::collections::HashMap;
use std::sync::{Arc, OnceLock, RwLock};
use std::time::{Duration, Instant};
#[cfg(any(feature = "fidostore", feature = "fidokey"))]
use zeroize::Zeroizing;

const SECRET_CACHE_IDLE_TIMEOUT: Duration = Duration::from_secs(15 * 60);

#[derive(Clone)]
struct CacheEntry<T> {
    value: T,
    last_secret_use: Instant,
}

impl<T> CacheEntry<T> {
    fn new(value: T) -> Self {
        Self {
            value,
            last_secret_use: Instant::now(),
        }
    }

    fn is_expired_at(&self, now: Instant) -> bool {
        now.duration_since(self.last_secret_use) >= SECRET_CACHE_IDLE_TIMEOUT
    }
}

fn unlocked_ripasso_private_keys() -> &'static RwLock<HashMap<String, CacheEntry<Arc<Cert>>>> {
    static UNLOCKED_KEYS: OnceLock<RwLock<HashMap<String, CacheEntry<Arc<Cert>>>>> =
        OnceLock::new();
    UNLOCKED_KEYS.get_or_init(|| RwLock::new(HashMap::new()))
}

fn unlocked_hardware_private_keys(
) -> &'static RwLock<HashMap<String, CacheEntry<HardwareSessionPolicy>>> {
    static UNLOCKED_KEYS: OnceLock<RwLock<HashMap<String, CacheEntry<HardwareSessionPolicy>>>> =
        OnceLock::new();
    UNLOCKED_KEYS.get_or_init(|| RwLock::new(HashMap::new()))
}

#[cfg(any(feature = "fidostore", feature = "fidokey"))]
fn cached_fido2_pins() -> &'static RwLock<HashMap<String, CacheEntry<Arc<Zeroizing<Vec<u8>>>>>> {
    static FIDO2_PINS: OnceLock<RwLock<HashMap<String, CacheEntry<Arc<Zeroizing<Vec<u8>>>>>>> =
        OnceLock::new();
    FIDO2_PINS.get_or_init(|| RwLock::new(HashMap::new()))
}

#[cfg(any(feature = "fidostore", feature = "fidokey"))]
fn pending_fido2_enrollments(
) -> &'static RwLock<HashMap<String, CacheEntry<PendingFido2Enrollment>>> {
    static FIDO2_ENROLLMENTS: OnceLock<
        RwLock<HashMap<String, CacheEntry<PendingFido2Enrollment>>>,
    > = OnceLock::new();
    FIDO2_ENROLLMENTS.get_or_init(|| RwLock::new(HashMap::new()))
}

#[cfg(any(feature = "fidostore", feature = "fidokey"))]
#[derive(Debug)]
pub(in crate::backend::integrated) struct PendingFido2Enrollment {
    credential_id: Vec<u8>,
    hmac_salt: Zeroizing<Vec<u8>>,
    hmac_secret: Zeroizing<Vec<u8>>,
}

#[cfg(any(feature = "fidostore", feature = "fidokey"))]
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

#[cfg(any(feature = "fidostore", feature = "fidokey"))]
impl Clone for PendingFido2Enrollment {
    fn clone(&self) -> Self {
        Self::new(&self.credential_id, self.hmac_salt(), self.hmac_secret())
    }
}

fn with_cache_write<T, R>(
    cache: &'static RwLock<HashMap<String, CacheEntry<T>>>,
    f: impl FnOnce(&mut HashMap<String, CacheEntry<T>>, Instant) -> R,
) -> R {
    match cache.write() {
        Ok(mut entries) => {
            let now = Instant::now();
            prune_expired_entries(&mut entries, now);
            f(&mut entries, now)
        }
        Err(poisoned) => {
            let mut entries = poisoned.into_inner();
            let now = Instant::now();
            prune_expired_entries(&mut entries, now);
            f(&mut entries, now)
        }
    }
}

fn prune_expired_entries<T>(entries: &mut HashMap<String, CacheEntry<T>>, now: Instant) {
    entries.retain(|_, entry| !entry.is_expired_at(now));
}

fn peek_cache_value<T: Clone>(
    cache: &'static RwLock<HashMap<String, CacheEntry<T>>>,
    fingerprint: &str,
) -> Option<T> {
    with_cache_write(cache, |entries, _| {
        entries.get(fingerprint).map(|entry| entry.value.clone())
    })
}

fn borrow_cache_value<T: Clone>(
    cache: &'static RwLock<HashMap<String, CacheEntry<T>>>,
    fingerprint: &str,
) -> Option<T> {
    with_cache_write(cache, |entries, now| {
        let entry = entries.get_mut(fingerprint)?;
        entry.last_secret_use = now;
        Some(entry.value.clone())
    })
}

fn cache_value<T>(
    cache: &'static RwLock<HashMap<String, CacheEntry<T>>>,
    fingerprint: String,
    value: T,
) {
    with_cache_write(cache, |entries, _| {
        entries.insert(fingerprint, CacheEntry::new(value));
    });
}

fn remove_cache_value<T>(
    cache: &'static RwLock<HashMap<String, CacheEntry<T>>>,
    fingerprint: &str,
) {
    with_cache_write(cache, |entries, _| {
        entries.remove(fingerprint);
    });
}

fn clear_cache<T>(cache: &'static RwLock<HashMap<String, CacheEntry<T>>>) {
    with_cache_write(cache, |entries, _| entries.clear());
}

pub(in crate::backend::integrated) fn peek_unlocked_ripasso_private_key(
    fingerprint: &str,
) -> Result<Option<Arc<Cert>>, String> {
    let fingerprint = normalized_fingerprint(fingerprint)?;
    Ok(peek_cache_value(
        unlocked_ripasso_private_keys(),
        &fingerprint,
    ))
}

pub(in crate::backend::integrated) fn borrow_unlocked_ripasso_private_key(
    fingerprint: &str,
) -> Result<Option<Arc<Cert>>, String> {
    let fingerprint = normalized_fingerprint(fingerprint)?;
    Ok(borrow_cache_value(
        unlocked_ripasso_private_keys(),
        &fingerprint,
    ))
}

pub(in crate::backend::integrated) fn cache_unlocked_ripasso_private_key(cert: Cert) {
    cache_value(
        unlocked_ripasso_private_keys(),
        cert.fingerprint().to_hex(),
        Arc::new(cert),
    );
}

pub(in crate::backend::integrated) fn peek_unlocked_hardware_private_key(
    fingerprint: &str,
) -> Result<Option<HardwareSessionPolicy>, String> {
    let fingerprint = normalized_fingerprint(fingerprint)?;
    Ok(peek_cache_value(
        unlocked_hardware_private_keys(),
        &fingerprint,
    ))
}

pub(in crate::backend::integrated) fn borrow_unlocked_hardware_private_key(
    fingerprint: &str,
) -> Result<Option<HardwareSessionPolicy>, String> {
    let fingerprint = normalized_fingerprint(fingerprint)?;
    Ok(borrow_cache_value(
        unlocked_hardware_private_keys(),
        &fingerprint,
    ))
}

pub(in crate::backend::integrated) fn cache_unlocked_hardware_private_key(
    fingerprint: &str,
    session: HardwareSessionPolicy,
) -> Result<(), String> {
    let fingerprint = normalized_fingerprint(fingerprint)?;
    cache_value(unlocked_hardware_private_keys(), fingerprint, session);
    Ok(())
}

#[cfg(any(feature = "fidostore", feature = "fidokey"))]
#[cfg_attr(not(test), allow(dead_code))]
pub(in crate::backend::integrated) fn peek_cached_fido2_pin(
    fingerprint: &str,
) -> Result<Option<Arc<Zeroizing<Vec<u8>>>>, String> {
    let fingerprint = normalized_fingerprint(fingerprint)?;
    Ok(peek_cache_value(cached_fido2_pins(), &fingerprint))
}

#[cfg(any(feature = "fidostore", feature = "fidokey"))]
pub(in crate::backend::integrated) fn borrow_cached_fido2_pin(
    fingerprint: &str,
) -> Result<Option<Arc<Zeroizing<Vec<u8>>>>, String> {
    let fingerprint = normalized_fingerprint(fingerprint)?;
    Ok(borrow_cache_value(cached_fido2_pins(), &fingerprint))
}

#[cfg(any(feature = "fidostore", feature = "fidokey"))]
pub(in crate::backend::integrated) fn cache_fido2_pin(
    fingerprint: &str,
    pin: impl AsRef<[u8]>,
) -> Result<(), String> {
    let fingerprint = normalized_fingerprint(fingerprint)?;
    cache_value(
        cached_fido2_pins(),
        fingerprint,
        Arc::new(Zeroizing::new(pin.as_ref().to_vec())),
    );
    Ok(())
}

#[cfg(any(feature = "fidostore", feature = "fidokey"))]
pub(in crate::backend::integrated) fn clear_cached_fido2_pin(
    fingerprint: &str,
) -> Result<(), String> {
    let fingerprint = normalized_fingerprint(fingerprint)?;
    remove_cache_value(cached_fido2_pins(), &fingerprint);
    Ok(())
}

#[cfg(any(feature = "fidostore", feature = "fidokey"))]
#[cfg_attr(not(test), allow(dead_code))]
pub(in crate::backend::integrated) fn peek_pending_fido2_enrollment(
    fingerprint: &str,
) -> Result<Option<PendingFido2Enrollment>, String> {
    let fingerprint = normalized_fingerprint(fingerprint)?;
    Ok(peek_cache_value(pending_fido2_enrollments(), &fingerprint))
}

#[cfg(any(feature = "fidostore", feature = "fidokey"))]
pub(in crate::backend::integrated) fn borrow_pending_fido2_enrollment(
    fingerprint: &str,
) -> Result<Option<PendingFido2Enrollment>, String> {
    let fingerprint = normalized_fingerprint(fingerprint)?;
    Ok(borrow_cache_value(
        pending_fido2_enrollments(),
        &fingerprint,
    ))
}

#[cfg_attr(not(feature = "fidostore"), allow(dead_code))]
#[cfg(any(feature = "fidostore", feature = "fidokey"))]
pub(in crate::backend::integrated) fn cache_pending_fido2_enrollment(
    fingerprint: &str,
    credential_id: impl AsRef<[u8]>,
    hmac_salt: impl AsRef<[u8]>,
    hmac_secret: impl AsRef<[u8]>,
) -> Result<(), String> {
    let fingerprint = normalized_fingerprint(fingerprint)?;
    let enrollment = PendingFido2Enrollment::new(credential_id, hmac_salt, hmac_secret);
    cache_value(pending_fido2_enrollments(), fingerprint, enrollment);
    Ok(())
}

#[cfg(any(feature = "fidostore", feature = "fidokey"))]
pub(in crate::backend::integrated) fn clear_pending_fido2_enrollment(
    fingerprint: &str,
) -> Result<(), String> {
    let fingerprint = normalized_fingerprint(fingerprint)?;
    remove_cache_value(pending_fido2_enrollments(), &fingerprint);
    Ok(())
}

#[cfg(not(any(feature = "fidostore", feature = "fidokey")))]
pub(in crate::backend::integrated) fn clear_pending_fido2_enrollment(
    _fingerprint: &str,
) -> Result<(), String> {
    Ok(())
}

pub(in crate::backend::integrated) fn remove_cached_unlocked_ripasso_private_key(
    fingerprint: &str,
) -> Result<(), String> {
    let fingerprint = normalized_fingerprint(fingerprint)?;
    remove_cache_value(unlocked_ripasso_private_keys(), &fingerprint);
    remove_cache_value(unlocked_hardware_private_keys(), &fingerprint);
    #[cfg(any(feature = "fidostore", feature = "fidokey"))]
    remove_cache_value(cached_fido2_pins(), &fingerprint);
    #[cfg(any(feature = "fidostore", feature = "fidokey"))]
    remove_cache_value(pending_fido2_enrollments(), &fingerprint);
    Ok(())
}

pub(in crate::backend) fn clear_integrated_runtime_secret_state() {
    clear_cache(unlocked_ripasso_private_keys());
    clear_cache(unlocked_hardware_private_keys());
    #[cfg(any(feature = "fidostore", feature = "fidokey"))]
    clear_cache(cached_fido2_pins());
    #[cfg(any(feature = "fidostore", feature = "fidokey"))]
    clear_cache(pending_fido2_enrollments());
}

#[cfg(test)]
pub(in crate::backend) fn clear_cached_unlocked_ripasso_private_keys() {
    clear_integrated_runtime_secret_state();
}

#[cfg(test)]
mod tests {
    use super::{
        borrow_cache_value, borrow_cached_fido2_pin, borrow_unlocked_ripasso_private_key,
        cache_fido2_pin, cache_unlocked_ripasso_private_key, clear_integrated_runtime_secret_state,
        peek_cache_value, peek_cached_fido2_pin, peek_unlocked_ripasso_private_key,
        pending_fido2_enrollments, unlocked_ripasso_private_keys, CacheEntry,
        SECRET_CACHE_IDLE_TIMEOUT,
    };
    use sequoia_openpgp::Cert;
    use std::sync::Arc;
    use std::time::Duration;
    use zeroize::Zeroizing;

    fn test_cert() -> Cert {
        let (cert, _) = sequoia_openpgp::cert::CertBuilder::general_purpose(Some("Cache Test"))
            .generate()
            .expect("generate test cert");
        cert
    }

    fn expire_ripasso_entry(fingerprint: &str) {
        match unlocked_ripasso_private_keys().write() {
            Ok(mut entries) => {
                let entry = entries
                    .get_mut(fingerprint)
                    .expect("ripasso cache entry should exist");
                entry.last_secret_use -= SECRET_CACHE_IDLE_TIMEOUT + Duration::from_secs(1);
            }
            Err(poisoned) => {
                let mut entries = poisoned.into_inner();
                let entry = entries
                    .get_mut(fingerprint)
                    .expect("ripasso cache entry should exist");
                entry.last_secret_use -= SECRET_CACHE_IDLE_TIMEOUT + Duration::from_secs(1);
            }
        }
    }

    #[cfg(any(feature = "fidostore", feature = "fidokey"))]
    fn expire_fido_pin_entry(fingerprint: &str) {
        match super::cached_fido2_pins().write() {
            Ok(mut entries) => {
                let entry = entries
                    .get_mut(fingerprint)
                    .expect("fido2 pin cache entry should exist");
                entry.last_secret_use -= SECRET_CACHE_IDLE_TIMEOUT + Duration::from_secs(1);
            }
            Err(poisoned) => {
                let mut entries = poisoned.into_inner();
                let entry = entries
                    .get_mut(fingerprint)
                    .expect("fido2 pin cache entry should exist");
                entry.last_secret_use -= SECRET_CACHE_IDLE_TIMEOUT + Duration::from_secs(1);
            }
        }
    }

    #[test]
    fn peek_prunes_expired_ripasso_entries_without_refreshing() {
        clear_integrated_runtime_secret_state();
        let cert = test_cert();
        let fingerprint = cert.fingerprint().to_hex();
        cache_unlocked_ripasso_private_key(cert);
        expire_ripasso_entry(&fingerprint);

        assert!(peek_unlocked_ripasso_private_key(&fingerprint)
            .expect("peek cache")
            .is_none());
        assert!(borrow_unlocked_ripasso_private_key(&fingerprint)
            .expect("borrow cache")
            .is_none());
    }

    #[test]
    fn borrow_refreshes_secret_use_for_ripasso_entries() {
        clear_integrated_runtime_secret_state();
        let cert = test_cert();
        let fingerprint = cert.fingerprint().to_hex();
        cache_unlocked_ripasso_private_key(cert.clone());
        expire_ripasso_entry(&fingerprint);

        // Reinsert with a fresh timestamp, then make sure borrow keeps it alive.
        cache_unlocked_ripasso_private_key(cert);
        let borrowed = borrow_unlocked_ripasso_private_key(&fingerprint)
            .expect("borrow cache")
            .expect("entry should exist");
        assert_eq!(borrowed.fingerprint().to_hex(), fingerprint);
        assert!(peek_unlocked_ripasso_private_key(&fingerprint)
            .expect("peek cache")
            .is_some());
    }

    #[cfg(any(feature = "fidostore", feature = "fidokey"))]
    #[test]
    fn peek_does_not_refresh_fido_pin_entries() {
        clear_integrated_runtime_secret_state();
        let fingerprint = "0123456789abcdef0123456789abcdef01234567";
        cache_fido2_pin(fingerprint, b"1234").expect("cache fido2 pin");
        let _ = peek_cached_fido2_pin(fingerprint).expect("peek fido2 pin");
        expire_fido_pin_entry(fingerprint);

        assert!(peek_cached_fido2_pin(fingerprint)
            .expect("peek expired pin")
            .is_none());
    }

    #[cfg(any(feature = "fidostore", feature = "fidokey"))]
    #[test]
    fn borrow_refreshes_fido_pin_entries() {
        clear_integrated_runtime_secret_state();
        let fingerprint = "0123456789abcdef0123456789abcdef01234567";
        cache_fido2_pin(fingerprint, b"1234").expect("cache fido2 pin");
        let borrowed = borrow_cached_fido2_pin(fingerprint)
            .expect("borrow pin")
            .expect("cached pin should exist");
        assert_eq!(borrowed.as_slice(), b"1234");
    }

    #[test]
    fn shutdown_cleanup_clears_runtime_secret_state() {
        clear_integrated_runtime_secret_state();
        let cert = test_cert();
        let fingerprint = cert.fingerprint().to_hex();
        cache_unlocked_ripasso_private_key(cert);

        clear_integrated_runtime_secret_state();

        assert!(peek_unlocked_ripasso_private_key(&fingerprint)
            .expect("peek cache")
            .is_none());
    }
}
