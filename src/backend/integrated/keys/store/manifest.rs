#[cfg(feature = "fidokey")]
use super::super::cert::parse_fido2_public_key_bytes;
#[cfg(any(
    feature = "smartcard",
    feature = "hardwarekey",
    feature = "fidokey",
    all(test, feature = "legacy-compat")
))]
use super::super::cert::ManagedRipassoPrivateKey;
#[cfg(feature = "fidokey")]
use super::super::cert::{normalized_fingerprint, ManagedRipassoPrivateKeyProtection};
use super::super::cert::{parse_hardware_public_key_bytes, ManagedRipassoHardwareKey};
#[cfg(feature = "fidokey")]
use crate::backend::PrivateKeyError;
use crate::support::toml_safety::{parse_toml_with_limits, MANAGED_KEY_MANIFEST_TOML_LIMITS};
#[cfg(feature = "fidokey")]
use sequoia_openpgp::Cert;
use serde::{Deserialize, Serialize as SerdeSerialize};
use std::path::Path;

const HARDWARE_MANIFEST_FORMAT: u32 = 1;
const HARDWARE_PROTECTION_KIND: &str = "hardware-openpgp-card";
#[cfg(feature = "fidokey")]
const FIDO2_PRIVATE_KEY_MANIFEST_FORMAT: u32 = 1;
#[cfg(feature = "fidokey")]
const FIDO2_PRIVATE_KEY_PROTECTION_KIND: &str = "fido2-hmac-secret";

#[derive(Debug, Clone, SerdeSerialize, Deserialize)]
pub(super) struct HardwarePrivateKeyManifest {
    format: u32,
    protection: String,
    fingerprint: String,
    user_ids: Vec<String>,
    ident: String,
    signing_fingerprint: Option<String>,
    decryption_fingerprint: Option<String>,
    reader_hint: Option<String>,
}

#[cfg(feature = "fidokey")]
#[derive(Debug, Clone, SerdeSerialize, Deserialize)]
pub(super) struct Fido2PrivateKeyManifest {
    pub(super) format: u32,
    pub(super) protection: String,
    pub(super) fingerprint: String,
    pub(super) public_key: String,
    pub(super) encrypted_private_key: String,
}

impl HardwarePrivateKeyManifest {
    #[cfg(any(feature = "smartcard", all(test, feature = "legacy-compat")))]
    pub(super) fn from_key(
        key: &ManagedRipassoPrivateKey,
        hardware: &ManagedRipassoHardwareKey,
    ) -> Self {
        Self {
            format: HARDWARE_MANIFEST_FORMAT,
            protection: HARDWARE_PROTECTION_KIND.to_string(),
            fingerprint: key.fingerprint.clone(),
            user_ids: key.user_ids.clone(),
            ident: hardware.ident.clone(),
            signing_fingerprint: hardware.signing_fingerprint.clone(),
            decryption_fingerprint: hardware.decryption_fingerprint.clone(),
            reader_hint: hardware.reader_hint.clone(),
        }
    }

    pub(super) fn hardware(&self) -> ManagedRipassoHardwareKey {
        ManagedRipassoHardwareKey {
            ident: self.ident.clone(),
            signing_fingerprint: self.signing_fingerprint.clone(),
            decryption_fingerprint: self.decryption_fingerprint.clone(),
            reader_hint: self.reader_hint.clone(),
        }
    }
}

#[cfg(feature = "fidokey")]
pub(super) fn managed_fido2_private_key_from_cert(cert: &Cert) -> ManagedRipassoPrivateKey {
    ManagedRipassoPrivateKey {
        fingerprint: cert.fingerprint().to_hex(),
        user_ids: cert
            .userids()
            .map(|user_id| user_id.userid().to_string())
            .filter(|value| !value.trim().is_empty())
            .collect(),
        protection: ManagedRipassoPrivateKeyProtection::Fido2HmacSecret,
        hardware: None,
    }
}

#[cfg(feature = "fidokey")]
pub(super) fn parse_fido2_private_key_manifest(
    contents: &str,
) -> Result<Option<Fido2PrivateKeyManifest>, String> {
    crate::support::toml_safety::validate_toml_input(
        contents,
        MANAGED_KEY_MANIFEST_TOML_LIMITS,
        "FIDO2 private key manifest",
    )?;
    toml::from_str(contents).map(Some).or(Ok(None))
}

#[cfg(feature = "fidokey")]
pub(super) fn parse_fido2_private_key_manifest_bytes(
    bytes: &[u8],
) -> Result<Option<Fido2PrivateKeyManifest>, String> {
    let Ok(contents) = std::str::from_utf8(bytes) else {
        return Ok(None);
    };
    parse_fido2_private_key_manifest(contents)
}

#[cfg(feature = "fidokey")]
pub(super) fn validate_fido2_private_key_manifest(
    manifest: &Fido2PrivateKeyManifest,
) -> Result<(Cert, ManagedRipassoPrivateKey), String> {
    if manifest.format != FIDO2_PRIVATE_KEY_MANIFEST_FORMAT {
        return Err(format!(
            "Unsupported FIDO2 private key format {}.",
            manifest.format
        ));
    }
    if manifest.protection != FIDO2_PRIVATE_KEY_PROTECTION_KIND {
        return Err(format!(
            "Unsupported FIDO2 private key protection '{}'.",
            manifest.protection
        ));
    }

    let (cert, key) = parse_fido2_public_key_bytes(manifest.public_key.as_bytes())
        .map_err(|err| err.to_string())?;
    let expected = normalized_fingerprint(&manifest.fingerprint)?;
    if !key.fingerprint.eq_ignore_ascii_case(&expected) {
        return Err("That FIDO2-protected key is invalid.".to_string());
    }

    Ok((cert, key))
}

#[cfg(feature = "fidokey")]
pub(super) fn read_fido2_private_key_manifest_entry(
    path: &Path,
    manifest: Fido2PrivateKeyManifest,
) -> Result<super::storage::StoredPrivateKeyEntry, String> {
    let (cert, key) = validate_fido2_private_key_manifest(&manifest)?;

    Ok(super::storage::StoredPrivateKeyEntry {
        cert: Some(cert),
        key,
        location: super::storage::StoredPrivateKeyLocation::Fido2 {
            path: path.to_path_buf(),
        },
    })
}

#[cfg(feature = "fidokey")]
pub(super) fn fido2_private_key_manifest_contents(
    manifest: &Fido2PrivateKeyManifest,
) -> Result<String, PrivateKeyError> {
    toml::to_string_pretty(manifest).map_err(|err| PrivateKeyError::other(err.to_string()))
}

pub(super) fn read_hardware_private_key_manifest_entry(
    dir: &Path,
    manifest: HardwarePrivateKeyManifest,
) -> Result<super::storage::StoredPrivateKeyEntry, String> {
    if manifest.format != HARDWARE_MANIFEST_FORMAT {
        return Err(format!(
            "Unsupported hardware key manifest format {}.",
            manifest.format
        ));
    }
    if manifest.protection != HARDWARE_PROTECTION_KIND {
        return Err(format!(
            "Unsupported hardware key protection '{}'.",
            manifest.protection
        ));
    }

    let hardware = manifest.hardware();
    let (cert, mut key) = parse_hardware_public_key_bytes(
        &std::fs::read(super::paths::hardware_public_key_path(dir))
            .map_err(|err| err.to_string())?,
        hardware.clone(),
    )
    .map_err(|err| err.to_string())?;
    key.user_ids = manifest.user_ids;

    Ok(super::storage::StoredPrivateKeyEntry {
        cert: Some(cert),
        key,
        location: super::storage::StoredPrivateKeyLocation::Hardware {
            dir: dir.to_path_buf(),
            hardware,
        },
    })
}

pub(super) fn read_hardware_private_key_manifest(
    dir: &Path,
) -> Result<HardwarePrivateKeyManifest, String> {
    let contents = std::fs::read_to_string(super::paths::hardware_manifest_path(dir))
        .map_err(|err| err.to_string())?;
    parse_toml_with_limits(
        &contents,
        MANAGED_KEY_MANIFEST_TOML_LIMITS,
        "hardware key manifest",
    )
}
