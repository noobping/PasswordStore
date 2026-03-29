mod cache;
mod cert;
mod fido2;
mod hardware;
#[cfg(target_os = "linux")]
mod hardware_crypto;
mod store;

pub(in crate::backend::integrated) use self::cache::cached_unlocked_hardware_private_key;
pub(in crate::backend::integrated) use self::cache::cached_unlocked_ripasso_private_key;
#[cfg(test)]
pub(in crate::backend) use self::cache::clear_cached_unlocked_ripasso_private_keys;
pub(in crate::backend::integrated) use self::cert::fingerprint_from_string;
#[cfg(test)]
pub(in crate::backend::integrated) use self::cert::{
    parse_managed_private_key_bytes, prepare_managed_private_key_bytes,
};
pub use self::cert::{
    ManagedRipassoHardwareKey, ManagedRipassoPrivateKey, ManagedRipassoPrivateKeyProtection,
    PrivateKeyUnlockRequest,
};
pub(in crate::backend::integrated) use self::fido2::{
    ciphertext_is_any_managed_bundle, decrypt_fido2_any_managed_bundle_for_fingerprint,
    decrypt_fido2_direct_required_layer, decrypt_payload_from_any_managed_bundle,
    direct_binding_from_store_recipient, encrypt_fido2_any_managed_bundle,
    encrypt_fido2_direct_required_layer, extract_pgp_wrapped_dek_from_any_managed_bundle,
    Fido2DirectBinding,
};
#[cfg(test)]
pub(in crate::backend::integrated) use self::fido2::{
    reset_fido2_transport_for_tests, set_fido2_transport_for_tests, Fido2AssertionOutput,
    Fido2DeviceLabel, Fido2Enrollment, Fido2Transport, Fido2TransportError,
};
pub use self::hardware::DiscoveredHardwareToken;
pub(in crate::backend::integrated) use self::hardware::{
    decrypt_with_hardware_session, sign_with_hardware_session, HardwareSessionPolicy,
};
#[cfg(test)]
pub(in crate::backend::integrated) use self::hardware::{
    reset_hardware_transport_for_tests, set_hardware_transport_for_tests, HardwareTransport,
};
#[cfg(test)]
pub use self::store::resolved_ripasso_own_fingerprint;
#[cfg(test)]
pub(in crate::backend::integrated) use self::store::ripasso_keys_dir;
#[cfg(test)]
pub use self::store::store_ripasso_hardware_key_bytes;
pub use self::store::{
    armored_ripasso_private_key, armored_ripasso_public_key, create_fido2_store_recipient,
    discover_ripasso_hardware_keys, generate_ripasso_private_key,
    import_ripasso_hardware_key_bytes, import_ripasso_private_key_bytes,
    is_ripasso_private_key_unlocked, list_ripasso_private_keys, remove_ripasso_private_key,
    ripasso_private_key_requires_passphrase, ripasso_private_key_requires_session_unlock,
    ripasso_private_key_title, store_ripasso_private_key_bytes,
    unlock_fido2_store_recipient_for_session, unlock_ripasso_private_key_for_session,
};
pub(in crate::backend::integrated) use self::store::{
    build_ripasso_crypto_from_key_ring, ensure_ripasso_private_key_is_ready,
    imported_private_key_fingerprints, load_ripasso_key_ring, load_stored_ripasso_key_ring,
    missing_private_key_error, selected_ripasso_own_fingerprint,
};
