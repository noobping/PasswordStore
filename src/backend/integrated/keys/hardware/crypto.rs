use anyhow::{anyhow, Context, Result};
use openpgp_card::ocard;
use openpgp_card::ocard::crypto::{Cryptogram, Hash};
use sequoia_openpgp as openpgp;
use sequoia_openpgp::armor;
use sequoia_openpgp::cert::amalgamation::key::ErasedKeyAmalgamation;
use sequoia_openpgp::crypto;
use sequoia_openpgp::crypto::mpi;
use sequoia_openpgp::packet::key;
use sequoia_openpgp::parse::stream::{
    DecryptionHelper, DecryptorBuilder, MessageStructure, VerificationHelper,
};
use sequoia_openpgp::parse::Parse;
use sequoia_openpgp::policy::StandardPolicy;
use sequoia_openpgp::serialize::stream::{Message, Signer};
use sequoia_openpgp::types::{Curve, HashAlgorithm, SymmetricAlgorithm};
use sequoia_openpgp::{Cert, Fingerprint, KeyHandle};
use std::convert::TryInto;
use std::io;

type PublicKey = openpgp::packet::Key<key::PublicParts, key::UnspecifiedRole>;

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

    fn sign(
        &mut self,
        hash_algo: HashAlgorithm,
        digest: &[u8],
    ) -> openpgp::Result<mpi::Signature> {
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
                    _ => {
                        return Err(anyhow!("Unsupported hash algorithm for RSA: {hash_algo:?}"))
                    }
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
            _ => return Err(anyhow!("Unsupported signing algorithm: {:?}", self.public.pk_algo())),
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
