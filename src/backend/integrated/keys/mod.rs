mod cache;
mod cert;
mod store;

pub(in crate::backend::integrated) use self::cache::cached_unlocked_ripasso_private_key;
#[cfg(test)]
pub(in crate::backend) use self::cache::clear_cached_unlocked_ripasso_private_keys;
pub(in crate::backend::integrated) use self::cert::fingerprint_from_string;
pub use self::cert::ManagedRipassoPrivateKey;
#[cfg(test)]
pub(in crate::backend::integrated) use self::cert::{
    parse_managed_private_key_bytes, prepare_managed_private_key_bytes,
};
#[cfg(test)]
pub use self::store::resolved_ripasso_own_fingerprint;
#[cfg(test)]
pub(in crate::backend::integrated) use self::store::ripasso_keys_dir;
pub(in crate::backend::integrated) use self::store::{
    build_ripasso_crypto_from_key_ring, ensure_ripasso_private_key_is_ready,
    imported_private_key_fingerprints, load_ripasso_key_ring, load_stored_ripasso_key_ring,
    missing_private_key_error, selected_ripasso_own_fingerprint,
};
pub use self::store::{
    import_ripasso_private_key_bytes, is_ripasso_private_key_unlocked, list_ripasso_private_keys,
    remove_ripasso_private_key, ripasso_private_key_requires_passphrase,
    ripasso_private_key_requires_session_unlock, ripasso_private_key_title,
    unlock_ripasso_private_key_for_session,
};
