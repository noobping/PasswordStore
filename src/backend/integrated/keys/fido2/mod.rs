#[cfg(any(feature = "fidostore", feature = "fidokey"))]
mod common;
#[cfg(not(any(feature = "fidostore", feature = "fidokey")))]
mod disabled;
#[cfg(feature = "fidokey")]
mod key;
#[cfg(any(feature = "fidostore", feature = "fidokey"))]
mod store;
#[cfg(all(test, any(feature = "fidostore", feature = "fidokey")))]
mod transport_test;

#[cfg(any(feature = "fidostore", feature = "fidokey"))]
pub use self::common::set_fido2_security_key_pin;
#[cfg(any(feature = "fidostore", feature = "fidokey"))]
pub(in crate::backend::integrated) use self::common::{
    ciphertext_is_any_managed_bundle, extract_pgp_wrapped_dek_from_any_managed_bundle,
    Fido2DirectBinding, Fido2ReadProgress, Fido2WriteProgress,
};
#[cfg(feature = "fidokey")]
pub(in crate::backend::integrated) use self::key::{
    create_fido2_private_key_binding, unlock_fido2_private_key_material_for_session,
};
#[cfg(any(feature = "fidostore", feature = "fidokey"))]
pub use self::store::{create_fido2_store_recipient, unlock_fido2_store_recipient_for_session};
#[cfg(any(feature = "fidostore", feature = "fidokey"))]
pub(in crate::backend::integrated) use self::store::{
    decrypt_fido2_any_managed_bundle_dek_for_bindings,
    decrypt_fido2_any_managed_bundle_dek_for_fingerprint,
    decrypt_fido2_any_managed_bundle_for_fingerprint, decrypt_fido2_direct_required_layer,
    decrypt_payload_from_any_managed_bundle, direct_binding_from_store_recipient,
    encrypt_fido2_any_managed_bundle_with_progress, encrypt_fido2_direct_required_layer,
    reencrypt_fido2_any_managed_bundle_with_progress,
};
#[cfg(all(test, any(feature = "fidostore", feature = "fidokey")))]
pub(in crate::backend::integrated) use self::transport_test::{
    reset_fido2_transport_for_tests, set_fido2_transport_for_tests, Fido2AssertionOutput,
    Fido2DeviceLabel, Fido2Enrollment, Fido2Transport, Fido2TransportError,
};

#[cfg(not(any(feature = "fidostore", feature = "fidokey")))]
pub(in crate::backend::integrated) use self::disabled::{
    ciphertext_is_any_managed_bundle, decrypt_fido2_any_managed_bundle_dek_for_bindings,
    decrypt_fido2_any_managed_bundle_dek_for_fingerprint,
    decrypt_fido2_any_managed_bundle_for_fingerprint, decrypt_fido2_direct_required_layer,
    decrypt_payload_from_any_managed_bundle, direct_binding_from_store_recipient,
    encrypt_fido2_any_managed_bundle_with_progress, encrypt_fido2_direct_required_layer,
    extract_pgp_wrapped_dek_from_any_managed_bundle,
    reencrypt_fido2_any_managed_bundle_with_progress, Fido2DirectBinding, Fido2ReadProgress,
    Fido2WriteProgress,
};
#[cfg(not(any(feature = "fidostore", feature = "fidokey")))]
pub use self::disabled::{
    create_fido2_store_recipient, set_fido2_security_key_pin,
    unlock_fido2_store_recipient_for_session,
};
