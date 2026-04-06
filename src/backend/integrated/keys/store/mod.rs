mod manifest;
mod migration;
mod paths;
mod storage;
mod unlock;

use super::errors::{
    INCOMPATIBLE_PRIVATE_KEY_ERROR, LOCKED_PRIVATE_KEY_ERROR, MISSING_PRIVATE_KEY_ERROR,
};

const PRIVATE_KEY_NOT_STORED_ERROR: &str = "That private key is not stored in the app.";
#[cfg(not(feature = "fidokey"))]
const FIDO2_PRIVATE_KEY_FEATURE_DISABLED_ERROR: &str =
    "FIDO2 private-key support is disabled in this build of Keycord.";

pub(in crate::backend::integrated) fn missing_private_key_error() -> String {
    MISSING_PRIVATE_KEY_ERROR.to_string()
}

pub(in crate::backend::integrated) fn locked_private_key_error() -> String {
    LOCKED_PRIVATE_KEY_ERROR.to_string()
}

pub(in crate::backend::integrated) fn incompatible_private_key_error() -> String {
    INCOMPATIBLE_PRIVATE_KEY_ERROR.to_string()
}

fn private_key_not_stored_error() -> String {
    PRIVATE_KEY_NOT_STORED_ERROR.to_string()
}

#[cfg(all(test, feature = "hardwarekey"))]
pub use storage::store_ripasso_hardware_key_bytes;
pub use storage::{
    armored_ripasso_private_key, armored_ripasso_public_key, discover_ripasso_hardware_keys,
    generate_fido2_private_key, generate_ripasso_hardware_key, generate_ripasso_private_key,
    import_ripasso_hardware_key_bytes, import_ripasso_private_key_bytes,
    list_connected_smartcard_keys, list_ripasso_private_keys, remove_ripasso_private_key,
    ripasso_private_key_title, store_ripasso_private_key_bytes,
};
pub use unlock::{
    create_fido2_store_recipient, is_ripasso_private_key_unlocked,
    ripasso_private_key_requires_passphrase, ripasso_private_key_requires_session_unlock,
    set_fido2_security_key_pin, unlock_fido2_store_recipient_for_session,
    unlock_ripasso_private_key_for_session,
};

pub(crate) use migration::{
    continue_after_managed_key_storage_recovery, prepare_managed_private_key_storage_for_startup,
    ManagedKeyStorageRecovery, ManagedKeyStorageStartup,
};
#[cfg(test)]
pub(in crate::backend) use paths::ripasso_keys_dir;
#[cfg(feature = "audit")]
pub(in crate::backend) use storage::available_standard_public_certs;
#[cfg(test)]
pub use storage::resolved_ripasso_own_fingerprint;
pub(in crate::backend::integrated) use storage::{
    available_private_key_fingerprints, build_ripasso_crypto_from_key_ring,
    load_available_standard_key_ring, load_ripasso_key_ring, selected_ripasso_own_fingerprint,
};
pub(in crate::backend::integrated) use unlock::ensure_ripasso_private_key_is_ready;
