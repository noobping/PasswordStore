use crate::backend::PrivateKeyError;
use sequoia_openpgp::{
    cert::amalgamation::key::PrimaryKey, crypto::Password, parse::Parse, Cert, Fingerprint, Packet,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ManagedRipassoPrivateKeyProtection {
    Password,
    HardwareOpenPgpCard,
    #[cfg(feature = "fidokey")]
    Fido2HmacSecret,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PrivateKeyUnlockKind {
    Password,
    HardwareOpenPgpCard,
    Fido2SecurityKey,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ManagedRipassoHardwareKey {
    pub ident: String,
    pub signing_fingerprint: Option<String>,
    pub decryption_fingerprint: Option<String>,
    pub reader_hint: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ManagedRipassoPrivateKey {
    pub fingerprint: String,
    pub user_ids: Vec<String>,
    pub protection: ManagedRipassoPrivateKeyProtection,
    pub hardware: Option<ManagedRipassoHardwareKey>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PrivateKeyUnlockRequest {
    Password(String),
    HardwarePin(String),
    HardwareExternal,
    Fido2(Option<String>),
}

impl ManagedRipassoPrivateKey {
    pub fn title(&self) -> String {
        self.user_ids
            .first()
            .cloned()
            .filter(|value| !value.trim().is_empty())
            .unwrap_or_else(|| "Unnamed private key".to_string())
    }
}

impl From<ManagedRipassoPrivateKeyProtection> for PrivateKeyUnlockKind {
    fn from(value: ManagedRipassoPrivateKeyProtection) -> Self {
        match value {
            ManagedRipassoPrivateKeyProtection::Password => Self::Password,
            ManagedRipassoPrivateKeyProtection::HardwareOpenPgpCard => Self::HardwareOpenPgpCard,
            #[cfg(feature = "fidokey")]
            ManagedRipassoPrivateKeyProtection::Fido2HmacSecret => Self::Fido2SecurityKey,
        }
    }
}

fn managed_private_key_from_cert(
    cert: &Cert,
    protection: ManagedRipassoPrivateKeyProtection,
    hardware: Option<ManagedRipassoHardwareKey>,
) -> ManagedRipassoPrivateKey {
    ManagedRipassoPrivateKey {
        fingerprint: cert.fingerprint().to_hex(),
        user_ids: cert
            .userids()
            .map(|user_id| user_id.userid().to_string())
            .filter(|value| !value.trim().is_empty())
            .collect(),
        protection,
        hardware,
    }
}

pub(in crate::backend::integrated) fn fingerprint_from_string(
    value: &str,
) -> Result<[u8; 20], String> {
    let fingerprint = Fingerprint::from_hex(value)
        .map_err(|err| format!("Invalid private key fingerprint '{value}': {err}"))?;
    let bytes = fingerprint.as_bytes();
    if bytes.len() != 20 {
        return Err(format!(
            "Private key fingerprint '{value}' does not have the expected length."
        ));
    }

    let mut parsed = [0u8; 20];
    parsed.copy_from_slice(bytes);
    Ok(parsed)
}

pub(in crate::backend::integrated) fn normalized_fingerprint(
    value: &str,
) -> Result<String, String> {
    Ok(Fingerprint::from_hex(value)
        .map_err(|err| format!("Invalid private key fingerprint '{value}': {err}"))?
        .to_hex())
}

pub(in crate::backend::integrated) fn parse_managed_private_key_bytes(
    bytes: &[u8],
) -> Result<(Cert, ManagedRipassoPrivateKey), PrivateKeyError> {
    let cert = Cert::from_bytes(bytes).map_err(|err| PrivateKeyError::other(err.to_string()))?;
    if !cert.is_tsk() {
        return Err(PrivateKeyError::missing_private_key_material(
            "That OpenPGP key file does not include a private key.",
        ));
    }

    let key =
        managed_private_key_from_cert(&cert, ManagedRipassoPrivateKeyProtection::Password, None);
    Ok((cert, key))
}

pub(in crate::backend::integrated) fn parse_hardware_public_key_bytes(
    bytes: &[u8],
    hardware: ManagedRipassoHardwareKey,
) -> Result<(Cert, ManagedRipassoPrivateKey), PrivateKeyError> {
    let cert = Cert::from_bytes(bytes).map_err(|err| PrivateKeyError::other(err.to_string()))?;
    let cert = cert.strip_secret_key_material();
    let key = managed_private_key_from_cert(
        &cert,
        ManagedRipassoPrivateKeyProtection::HardwareOpenPgpCard,
        Some(hardware),
    );
    Ok((cert, key))
}

#[cfg(feature = "fidokey")]
pub(in crate::backend::integrated) fn parse_fido2_public_key_bytes(
    bytes: &[u8],
) -> Result<(Cert, ManagedRipassoPrivateKey), PrivateKeyError> {
    let cert = Cert::from_bytes(bytes).map_err(|err| PrivateKeyError::other(err.to_string()))?;
    let cert = cert.strip_secret_key_material();
    let key = managed_private_key_from_cert(
        &cert,
        ManagedRipassoPrivateKeyProtection::Fido2HmacSecret,
        None,
    );
    Ok((cert, key))
}

pub(in crate::backend::integrated) fn cert_requires_passphrase(cert: &Cert) -> bool {
    cert.keys()
        .secret()
        .any(|key_amalgamation| !key_amalgamation.key().has_unencrypted_secret())
}

pub(in crate::backend::integrated) fn cert_has_transport_encryption_key(cert: &Cert) -> bool {
    let policy = sequoia_openpgp::policy::StandardPolicy::new();
    cert.keys()
        .with_policy(&policy, None)
        .supported()
        .alive()
        .revoked(false)
        .for_transport_encryption()
        .next()
        .is_some()
}

pub(in crate::backend::integrated) fn cert_can_decrypt_password_entries(cert: &Cert) -> bool {
    cert_has_transport_encryption_key(cert)
        && cert
            .keys()
            .with_policy(&sequoia_openpgp::policy::StandardPolicy::new(), None)
            .supported()
            .alive()
            .revoked(false)
            .for_transport_encryption()
            .unencrypted_secret()
            .next()
            .is_some()
}

fn unlock_managed_private_key_cert(cert: &Cert, passphrase: &str) -> Result<Cert, PrivateKeyError> {
    let trimmed = passphrase.trim();
    if trimmed.is_empty() {
        return Err(PrivateKeyError::passphrase_required(
            "Enter the private key password.",
        ));
    }

    let password: Password = trimmed.into();
    let mut unlocked = cert.clone();
    for key_amalgamation in cert.keys().secret() {
        if key_amalgamation.key().has_unencrypted_secret() {
            continue;
        }

        let key = key_amalgamation
            .key()
            .clone()
            .decrypt_secret(&password)
            .map_err(|_| {
                PrivateKeyError::incorrect_passphrase("The private key password is incorrect.")
            })?;
        let packet: Packet = if key_amalgamation.primary() {
            key.role_into_primary().into()
        } else {
            key.role_into_subordinate().into()
        };
        unlocked = unlocked
            .insert_packets(vec![packet])
            .map_err(|err| PrivateKeyError::other(err.to_string()))?
            .0;
    }

    Ok(unlocked)
}

pub(in crate::backend::integrated) fn prepare_managed_private_key_bytes(
    bytes: &[u8],
    passphrase: Option<&str>,
) -> Result<(Cert, ManagedRipassoPrivateKey), PrivateKeyError> {
    let (parsed_cert, key) = parse_managed_private_key_bytes(bytes)?;
    let cert = if cert_requires_passphrase(&parsed_cert) {
        let passphrase = passphrase.ok_or_else(|| {
            PrivateKeyError::passphrase_required("This private key is password protected.")
        })?;
        unlock_managed_private_key_cert(&parsed_cert, passphrase)?
    } else {
        parsed_cert
    };

    if !cert_can_decrypt_password_entries(&cert) {
        return Err(PrivateKeyError::incompatible(
            "That private key cannot decrypt password store entries.",
        ));
    }

    Ok((cert, key))
}
