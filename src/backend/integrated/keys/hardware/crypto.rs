use anyhow::{anyhow, Context, Result};
use openpgp_card::ocard;
#[cfg(feature = "hardwarekey")]
use openpgp_card::ocard::algorithm::{AlgorithmAttributes, Curve as CardCurve};
#[cfg(feature = "hardwarekey")]
use openpgp_card::ocard::crypto::PublicKeyMaterial;
use openpgp_card::ocard::crypto::{Cryptogram, Hash};
#[cfg(feature = "hardwarekey")]
use openpgp_card::ocard::data::{Fingerprint as CardFingerprint, KeyGenerationTime};
#[cfg(feature = "hardwarekey")]
use openpgp_card::ocard::KeyType as CardKeyType;
#[cfg(feature = "hardwarekey")]
use openpgp_card::{state, Card, Error as CardError};
use sequoia_openpgp as openpgp;
use sequoia_openpgp::armor;
use sequoia_openpgp::cert::amalgamation::key::ErasedKeyAmalgamation;
use sequoia_openpgp::crypto;
use sequoia_openpgp::crypto::mpi;
use sequoia_openpgp::packet::key;
#[cfg(feature = "hardwarekey")]
use sequoia_openpgp::packet::key::{Key4, KeyRole, PrimaryRole, SubordinateRole};
#[cfg(feature = "hardwarekey")]
use sequoia_openpgp::packet::signature::SignatureBuilder;
#[cfg(feature = "hardwarekey")]
use sequoia_openpgp::packet::Signature;
#[cfg(feature = "hardwarekey")]
use sequoia_openpgp::packet::{Key, UserID};
use sequoia_openpgp::parse::stream::{
    DecryptionHelper, DecryptorBuilder, MessageStructure, VerificationHelper,
};
use sequoia_openpgp::parse::Parse;
use sequoia_openpgp::policy::StandardPolicy;
use sequoia_openpgp::serialize::stream::{Message, Signer};
use sequoia_openpgp::types::{Curve, HashAlgorithm, SymmetricAlgorithm};
#[cfg(feature = "hardwarekey")]
use sequoia_openpgp::types::{KeyFlags, PublicKeyAlgorithm, SignatureType, Timestamp};
#[cfg(feature = "hardwarekey")]
use sequoia_openpgp::Packet;
use sequoia_openpgp::{Cert, Fingerprint, KeyHandle};
#[cfg(feature = "hardwarekey")]
use std::convert::TryInto;
use std::io;

type PublicKey = openpgp::packet::Key<key::PublicParts, key::UnspecifiedRole>;

#[cfg(feature = "hardwarekey")]
pub(super) fn build_public_cert(
    open: &mut Card<state::Transaction<'_>>,
    signing_key: PublicKey,
    decryption_key: Option<PublicKey>,
    authentication_key: Option<PublicKey>,
    user_pin: Option<&str>,
    pinpad_prompt: &dyn Fn(),
    touch_prompt: &(dyn Fn() + Send + Sync),
    user_ids: &[String],
) -> Result<Cert> {
    let mut packets = Vec::new();

    let mut sign_on_card =
        |op: &mut dyn Fn(&mut dyn sequoia_openpgp::crypto::Signer) -> Result<Signature>| {
            if let Some(user_pin) = user_pin {
                open.verify_user_signing_pin(user_pin.to_string().into())?;
            } else {
                open.verify_user_signing_pinpad(pinpad_prompt)?;
            }

            let mut signer = CardSigner::new(open.card(), signing_key.clone(), touch_prompt);
            op(&mut signer)
        };

    let primary_key = PrimaryRole::convert_key(signing_key.clone());
    packets.push(Packet::from(primary_key));

    let direct_key_signature = sign_on_card(&mut |signer| {
        SignatureBuilder::new(SignatureType::DirectKey)
            .set_key_flags(KeyFlags::empty().set_signing().set_certification())?
            .sign_direct_key(signer, signing_key.role_as_primary())
    })?;
    packets.push(direct_key_signature.into());

    if let Some(decryption_key) = decryption_key {
        let decryption_subkey = SubordinateRole::convert_key(decryption_key);
        packets.push(Packet::from(decryption_subkey.clone()));

        let cert = Cert::try_from(packets.clone())?;
        let binding = sign_on_card(&mut |signer| {
            decryption_subkey.bind(
                signer,
                &cert,
                SignatureBuilder::new(SignatureType::SubkeyBinding).set_key_flags(
                    KeyFlags::empty()
                        .set_storage_encryption()
                        .set_transport_encryption(),
                )?,
            )
        })?;
        packets.push(binding.into());
    }

    if let Some(authentication_key) = authentication_key {
        let authentication_subkey = SubordinateRole::convert_key(authentication_key);
        packets.push(Packet::from(authentication_subkey.clone()));

        let cert = Cert::try_from(packets.clone())?;
        let binding = sign_on_card(&mut |signer| {
            authentication_subkey.bind(
                signer,
                &cert,
                SignatureBuilder::new(SignatureType::SubkeyBinding)
                    .set_key_flags(KeyFlags::empty().set_authentication())?,
            )
        })?;
        packets.push(binding.into());
    }

    for user_id in user_ids.iter().map(|value| value.as_bytes()) {
        let user_id: UserID = user_id.into();
        packets.push(user_id.clone().into());

        let cert = Cert::try_from(packets.clone())?;
        let binding = sign_on_card(&mut |signer| {
            user_id.bind(
                signer,
                &cert,
                SignatureBuilder::new(SignatureType::PositiveCertification)
                    .set_key_flags(KeyFlags::empty().set_signing().set_certification())?,
            )
        })?;
        packets.push(binding.into());
    }

    Ok(Cert::try_from(packets)?)
}

#[cfg(feature = "hardwarekey")]
pub(super) fn public_key_material_and_fp_to_key(
    public_key: &PublicKeyMaterial,
    key_type: CardKeyType,
    time: &KeyGenerationTime,
    fingerprint: &CardFingerprint,
) -> Result<PublicKey, CardError> {
    let parameters: &[(Option<HashAlgorithm>, Option<SymmetricAlgorithm>)] =
        match (public_key, key_type) {
            (PublicKeyMaterial::E(_), CardKeyType::Decryption) => &[
                (
                    Some(HashAlgorithm::SHA256),
                    Some(SymmetricAlgorithm::AES128),
                ),
                (
                    Some(HashAlgorithm::SHA512),
                    Some(SymmetricAlgorithm::AES256),
                ),
                (
                    Some(HashAlgorithm::SHA384),
                    Some(SymmetricAlgorithm::AES256),
                ),
                (
                    Some(HashAlgorithm::SHA384),
                    Some(SymmetricAlgorithm::AES192),
                ),
                (
                    Some(HashAlgorithm::SHA256),
                    Some(SymmetricAlgorithm::AES256),
                ),
            ],
            _ => &[(None, None)],
        };

    for (hash, sym) in parameters {
        if let Ok(key) = public_key_material_to_key(public_key, key_type, time, *hash, *sym) {
            if key.fingerprint().as_bytes() == fingerprint.as_bytes() {
                return Ok(key);
            }
        }
    }

    Err(CardError::InternalError(
        "Couldn't find key with matching fingerprint".to_string(),
    ))
}

#[cfg(feature = "hardwarekey")]
pub(super) fn public_to_fingerprint(
    public_key: &PublicKeyMaterial,
    time: KeyGenerationTime,
    key_type: CardKeyType,
) -> Result<CardFingerprint, CardError> {
    let key = public_key_material_to_key(public_key, key_type, &time, None, None)?;
    key.fingerprint().as_bytes().try_into()
}

#[cfg(feature = "hardwarekey")]
fn public_key_material_to_key(
    public_key: &PublicKeyMaterial,
    key_type: CardKeyType,
    time: &KeyGenerationTime,
    hash: Option<HashAlgorithm>,
    sym: Option<SymmetricAlgorithm>,
) -> Result<PublicKey, CardError> {
    let time = Timestamp::from(time.get()).into();

    match public_key {
        PublicKeyMaterial::R(rsa) => {
            let key = Key4::import_public_rsa(rsa.v(), rsa.n(), Some(time)).map_err(|err| {
                CardError::InternalError(format!("sequoia Key4::import_public_rsa failed: {err:?}"))
            })?;
            Ok(key.into())
        }
        PublicKeyMaterial::E(ecc) => {
            let algorithm = ecc.algo();
            let AlgorithmAttributes::Ecc(ecc_algorithm) = &algorithm else {
                return Err(CardError::InternalError(format!(
                    "unexpected ECC algorithm attributes: {algorithm:?}"
                )));
            };

            let curve = match ecc_algorithm.curve() {
                CardCurve::NistP256r1 => Curve::NistP256,
                CardCurve::NistP384r1 => Curve::NistP384,
                CardCurve::NistP521r1 => Curve::NistP521,
                CardCurve::Ed25519 => Curve::Ed25519,
                CardCurve::Curve25519 => Curve::Cv25519,
                other => {
                    return Err(CardError::UnsupportedAlgo(format!(
                        "unhandled curve: {other:?}"
                    )))
                }
            };

            match key_type {
                CardKeyType::Authentication | CardKeyType::Signing => {
                    if ecc_algorithm.curve() == &CardCurve::Ed25519 {
                        let key = Key4::import_public_ed25519(ecc.data(), time).map_err(|err| {
                            CardError::InternalError(format!(
                                "sequoia Key4::import_public_ed25519 failed: {err:?}"
                            ))
                        })?;
                        Ok(Key::from(key))
                    } else {
                        let key = Key4::new(
                            time,
                            PublicKeyAlgorithm::ECDSA,
                            mpi::PublicKey::ECDSA {
                                curve,
                                q: mpi::MPI::new(ecc.data()),
                            },
                        )
                        .map_err(|err| {
                            CardError::InternalError(format!(
                                "sequoia Key4::new for ECDSA failed: {err:?}"
                            ))
                        })?;
                        Ok(key.into())
                    }
                }
                CardKeyType::Decryption => {
                    if ecc_algorithm.curve() == &CardCurve::Curve25519 {
                        let key = Key4::import_public_cv25519(ecc.data(), hash, sym, time)
                            .map_err(|err| {
                                CardError::InternalError(format!(
                                    "sequoia Key4::import_public_cv25519 failed: {err:?}"
                                ))
                            })?;
                        Ok(key.into())
                    } else {
                        let key = Key4::new(
                            time,
                            PublicKeyAlgorithm::ECDH,
                            mpi::PublicKey::ECDH {
                                curve,
                                q: mpi::MPI::new(ecc.data()),
                                hash: hash.unwrap_or_default(),
                                sym: sym.unwrap_or_default(),
                            },
                        )
                        .map_err(|err| {
                            CardError::InternalError(format!(
                                "sequoia Key4::new for ECDH failed: {err:?}"
                            ))
                        })?;
                        Ok(key.into())
                    }
                }
                other => Err(CardError::InternalError(format!(
                    "unsupported key type: {other:?}"
                ))),
            }
        }
    }
}

pub(super) fn decrypt_with_card_transaction(
    tx: &mut ocard::Transaction<'_>,
    cert: &Cert,
    fingerprint: Option<&str>,
    ciphertext: &[u8],
) -> Result<String> {
    let decryptor = CardDecryptor::new(tx, cert, decryption_public_key(cert, fingerprint)?, &|| {});
    let plaintext = decrypt_message(decryptor, ciphertext.to_vec(), &StandardPolicy::new())?;
    String::from_utf8(plaintext).context("Failed to decode decrypted UTF-8 data")
}

pub(super) fn sign_with_card_transaction(
    tx: &mut ocard::Transaction<'_>,
    cert: &Cert,
    fingerprint: Option<&str>,
    data: &str,
) -> Result<String> {
    let signer = CardSigner::new(tx, signing_public_key(cert, fingerprint)?, &|| {});
    sign_message(signer, &mut io::Cursor::new(data.as_bytes()))
}

fn public_key_by_fingerprint(cert: &Cert, fingerprint: &Fingerprint) -> Result<Option<PublicKey>> {
    let keys: Vec<ErasedKeyAmalgamation<'_, key::PublicParts>> = cert
        .keys()
        .filter(|ka| &ka.key().fingerprint() == fingerprint)
        .collect();

    match keys.len() {
        0 => Ok(None),
        1 => Ok(Some(keys[0].key().clone().role_into_unspecified())),
        count => Err(anyhow!(
            "Unexpected number of matching public subkeys: {count}"
        )),
    }
}

fn decryption_public_key(cert: &Cert, fingerprint: Option<&str>) -> Result<PublicKey> {
    if let Some(fingerprint) = fingerprint {
        let fingerprint = Fingerprint::from_hex(fingerprint)?;
        return public_key_by_fingerprint(cert, &fingerprint)?.ok_or_else(|| {
            anyhow!("The stored public key is missing the hardware decryption key.")
        });
    }

    let policy = StandardPolicy::new();
    cert.keys()
        .with_policy(&policy, None)
        .supported()
        .alive()
        .revoked(false)
        .for_transport_encryption()
        .next()
        .map(|ka| ka.key().clone().role_into_unspecified())
        .ok_or_else(|| anyhow!("The stored public key has no transport-encryption subkey."))
}

fn signing_public_key(cert: &Cert, fingerprint: Option<&str>) -> Result<PublicKey> {
    if let Some(fingerprint) = fingerprint {
        let fingerprint = Fingerprint::from_hex(fingerprint)?;
        return public_key_by_fingerprint(cert, &fingerprint)?
            .ok_or_else(|| anyhow!("The stored public key is missing the hardware signing key."));
    }

    let policy = StandardPolicy::new();
    cert.keys()
        .with_policy(&policy, None)
        .supported()
        .alive()
        .revoked(false)
        .for_signing()
        .next()
        .map(|ka| ka.key().clone().role_into_unspecified())
        .ok_or_else(|| anyhow!("The stored public key has no signing subkey."))
}

struct CardDecryptor<'a, 'tx> {
    tx: &'a mut ocard::Transaction<'tx>,
    cert: Cert,
    public: PublicKey,
    touch_prompt: &'a (dyn Fn() + Send + Sync),
}

impl<'a, 'tx> CardDecryptor<'a, 'tx> {
    fn new(
        tx: &'a mut ocard::Transaction<'tx>,
        cert: &Cert,
        public: PublicKey,
        touch_prompt: &'a (dyn Fn() + Send + Sync),
    ) -> Self {
        Self {
            tx,
            cert: cert.clone(),
            public,
            touch_prompt,
        }
    }
}

impl crypto::Decryptor for CardDecryptor<'_, '_> {
    fn public(&self) -> &PublicKey {
        &self.public
    }

    fn decrypt(
        &mut self,
        ciphertext: &mpi::Ciphertext,
        _plaintext_len: Option<usize>,
    ) -> openpgp::Result<openpgp::crypto::SessionKey> {
        let ard = self.tx.application_related_data()?;
        let touch_required = ard
            .uif_pso_dec()?
            .is_some_and(|uif| uif.touch_policy().touch_required());

        match (ciphertext, self.public.mpis()) {
            (mpi::Ciphertext::RSA { c: ct }, mpi::PublicKey::RSA { .. }) => {
                if touch_required {
                    (self.touch_prompt)();
                }

                let decrypted = self.tx.decipher(Cryptogram::RSA(ct.value()))?;
                Ok(openpgp::crypto::SessionKey::from(&decrypted[..]))
            }
            (mpi::Ciphertext::ECDH { e, .. }, mpi::PublicKey::ECDH { curve, .. }) => {
                let cryptogram = if curve == &Curve::Cv25519 {
                    if e.value().first().copied() != Some(0x40) {
                        return Err(anyhow!("Unexpected Cv25519 ciphertext shape"));
                    }
                    Cryptogram::ECDH(&e.value()[1..])
                } else {
                    Cryptogram::ECDH(e.value())
                };

                if touch_required {
                    (self.touch_prompt)();
                }

                let mut decrypted = self.tx.decipher(cryptogram)?;
                if curve == &Curve::NistP256 && decrypted.len() == 65 {
                    if decrypted[0] != 0x04 {
                        return Err(anyhow!("Unexpected NistP256 ciphertext shape"));
                    }
                    decrypted = decrypted[1..33].to_vec();
                }

                let shared_secret = decrypted.into();
                Ok(crypto::ecdh::decrypt_unwrap(
                    &self.public,
                    &shared_secret,
                    ciphertext,
                    None,
                )?)
            }
            (ciphertext, public) => Err(anyhow!(
                "Unsupported combination of ciphertext {:?} and public key {:?}",
                ciphertext,
                public
            )),
        }
    }
}

impl DecryptionHelper for CardDecryptor<'_, '_> {
    fn decrypt(
        &mut self,
        pkesks: &[openpgp::packet::PKESK],
        _skesks: &[openpgp::packet::SKESK],
        sym_algo: Option<SymmetricAlgorithm>,
        decrypt: &mut dyn FnMut(Option<SymmetricAlgorithm>, &openpgp::crypto::SessionKey) -> bool,
    ) -> openpgp::Result<Option<Cert>> {
        for pkesk in pkesks {
            if pkesk
                .recipient()
                .as_ref()
                .is_some_and(|recipient| recipient != &self.public.key_handle())
            {
                continue;
            }

            if pkesk
                .decrypt(self, sym_algo)
                .map(|(algo, session_key)| decrypt(algo, &session_key))
                .unwrap_or(false)
            {
                return Ok(Some(self.cert.clone()));
            }
        }

        Ok(None)
    }
}

impl VerificationHelper for CardDecryptor<'_, '_> {
    fn get_certs(&mut self, _ids: &[KeyHandle]) -> openpgp::Result<Vec<Cert>> {
        Ok(Vec::new())
    }

    fn check(&mut self, _structure: MessageStructure) -> openpgp::Result<()> {
        Ok(())
    }
}

struct CardSigner<'a, 'tx> {
    tx: &'a mut ocard::Transaction<'tx>,
    public: PublicKey,
    touch_prompt: &'a (dyn Fn() + Send + Sync),
}

impl<'a, 'tx> CardSigner<'a, 'tx> {
    fn new(
        tx: &'a mut ocard::Transaction<'tx>,
        public: PublicKey,
        touch_prompt: &'a (dyn Fn() + Send + Sync),
    ) -> Self {
        Self {
            tx,
            public,
            touch_prompt,
        }
    }
}

impl crypto::Signer for CardSigner<'_, '_> {
    fn public(&self) -> &PublicKey {
        &self.public
    }

    fn sign(&mut self, hash_algo: HashAlgorithm, digest: &[u8]) -> openpgp::Result<mpi::Signature> {
        let ard = self.tx.application_related_data()?;
        let touch_required = ard
            .uif_pso_cds()?
            .is_some_and(|uif| uif.touch_policy().touch_required());

        let signature = match self.public.mpis() {
            mpi::PublicKey::RSA { .. } => {
                let hash = match hash_algo {
                    HashAlgorithm::SHA256 => Hash::SHA256(
                        digest
                            .try_into()
                            .map_err(|_| anyhow!("Invalid SHA256 digest length"))?,
                    ),
                    HashAlgorithm::SHA384 => Hash::SHA384(
                        digest
                            .try_into()
                            .map_err(|_| anyhow!("Invalid SHA384 digest length"))?,
                    ),
                    HashAlgorithm::SHA512 => Hash::SHA512(
                        digest
                            .try_into()
                            .map_err(|_| anyhow!("Invalid SHA512 digest length"))?,
                    ),
                    _ => return Err(anyhow!("Unsupported hash algorithm for RSA: {hash_algo:?}")),
                };

                if touch_required {
                    (self.touch_prompt)();
                }

                let signature = self.tx.signature_for_hash(hash)?;
                let mpi = mpi::MPI::new(&signature[..]);
                mpi::Signature::RSA { s: mpi }
            }
            mpi::PublicKey::EdDSA { .. } => {
                if touch_required {
                    (self.touch_prompt)();
                }

                let signature = self.tx.signature_for_hash(Hash::EdDSA(digest))?;
                mpi::Signature::EdDSA {
                    r: mpi::MPI::new(&signature[..32]),
                    s: mpi::MPI::new(&signature[32..]),
                }
            }
            mpi::PublicKey::ECDSA { curve, .. } => {
                let hash = match curve {
                    Curve::NistP256 => Hash::ECDSA(&digest[..32]),
                    Curve::NistP384 => Hash::ECDSA(&digest[..48]),
                    Curve::NistP521 => Hash::ECDSA(&digest[..64]),
                    _ => Hash::ECDSA(digest),
                };

                if touch_required {
                    (self.touch_prompt)();
                }

                let signature = self.tx.signature_for_hash(hash)?;
                let midpoint = signature.len() / 2;
                mpi::Signature::ECDSA {
                    r: mpi::MPI::new(&signature[..midpoint]),
                    s: mpi::MPI::new(&signature[midpoint..]),
                }
            }
            _ => {
                return Err(anyhow!(
                    "Unsupported signing algorithm: {:?}",
                    self.public.pk_algo()
                ))
            }
        };

        Ok(signature)
    }
}

fn sign_message<S>(signer: S, input: &mut dyn io::Read) -> Result<String>
where
    S: crypto::Signer + Send + Sync,
{
    let mut armorer = armor::Writer::new(vec![], armor::Kind::Signature)?;
    {
        let message = Message::new(&mut armorer);
        let mut message = Signer::new(message, signer)?.detached().build()?;
        io::copy(input, &mut message)?;
        message.finalize()?;
    }

    let buffer = armorer.finalize()?;
    String::from_utf8(buffer).context("Failed to decode armored signature")
}

fn decrypt_message<D>(decryptor: D, message: Vec<u8>, policy: &StandardPolicy) -> Result<Vec<u8>>
where
    D: VerificationHelper + DecryptionHelper,
{
    let mut decrypted = Vec::new();
    let reader = io::BufReader::new(&message[..]);
    let builder = DecryptorBuilder::from_reader(reader)?;
    let mut decryptor = builder.with_policy(policy, None, decryptor)?;
    io::copy(&mut decryptor, &mut decrypted)?;
    Ok(decrypted)
}
