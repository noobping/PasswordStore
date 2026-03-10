mod cache;
mod cert;
mod store;

pub use self::cert::ManagedRipassoPrivateKey;
#[cfg(test)]
pub(in crate::backend::integrated) use self::cache::clear_cached_unlocked_ripasso_private_keys;
pub(in crate::backend::integrated) use self::cache::available_unlocked_private_key_fingerprints;
pub(in crate::backend::integrated) use self::cert::fingerprint_from_string;
#[cfg(test)]
pub(in crate::backend::integrated) use self::cert::{
    parse_managed_private_key_bytes, prepare_managed_private_key_bytes,
};
pub(in crate::backend::integrated) use self::store::{
    build_ripasso_crypto_from_key_ring, ensure_ripasso_private_key_is_ready,
    imported_private_key_fingerprints, incompatible_private_key_error,
    load_ripasso_key_ring, load_stored_ripasso_key_ring, locked_private_key_error,
    missing_private_key_error, selected_ripasso_own_fingerprint,
};
#[cfg(test)]
pub(in crate::backend::integrated) use self::store::ripasso_keys_dir;
pub use self::store::{
    import_ripasso_private_key_bytes, is_ripasso_private_key_unlocked, list_ripasso_private_keys,
    remove_ripasso_private_key, resolved_ripasso_own_fingerprint,
    ripasso_private_key_requires_passphrase, ripasso_private_key_requires_session_unlock,
    ripasso_private_key_title, unlock_ripasso_private_key_for_session,
};
