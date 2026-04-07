use super::super::cache::{
    cache_unlocked_hardware_private_key, cache_unlocked_ripasso_private_key,
    peek_unlocked_hardware_private_key, peek_unlocked_ripasso_private_key,
};
use super::super::cert::{
    cert_can_decrypt_password_entries, cert_has_transport_encryption_key, cert_requires_passphrase,
    parse_managed_private_key_bytes, prepare_managed_private_key_bytes, ManagedRipassoPrivateKey,
    ManagedRipassoPrivateKeyProtection, PrivateKeyUnlockRequest,
};
use super::super::hardware::{
    private_key_error_from_hardware_transport_error, verify_hardware_session,
    HardwareSessionPolicy, HardwareUnlockMode,
};
#[cfg(feature = "fidokey")]
use super::manifest::{parse_fido2_private_key_manifest, validate_fido2_private_key_manifest};
use super::storage::{
    find_connected_smartcard_key, find_stored_private_key, ConnectedSmartcardEntry,
    StoredPrivateKeyEntry, StoredPrivateKeyLocation,
};
use super::{
    incompatible_private_key_error, locked_private_key_error, private_key_not_stored_error,
    PRIVATE_KEY_NOT_STORED_ERROR,
};
use crate::backend::{PasswordEntryError, PrivateKeyError};
use secrecy::{ExposeSecret, SecretString};
use std::fs;

fn stored_key_can_decrypt(entry: &StoredPrivateKeyEntry) -> bool {
    match entry.key.protection {
        ManagedRipassoPrivateKeyProtection::Password => entry
            .cert
            .as_ref()
            .is_some_and(cert_can_decrypt_password_entries),
        ManagedRipassoPrivateKeyProtection::HardwareOpenPgpCard => entry
            .cert
            .as_ref()
            .is_some_and(cert_has_transport_encryption_key),
        #[cfg(feature = "fidokey")]
        ManagedRipassoPrivateKeyProtection::Fido2HmacSecret => entry
            .cert
            .as_ref()
            .is_some_and(cert_has_transport_encryption_key),
    }
}

enum UnlockablePrivateKeyEntry {
    Stored(StoredPrivateKeyEntry),
    ConnectedSmartcard(ConnectedSmartcardEntry),
}

fn resolve_unlockable_private_key(fingerprint: &str) -> Result<UnlockablePrivateKeyEntry, String> {
    match find_stored_private_key(fingerprint) {
        Ok(entry) => Ok(UnlockablePrivateKeyEntry::Stored(entry)),
        Err(err) if err == PRIVATE_KEY_NOT_STORED_ERROR => {
            find_connected_smartcard_key(fingerprint)?
                .map(UnlockablePrivateKeyEntry::ConnectedSmartcard)
                .ok_or(err)
        }
        Err(err) => Err(err),
    }
}

#[cfg(feature = "fidokey")]
fn cached_fido2_private_key_is_unlocked(fingerprint: &str) -> Result<bool, String> {
    Ok(peek_unlocked_ripasso_private_key(fingerprint)?.is_some())
}

pub(in crate::backend::integrated) fn ensure_ripasso_private_key_is_ready(
    fingerprint: &str,
) -> Result<(), PasswordEntryError> {
    if let Some(cert) =
        peek_unlocked_ripasso_private_key(fingerprint).map_err(PasswordEntryError::other)?
    {
        if !cert_can_decrypt_password_entries(&cert) {
            return Err(PasswordEntryError::incompatible_private_key(
                incompatible_private_key_error(),
            ));
        }
        return Ok(());
    }

    let entry = resolve_unlockable_private_key(fingerprint).map_err(|err| {
        if err == PRIVATE_KEY_NOT_STORED_ERROR {
            PasswordEntryError::missing_private_key(err)
        } else {
            PasswordEntryError::other(err)
        }
    })?;

    match entry {
        UnlockablePrivateKeyEntry::Stored(entry)
            if matches!(
                entry.key.protection,
                ManagedRipassoPrivateKeyProtection::Password
            ) =>
        {
            let cert = entry
                .cert
                .as_ref()
                .ok_or_else(|| PasswordEntryError::other(private_key_not_stored_error()))?;
            if cert_requires_passphrase(cert) {
                return Err(PasswordEntryError::locked_private_key(
                    locked_private_key_error(),
                ));
            }
            if !cert_can_decrypt_password_entries(cert) {
                return Err(PasswordEntryError::incompatible_private_key(
                    incompatible_private_key_error(),
                ));
            }
            Ok(())
        }
        UnlockablePrivateKeyEntry::Stored(entry)
            if matches!(
                entry.key.protection,
                ManagedRipassoPrivateKeyProtection::HardwareOpenPgpCard
            ) =>
        {
            if peek_unlocked_hardware_private_key(fingerprint)
                .map_err(PasswordEntryError::other)?
                .is_none()
            {
                return Err(PasswordEntryError::locked_private_key(
                    locked_private_key_error(),
                ));
            }
            if !stored_key_can_decrypt(&entry) {
                return Err(PasswordEntryError::incompatible_private_key(
                    incompatible_private_key_error(),
                ));
            }
            Ok(())
        }
        UnlockablePrivateKeyEntry::ConnectedSmartcard(entry) => {
            if peek_unlocked_hardware_private_key(fingerprint)
                .map_err(PasswordEntryError::other)?
                .is_none()
            {
                return Err(PasswordEntryError::locked_private_key(
                    locked_private_key_error(),
                ));
            }
            if !cert_has_transport_encryption_key(&entry.cert) {
                return Err(PasswordEntryError::incompatible_private_key(
                    incompatible_private_key_error(),
                ));
            }
            Ok(())
        }
        #[cfg(feature = "fidokey")]
        UnlockablePrivateKeyEntry::Stored(entry)
            if matches!(
                entry.key.protection,
                ManagedRipassoPrivateKeyProtection::Fido2HmacSecret
            ) =>
        {
            if !cached_fido2_private_key_is_unlocked(fingerprint)
                .map_err(PasswordEntryError::other)?
            {
                return Err(PasswordEntryError::locked_private_key(
                    locked_private_key_error(),
                ));
            }
            if !stored_key_can_decrypt(&entry) {
                return Err(PasswordEntryError::incompatible_private_key(
                    incompatible_private_key_error(),
                ));
            }
            Ok(())
        }
        UnlockablePrivateKeyEntry::Stored(_) => {
            Err(PasswordEntryError::other(private_key_not_stored_error()))
        }
    }
}

pub fn is_ripasso_private_key_unlocked(fingerprint: &str) -> Result<bool, String> {
    match resolve_unlockable_private_key(fingerprint)? {
        UnlockablePrivateKeyEntry::Stored(entry) => match entry.key.protection {
            ManagedRipassoPrivateKeyProtection::Password => {
                Ok(peek_unlocked_ripasso_private_key(fingerprint)?.is_some())
            }
            ManagedRipassoPrivateKeyProtection::HardwareOpenPgpCard => {
                Ok(peek_unlocked_hardware_private_key(fingerprint)?.is_some())
            }
            #[cfg(feature = "fidokey")]
            ManagedRipassoPrivateKeyProtection::Fido2HmacSecret => {
                cached_fido2_private_key_is_unlocked(fingerprint)
            }
        },
        UnlockablePrivateKeyEntry::ConnectedSmartcard(_) => {
            Ok(peek_unlocked_hardware_private_key(fingerprint)?.is_some())
        }
    }
}

pub fn ripasso_private_key_requires_session_unlock(fingerprint: &str) -> Result<bool, String> {
    match resolve_unlockable_private_key(fingerprint)? {
        UnlockablePrivateKeyEntry::Stored(entry) => match entry.key.protection {
            ManagedRipassoPrivateKeyProtection::Password => {
                if peek_unlocked_ripasso_private_key(fingerprint)?.is_some() {
                    return Ok(false);
                }
                let cert = entry
                    .cert
                    .as_ref()
                    .ok_or_else(private_key_not_stored_error)?;
                Ok(cert_requires_passphrase(cert))
            }
            ManagedRipassoPrivateKeyProtection::HardwareOpenPgpCard => {
                Ok(peek_unlocked_hardware_private_key(fingerprint)?.is_none())
            }
            #[cfg(feature = "fidokey")]
            ManagedRipassoPrivateKeyProtection::Fido2HmacSecret => {
                Ok(!cached_fido2_private_key_is_unlocked(fingerprint)?)
            }
        },
        UnlockablePrivateKeyEntry::ConnectedSmartcard(_) => {
            Ok(peek_unlocked_hardware_private_key(fingerprint)?.is_none())
        }
    }
}

fn password_unlock_request(
    request: PrivateKeyUnlockRequest,
) -> Result<SecretString, PrivateKeyError> {
    match request {
        PrivateKeyUnlockRequest::Password(passphrase) => Ok(passphrase),
        PrivateKeyUnlockRequest::HardwarePin(_)
        | PrivateKeyUnlockRequest::HardwareExternal
        | PrivateKeyUnlockRequest::Fido2(_) => Err(PrivateKeyError::other(
            "This private key is password protected.",
        )),
    }
}

fn hardware_unlock_mode(
    request: PrivateKeyUnlockRequest,
) -> Result<HardwareUnlockMode, PrivateKeyError> {
    match request {
        PrivateKeyUnlockRequest::HardwarePin(pin) => {
            let trimmed = pin.expose_secret().trim();
            if trimmed.is_empty() {
                return Err(PrivateKeyError::hardware_pin_required(
                    "Enter the hardware key PIN.",
                ));
            }
            Ok(HardwareUnlockMode::from_pin(trimmed))
        }
        PrivateKeyUnlockRequest::HardwareExternal => Ok(HardwareUnlockMode::External),
        PrivateKeyUnlockRequest::Password(_) | PrivateKeyUnlockRequest::Fido2(_) => Err(
            PrivateKeyError::other("This private key requires a hardware key."),
        ),
    }
}

#[cfg(feature = "fidokey")]
fn fido2_unlock_pin(
    request: PrivateKeyUnlockRequest,
) -> Result<Option<SecretString>, PrivateKeyError> {
    match request {
        PrivateKeyUnlockRequest::Fido2(Some(pin)) => {
            let trimmed = pin.expose_secret().trim();
            if trimmed.is_empty() {
                return Err(PrivateKeyError::fido2_pin_required(
                    "Enter the FIDO2 security key PIN.",
                ));
            }
            Ok(Some(SecretString::from(trimmed)))
        }
        PrivateKeyUnlockRequest::Fido2(None) => Ok(None),
        PrivateKeyUnlockRequest::Password(_)
        | PrivateKeyUnlockRequest::HardwarePin(_)
        | PrivateKeyUnlockRequest::HardwareExternal => Err(PrivateKeyError::other(
            "This private key requires a FIDO2 security key.",
        )),
    }
}

#[cfg(feature = "fidokey")]
fn unlock_fido2_private_key_for_session(
    fingerprint: &str,
    request: PrivateKeyUnlockRequest,
) -> Result<ManagedRipassoPrivateKey, PrivateKeyError> {
    let pin = fido2_unlock_pin(request)?;
    let entry = find_stored_private_key(fingerprint).map_err(|err| {
        if err == PRIVATE_KEY_NOT_STORED_ERROR {
            PrivateKeyError::not_stored(err)
        } else {
            PrivateKeyError::other(err)
        }
    })?;
    let key = entry.key.clone();
    let StoredPrivateKeyLocation::Fido2 { path } = entry.location else {
        return Err(PrivateKeyError::other(
            "This private key requires a different unlock method.",
        ));
    };
    let manifest = parse_fido2_private_key_manifest(
        &fs::read_to_string(&path).map_err(|err| PrivateKeyError::other(err.to_string()))?,
    )
    .map_err(PrivateKeyError::other)?
    .ok_or_else(|| PrivateKeyError::other("That FIDO2-protected key is invalid."))?;
    let _ = validate_fido2_private_key_manifest(&manifest).map_err(PrivateKeyError::other)?;
    let unlocked_bytes = super::super::fido2::unlock_fido2_private_key_material_for_session(
        manifest.encrypted_private_key.as_bytes(),
        pin.as_ref().map(|pin| pin.expose_secret()),
    )?;
    let unlocked = prepare_managed_private_key_bytes(&unlocked_bytes, None)?.0;
    cache_unlocked_ripasso_private_key(unlocked);
    Ok(key)
}

pub fn unlock_ripasso_private_key_for_session(
    fingerprint: &str,
    request: PrivateKeyUnlockRequest,
) -> Result<ManagedRipassoPrivateKey, PrivateKeyError> {
    let entry = resolve_unlockable_private_key(fingerprint).map_err(|err| {
        if err == PRIVATE_KEY_NOT_STORED_ERROR {
            PrivateKeyError::not_stored(err)
        } else {
            PrivateKeyError::other(err)
        }
    })?;

    match entry {
        UnlockablePrivateKeyEntry::Stored(entry) => match entry.location {
            StoredPrivateKeyLocation::Password { path } => {
                let passphrase = password_unlock_request(request)?;
                let cert = entry
                    .cert
                    .as_ref()
                    .ok_or_else(|| PrivateKeyError::other(private_key_not_stored_error()))?;
                let unlocked = if cert_requires_passphrase(cert) {
                    prepare_managed_private_key_bytes(
                        &fs::read(&path).map_err(|err| PrivateKeyError::other(err.to_string()))?,
                        Some(passphrase.expose_secret()),
                    )?
                    .0
                } else {
                    cert.clone()
                };

                if !cert_can_decrypt_password_entries(&unlocked) {
                    return Err(PrivateKeyError::incompatible(
                        "That private key cannot decrypt password store entries.",
                    ));
                }

                cache_unlocked_ripasso_private_key(unlocked);
                Ok(entry.key)
            }
            StoredPrivateKeyLocation::Hardware { ref hardware, .. } => {
                if !stored_key_can_decrypt(&entry) {
                    return Err(PrivateKeyError::incompatible(
                        "That hardware key cannot decrypt password store entries.",
                    ));
                }
                let cert = entry
                    .cert
                    .clone()
                    .ok_or_else(|| PrivateKeyError::other(private_key_not_stored_error()))?;

                let session = HardwareSessionPolicy::from_managed_key(
                    hardware,
                    cert,
                    hardware_unlock_mode(request)?,
                );
                verify_hardware_session(&session)
                    .map_err(private_key_error_from_hardware_transport_error)?;
                cache_unlocked_hardware_private_key(fingerprint, session)
                    .map_err(PrivateKeyError::other)?;
                Ok(entry.key)
            }
            #[cfg(feature = "fidokey")]
            StoredPrivateKeyLocation::Fido2 { .. } => {
                unlock_fido2_private_key_for_session(&entry.key.fingerprint, request)
            }
        },
        UnlockablePrivateKeyEntry::ConnectedSmartcard(entry) => {
            let managed: ManagedRipassoPrivateKey = entry.key.clone().into();
            let hardware = managed
                .hardware
                .as_ref()
                .ok_or_else(|| PrivateKeyError::other(private_key_not_stored_error()))?;
            let session = HardwareSessionPolicy::from_managed_key(
                hardware,
                entry.cert,
                hardware_unlock_mode(request)?,
            );
            verify_hardware_session(&session)
                .map_err(private_key_error_from_hardware_transport_error)?;
            cache_unlocked_hardware_private_key(fingerprint, session)
                .map_err(PrivateKeyError::other)?;
            Ok(managed)
        }
    }
}

pub fn ripasso_private_key_requires_passphrase(bytes: &[u8]) -> Result<bool, PrivateKeyError> {
    #[cfg(feature = "fidokey")]
    if let Some(manifest) = super::manifest::parse_fido2_private_key_manifest_bytes(bytes)
        .map_err(PrivateKeyError::other)?
    {
        let _ = validate_fido2_private_key_manifest(&manifest).map_err(PrivateKeyError::other)?;
        return Ok(false);
    }

    let (cert, _) = parse_managed_private_key_bytes(bytes)?;
    Ok(cert_requires_passphrase(&cert))
}

pub fn create_fido2_store_recipient(pin: Option<&str>) -> Result<String, PrivateKeyError> {
    super::super::fido2::create_fido2_store_recipient(pin)
}

pub fn set_fido2_security_key_pin(new_pin: &str) -> Result<(), PrivateKeyError> {
    super::super::fido2::set_fido2_security_key_pin(new_pin)
}

pub fn unlock_fido2_store_recipient_for_session(
    recipient: &str,
    pin: Option<&str>,
) -> Result<(), PrivateKeyError> {
    super::super::fido2::unlock_fido2_store_recipient_for_session(recipient, pin)
}
