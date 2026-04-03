#![expect(
    clippy::significant_drop_tightening,
    reason = "SystemBackendTestEnv intentionally stays alive through each test to preserve the temp workspace and test env vars."
)]

use super::crypto::IntegratedCryptoContext;
use super::entries::{
    delete_password_entry, password_entry_is_readable, read_password_entry, rename_password_entry,
    save_password_entry,
};
#[cfg(any(feature = "fidostore", feature = "fidokey"))]
use super::entries::{
    password_entry_fido2_recipient_count, read_password_entry_with_progress,
    save_password_entry_with_progress,
};
use super::git::{
    git_commit_private_key_requiring_unlock_for_entry,
    git_commit_private_key_requiring_unlock_for_store_recipients as git_commit_private_key_requiring_unlock_for_split_store_recipients,
};
#[cfg(feature = "fidokey")]
use super::keys::generate_fido2_private_key;
use super::keys::{
    armored_ripasso_private_key, clear_cached_unlocked_ripasso_private_keys,
    discover_ripasso_hardware_keys, ensure_ripasso_private_key_is_ready,
    generate_ripasso_private_key, import_ripasso_hardware_key_bytes,
    import_ripasso_private_key_bytes, is_ripasso_private_key_unlocked, list_ripasso_private_keys,
    parse_managed_private_key_bytes, prepare_managed_private_key_bytes, remove_ripasso_private_key,
    reset_hardware_transport_for_tests, resolved_ripasso_own_fingerprint, ripasso_keys_dir,
    ripasso_private_key_requires_passphrase, ripasso_private_key_requires_session_unlock,
    set_hardware_transport_for_tests, store_ripasso_hardware_key_bytes,
    unlock_ripasso_private_key_for_session, DiscoveredHardwareToken, HardwareSessionPolicy,
    HardwareTransport, HardwareTransportError, ManagedRipassoHardwareKey,
    ManagedRipassoPrivateKeyProtection, PrivateKeyUnlockRequest,
};
#[cfg(any(feature = "fidostore", feature = "fidokey"))]
use super::keys::{
    create_fido2_store_recipient, decrypt_fido2_any_managed_bundle_dek_for_fingerprint,
    decrypt_fido2_any_managed_bundle_for_fingerprint, decrypt_payload_from_any_managed_bundle,
    direct_binding_from_store_recipient, encrypt_fido2_any_managed_bundle_with_progress,
    reset_fido2_transport_for_tests, set_fido2_transport_for_tests,
    unlock_fido2_store_recipient_for_session, Fido2AssertionOutput, Fido2DeviceLabel,
    Fido2Enrollment, Fido2Transport, Fido2TransportError,
};
use super::paths::{recipients_file_for_label, secret_entry_relative_path};
#[cfg(any(feature = "fidostore", feature = "fidokey"))]
use super::store::save_store_recipients_with_progress as save_split_store_recipients_with_progress;
use super::store::{
    save_store_recipients as save_split_store_recipients,
    store_recipients_private_key_requiring_unlock,
};
use crate::backend::{
    test_support::SystemBackendTestEnv, PasswordEntryError, PasswordEntryWriteError,
    PrivateKeyError, StoreRecipientsError, StoreRecipientsPrivateKeyRequirement,
};
#[cfg(any(feature = "fidostore", feature = "fidokey"))]
use crate::backend::{
    PasswordEntryReadProgress, PasswordEntryWriteProgress, StoreRecipientsSaveProgress,
    StoreRecipientsSaveStage,
};
#[cfg(feature = "fidokey")]
use crate::fido2_recipient::is_fido2_recipient_string;
#[cfg(any(feature = "fidostore", feature = "fidokey"))]
use crate::fido2_recipient::{build_fido2_recipient_string, FIDO2_RECIPIENTS_FILE_NAME};
use crate::preferences::Preferences;
use crate::store::recipients::split_store_recipients;
use crate::support::git::has_git_repository;
use sequoia_openpgp::{cert::CertBuilder, crypto::Password, parse::Parse, serialize::Serialize};
#[cfg(any(feature = "fidostore", feature = "fidokey"))]
use std::collections::HashSet;
use std::fs;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

fn cert_bytes(email: &str) -> Vec<u8> {
    let (cert, _) = CertBuilder::general_purpose(Some(email))
        .generate()
        .expect("failed to generate test certificate");
    let mut bytes = Vec::new();
    cert.as_tsk()
        .serialize(&mut bytes)
        .expect("failed to serialize test certificate");
    bytes
}

fn protected_cert(email: &str) -> (sequoia_openpgp::Cert, Vec<u8>) {
    let password: Password = "hunter2".into();
    let (cert, _) = CertBuilder::general_purpose(Some(email))
        .set_password(Some(password))
        .generate()
        .expect("failed to generate password-protected certificate");
    let mut bytes = Vec::new();
    cert.as_tsk()
        .serialize(&mut bytes)
        .expect("failed to serialize protected test certificate");
    (cert, bytes)
}

fn protected_cert_bytes(email: &str) -> Vec<u8> {
    protected_cert(email).1
}

fn public_cert_bytes(email: &str) -> Vec<u8> {
    let (cert, _) = CertBuilder::general_purpose(Some(email))
        .generate()
        .expect("failed to generate public test certificate");
    let public_only = cert.strip_secret_key_material();
    let mut bytes = Vec::new();
    public_only
        .serialize(&mut bytes)
        .expect("failed to serialize public test certificate");
    bytes
}

fn save_store_recipients(
    store_root: &str,
    recipients: &[String],
    private_key_requirement: StoreRecipientsPrivateKeyRequirement,
) -> Result<(), StoreRecipientsError> {
    let recipients = split_store_recipients(recipients);
    save_split_store_recipients(store_root, &recipients, private_key_requirement)
}

#[cfg(any(feature = "fidostore", feature = "fidokey"))]
fn save_store_recipients_with_progress(
    store_root: &str,
    recipients: &[String],
    private_key_requirement: StoreRecipientsPrivateKeyRequirement,
    report_progress: &mut dyn FnMut(StoreRecipientsSaveProgress),
) -> Result<(), StoreRecipientsError> {
    let recipients = split_store_recipients(recipients);
    save_split_store_recipients_with_progress(
        store_root,
        &recipients,
        private_key_requirement,
        report_progress,
    )
}

fn git_commit_private_key_requiring_unlock_for_store_recipients(
    store_root: &str,
    recipients: &[String],
    private_key_requirement: StoreRecipientsPrivateKeyRequirement,
) -> Result<Option<String>, String> {
    let recipients = split_store_recipients(recipients);
    git_commit_private_key_requiring_unlock_for_split_store_recipients(
        store_root,
        &recipients,
        private_key_requirement,
    )
}

#[derive(Default)]
struct MockHardwareTransport {
    tokens: Mutex<Vec<DiscoveredHardwareToken>>,
    decrypt_response: Mutex<Option<String>>,
    sign_response: Mutex<Option<String>>,
}

impl MockHardwareTransport {
    fn with_tokens(tokens: Vec<DiscoveredHardwareToken>) -> Self {
        Self {
            tokens: Mutex::new(tokens),
            decrypt_response: Mutex::new(None),
            sign_response: Mutex::new(None),
        }
    }

    fn with_decrypt_response(mut self, plaintext: &str) -> Self {
        self.decrypt_response
            .get_mut()
            .expect("decrypt mutex poisoned")
            .replace(plaintext.to_string());
        self
    }
}

impl HardwareTransport for MockHardwareTransport {
    fn list_tokens(&self) -> Result<Vec<DiscoveredHardwareToken>, HardwareTransportError> {
        Ok(self.tokens.lock().expect("tokens mutex poisoned").clone())
    }

    fn verify_session(
        &self,
        _session: &HardwareSessionPolicy,
    ) -> Result<(), HardwareTransportError> {
        Ok(())
    }

    fn decrypt_ciphertext(
        &self,
        _session: &HardwareSessionPolicy,
        _ciphertext: &[u8],
    ) -> Result<String, HardwareTransportError> {
        self.decrypt_response
            .lock()
            .expect("decrypt mutex poisoned")
            .clone()
            .ok_or_else(|| {
                HardwareTransportError::Other("No mock decrypt response configured.".to_string())
            })
    }

    fn sign_cleartext(
        &self,
        _session: &HardwareSessionPolicy,
        _data: &str,
    ) -> Result<String, HardwareTransportError> {
        self.sign_response
            .lock()
            .expect("sign mutex poisoned")
            .clone()
            .ok_or_else(|| {
                HardwareTransportError::Other("No mock sign response configured.".to_string())
            })
    }
}

struct HardwareTransportGuard;

impl HardwareTransportGuard {
    fn install(transport: Arc<dyn HardwareTransport>) -> Self {
        set_hardware_transport_for_tests(transport);
        Self
    }
}

impl Drop for HardwareTransportGuard {
    fn drop(&mut self) {
        reset_hardware_transport_for_tests();
    }
}

#[cfg(any(feature = "fidostore", feature = "fidokey"))]
#[derive(Default)]
struct MockFido2Transport {
    enrollments: Mutex<Vec<Result<Fido2Enrollment, Fido2TransportError>>>,
    assertions: Mutex<Vec<Result<Fido2AssertionOutput, Fido2TransportError>>>,
}

#[cfg(any(feature = "fidostore", feature = "fidokey"))]
impl MockFido2Transport {
    fn with_enrollment_result(
        mut self,
        result: Result<Fido2Enrollment, Fido2TransportError>,
    ) -> Self {
        self.enrollments
            .get_mut()
            .expect("enrollment mutex poisoned")
            .push(result);
        self
    }

    fn with_assertion_results(
        mut self,
        results: Vec<Result<Fido2AssertionOutput, Fido2TransportError>>,
    ) -> Self {
        self.assertions
            .get_mut()
            .expect("assertion mutex poisoned")
            .extend(results);
        self
    }

    fn next_enrollment(&self) -> Result<Fido2Enrollment, Fido2TransportError> {
        self.enrollments
            .lock()
            .expect("enrollment mutex poisoned")
            .remove(0)
    }

    fn next_assertion(&self) -> Result<Fido2AssertionOutput, Fido2TransportError> {
        self.assertions
            .lock()
            .expect("assertion mutex poisoned")
            .remove(0)
    }
}

#[cfg(any(feature = "fidostore", feature = "fidokey"))]
impl Fido2Transport for MockFido2Transport {
    fn enroll_hmac_secret(
        &self,
        _rp_id: &str,
        _user_name: &str,
        _user_display_name: &str,
        _pin: Option<&str>,
        _salt: &[u8],
    ) -> Result<Fido2Enrollment, Fido2TransportError> {
        self.next_enrollment()
    }

    fn derive_hmac_secret(
        &self,
        _rp_id: &str,
        _credential_id: &[u8],
        _pin: Option<&str>,
        _salt: &[u8],
        _excluded_devices: &[Fido2DeviceLabel],
    ) -> Result<Fido2AssertionOutput, Fido2TransportError> {
        self.next_assertion()
    }
}

#[cfg(any(feature = "fidostore", feature = "fidokey"))]
struct SequentialOnlyFido2Transport {
    state: Mutex<SequentialOnlyFido2TransportState>,
    credentials: Vec<(Vec<u8>, Vec<u8>)>,
}

#[cfg(any(feature = "fidostore", feature = "fidokey"))]
#[derive(Clone, Debug, Default, PartialEq, Eq)]
struct SequentialOnlyFido2TransportState {
    active_calls: usize,
    poisoned: bool,
}

#[cfg(any(feature = "fidostore", feature = "fidokey"))]
impl SequentialOnlyFido2Transport {
    fn new(credentials: &[(&[u8], &[u8])]) -> Self {
        Self {
            state: Mutex::new(SequentialOnlyFido2TransportState::default()),
            credentials: credentials
                .iter()
                .map(|(credential_id, secret)| (credential_id.to_vec(), secret.to_vec()))
                .collect(),
        }
    }
}

#[cfg(any(feature = "fidostore", feature = "fidokey"))]
impl Fido2Transport for SequentialOnlyFido2Transport {
    fn enroll_hmac_secret(
        &self,
        _rp_id: &str,
        _user_name: &str,
        _user_display_name: &str,
        _pin: Option<&str>,
        _salt: &[u8],
    ) -> Result<Fido2Enrollment, Fido2TransportError> {
        Err(Fido2TransportError::Other(
            "SequentialOnlyFido2Transport does not support enrollment.".to_string(),
        ))
    }

    fn derive_hmac_secret(
        &self,
        _rp_id: &str,
        credential_id: &[u8],
        _pin: Option<&str>,
        _salt: &[u8],
        _excluded_devices: &[Fido2DeviceLabel],
    ) -> Result<Fido2AssertionOutput, Fido2TransportError> {
        let mut state = self
            .state
            .lock()
            .expect("sequential-only state mutex poisoned");
        if state.poisoned {
            return Err(Fido2TransportError::TokenRemoved);
        }
        state.active_calls += 1;
        if state.active_calls > 1 {
            state.poisoned = true;
            state.active_calls -= 1;
            return Err(Fido2TransportError::TokenRemoved);
        }
        drop(state);

        let result = self
            .credentials
            .iter()
            .find(|(known_credential_id, _)| credential_id == known_credential_id.as_slice())
            .map(|(_, secret)| mock_fido2_assertion(secret))
            .ok_or(Fido2TransportError::TokenNotPresent);

        let mut state = self
            .state
            .lock()
            .expect("sequential-only state mutex poisoned");
        state.active_calls -= 1;
        result
    }
}

#[cfg(any(feature = "fidostore", feature = "fidokey"))]
struct RecordingSequentialFido2Transport {
    state: Mutex<SequentialOnlyFido2TransportState>,
    credentials: Vec<(Vec<u8>, Vec<u8>)>,
    observed_credentials: Mutex<Vec<Vec<u8>>>,
}

#[cfg(any(feature = "fidostore", feature = "fidokey"))]
impl RecordingSequentialFido2Transport {
    fn new(credentials: &[(&[u8], &[u8])]) -> Self {
        Self {
            state: Mutex::new(SequentialOnlyFido2TransportState::default()),
            credentials: credentials
                .iter()
                .map(|(credential_id, secret)| (credential_id.to_vec(), secret.to_vec()))
                .collect(),
            observed_credentials: Mutex::new(Vec::new()),
        }
    }

    fn observed_credentials(&self) -> Vec<Vec<u8>> {
        self.observed_credentials
            .lock()
            .expect("recording sequential observed mutex poisoned")
            .clone()
    }
}

#[cfg(any(feature = "fidostore", feature = "fidokey"))]
impl Fido2Transport for RecordingSequentialFido2Transport {
    fn enroll_hmac_secret(
        &self,
        _rp_id: &str,
        _user_name: &str,
        _user_display_name: &str,
        _pin: Option<&str>,
        _salt: &[u8],
    ) -> Result<Fido2Enrollment, Fido2TransportError> {
        Err(Fido2TransportError::Other(
            "RecordingSequentialFido2Transport does not support enrollment.".to_string(),
        ))
    }

    fn derive_hmac_secret(
        &self,
        _rp_id: &str,
        credential_id: &[u8],
        _pin: Option<&str>,
        _salt: &[u8],
        _excluded_devices: &[Fido2DeviceLabel],
    ) -> Result<Fido2AssertionOutput, Fido2TransportError> {
        let mut state = self
            .state
            .lock()
            .expect("recording sequential state mutex poisoned");
        if state.poisoned {
            return Err(Fido2TransportError::TokenRemoved);
        }
        state.active_calls += 1;
        if state.active_calls > 1 {
            state.poisoned = true;
            state.active_calls -= 1;
            return Err(Fido2TransportError::TokenRemoved);
        }
        drop(state);

        self.observed_credentials
            .lock()
            .expect("recording sequential observed mutex poisoned")
            .push(credential_id.to_vec());

        let result = self
            .credentials
            .iter()
            .find(|(known_credential_id, _)| credential_id == known_credential_id.as_slice())
            .map(|(_, secret)| mock_fido2_assertion(secret))
            .ok_or(Fido2TransportError::TokenNotPresent);

        let mut state = self
            .state
            .lock()
            .expect("recording sequential state mutex poisoned");
        state.active_calls -= 1;
        result
    }
}

#[cfg(any(feature = "fidostore", feature = "fidokey"))]
struct MisleadingMultiDeviceFido2Transport {
    state: Mutex<SequentialOnlyFido2TransportState>,
    first_label: Fido2DeviceLabel,
    second_label: Fido2DeviceLabel,
    first_credential: Vec<u8>,
    first_secret: Vec<u8>,
    second_credential: Vec<u8>,
    wrong_second_secret: Vec<u8>,
    correct_second_secret: Vec<u8>,
    observed: Mutex<Vec<(Vec<u8>, String)>>,
}

#[cfg(any(feature = "fidostore", feature = "fidokey"))]
impl MisleadingMultiDeviceFido2Transport {
    fn new(
        first_credential: &[u8],
        first_secret: &[u8],
        second_credential: &[u8],
        wrong_second_secret: &[u8],
        correct_second_secret: &[u8],
    ) -> Self {
        Self {
            state: Mutex::new(SequentialOnlyFido2TransportState::default()),
            first_label: Fido2DeviceLabel {
                manufacturer: Some("Mock".to_string()),
                product: Some("First Device".to_string()),
                vendor_id: Some(1),
                product_id: Some(1),
            },
            second_label: Fido2DeviceLabel {
                manufacturer: Some("Mock".to_string()),
                product: Some("Second Device".to_string()),
                vendor_id: Some(1),
                product_id: Some(2),
            },
            first_credential: first_credential.to_vec(),
            first_secret: first_secret.to_vec(),
            second_credential: second_credential.to_vec(),
            wrong_second_secret: wrong_second_secret.to_vec(),
            correct_second_secret: correct_second_secret.to_vec(),
            observed: Mutex::new(Vec::new()),
        }
    }

    fn observed(&self) -> Vec<(Vec<u8>, String)> {
        self.observed
            .lock()
            .expect("misleading observed mutex poisoned")
            .clone()
    }
}

#[cfg(any(feature = "fidostore", feature = "fidokey"))]
impl Fido2Transport for MisleadingMultiDeviceFido2Transport {
    fn enroll_hmac_secret(
        &self,
        _rp_id: &str,
        _user_name: &str,
        _user_display_name: &str,
        _pin: Option<&str>,
        _salt: &[u8],
    ) -> Result<Fido2Enrollment, Fido2TransportError> {
        Err(Fido2TransportError::Other(
            "MisleadingMultiDeviceFido2Transport does not support enrollment.".to_string(),
        ))
    }

    fn derive_hmac_secret(
        &self,
        _rp_id: &str,
        credential_id: &[u8],
        _pin: Option<&str>,
        _salt: &[u8],
        excluded_devices: &[Fido2DeviceLabel],
    ) -> Result<Fido2AssertionOutput, Fido2TransportError> {
        let mut state = self
            .state
            .lock()
            .expect("misleading transport state mutex poisoned");
        if state.poisoned {
            return Err(Fido2TransportError::TokenRemoved);
        }
        state.active_calls += 1;
        if state.active_calls > 1 {
            state.poisoned = true;
            state.active_calls -= 1;
            return Err(Fido2TransportError::TokenRemoved);
        }
        drop(state);

        let result = if credential_id == self.first_credential.as_slice() {
            self.observed
                .lock()
                .expect("misleading observed mutex poisoned")
                .push((credential_id.to_vec(), "First Device".to_string()));
            Ok(Fido2AssertionOutput {
                hmac_secret: self.first_secret.clone(),
                device: Some(self.first_label.clone()),
            })
        } else if credential_id == self.second_credential.as_slice() {
            if excluded_devices
                .iter()
                .any(|label| label == &self.first_label)
            {
                self.observed
                    .lock()
                    .expect("misleading observed mutex poisoned")
                    .push((credential_id.to_vec(), "Second Device".to_string()));
                Ok(Fido2AssertionOutput {
                    hmac_secret: self.correct_second_secret.clone(),
                    device: Some(self.second_label.clone()),
                })
            } else {
                self.observed
                    .lock()
                    .expect("misleading observed mutex poisoned")
                    .push((credential_id.to_vec(), "First Device".to_string()));
                Ok(Fido2AssertionOutput {
                    hmac_secret: self.wrong_second_secret.clone(),
                    device: Some(self.first_label.clone()),
                })
            }
        } else {
            Err(Fido2TransportError::TokenNotPresent)
        };

        let mut state = self
            .state
            .lock()
            .expect("misleading transport state mutex poisoned");
        state.active_calls -= 1;
        result
    }
}

#[cfg(any(feature = "fidostore", feature = "fidokey"))]
struct DelayedSecondKeyFido2Transport {
    state: Mutex<SequentialOnlyFido2TransportState>,
    first_credential: Vec<u8>,
    first_secret: Vec<u8>,
    second_credential: Vec<u8>,
    second_secret: Vec<u8>,
    second_attempts_before_present: usize,
    second_attempts: Mutex<usize>,
    observed_credentials: Mutex<Vec<Vec<u8>>>,
}

#[cfg(any(feature = "fidostore", feature = "fidokey"))]
impl DelayedSecondKeyFido2Transport {
    fn new(
        first_credential: &[u8],
        first_secret: &[u8],
        second_credential: &[u8],
        second_secret: &[u8],
        second_attempts_before_present: usize,
    ) -> Self {
        Self {
            state: Mutex::new(SequentialOnlyFido2TransportState::default()),
            first_credential: first_credential.to_vec(),
            first_secret: first_secret.to_vec(),
            second_credential: second_credential.to_vec(),
            second_secret: second_secret.to_vec(),
            second_attempts_before_present,
            second_attempts: Mutex::new(0),
            observed_credentials: Mutex::new(Vec::new()),
        }
    }

    fn observed_credentials(&self) -> Vec<Vec<u8>> {
        self.observed_credentials
            .lock()
            .expect("delayed second key observed mutex poisoned")
            .clone()
    }
}

#[cfg(any(feature = "fidostore", feature = "fidokey"))]
impl Fido2Transport for DelayedSecondKeyFido2Transport {
    fn enroll_hmac_secret(
        &self,
        _rp_id: &str,
        _user_name: &str,
        _user_display_name: &str,
        _pin: Option<&str>,
        _salt: &[u8],
    ) -> Result<Fido2Enrollment, Fido2TransportError> {
        Err(Fido2TransportError::Other(
            "DelayedSecondKeyFido2Transport does not support enrollment.".to_string(),
        ))
    }

    fn derive_hmac_secret(
        &self,
        _rp_id: &str,
        credential_id: &[u8],
        _pin: Option<&str>,
        _salt: &[u8],
        _excluded_devices: &[Fido2DeviceLabel],
    ) -> Result<Fido2AssertionOutput, Fido2TransportError> {
        let mut state = self
            .state
            .lock()
            .expect("delayed second key state mutex poisoned");
        if state.poisoned {
            return Err(Fido2TransportError::TokenRemoved);
        }
        state.active_calls += 1;
        if state.active_calls > 1 {
            state.poisoned = true;
            state.active_calls -= 1;
            return Err(Fido2TransportError::TokenRemoved);
        }
        drop(state);

        self.observed_credentials
            .lock()
            .expect("delayed second key observed mutex poisoned")
            .push(credential_id.to_vec());

        let result = if credential_id == self.first_credential.as_slice() {
            Ok(mock_fido2_assertion(&self.first_secret))
        } else if credential_id == self.second_credential.as_slice() {
            let mut attempts = self
                .second_attempts
                .lock()
                .expect("delayed second key attempts mutex poisoned");
            *attempts += 1;
            if *attempts <= self.second_attempts_before_present {
                Err(Fido2TransportError::TokenNotPresent)
            } else {
                Ok(mock_fido2_assertion(&self.second_secret))
            }
        } else {
            Err(Fido2TransportError::TokenNotPresent)
        };

        let mut state = self
            .state
            .lock()
            .expect("delayed second key state mutex poisoned");
        state.active_calls -= 1;
        result
    }
}

#[cfg(any(feature = "fidostore", feature = "fidokey"))]
struct Fido2TransportGuard;

#[cfg(any(feature = "fidostore", feature = "fidokey"))]
impl Fido2TransportGuard {
    fn install(transport: Arc<dyn Fido2Transport>) -> Self {
        set_fido2_transport_for_tests(transport);
        Self
    }
}

#[cfg(any(feature = "fidostore", feature = "fidokey"))]
impl Drop for Fido2TransportGuard {
    fn drop(&mut self) {
        reset_fido2_transport_for_tests();
    }
}

#[cfg(any(feature = "fidostore", feature = "fidokey"))]
fn mock_fido2_enrollment(secret: &[u8]) -> Fido2Enrollment {
    Fido2Enrollment {
        credential_id: b"mock-credential-id".to_vec(),
        device: Fido2DeviceLabel {
            manufacturer: Some("Mock".to_string()),
            product: Some("Security Key".to_string()),
            vendor_id: Some(1),
            product_id: Some(2),
        },
        hmac_secret: secret.to_vec(),
    }
}

#[cfg(any(feature = "fidostore", feature = "fidokey"))]
fn mock_fido2_enrollment_with_credential(credential_id: &[u8], secret: &[u8]) -> Fido2Enrollment {
    Fido2Enrollment {
        credential_id: credential_id.to_vec(),
        device: Fido2DeviceLabel {
            manufacturer: Some("Mock".to_string()),
            product: Some("Security Key".to_string()),
            vendor_id: Some(1),
            product_id: Some(2),
        },
        hmac_secret: secret.to_vec(),
    }
}

#[cfg(any(feature = "fidostore", feature = "fidokey"))]
fn mock_fido2_assertion(secret: &[u8]) -> Fido2AssertionOutput {
    Fido2AssertionOutput {
        hmac_secret: secret.to_vec(),
        device: Some(Fido2DeviceLabel {
            manufacturer: Some("Mock".to_string()),
            product: Some("Security Key".to_string()),
            vendor_id: Some(1),
            product_id: Some(2),
        }),
    }
}

#[test]
fn ripasso_private_key_parser_reads_secret_keys() {
    let bytes = cert_bytes("Alice Example <alice@example.com>");

    let (_, key) = parse_managed_private_key_bytes(&bytes)
        .expect("expected secret key to parse as a managed private key");

    assert_eq!(key.fingerprint.len(), 40);
    assert!(key
        .user_ids
        .iter()
        .any(|user_id| user_id.contains("alice@example.com")));
}

#[test]
fn ripasso_private_key_parser_rejects_public_only_keys() {
    let (cert, _) = CertBuilder::general_purpose(Some("Bob Example <bob@example.com>"))
        .generate()
        .expect("failed to generate test certificate");
    let public_only = cert.strip_secret_key_material();
    let mut bytes = Vec::new();
    public_only
        .serialize(&mut bytes)
        .expect("failed to serialize public test certificate");

    let err = parse_managed_private_key_bytes(&bytes)
        .expect_err("public-only keys should not be accepted as managed private keys");
    assert!(matches!(err, PrivateKeyError::MissingPrivateKeyMaterial(_)));
}

#[test]
fn encrypted_private_keys_report_that_a_passphrase_is_required() {
    let password: Password = "hunter2".into();
    let (cert, _) = CertBuilder::general_purpose(Some("Carol Example <carol@example.com>"))
        .set_password(Some(password))
        .generate()
        .expect("failed to generate password-protected certificate");
    let mut bytes = Vec::new();
    cert.as_tsk()
        .serialize(&mut bytes)
        .expect("failed to serialize protected test certificate");

    assert!(ripasso_private_key_requires_passphrase(&bytes)
        .expect("expected password inspection to work"));
}

#[test]
fn protected_private_keys_can_be_unlocked_for_ripasso_storage() {
    let password: Password = "hunter2".into();
    let (cert, _) = CertBuilder::general_purpose(Some("Dana Example <dana@example.com>"))
        .set_password(Some(password))
        .generate()
        .expect("failed to generate password-protected certificate");
    let mut bytes = Vec::new();
    cert.as_tsk()
        .serialize(&mut bytes)
        .expect("failed to serialize protected test certificate");

    let (unlocked, key) = prepare_managed_private_key_bytes(&bytes, Some("hunter2"))
        .expect("expected protected key to unlock successfully");

    assert_eq!(key.fingerprint.len(), 40);
    assert!(unlocked
        .keys()
        .all(|key| key.key().has_unencrypted_secret()));
}

#[test]
fn hardware_public_keys_can_be_stored_and_unlocked_for_a_session() {
    let env = SystemBackendTestEnv::new();
    env.activate_profile("hardware-key-store");
    let public_bytes = public_cert_bytes("Hardware User <hardware@example.com>");
    let _guard =
        HardwareTransportGuard::install(Arc::new(MockHardwareTransport::with_tokens(vec![
            DiscoveredHardwareToken {
                ident: "mock-token".to_string(),
                reader_hint: Some("Mock Reader".to_string()),
                cardholder_certificate: Some(public_bytes.clone()),
                signing_fingerprint: None,
                decryption_fingerprint: None,
            },
        ])));

    let discovered = discover_ripasso_hardware_keys().expect("discover hardware keys");
    assert_eq!(discovered.len(), 1);

    let imported = store_ripasso_hardware_key_bytes(
        &public_bytes,
        ManagedRipassoHardwareKey {
            ident: "mock-token".to_string(),
            signing_fingerprint: None,
            decryption_fingerprint: None,
            reader_hint: Some("Mock Reader".to_string()),
        },
    )
    .expect("store hardware key");

    assert_eq!(
        imported.protection,
        ManagedRipassoPrivateKeyProtection::HardwareOpenPgpCard
    );
    assert!(ripasso_private_key_requires_session_unlock(&imported.fingerprint).unwrap());

    unlock_ripasso_private_key_for_session(
        &imported.fingerprint,
        PrivateKeyUnlockRequest::HardwareExternal,
    )
    .expect("unlock hardware key");
    assert!(is_ripasso_private_key_unlocked(&imported.fingerprint).unwrap());
}

#[test]
fn hardware_keys_can_decrypt_password_entries_after_unlock() {
    let env = SystemBackendTestEnv::new();
    env.activate_profile("hardware-key-decrypt");
    let public_bytes = public_cert_bytes("Hardware Read <hardware-read@example.com>");
    let _guard = HardwareTransportGuard::install(Arc::new(
        MockHardwareTransport::with_tokens(vec![DiscoveredHardwareToken {
            ident: "mock-token".to_string(),
            reader_hint: Some("Mock Reader".to_string()),
            cardholder_certificate: None,
            signing_fingerprint: None,
            decryption_fingerprint: None,
        }])
        .with_decrypt_response("supersecret\nusername: alice"),
    ));

    let imported = import_ripasso_hardware_key_bytes(
        &public_bytes,
        ManagedRipassoHardwareKey {
            ident: "mock-token".to_string(),
            signing_fingerprint: None,
            decryption_fingerprint: None,
            reader_hint: Some("Mock Reader".to_string()),
        },
    )
    .expect("import hardware public key");

    let store = env.root_dir().join("hardware-store");
    fs::create_dir_all(&store).expect("create hardware store");
    fs::write(store.join(".gpg-id"), format!("{}\n", imported.fingerprint))
        .expect("write recipients");

    save_password_entry(
        store.to_string_lossy().as_ref(),
        "team/service",
        "supersecret\nusername: alice",
        true,
    )
    .expect("save entry for hardware key");

    unlock_ripasso_private_key_for_session(
        &imported.fingerprint,
        PrivateKeyUnlockRequest::HardwareExternal,
    )
    .expect("unlock hardware key");

    assert_eq!(
        read_password_entry(store.to_string_lossy().as_ref(), "team/service")
            .expect("read hardware-backed entry"),
        "supersecret\nusername: alice"
    );
}

#[test]
fn generated_private_keys_are_stored_and_listed() {
    let env = SystemBackendTestEnv::new();
    env.activate_profile("generated-key");

    let key = generate_ripasso_private_key("Generated User", "generated@example.com", "hunter2")
        .expect("generate private key");

    assert!(is_ripasso_private_key_unlocked(&key.fingerprint).expect("inspect unlocked state"));
    assert!(key
        .user_ids
        .iter()
        .any(|user_id| user_id.contains("Generated User <generated@example.com>")));
    assert!(list_ripasso_private_keys()
        .expect("list generated keys")
        .into_iter()
        .any(|stored| stored.fingerprint == key.fingerprint));
}

#[test]
fn armored_private_keys_can_be_exported() {
    let env = SystemBackendTestEnv::new();
    env.activate_profile("exported-key");

    let key = generate_ripasso_private_key("Export User", "export@example.com", "hunter2")
        .expect("generate private key");
    let armored = armored_ripasso_private_key(&key.fingerprint).expect("export armored key");
    let parsed = sequoia_openpgp::Cert::from_bytes(armored.as_bytes()).expect("parse armored key");

    assert!(armored.starts_with("-----BEGIN PGP PRIVATE KEY BLOCK-----"));
    assert_eq!(parsed.fingerprint().to_hex(), key.fingerprint);
}

#[test]
fn armored_private_keys_can_be_reimported_from_text_bytes() {
    let env = SystemBackendTestEnv::new();
    env.activate_profile("clipboard-import");

    let key = generate_ripasso_private_key("Clipboard User", "clipboard@example.com", "hunter2")
        .expect("generate private key");
    let armored = armored_ripasso_private_key(&key.fingerprint).expect("export armored key");

    remove_ripasso_private_key(&key.fingerprint).expect("remove generated key");
    let imported = import_ripasso_private_key_bytes(armored.as_bytes(), Some("hunter2"))
        .expect("re-import armored private key");

    assert_eq!(imported.fingerprint, key.fingerprint);
}

#[test]
fn imported_private_keys_stay_encrypted_on_disk() {
    let _env = SystemBackendTestEnv::new();
    let password: Password = "hunter2".into();
    let (cert, _) = CertBuilder::general_purpose(Some("Eve Example <eve@example.com>"))
        .set_password(Some(password))
        .generate()
        .expect("failed to generate password-protected certificate");
    let mut bytes = Vec::new();
    cert.as_tsk()
        .serialize(&mut bytes)
        .expect("failed to serialize protected test certificate");

    let imported = import_ripasso_private_key_bytes(&bytes, Some("hunter2"))
        .expect("expected private key import to succeed");
    let stored_path = ripasso_keys_dir()
        .expect("expected keys dir")
        .join(imported.fingerprint.to_ascii_lowercase());
    let stored_bytes = fs::read(stored_path).expect("read stored key");
    let (stored_cert, _) =
        parse_managed_private_key_bytes(&stored_bytes).expect("parse stored key");

    assert!(ripasso_private_key_requires_passphrase(&stored_bytes).unwrap());
    assert!(stored_cert
        .keys()
        .any(|key| !key.key().has_unencrypted_secret()));
    assert!(is_ripasso_private_key_unlocked(&imported.fingerprint).unwrap());
}

#[test]
fn encrypted_private_keys_unlock_for_the_current_session_only() {
    let _env = SystemBackendTestEnv::new();
    let password: Password = "hunter2".into();
    let (cert, _) = CertBuilder::general_purpose(Some("Frank Example <frank@example.com>"))
        .set_password(Some(password))
        .generate()
        .expect("failed to generate password-protected certificate");
    let mut bytes = Vec::new();
    cert.as_tsk()
        .serialize(&mut bytes)
        .expect("failed to serialize protected test certificate");

    let imported = import_ripasso_private_key_bytes(&bytes, Some("hunter2"))
        .expect("expected private key import to succeed");
    assert!(ensure_ripasso_private_key_is_ready(&imported.fingerprint).is_ok());

    clear_cached_unlocked_ripasso_private_keys();
    assert!(!is_ripasso_private_key_unlocked(&imported.fingerprint).unwrap());
    assert!(matches!(
        ensure_ripasso_private_key_is_ready(&imported.fingerprint)
            .expect_err("locked key should not be ready"),
        PasswordEntryError::LockedPrivateKey(_)
    ));

    unlock_ripasso_private_key_for_session(
        &imported.fingerprint,
        PrivateKeyUnlockRequest::Password("hunter2".into()),
    )
    .expect("unlock private key for session");
    assert!(is_ripasso_private_key_unlocked(&imported.fingerprint).unwrap());
    assert!(ensure_ripasso_private_key_is_ready(&imported.fingerprint).is_ok());
}

#[cfg(feature = "fidokey")]
#[test]
fn fido2_private_key_unlocks_via_the_fidokey_feature() {
    let _env = SystemBackendTestEnv::new();
    let _guard = Fido2TransportGuard::install(Arc::new(
        MockFido2Transport::default()
            .with_enrollment_result(Ok(mock_fido2_enrollment(b"fidokey-secret")))
            .with_assertion_results(vec![
                Err(Fido2TransportError::PinRequired),
                Ok(mock_fido2_assertion(b"fidokey-secret")),
            ]),
    ));
    let generated = generate_fido2_private_key(None).expect("generate FIDO2-protected key");
    clear_cached_unlocked_ripasso_private_keys();

    let err = unlock_ripasso_private_key_for_session(
        &generated.fingerprint,
        PrivateKeyUnlockRequest::Fido2(None),
    )
    .expect_err("missing PIN should be reported");
    assert!(matches!(err, PrivateKeyError::Fido2PinRequired(_)));

    let unlocked = unlock_ripasso_private_key_for_session(
        &generated.fingerprint,
        PrivateKeyUnlockRequest::Fido2(Some("123456".into())),
    )
    .expect("unlock FIDO2-backed private key");

    assert_eq!(
        unlocked.protection,
        ManagedRipassoPrivateKeyProtection::Fido2HmacSecret
    );
    assert_eq!(unlocked.fingerprint, generated.fingerprint);
}

#[cfg(feature = "fidokey")]
#[test]
fn exported_fido2_private_keys_import_as_managed_keys() {
    let _env = SystemBackendTestEnv::new();
    let _guard = Fido2TransportGuard::install(Arc::new(
        MockFido2Transport::default()
            .with_enrollment_result(Ok(mock_fido2_enrollment(b"travel-key-secret"))),
    ));
    let generated =
        generate_fido2_private_key(Some("123456")).expect("generate FIDO2-protected key");
    let exported =
        armored_ripasso_private_key(&generated.fingerprint).expect("export FIDO2-protected key");
    remove_ripasso_private_key(&generated.fingerprint).expect("remove generated FIDO key");

    let imported = import_ripasso_private_key_bytes(exported.as_bytes(), None)
        .expect("import FIDO2-protected key");

    assert_eq!(
        imported.protection,
        ManagedRipassoPrivateKeyProtection::Fido2HmacSecret
    );
    assert_eq!(imported.fingerprint, generated.fingerprint);
    assert!(!ripasso_private_key_requires_passphrase(exported.as_bytes()).unwrap());
    assert!(list_ripasso_private_keys()
        .expect("list private keys")
        .into_iter()
        .any(|key| key.fingerprint == imported.fingerprint));
}

#[cfg(feature = "fidokey")]
#[test]
fn generated_fido2_private_keys_are_listed_and_start_unlocked_when_a_pin_is_cached() {
    let _env = SystemBackendTestEnv::new();
    let _guard = Fido2TransportGuard::install(Arc::new(
        MockFido2Transport::default()
            .with_enrollment_result(Ok(mock_fido2_enrollment(b"generated-fidokey-secret"))),
    ));

    let generated =
        generate_fido2_private_key(Some("123456")).expect("generate FIDO2-protected key");

    assert_eq!(
        generated.protection,
        ManagedRipassoPrivateKeyProtection::Fido2HmacSecret
    );
    assert!(!is_fido2_recipient_string(&generated.fingerprint));
    assert!(is_ripasso_private_key_unlocked(&generated.fingerprint).unwrap());
    assert!(list_ripasso_private_keys()
        .expect("list private keys")
        .into_iter()
        .any(|key| key.fingerprint == generated.fingerprint));
}

#[cfg(feature = "fidokey")]
#[test]
fn generated_fido2_private_keys_can_be_combined_with_password_keys() {
    let env = SystemBackendTestEnv::new();
    let _guard = Fido2TransportGuard::install(Arc::new(
        MockFido2Transport::default()
            .with_enrollment_result(Ok(mock_fido2_enrollment(b"mixed-fidokey-secret"))),
    ));
    let password_key = generate_ripasso_private_key("Alice", "alice@example.com", "hunter2")
        .expect("generate password-protected key");
    let fido_key =
        generate_fido2_private_key(Some("123456")).expect("generate FIDO2-protected key");
    let store = env.root_dir().join("mixed-managed-store");

    save_store_recipients(
        store.to_string_lossy().as_ref(),
        &[
            password_key.fingerprint.clone(),
            fido_key.fingerprint.clone(),
        ],
        StoreRecipientsPrivateKeyRequirement::AnyManagedKey,
    )
    .expect("save mixed recipients");

    let gpg_id = fs::read_to_string(store.join(".gpg-id")).expect("read .gpg-id");
    assert!(gpg_id.contains(&password_key.fingerprint));
    assert!(gpg_id.contains(&fido_key.fingerprint));
    assert!(!store.join(FIDO2_RECIPIENTS_FILE_NAME).exists());
}

#[cfg(feature = "fidokey")]
#[test]
fn removing_fido2_private_keys_removes_the_stored_key() {
    let _env = SystemBackendTestEnv::new();
    let _guard = Fido2TransportGuard::install(Arc::new(
        MockFido2Transport::default()
            .with_enrollment_result(Ok(mock_fido2_enrollment(b"backup-key-secret"))),
    ));
    let imported =
        generate_fido2_private_key(Some("123456")).expect("generate FIDO2-protected key");

    remove_ripasso_private_key(&imported.fingerprint).expect("remove FIDO2 private key");

    assert!(!list_ripasso_private_keys()
        .expect("list private keys")
        .into_iter()
        .any(|key| key.fingerprint == imported.fingerprint));
}

#[cfg(any(feature = "fidostore", feature = "fidokey"))]
#[test]
fn pure_fido2_recipients_can_retry_after_pin_required() {
    let _env = SystemBackendTestEnv::new();
    let _guard = Fido2TransportGuard::install(Arc::new(
        MockFido2Transport::default()
            .with_enrollment_result(Ok(mock_fido2_enrollment(b"pure-fido2-secret")))
            .with_assertion_results(vec![
                Err(Fido2TransportError::PinRequired),
                Ok(mock_fido2_assertion(b"pure-fido2-secret")),
            ]),
    ));

    let recipient = create_fido2_store_recipient(None).expect("create FIDO2 recipient");

    let err = unlock_fido2_store_recipient_for_session(&recipient, None)
        .expect_err("missing PIN should be reported");
    assert!(matches!(err, PrivateKeyError::Fido2PinRequired(_)));

    unlock_fido2_store_recipient_for_session(&recipient, Some("123456"))
        .expect("unlock FIDO2 recipient with PIN");
}

#[cfg(any(feature = "fidostore", feature = "fidokey"))]
#[test]
fn pure_fido2_store_reads_require_all_fido2_recipients_in_order() {
    let env = SystemBackendTestEnv::new();
    let first_credential = b"parallel-credential-1";
    let second_credential = b"parallel-credential-2";
    let first_recipient = build_fido2_recipient_string(
        "0123456789abcdef0123456789abcdef01234567",
        "First Key",
        first_credential,
    )
    .expect("build first FIDO2 recipient");
    let second_recipient = build_fido2_recipient_string(
        "89abcdef0123456789abcdef0123456789abcdef",
        "Second Key",
        second_credential,
    )
    .expect("build second FIDO2 recipient");

    let _setup_guard =
        Fido2TransportGuard::install(Arc::new(SequentialOnlyFido2Transport::new(&[
            (
                first_credential.as_slice(),
                b"first-parallel-secret".as_slice(),
            ),
            (
                second_credential.as_slice(),
                b"second-parallel-secret".as_slice(),
            ),
        ])));

    let store = env.root_dir().join("fido2-parallel-store");
    save_store_recipients(
        store.to_string_lossy().as_ref(),
        &[first_recipient.clone(), second_recipient.clone()],
        StoreRecipientsPrivateKeyRequirement::AnyManagedKey,
    )
    .expect("save FIDO2 recipients");
    save_password_entry(
        store.to_string_lossy().as_ref(),
        "team/service",
        "supersecret\nusername: alice",
        true,
    )
    .expect("save password entry");

    drop(_setup_guard);
    let read_transport = Arc::new(RecordingSequentialFido2Transport::new(&[
        (
            first_credential.as_slice(),
            b"first-parallel-secret".as_slice(),
        ),
        (
            second_credential.as_slice(),
            b"second-parallel-secret".as_slice(),
        ),
    ]));
    let _read_guard = Fido2TransportGuard::install(read_transport.clone());

    assert_eq!(
        read_password_entry(store.to_string_lossy().as_ref(), "team/service")
            .expect("read entry that requires both FIDO2 recipients"),
        "supersecret\nusername: alice"
    );
    assert_eq!(
        read_transport.observed_credentials(),
        vec![first_credential.to_vec(), second_credential.to_vec()]
    );
}

#[cfg(any(feature = "fidostore", feature = "fidokey"))]
#[test]
fn pure_fido2_store_reads_can_fall_through_to_the_second_device_for_the_next_key() {
    let env = SystemBackendTestEnv::new();
    let first_credential = b"fallthrough-credential-1";
    let second_credential = b"fallthrough-credential-2";
    let first_recipient = build_fido2_recipient_string(
        "0123456789abcdef0123456789abcdef01234567",
        "First Key",
        first_credential,
    )
    .expect("build first FIDO2 recipient");
    let second_recipient = build_fido2_recipient_string(
        "89abcdef0123456789abcdef0123456789abcdef",
        "Second Key",
        second_credential,
    )
    .expect("build second FIDO2 recipient");

    let _save_guard = Fido2TransportGuard::install(Arc::new(SequentialOnlyFido2Transport::new(&[
        (
            first_credential.as_slice(),
            b"fallthrough-secret-1".as_slice(),
        ),
        (
            second_credential.as_slice(),
            b"fallthrough-secret-2".as_slice(),
        ),
    ])));

    let store = env.root_dir().join("fido2-fallthrough-store");
    save_store_recipients(
        store.to_string_lossy().as_ref(),
        &[first_recipient, second_recipient],
        StoreRecipientsPrivateKeyRequirement::AnyManagedKey,
    )
    .expect("save FIDO2 recipients");
    save_password_entry(
        store.to_string_lossy().as_ref(),
        "team/service",
        "supersecret\nusername: alice",
        true,
    )
    .expect("save password entry");

    drop(_save_guard);
    let read_transport = Arc::new(MisleadingMultiDeviceFido2Transport::new(
        first_credential,
        b"fallthrough-secret-1",
        second_credential,
        b"wrong-secret-from-first-device",
        b"fallthrough-secret-2",
    ));
    let _read_guard = Fido2TransportGuard::install(read_transport.clone());

    assert_eq!(
        read_password_entry(store.to_string_lossy().as_ref(), "team/service")
            .expect("read entry after excluding the wrong device"),
        "supersecret\nusername: alice"
    );
    assert_eq!(
        read_transport.observed(),
        vec![
            (first_credential.to_vec(), "First Device".to_string()),
            (second_credential.to_vec(), "First Device".to_string()),
            (second_credential.to_vec(), "Second Device".to_string()),
        ]
    );
}

#[cfg(any(feature = "fidostore", feature = "fidokey"))]
#[test]
fn pure_fido2_store_reads_wait_briefly_for_the_next_security_key() {
    let env = SystemBackendTestEnv::new();
    let first_credential = b"delayed-credential-1";
    let second_credential = b"delayed-credential-2";
    let first_recipient = build_fido2_recipient_string(
        "0123456789abcdef0123456789abcdef01234567",
        "First Key",
        first_credential,
    )
    .expect("build first FIDO2 recipient");
    let second_recipient = build_fido2_recipient_string(
        "89abcdef0123456789abcdef0123456789abcdef",
        "Second Key",
        second_credential,
    )
    .expect("build second FIDO2 recipient");

    let _save_guard = Fido2TransportGuard::install(Arc::new(SequentialOnlyFido2Transport::new(&[
        (first_credential.as_slice(), b"delayed-secret-1".as_slice()),
        (second_credential.as_slice(), b"delayed-secret-2".as_slice()),
    ])));

    let store = env.root_dir().join("fido2-delayed-store");
    save_store_recipients(
        store.to_string_lossy().as_ref(),
        &[first_recipient, second_recipient],
        StoreRecipientsPrivateKeyRequirement::AnyManagedKey,
    )
    .expect("save FIDO2 recipients");
    save_password_entry(
        store.to_string_lossy().as_ref(),
        "team/service",
        "supersecret\nusername: alice",
        true,
    )
    .expect("save password entry");

    drop(_save_guard);
    let read_transport = Arc::new(DelayedSecondKeyFido2Transport::new(
        first_credential,
        b"delayed-secret-1",
        second_credential,
        b"delayed-secret-2",
        2,
    ));
    let _read_guard = Fido2TransportGuard::install(read_transport.clone());

    assert_eq!(
        read_password_entry(store.to_string_lossy().as_ref(), "team/service")
            .expect("read entry after retrying for the second key"),
        "supersecret\nusername: alice"
    );
    assert_eq!(
        read_transport.observed_credentials(),
        vec![
            first_credential.to_vec(),
            second_credential.to_vec(),
            second_credential.to_vec(),
            second_credential.to_vec(),
        ]
    );
}

#[cfg(any(feature = "fidostore", feature = "fidokey"))]
#[test]
fn pure_fido2_store_reads_current_any_managed_entries_with_all_fido2_recipients() {
    let env = SystemBackendTestEnv::new();
    let first_credential = b"current-credential-1";
    let second_credential = b"current-credential-2";
    let first_recipient = build_fido2_recipient_string(
        "0123456789abcdef0123456789abcdef01234567",
        "First Key",
        first_credential,
    )
    .expect("build first FIDO2 recipient");
    let second_recipient = build_fido2_recipient_string(
        "89abcdef0123456789abcdef0123456789abcdef",
        "Second Key",
        second_credential,
    )
    .expect("build second FIDO2 recipient");

    let store = env.root_dir().join("fido2-current-store");
    save_store_recipients(
        store.to_string_lossy().as_ref(),
        &[first_recipient.clone(), second_recipient.clone()],
        StoreRecipientsPrivateKeyRequirement::AnyManagedKey,
    )
    .expect("save FIDO2 recipients");

    let first_binding = direct_binding_from_store_recipient(&first_recipient)
        .expect("parse first binding")
        .expect("first binding");
    let second_binding = direct_binding_from_store_recipient(&second_recipient)
        .expect("parse second binding")
        .expect("second binding");

    let _write_guard =
        Fido2TransportGuard::install(Arc::new(SequentialOnlyFido2Transport::new(&[
            (first_credential.as_slice(), b"current-secret-1".as_slice()),
            (second_credential.as_slice(), b"current-secret-2".as_slice()),
        ])));
    let ciphertext = encrypt_fido2_any_managed_bundle_with_progress(
        &[first_binding, second_binding],
        &[7u8; 32],
        b"supersecret\nusername: alice",
        None,
        None,
    )
    .expect("build current any-managed FIDO2 entry");
    let entry_path = store.join("team/service.gpg");
    fs::create_dir_all(entry_path.parent().expect("entry parent")).expect("create entry parent");
    fs::write(&entry_path, ciphertext).expect("write legacy entry");
    drop(_write_guard);

    let read_transport = Arc::new(RecordingSequentialFido2Transport::new(&[
        (first_credential.as_slice(), b"current-secret-1".as_slice()),
        (second_credential.as_slice(), b"current-secret-2".as_slice()),
    ]));
    let _read_guard = Fido2TransportGuard::install(read_transport.clone());

    assert_eq!(
        read_password_entry(store.to_string_lossy().as_ref(), "team/service")
            .expect("read current any-managed entry with all FIDO2 recipients"),
        "supersecret\nusername: alice"
    );
    assert_eq!(
        read_transport.observed_credentials(),
        vec![first_credential.to_vec(), second_credential.to_vec()]
    );
}

#[cfg(any(feature = "fidostore", feature = "fidokey"))]
#[test]
fn pure_fido2_store_can_add_a_second_recipient_without_parallel_reencrypt_access() {
    let env = SystemBackendTestEnv::new();
    let first_credential = b"save-credential-1";
    let second_credential = b"save-credential-2";
    let first_recipient = build_fido2_recipient_string(
        "0123456789abcdef0123456789abcdef01234567",
        "First Key",
        first_credential,
    )
    .expect("build first FIDO2 recipient");
    let second_recipient = build_fido2_recipient_string(
        "89abcdef0123456789abcdef0123456789abcdef",
        "Second Key",
        second_credential,
    )
    .expect("build second FIDO2 recipient");

    let _guard = Fido2TransportGuard::install(Arc::new(SequentialOnlyFido2Transport::new(&[
        (first_credential.as_slice(), b"first-save-secret".as_slice()),
        (
            second_credential.as_slice(),
            b"second-save-secret".as_slice(),
        ),
    ])));

    let store = env.root_dir().join("fido2-save-store");
    save_store_recipients(
        store.to_string_lossy().as_ref(),
        std::slice::from_ref(&first_recipient),
        StoreRecipientsPrivateKeyRequirement::AnyManagedKey,
    )
    .expect("save first FIDO2 recipient");
    save_password_entry(
        store.to_string_lossy().as_ref(),
        "team/service",
        "supersecret\nusername: alice",
        true,
    )
    .expect("save password entry");

    save_store_recipients(
        store.to_string_lossy().as_ref(),
        &[first_recipient.clone(), second_recipient.clone()],
        StoreRecipientsPrivateKeyRequirement::AnyManagedKey,
    )
    .expect("save both FIDO2 recipients");

    let recipients = fs::read_to_string(store.join(".gpg-id")).expect("read recipients file");
    assert_eq!(recipients, "\n");

    let fido2_recipients =
        fs::read_to_string(store.join(FIDO2_RECIPIENTS_FILE_NAME)).expect("read FIDO2 sidecar");
    let fido2_lines = fido2_recipients.lines().collect::<HashSet<_>>();
    assert_eq!(fido2_lines.len(), 2);
    assert!(fido2_lines.contains(first_recipient.as_str()));
    assert!(fido2_lines.contains(second_recipient.as_str()));
}

#[cfg(any(feature = "fidostore", feature = "fidokey"))]
#[test]
fn pure_fido2_store_keeps_a_newly_enrolled_second_recipient_after_save() {
    let env = SystemBackendTestEnv::new();
    let first_credential = b"persist-credential-1";
    let second_credential = b"persist-credential-2";
    let first_recipient = build_fido2_recipient_string(
        "0123456789abcdef0123456789abcdef01234567",
        "First Key",
        first_credential,
    )
    .expect("build first FIDO2 recipient");

    let _setup_guard =
        Fido2TransportGuard::install(Arc::new(SequentialOnlyFido2Transport::new(&[(
            first_credential.as_slice(),
            b"first-persist-secret".as_slice(),
        )])));

    let store = env.root_dir().join("fido2-persist-store");
    save_store_recipients(
        store.to_string_lossy().as_ref(),
        std::slice::from_ref(&first_recipient),
        StoreRecipientsPrivateKeyRequirement::AnyManagedKey,
    )
    .expect("save first FIDO2 recipient");
    save_password_entry(
        store.to_string_lossy().as_ref(),
        "team/service",
        "supersecret\nusername: alice",
        true,
    )
    .expect("save password entry");

    drop(_setup_guard);
    let _enrollment_guard = Fido2TransportGuard::install(Arc::new(
        MockFido2Transport::default().with_enrollment_result(Ok(
            mock_fido2_enrollment_with_credential(second_credential, b"second-persist-secret"),
        )),
    ));
    let second_recipient = create_fido2_store_recipient(None).expect("create second FIDO2 key");

    drop(_enrollment_guard);
    let _save_guard =
        Fido2TransportGuard::install(Arc::new(SequentialOnlyFido2Transport::new(&[(
            first_credential.as_slice(),
            b"first-persist-secret".as_slice(),
        )])));

    save_store_recipients(
        store.to_string_lossy().as_ref(),
        &[first_recipient.clone(), second_recipient.clone()],
        StoreRecipientsPrivateKeyRequirement::AnyManagedKey,
    )
    .expect("save both FIDO2 recipients with the original key only");

    let fido2_recipients =
        fs::read_to_string(store.join(FIDO2_RECIPIENTS_FILE_NAME)).expect("read FIDO2 sidecar");
    let fido2_lines = fido2_recipients.lines().collect::<HashSet<_>>();
    assert_eq!(fido2_lines.len(), 2);
    assert!(fido2_lines.contains(first_recipient.as_str()));
    assert!(fido2_lines.contains(second_recipient.as_str()));

    drop(_save_guard);
    let read_transport = Arc::new(RecordingSequentialFido2Transport::new(&[
        (
            first_credential.as_slice(),
            b"first-persist-secret".as_slice(),
        ),
        (
            second_credential.as_slice(),
            b"second-persist-secret".as_slice(),
        ),
    ]));
    let _read_guard = Fido2TransportGuard::install(read_transport.clone());

    assert_eq!(
        read_password_entry(store.to_string_lossy().as_ref(), "team/service")
            .expect("read entry with both saved FIDO2 recipients"),
        "supersecret\nusername: alice"
    );
    assert_eq!(
        read_transport.observed_credentials(),
        vec![first_credential.to_vec(), second_credential.to_vec()]
    );
}

#[cfg(any(feature = "fidostore", feature = "fidokey"))]
#[test]
fn single_fido2_recipient_entries_use_the_security_key_directly() {
    let env = SystemBackendTestEnv::new();
    let _guard = Fido2TransportGuard::install(Arc::new(
        MockFido2Transport::default()
            .with_enrollment_result(Ok(mock_fido2_enrollment(b"direct-entry-secret")))
            .with_assertion_results(vec![
                Ok(mock_fido2_assertion(b"direct-entry-secret")),
                Ok(mock_fido2_assertion(b"direct-entry-secret")),
            ]),
    ));

    let recipient = create_fido2_store_recipient(None).expect("create FIDO2 recipient");

    let store = env.root_dir().join("fido2-direct-store");
    save_store_recipients(
        store.to_string_lossy().as_ref(),
        std::slice::from_ref(&recipient),
        StoreRecipientsPrivateKeyRequirement::AnyManagedKey,
    )
    .expect("save FIDO2-only recipients");
    assert_eq!(
        fs::read_to_string(store.join(".gpg-id")).expect("read recipients file"),
        "\n"
    );
    assert_eq!(
        fs::read_to_string(store.join(FIDO2_RECIPIENTS_FILE_NAME)).expect("read FIDO2 sidecar"),
        format!("{recipient}\n")
    );

    save_password_entry(
        store.to_string_lossy().as_ref(),
        "team/service",
        "supersecret\nusername: alice",
        true,
    )
    .expect("save FIDO2 direct entry");

    assert!(!store.join("team/service.gpg").exists());
    let ciphertext = fs::read(store.join("team/service.keycord")).expect("read direct entry bytes");
    assert!(ciphertext.starts_with(b"keycord-fido2-any-managed-v1\n"));
    assert_eq!(
        read_password_entry(store.to_string_lossy().as_ref(), "team/service")
            .expect("read entry directly from the FIDO2 security key"),
        "supersecret\nusername: alice"
    );
}

#[cfg(any(feature = "fidostore", feature = "fidokey"))]
#[test]
fn password_entry_fido2_usage_detection_matches_selected_recipients() {
    let env = SystemBackendTestEnv::new();
    let bytes = protected_cert_bytes("Key A <a@example.com>");
    let key = import_ripasso_private_key_bytes(&bytes, Some("hunter2"))
        .expect("import standard private key");
    let _guard = Fido2TransportGuard::install(Arc::new(
        MockFido2Transport::default()
            .with_enrollment_result(Ok(mock_fido2_enrollment(b"mixed-store-secret"))),
    ));
    let fido2_recipient = create_fido2_store_recipient(None).expect("create FIDO2 recipient");

    let store = env.root_dir().join("mixed-store");
    save_store_recipients(
        store.to_string_lossy().as_ref(),
        &[key.fingerprint.clone(), fido2_recipient],
        StoreRecipientsPrivateKeyRequirement::AnyManagedKey,
    )
    .expect("save mixed recipients");

    assert_eq!(
        password_entry_fido2_recipient_count(store.to_string_lossy().as_ref(), "team/service"),
        1
    );
    assert!(
        password_entry_fido2_recipient_count(store.to_string_lossy().as_ref(), "team/service") > 0
    );
}

#[cfg(any(feature = "fidostore", feature = "fidokey"))]
#[test]
fn password_entry_fido2_recipient_count_matches_multiple_selected_security_keys() {
    let env = SystemBackendTestEnv::new();
    let first_recipient = build_fido2_recipient_string(
        "0123456789abcdef0123456789abcdef01234567",
        "First Key",
        b"count-credential-1",
    )
    .expect("build first FIDO2 recipient");
    let second_recipient = build_fido2_recipient_string(
        "89abcdef0123456789abcdef0123456789abcdef",
        "Second Key",
        b"count-credential-2",
    )
    .expect("build second FIDO2 recipient");

    let store = env.root_dir().join("fido2-count-store");
    save_store_recipients(
        store.to_string_lossy().as_ref(),
        &[first_recipient, second_recipient],
        StoreRecipientsPrivateKeyRequirement::AnyManagedKey,
    )
    .expect("save two FIDO2 recipients");

    assert_eq!(
        password_entry_fido2_recipient_count(store.to_string_lossy().as_ref(), "team/service"),
        2
    );
    assert!(
        password_entry_fido2_recipient_count(store.to_string_lossy().as_ref(), "team/service") > 0
    );
}

#[cfg(any(feature = "fidostore", feature = "fidokey"))]
#[test]
fn multi_fido2_password_saves_report_step_progress() {
    let env = SystemBackendTestEnv::new();
    let first_credential = b"progress-credential-1";
    let second_credential = b"progress-credential-2";
    let first_recipient = build_fido2_recipient_string(
        "0123456789abcdef0123456789abcdef01234567",
        "First Key",
        first_credential,
    )
    .expect("build first FIDO2 recipient");
    let second_recipient = build_fido2_recipient_string(
        "89abcdef0123456789abcdef0123456789abcdef",
        "Second Key",
        second_credential,
    )
    .expect("build second FIDO2 recipient");

    let _guard = Fido2TransportGuard::install(Arc::new(SequentialOnlyFido2Transport::new(&[
        (first_credential.as_slice(), b"progress-secret-1".as_slice()),
        (
            second_credential.as_slice(),
            b"progress-secret-2".as_slice(),
        ),
    ])));

    let store = env.root_dir().join("fido2-progress-store");
    save_store_recipients(
        store.to_string_lossy().as_ref(),
        &[first_recipient, second_recipient],
        StoreRecipientsPrivateKeyRequirement::AnyManagedKey,
    )
    .expect("save FIDO2 recipients");

    let mut progress = Vec::new();
    save_password_entry_with_progress(
        store.to_string_lossy().as_ref(),
        "team/service",
        "supersecret\nusername: alice",
        true,
        &mut |step| progress.push(step),
    )
    .expect("save password entry with progress");

    assert_eq!(
        progress,
        vec![
            PasswordEntryWriteProgress {
                current_step: 1,
                total_steps: 2,
            },
            PasswordEntryWriteProgress {
                current_step: 2,
                total_steps: 2,
            },
        ]
    );
}

#[cfg(any(feature = "fidostore", feature = "fidokey"))]
#[test]
fn multi_fido2_password_reads_report_step_progress() {
    let env = SystemBackendTestEnv::new();
    let first_credential = b"read-progress-credential-1";
    let second_credential = b"read-progress-credential-2";
    let first_recipient = build_fido2_recipient_string(
        "0123456789abcdef0123456789abcdef01234567",
        "First Key",
        first_credential,
    )
    .expect("build first FIDO2 recipient");
    let second_recipient = build_fido2_recipient_string(
        "89abcdef0123456789abcdef0123456789abcdef",
        "Second Key",
        second_credential,
    )
    .expect("build second FIDO2 recipient");

    let _save_guard = Fido2TransportGuard::install(Arc::new(SequentialOnlyFido2Transport::new(&[
        (
            first_credential.as_slice(),
            b"read-progress-secret-1".as_slice(),
        ),
        (
            second_credential.as_slice(),
            b"read-progress-secret-2".as_slice(),
        ),
    ])));

    let store = env.root_dir().join("fido2-read-progress-store");
    save_store_recipients(
        store.to_string_lossy().as_ref(),
        &[first_recipient, second_recipient],
        StoreRecipientsPrivateKeyRequirement::AnyManagedKey,
    )
    .expect("save FIDO2 recipients");

    save_password_entry(
        store.to_string_lossy().as_ref(),
        "team/service",
        "supersecret\nusername: alice",
        true,
    )
    .expect("save password entry");

    let read_transport = Arc::new(RecordingSequentialFido2Transport::new(&[
        (
            first_credential.as_slice(),
            b"read-progress-secret-1".as_slice(),
        ),
        (
            second_credential.as_slice(),
            b"read-progress-secret-2".as_slice(),
        ),
    ]));
    let _read_guard = Fido2TransportGuard::install(read_transport.clone());

    let mut progress = Vec::new();
    let contents = read_password_entry_with_progress(
        store.to_string_lossy().as_ref(),
        "team/service",
        &mut |step| progress.push(step),
    )
    .expect("read password entry with progress");

    assert_eq!(contents, "supersecret\nusername: alice");
    assert_eq!(
        progress,
        vec![
            PasswordEntryReadProgress {
                current_step: 1,
                total_steps: 2,
            },
            PasswordEntryReadProgress {
                current_step: 2,
                total_steps: 2,
            },
        ]
    );
    assert_eq!(
        read_transport.observed_credentials(),
        vec![first_credential.to_vec(), second_credential.to_vec()]
    );
}

#[cfg(any(feature = "fidostore", feature = "fidokey"))]
#[test]
fn store_recipients_fido2_rewrites_report_progress() {
    let env = SystemBackendTestEnv::new();
    let first_credential = b"rewrite-progress-credential-1";
    let second_credential = b"rewrite-progress-credential-2";
    let first_recipient = build_fido2_recipient_string(
        "0123456789abcdef0123456789abcdef01234567",
        "First Key",
        first_credential,
    )
    .expect("build first FIDO2 recipient");
    let second_recipient = build_fido2_recipient_string(
        "89abcdef0123456789abcdef0123456789abcdef",
        "Second Key",
        second_credential,
    )
    .expect("build second FIDO2 recipient");

    let _save_guard = Fido2TransportGuard::install(Arc::new(SequentialOnlyFido2Transport::new(&[
        (
            first_credential.as_slice(),
            b"rewrite-progress-secret-1".as_slice(),
        ),
        (
            second_credential.as_slice(),
            b"rewrite-progress-secret-2".as_slice(),
        ),
    ])));

    let store = env.root_dir().join("fido2-store-rewrite-progress");
    save_store_recipients(
        store.to_string_lossy().as_ref(),
        &[first_recipient.clone(), second_recipient.clone()],
        StoreRecipientsPrivateKeyRequirement::AnyManagedKey,
    )
    .expect("save initial FIDO2 recipients");
    save_password_entry(
        store.to_string_lossy().as_ref(),
        "team/service",
        "supersecret\nusername: alice",
        true,
    )
    .expect("save password entry");

    let read_transport = Arc::new(RecordingSequentialFido2Transport::new(&[
        (
            first_credential.as_slice(),
            b"rewrite-progress-secret-1".as_slice(),
        ),
        (
            second_credential.as_slice(),
            b"rewrite-progress-secret-2".as_slice(),
        ),
    ]));
    let _rewrite_guard = Fido2TransportGuard::install(read_transport);

    let mut progress = Vec::new();
    save_store_recipients_with_progress(
        store.to_string_lossy().as_ref(),
        std::slice::from_ref(&first_recipient),
        StoreRecipientsPrivateKeyRequirement::AnyManagedKey,
        &mut |step| progress.push(step),
    )
    .expect("rewrite store recipients");

    assert!(progress.iter().any(|step| {
        matches!(step.stage, StoreRecipientsSaveStage::ReadingExistingItems)
            && step.total_items == 1
    }));
    assert!(progress.iter().any(|step| {
        matches!(step.stage, StoreRecipientsSaveStage::WritingUpdatedItems) && step.total_items == 1
    }));
    assert!(progress.iter().any(|step| {
        matches!(step.stage, StoreRecipientsSaveStage::ReadingExistingItems)
            && step.current_touch > 0
            && step.total_touches > 0
    }));
    assert!(progress.iter().any(|step| {
        matches!(step.stage, StoreRecipientsSaveStage::WritingUpdatedItems)
            && step.current_touch > 0
            && step.total_touches > 0
    }));
    assert_eq!(
        fs::read_to_string(store.join(FIDO2_RECIPIENTS_FILE_NAME)).expect("read saved recipients"),
        format!("{first_recipient}\n")
    );
}

#[cfg(any(feature = "fidostore", feature = "fidokey"))]
#[test]
fn all_keys_mode_can_layer_a_fido2_security_key() {
    let env = SystemBackendTestEnv::new();
    let bytes_a = protected_cert_bytes("Key A <a@example.com>");
    let key_a = import_ripasso_private_key_bytes(&bytes_a, Some("hunter2"))
        .expect("import first private key");
    let _guard = Fido2TransportGuard::install(Arc::new(
        MockFido2Transport::default()
            .with_enrollment_result(Ok(mock_fido2_enrollment(b"layered-fido2-secret")))
            .with_assertion_results(vec![
                Ok(mock_fido2_assertion(b"layered-fido2-secret")),
                Ok(mock_fido2_assertion(b"layered-fido2-secret")),
            ]),
    ));
    let fido2_recipient = create_fido2_store_recipient(None).expect("create FIDO2 recipient");

    let store = env.root_dir().join("secondary-store");
    save_store_recipients(
        store.to_string_lossy().as_ref(),
        &[key_a.fingerprint.clone(), fido2_recipient.clone()],
        StoreRecipientsPrivateKeyRequirement::AllManagedKeys,
    )
    .expect("save all-keys recipients");
    save_password_entry(
        store.to_string_lossy().as_ref(),
        "team/service",
        "supersecret\nusername: alice",
        true,
    )
    .expect("save layered entry");

    assert!(!store.join("team/service.gpg").exists());
    let outer_layer = IntegratedCryptoContext::load_for_fingerprint(&key_a.fingerprint)
        .expect("load first-layer decrypt context")
        .decrypt_entry_with_progress(&store.join("team/service.keycord"), None)
        .expect("decrypt only the first layer");
    let (_, encoded_inner) = outer_layer
        .split_once('\n')
        .expect("split the layered all-keys wrapper");
    let encoded_inner = encoded_inner.trim();
    let inner_layer = (0..encoded_inner.len())
        .step_by(2)
        .map(|index| u8::from_str_radix(&encoded_inner[index..index + 2], 16))
        .collect::<Result<Vec<_>, _>>()
        .expect("decode the inner layer");
    assert!(inner_layer.starts_with(b"keycord-fido2-required-layer-v1\n"));

    clear_cached_unlocked_ripasso_private_keys();
    unlock_ripasso_private_key_for_session(
        &key_a.fingerprint,
        PrivateKeyUnlockRequest::Password("hunter2".into()),
    )
    .expect("unlock password-protected key for the layered read");
    assert_eq!(
        read_password_entry(store.to_string_lossy().as_ref(), "team/service")
            .expect("read entry that requires both a password key and a FIDO2 key"),
        "supersecret\nusername: alice"
    );
}

#[cfg(any(feature = "fidostore", feature = "fidokey"))]
#[test]
fn save_store_recipients_rotates_any_managed_dek_when_fido2_recipients_change() {
    let env = SystemBackendTestEnv::new();
    let bytes_a = protected_cert_bytes("Key A <a@example.com>");
    let key_a = import_ripasso_private_key_bytes(&bytes_a, Some("hunter2"))
        .expect("import first private key");
    let first_credential = b"mock-credential-id-1";
    let second_credential = b"mock-credential-id-2";

    let _first_guard = Fido2TransportGuard::install(Arc::new(
        MockFido2Transport::default()
            .with_enrollment_result(Ok(mock_fido2_enrollment_with_credential(
                first_credential,
                b"first-fido-secret",
            )))
            .with_enrollment_result(Ok(mock_fido2_enrollment_with_credential(
                second_credential,
                b"second-fido-secret",
            )))
            .with_assertion_results(vec![Ok(mock_fido2_assertion(b"first-fido-secret"))]),
    ));
    let first_fido = create_fido2_store_recipient(None).expect("create first FIDO2 recipient");
    let second_fido = create_fido2_store_recipient(None).expect("create second FIDO2 recipient");
    let first_binding = direct_binding_from_store_recipient(&first_fido)
        .expect("parse first binding")
        .expect("first binding");
    let second_binding = direct_binding_from_store_recipient(&second_fido)
        .expect("parse second binding")
        .expect("second binding");

    let store = env.root_dir().join("secondary-store");
    save_store_recipients(
        store.to_string_lossy().as_ref(),
        &[key_a.fingerprint.clone(), first_fido.clone()],
        StoreRecipientsPrivateKeyRequirement::AnyManagedKey,
    )
    .expect("save first FIDO2 recipient");
    save_password_entry(
        store.to_string_lossy().as_ref(),
        "team/service",
        "supersecret\nusername: alice",
        true,
    )
    .expect("save entry protected by the first FIDO2 recipient");
    let entry_path = store.join("team/service.keycord");
    let old_ciphertext = fs::read(&entry_path).expect("read original entry");

    drop(_first_guard);
    let _old_dek_guard = Fido2TransportGuard::install(Arc::new(
        MockFido2Transport::default()
            .with_assertion_results(vec![Ok(mock_fido2_assertion(b"first-fido-secret"))]),
    ));
    let old_dek = decrypt_fido2_any_managed_bundle_dek_for_fingerprint(
        &first_binding.fingerprint,
        &old_ciphertext,
    )
    .expect("decrypt the original DEK");
    drop(_old_dek_guard);

    let _second_guard = Fido2TransportGuard::install(Arc::new(
        MockFido2Transport::default()
            .with_assertion_results(vec![Ok(mock_fido2_assertion(b"second-fido-secret"))]),
    ));

    save_store_recipients(
        store.to_string_lossy().as_ref(),
        &[key_a.fingerprint.clone(), second_fido.clone()],
        StoreRecipientsPrivateKeyRequirement::AnyManagedKey,
    )
    .expect("replace the FIDO2 recipient");

    let new_ciphertext = fs::read(&entry_path).expect("read rewritten entry");
    assert!(
        decrypt_payload_from_any_managed_bundle(&new_ciphertext, &old_dek).is_err(),
        "rewritten entries must not keep using the old DEK after recipients change"
    );
    assert_eq!(
        decrypt_fido2_any_managed_bundle_for_fingerprint(
            &second_binding.fingerprint,
            &new_ciphertext
        )
        .expect("decrypt rewritten entry with the new recipient"),
        b"supersecret\nusername: alice"
    );

    let recipients = fs::read_to_string(store.join(".gpg-id")).expect("read recipients");
    assert!(!recipients.contains("keycord-fido2-recipient-v1="));

    let fido2_recipients =
        fs::read_to_string(store.join(FIDO2_RECIPIENTS_FILE_NAME)).expect("read FIDO2 recipients");
    let fido2_lines = fido2_recipients
        .lines()
        .filter(|line| line.contains("keycord-fido2-recipient-v1="))
        .collect::<HashSet<_>>();
    assert_eq!(fido2_lines.len(), 1);
    assert!(fido2_lines
        .iter()
        .any(|line| line.contains(second_fido.as_str())));
}

#[cfg(any(feature = "fidostore", feature = "fidokey"))]
#[test]
fn save_password_entry_preserves_existing_fido2_recipients_without_reconnecting_them() {
    let env = SystemBackendTestEnv::new();
    let bytes_a = protected_cert_bytes("Key A <a@example.com>");
    let key_a = import_ripasso_private_key_bytes(&bytes_a, Some("hunter2"))
        .expect("import first private key");

    let _first_guard = Fido2TransportGuard::install(Arc::new(
        MockFido2Transport::default()
            .with_enrollment_result(Ok(mock_fido2_enrollment_with_credential(
                b"mock-credential-id-1",
                b"first-fido-secret",
            )))
            .with_assertion_results(vec![Ok(mock_fido2_assertion(b"first-fido-secret"))]),
    ));
    let first_fido = create_fido2_store_recipient(None).expect("create first FIDO2 recipient");

    let store = env.root_dir().join("secondary-store");
    save_store_recipients(
        store.to_string_lossy().as_ref(),
        &[key_a.fingerprint.clone(), first_fido],
        StoreRecipientsPrivateKeyRequirement::AnyManagedKey,
    )
    .expect("save first FIDO2 recipient");
    save_password_entry(
        store.to_string_lossy().as_ref(),
        "team/service",
        "supersecret\nusername: alice",
        true,
    )
    .expect("save entry protected by the first FIDO2 recipient");

    drop(_first_guard);
    let _second_guard = Fido2TransportGuard::install(Arc::new(MockFido2Transport::default()));

    save_password_entry(
        store.to_string_lossy().as_ref(),
        "team/service",
        "newsecret\nusername: alice",
        true,
    )
    .expect("save entry without reconnecting the existing FIDO2 recipient");
    assert_eq!(
        read_password_entry(store.to_string_lossy().as_ref(), "team/service")
            .expect("read updated entry"),
        "newsecret\nusername: alice"
    );
}

#[test]
fn unprotected_private_keys_are_rejected_for_secure_import() {
    let _env = SystemBackendTestEnv::new();
    let bytes = cert_bytes("Grace Example <grace@example.com>");

    let err = import_ripasso_private_key_bytes(&bytes, None)
        .expect_err("unprotected private keys should be rejected");

    assert!(matches!(
        err,
        PrivateKeyError::RequiresPasswordProtection(_)
    ));
}

#[test]
fn dotted_entry_labels_keep_their_full_name() {
    assert_eq!(
        secret_entry_relative_path("chat/matrix.org").unwrap(),
        PathBuf::from("chat/matrix.org.gpg")
    );
}

#[test]
fn recipients_file_lookup_stays_inside_the_selected_store() {
    let env = SystemBackendTestEnv::new();
    let primary_store = env.root_dir().join("primary-store");
    let secondary_store = env.root_dir().join("secondary-store");

    fs::create_dir_all(primary_store.join("team")).expect("create primary store");
    fs::create_dir_all(secondary_store.join("team")).expect("create secondary store");
    fs::write(primary_store.join(".gpg-id"), "primary@example.com\n")
        .expect("write primary recipients");
    fs::write(secondary_store.join(".gpg-id"), "secondary@example.com\n")
        .expect("write secondary recipients");

    assert_eq!(
        recipients_file_for_label(secondary_store.to_string_lossy().as_ref(), "team/chat")
            .expect("resolve recipients file"),
        secondary_store.join(".gpg-id")
    );
}

#[test]
fn new_entries_can_be_saved_in_a_secondary_store() {
    let env = SystemBackendTestEnv::new();
    let password: Password = "hunter2".into();
    let (cert, _) = CertBuilder::general_purpose(Some("Store Example <store@example.com>"))
        .set_password(Some(password))
        .generate()
        .expect("failed to generate password-protected certificate");
    let mut bytes = Vec::new();
    cert.as_tsk()
        .serialize(&mut bytes)
        .expect("failed to serialize protected test certificate");
    let imported = import_ripasso_private_key_bytes(&bytes, Some("hunter2"))
        .expect("expected private key import to succeed");

    let primary_store = env.root_dir().join("primary-store");
    let secondary_store = env.root_dir().join("secondary-store");
    fs::create_dir_all(&primary_store).expect("create primary store");
    fs::create_dir_all(&secondary_store).expect("create secondary store");
    fs::write(
        primary_store.join(".gpg-id"),
        format!("{}\n", imported.fingerprint),
    )
    .expect("write primary recipients");
    fs::write(
        secondary_store.join(".gpg-id"),
        format!("{}\n", imported.fingerprint),
    )
    .expect("write secondary recipients");

    save_password_entry(
        secondary_store.to_string_lossy().as_ref(),
        "team/service",
        "supersecret\nusername: alice",
        true,
    )
    .expect("save entry in secondary store");

    assert!(secondary_store.join("team/service.gpg").is_file());
    assert_eq!(
        read_password_entry(secondary_store.to_string_lossy().as_ref(), "team/service")
            .expect("read saved entry"),
        "supersecret\nusername: alice".to_string()
    );
}

#[test]
fn duplicate_entry_saves_are_classified_as_already_existing() {
    let env = SystemBackendTestEnv::new();
    let bytes = protected_cert_bytes("Store Example <store@example.com>");
    let imported = import_ripasso_private_key_bytes(&bytes, Some("hunter2"))
        .expect("expected private key import to succeed");

    let store = env.root_dir().join("secondary-store");
    fs::create_dir_all(&store).expect("create secondary store");
    fs::write(store.join(".gpg-id"), format!("{}\n", imported.fingerprint))
        .expect("write recipients");

    save_password_entry(
        store.to_string_lossy().as_ref(),
        "team/service",
        "supersecret\nusername: alice",
        true,
    )
    .expect("save initial entry");

    let err = save_password_entry(
        store.to_string_lossy().as_ref(),
        "team/service",
        "supersecret\nusername: alice",
        false,
    )
    .expect_err("duplicate save should be rejected");

    assert!(matches!(
        err,
        PasswordEntryWriteError::EntryAlreadyExists(_)
    ));
}

#[test]
fn entries_are_encrypted_for_all_selected_private_keys() {
    let env = SystemBackendTestEnv::new();
    let bytes_a = protected_cert_bytes("Key A <a@example.com>");
    let bytes_b = protected_cert_bytes("Key B <b@example.com>");
    let key_a = import_ripasso_private_key_bytes(&bytes_a, Some("hunter2"))
        .expect("import first private key");
    let key_b = import_ripasso_private_key_bytes(&bytes_b, Some("hunter2"))
        .expect("import second private key");

    let store = env.root_dir().join("secondary-store");
    fs::create_dir_all(&store).expect("create secondary store");
    fs::write(
        store.join(".gpg-id"),
        format!("{}\n{}\n", key_a.fingerprint, key_b.fingerprint),
    )
    .expect("write recipients");

    save_password_entry(
        store.to_string_lossy().as_ref(),
        "team/service",
        "supersecret\nusername: alice",
        true,
    )
    .expect("save entry for multiple recipients");

    remove_ripasso_private_key(&key_b.fingerprint).expect("remove second key");
    assert_eq!(
        read_password_entry(store.to_string_lossy().as_ref(), "team/service")
            .expect("read entry with first key only"),
        "supersecret\nusername: alice".to_string()
    );

    import_ripasso_private_key_bytes(&bytes_b, Some("hunter2")).expect("re-import second key");
    remove_ripasso_private_key(&key_a.fingerprint).expect("remove first key");
    assert_eq!(
        read_password_entry(store.to_string_lossy().as_ref(), "team/service")
            .expect("read entry with second key only"),
        "supersecret\nusername: alice".to_string()
    );
}

#[test]
fn all_keys_mode_requires_every_selected_private_key() {
    let env = SystemBackendTestEnv::new();
    let bytes_a = protected_cert_bytes("Key A <a@example.com>");
    let bytes_b = protected_cert_bytes("Key B <b@example.com>");
    let key_a = import_ripasso_private_key_bytes(&bytes_a, Some("hunter2"))
        .expect("import first private key");
    let key_b = import_ripasso_private_key_bytes(&bytes_b, Some("hunter2"))
        .expect("import second private key");

    let store = env.root_dir().join("secondary-store");
    save_store_recipients(
        store.to_string_lossy().as_ref(),
        &[key_a.fingerprint.clone(), key_b.fingerprint.clone()],
        StoreRecipientsPrivateKeyRequirement::AllManagedKeys,
    )
    .expect("save all-keys recipients");

    save_password_entry(
        store.to_string_lossy().as_ref(),
        "team/service",
        "supersecret\nusername: alice",
        true,
    )
    .expect("save all-keys entry");

    assert_eq!(
        fs::read_to_string(store.join(".gpg-id")).expect("read recipients"),
        format!(
            "# keycord-private-key-requirement=all\n{}\n{}\n",
            key_a.fingerprint, key_b.fingerprint
        )
    );
    assert_eq!(
        read_password_entry(store.to_string_lossy().as_ref(), "team/service")
            .expect("read all-keys entry"),
        "supersecret\nusername: alice".to_string()
    );

    remove_ripasso_private_key(&key_b.fingerprint).expect("remove second key");
    assert!(matches!(
        read_password_entry(store.to_string_lossy().as_ref(), "team/service")
            .expect_err("missing one required key should fail"),
        PasswordEntryError::MissingPrivateKey(_)
    ));
    assert!(!password_entry_is_readable(
        store.to_string_lossy().as_ref(),
        "team/service"
    ));
}

#[test]
fn all_keys_mode_uses_a_nonstandard_layered_entry_format() {
    let env = SystemBackendTestEnv::new();
    let bytes_a = protected_cert_bytes("Key A <a@example.com>");
    let bytes_b = protected_cert_bytes("Key B <b@example.com>");
    let key_a = import_ripasso_private_key_bytes(&bytes_a, Some("hunter2"))
        .expect("import first private key");
    let key_b = import_ripasso_private_key_bytes(&bytes_b, Some("hunter2"))
        .expect("import second private key");

    let store = env.root_dir().join("secondary-store");
    save_store_recipients(
        store.to_string_lossy().as_ref(),
        &[key_a.fingerprint.clone(), key_b.fingerprint],
        StoreRecipientsPrivateKeyRequirement::AllManagedKeys,
    )
    .expect("save all-keys recipients");
    save_password_entry(
        store.to_string_lossy().as_ref(),
        "team/service",
        "supersecret\nusername: alice",
        true,
    )
    .expect("save all-keys entry");

    let outer_layer = IntegratedCryptoContext::load_for_fingerprint(&key_a.fingerprint)
        .expect("load first-layer decrypt context")
        .decrypt_entry_with_progress(&store.join("team/service.gpg"), None)
        .expect("decrypt only the first layer");

    assert!(outer_layer.starts_with("keycord-require-all-private-keys-v1\n"));
    assert_ne!(outer_layer, "supersecret\nusername: alice");
}

#[test]
fn readability_check_requires_at_least_one_ready_key_in_any_mode() {
    let env = SystemBackendTestEnv::new();
    let bytes_a = protected_cert_bytes("Key A <a@example.com>");
    let bytes_b = protected_cert_bytes("Key B <b@example.com>");
    let key_a = import_ripasso_private_key_bytes(&bytes_a, Some("hunter2"))
        .expect("import first private key");
    let key_b = import_ripasso_private_key_bytes(&bytes_b, Some("hunter2"))
        .expect("import second private key");

    let store = env.root_dir().join("secondary-store");
    fs::create_dir_all(&store).expect("create secondary store");
    fs::write(
        store.join(".gpg-id"),
        format!("{}\n{}\n", key_a.fingerprint, key_b.fingerprint),
    )
    .expect("write recipients");
    save_password_entry(
        store.to_string_lossy().as_ref(),
        "team/service",
        "supersecret\nusername: alice",
        true,
    )
    .expect("save entry");

    assert!(password_entry_is_readable(
        store.to_string_lossy().as_ref(),
        "team/service"
    ));

    remove_ripasso_private_key(&key_a.fingerprint).expect("remove first key");
    assert!(password_entry_is_readable(
        store.to_string_lossy().as_ref(),
        "team/service"
    ));

    remove_ripasso_private_key(&key_b.fingerprint).expect("remove second key");
    assert!(!password_entry_is_readable(
        store.to_string_lossy().as_ref(),
        "team/service"
    ));
}

#[test]
fn readability_check_treats_locked_keys_as_openable() {
    let env = SystemBackendTestEnv::new();
    let bytes = protected_cert_bytes("Key A <a@example.com>");
    let key =
        import_ripasso_private_key_bytes(&bytes, Some("hunter2")).expect("import private key");

    let store = env.root_dir().join("secondary-store");
    fs::create_dir_all(&store).expect("create secondary store");
    fs::write(store.join(".gpg-id"), format!("{}\n", key.fingerprint)).expect("write recipients");
    save_password_entry(
        store.to_string_lossy().as_ref(),
        "team/service",
        "supersecret\nusername: alice",
        true,
    )
    .expect("save entry");

    clear_cached_unlocked_ripasso_private_keys();

    assert!(password_entry_is_readable(
        store.to_string_lossy().as_ref(),
        "team/service"
    ));
    assert!(matches!(
        read_password_entry(store.to_string_lossy().as_ref(), "team/service")
            .expect_err("locked key should still block the actual read"),
        PasswordEntryError::LockedPrivateKey(_)
    ));
}

#[test]
fn missing_entry_renames_and_deletes_are_classified() {
    let env = SystemBackendTestEnv::new();
    let store = env.root_dir().join("secondary-store");
    fs::create_dir_all(&store).expect("create secondary store");

    let rename_err = rename_password_entry(
        store.to_string_lossy().as_ref(),
        "team/missing",
        "team/renamed",
    )
    .expect_err("missing rename should fail");
    assert!(matches!(
        rename_err,
        PasswordEntryWriteError::EntryNotFound(_)
    ));

    let delete_err = delete_password_entry(store.to_string_lossy().as_ref(), "team/missing")
        .expect_err("missing delete should fail");
    assert!(matches!(
        delete_err,
        PasswordEntryWriteError::EntryNotFound(_)
    ));
}

#[test]
fn recipient_saves_reject_non_directory_store_paths() {
    let env = SystemBackendTestEnv::new();
    let file_path = env.root_dir().join("store-file");
    fs::write(&file_path, "not a directory").expect("write store placeholder file");

    let err = save_store_recipients(
        file_path.to_string_lossy().as_ref(),
        &[String::from("alice@example.com")],
        StoreRecipientsPrivateKeyRequirement::AnyManagedKey,
    )
    .expect_err("non-directory store paths should fail");

    assert!(matches!(err, StoreRecipientsError::InvalidStorePath(_)));
}

#[test]
fn recipient_saves_initialize_git_for_new_stores() {
    let env = SystemBackendTestEnv::new();
    let bytes = protected_cert_bytes("Store Example <store@example.com>");
    let imported = import_ripasso_private_key_bytes(&bytes, Some("hunter2"))
        .expect("expected private key import to succeed");

    let store = env.root_dir().join("secondary-store");
    save_store_recipients(
        store.to_string_lossy().as_ref(),
        std::slice::from_ref(&imported.fingerprint),
        StoreRecipientsPrivateKeyRequirement::AnyManagedKey,
    )
    .expect("save recipients for a new store");

    assert!(has_git_repository(store.to_string_lossy().as_ref()));
}

#[test]
fn new_entries_can_use_email_recipients() {
    let env = SystemBackendTestEnv::new();
    let password: Password = "hunter2".into();
    let (cert, _) = CertBuilder::general_purpose(Some("Store Example <store@example.com>"))
        .set_password(Some(password))
        .generate()
        .expect("failed to generate password-protected certificate");
    let mut bytes = Vec::new();
    cert.as_tsk()
        .serialize(&mut bytes)
        .expect("failed to serialize protected test certificate");
    let imported = import_ripasso_private_key_bytes(&bytes, Some("hunter2"))
        .expect("expected private key import to succeed");

    let secondary_store = env.root_dir().join("secondary-store");
    fs::create_dir_all(&secondary_store).expect("create secondary store");
    fs::write(secondary_store.join(".gpg-id"), "store@example.com\n").expect("write recipients");

    save_password_entry(
        secondary_store.to_string_lossy().as_ref(),
        "team/service",
        "supersecret\nusername: alice",
        true,
    )
    .expect("save entry with email recipient");

    assert!(secondary_store.join("team/service.gpg").is_file());
    assert_eq!(
        read_password_entry(secondary_store.to_string_lossy().as_ref(), "team/service")
            .expect("read saved entry"),
        "supersecret\nusername: alice".to_string()
    );
    assert_eq!(imported.fingerprint.len(), 40);
}

#[test]
fn store_recipient_updates_leave_nested_gpg_id_entries_on_their_own_recipients() {
    let env = SystemBackendTestEnv::new();
    let root_key = import_ripasso_private_key_bytes(
        &protected_cert_bytes("Root Key <root@example.com>"),
        Some("hunter2"),
    )
    .expect("import root key");
    let nested_key = import_ripasso_private_key_bytes(
        &protected_cert_bytes("Nested Key <nested@example.com>"),
        Some("hunter2"),
    )
    .expect("import nested key");
    let replacement_root_key = import_ripasso_private_key_bytes(
        &protected_cert_bytes("Replacement Root <replacement@example.com>"),
        Some("hunter2"),
    )
    .expect("import replacement root key");

    let store = env.root_dir().join("secondary-store");
    fs::create_dir_all(store.join("team")).expect("create nested store dir");
    fs::write(store.join(".gpg-id"), format!("{}\n", root_key.fingerprint))
        .expect("write root recipients");
    fs::write(
        store.join("team/.gpg-id"),
        format!("{}\n", nested_key.fingerprint),
    )
    .expect("write nested recipients");

    save_password_entry(
        store.to_string_lossy().as_ref(),
        "root-entry",
        "root secret",
        true,
    )
    .expect("save root entry");
    save_password_entry(
        store.to_string_lossy().as_ref(),
        "team/service",
        "nested secret",
        true,
    )
    .expect("save nested entry");

    save_store_recipients(
        store.to_string_lossy().as_ref(),
        std::slice::from_ref(&replacement_root_key.fingerprint),
        StoreRecipientsPrivateKeyRequirement::AnyManagedKey,
    )
    .expect("update store recipients");

    assert_eq!(
        fs::read_to_string(store.join(".gpg-id")).expect("read root recipients"),
        format!("{}\n", replacement_root_key.fingerprint)
    );
    assert_eq!(
        fs::read_to_string(store.join("team/.gpg-id")).expect("read nested recipients"),
        format!("{}\n", nested_key.fingerprint)
    );
    assert_eq!(
        read_password_entry(store.to_string_lossy().as_ref(), "root-entry")
            .expect("read root entry after update"),
        "root secret".to_string()
    );
    assert_eq!(
        read_password_entry(store.to_string_lossy().as_ref(), "team/service")
            .expect("read nested entry after update"),
        "nested secret".to_string()
    );
}

#[test]
fn store_recipients_work_without_a_selected_default_key() {
    let env = SystemBackendTestEnv::new();
    let password: Password = "hunter2".into();
    let (cert, _) = CertBuilder::general_purpose(Some("Store Example <store@example.com>"))
        .set_password(Some(password))
        .generate()
        .expect("failed to generate password-protected certificate");
    let mut bytes = Vec::new();
    cert.as_tsk()
        .serialize(&mut bytes)
        .expect("failed to serialize protected test certificate");
    let imported = import_ripasso_private_key_bytes(&bytes, Some("hunter2"))
        .expect("expected private key import to succeed");

    let store = env.root_dir().join("secondary-store");
    fs::create_dir_all(&store).expect("create store");
    fs::write(store.join(".gpg-id"), format!("{}\n", imported.fingerprint))
        .expect("write recipients");

    Preferences::new()
        .set_ripasso_own_fingerprint(None)
        .expect("clear selected fingerprint");
    assert!(resolved_ripasso_own_fingerprint().is_err());

    save_password_entry(
        store.to_string_lossy().as_ref(),
        "team/service",
        "supersecret\nusername: alice",
        true,
    )
    .expect("save entry with store recipients only");

    assert_eq!(
        read_password_entry(store.to_string_lossy().as_ref(), "team/service")
            .expect("read saved entry"),
        "supersecret\nusername: alice".to_string()
    );
}

#[test]
fn store_recipients_save_can_decrypt_with_a_non_selected_imported_key() {
    let env = SystemBackendTestEnv::new();
    let password: Password = "hunter2".into();

    let (cert_a, _) = CertBuilder::general_purpose(Some("Key A <a@example.com>"))
        .set_password(Some(password.clone()))
        .generate()
        .expect("generate first certificate");
    let mut bytes_a = Vec::new();
    cert_a
        .as_tsk()
        .serialize(&mut bytes_a)
        .expect("serialize first certificate");
    let key_a = import_ripasso_private_key_bytes(&bytes_a, Some("hunter2"))
        .expect("import first private key");

    let (cert_b, _) = CertBuilder::general_purpose(Some("Key B <b@example.com>"))
        .set_password(Some(password))
        .generate()
        .expect("generate second certificate");
    let mut bytes_b = Vec::new();
    cert_b
        .as_tsk()
        .serialize(&mut bytes_b)
        .expect("serialize second certificate");
    let key_b = import_ripasso_private_key_bytes(&bytes_b, Some("hunter2"))
        .expect("import second private key");

    let store = env.root_dir().join("secondary-store");
    fs::create_dir_all(&store).expect("create store");
    fs::write(store.join(".gpg-id"), format!("{}\n", key_a.fingerprint))
        .expect("write initial recipients");

    save_password_entry(
        store.to_string_lossy().as_ref(),
        "team/service",
        "supersecret\nusername: alice",
        true,
    )
    .expect("save initial entry");

    Preferences::new()
        .set_ripasso_own_fingerprint(Some(&key_b.fingerprint))
        .expect("select second key");

    save_store_recipients(
        store.to_string_lossy().as_ref(),
        std::slice::from_ref(&key_b.fingerprint),
        StoreRecipientsPrivateKeyRequirement::AnyManagedKey,
    )
    .expect("re-encrypt store with second key");

    assert_eq!(
        read_password_entry(store.to_string_lossy().as_ref(), "team/service")
            .expect("read re-encrypted entry"),
        "supersecret\nusername: alice".to_string()
    );
}

#[test]
fn store_recipients_save_can_remove_the_selected_private_key_from_recipients() {
    let env = SystemBackendTestEnv::new();
    let password: Password = "hunter2".into();

    let (cert_a, _) = CertBuilder::general_purpose(Some("Key A <a@example.com>"))
        .set_password(Some(password.clone()))
        .generate()
        .expect("generate first certificate");
    let mut bytes_a = Vec::new();
    cert_a
        .as_tsk()
        .serialize(&mut bytes_a)
        .expect("serialize first certificate");
    let key_a = import_ripasso_private_key_bytes(&bytes_a, Some("hunter2"))
        .expect("import first private key");

    let (cert_b, _) = CertBuilder::general_purpose(Some("Key B <b@example.com>"))
        .set_password(Some(password))
        .generate()
        .expect("generate second certificate");
    let mut bytes_b = Vec::new();
    cert_b
        .as_tsk()
        .serialize(&mut bytes_b)
        .expect("serialize second certificate");
    let key_b = import_ripasso_private_key_bytes(&bytes_b, Some("hunter2"))
        .expect("import second private key");

    let store = env.root_dir().join("secondary-store");
    fs::create_dir_all(&store).expect("create store");
    fs::write(
        store.join(".gpg-id"),
        format!("{}\n{}\n", key_a.fingerprint, key_b.fingerprint),
    )
    .expect("write initial recipients");

    save_password_entry(
        store.to_string_lossy().as_ref(),
        "team/service",
        "supersecret\nusername: alice",
        true,
    )
    .expect("save initial entry");

    Preferences::new()
        .set_ripasso_own_fingerprint(Some(&key_a.fingerprint))
        .expect("select first key");

    save_store_recipients(
        store.to_string_lossy().as_ref(),
        std::slice::from_ref(&key_b.fingerprint),
        StoreRecipientsPrivateKeyRequirement::AnyManagedKey,
    )
    .expect("re-encrypt store without the selected key");

    assert_eq!(
        read_password_entry(store.to_string_lossy().as_ref(), "team/service")
            .expect("read re-encrypted entry"),
        "supersecret\nusername: alice".to_string()
    );
}

#[test]
fn integrated_backend_commits_git_backed_store_changes_with_private_key_identity() {
    let env = SystemBackendTestEnv::new();
    let (cert, bytes) = protected_cert("Git Signer <git-flatpak@example.com>");
    let imported =
        import_ripasso_private_key_bytes(&bytes, Some("hunter2")).expect("import private key");
    Preferences::new()
        .set_ripasso_own_fingerprint(Some(&imported.fingerprint))
        .expect("select signing key");

    let mut public_bytes = Vec::new();
    cert.serialize(&mut public_bytes)
        .expect("serialize public certificate");
    SystemBackendTestEnv::import_public_key(&public_bytes)
        .expect("import public key for signature verification");
    env.init_store_git_repository()
        .expect("initialize git repository");
    let store_root = env.store_root().to_string_lossy().to_string();

    save_store_recipients(
        &store_root,
        std::slice::from_ref(&imported.fingerprint),
        StoreRecipientsPrivateKeyRequirement::AnyManagedKey,
    )
    .expect("save store recipients");
    save_password_entry(
        &store_root,
        "team/service",
        "secret-value\nusername: alice",
        true,
    )
    .expect("save password entry");

    let subjects = env
        .store_git_commit_subjects()
        .expect("read commit subjects");
    assert_eq!(subjects.len(), 2);
    assert_eq!(subjects[0], "Add password for team/service");
    assert_eq!(subjects[1], "Update password store recipients");
    assert_eq!(
        env.store_git_head_author().expect("read head author"),
        "Git Signer <git-flatpak@example.com>"
    );
    assert!(env
        .store_head_commit_has_signature()
        .expect("inspect commit headers"));
    env.verify_store_head_commit_signature()
        .expect("verify head commit signature");
}

#[test]
fn integrated_backend_commits_with_the_entry_private_key_instead_of_an_unrelated_selected_key() {
    let env = SystemBackendTestEnv::new();
    let (cert_a, bytes_a) = protected_cert("Entry Key <entry@example.com>");
    let imported_a =
        import_ripasso_private_key_bytes(&bytes_a, Some("hunter2")).expect("import entry key");
    let (cert_b, bytes_b) = protected_cert("Selected Key <selected@example.com>");
    let imported_b = import_ripasso_private_key_bytes(&bytes_b, Some("hunter2"))
        .expect("import unrelated selected key");
    Preferences::new()
        .set_ripasso_own_fingerprint(Some(&imported_b.fingerprint))
        .expect("select unrelated key");

    let mut public_bytes_a = Vec::new();
    cert_a
        .serialize(&mut public_bytes_a)
        .expect("serialize entry public certificate");
    SystemBackendTestEnv::import_public_key(&public_bytes_a)
        .expect("import entry public key for signature verification");

    let mut public_bytes_b = Vec::new();
    cert_b
        .serialize(&mut public_bytes_b)
        .expect("serialize selected public certificate");
    SystemBackendTestEnv::import_public_key(&public_bytes_b)
        .expect("import unrelated selected public key for signature verification");
    env.init_store_git_repository()
        .expect("initialize git repository");
    let store_root = env.store_root().to_string_lossy().to_string();

    save_store_recipients(
        &store_root,
        std::slice::from_ref(&imported_a.fingerprint),
        StoreRecipientsPrivateKeyRequirement::AnyManagedKey,
    )
    .expect("save store recipients");
    save_password_entry(
        &store_root,
        "team/service",
        "secret-value\nusername: alice",
        true,
    )
    .expect("save password entry");

    let subjects = env
        .store_git_commit_subjects()
        .expect("read commit subjects");
    assert_eq!(subjects.len(), 2);
    assert_eq!(
        env.store_git_head_author().expect("read head author"),
        "Entry Key <entry@example.com>"
    );
    assert!(env
        .store_head_commit_has_signature()
        .expect("inspect commit headers"));
    env.verify_store_head_commit_signature()
        .expect("verify head commit signature");
}

#[test]
fn integrated_backend_commits_without_signature_when_private_key_is_locked() {
    let env = SystemBackendTestEnv::new();
    let bytes = protected_cert_bytes("Locked Signer <locked-flatpak@example.com>");
    let imported =
        import_ripasso_private_key_bytes(&bytes, Some("hunter2")).expect("import private key");
    Preferences::new()
        .set_ripasso_own_fingerprint(Some(&imported.fingerprint))
        .expect("select signing key");
    clear_cached_unlocked_ripasso_private_keys();
    env.init_store_git_repository()
        .expect("initialize git repository");
    let store_root = env.store_root().to_string_lossy().to_string();

    save_store_recipients(
        &store_root,
        std::slice::from_ref(&imported.fingerprint),
        StoreRecipientsPrivateKeyRequirement::AnyManagedKey,
    )
    .expect("save store recipients");
    save_password_entry(
        &store_root,
        "team/service",
        "secret-value\nusername: alice",
        true,
    )
    .expect("save password entry");

    let subjects = env
        .store_git_commit_subjects()
        .expect("read commit subjects");
    assert_eq!(subjects.len(), 2);
    assert_eq!(
        env.store_git_head_author().expect("read head author"),
        "Locked Signer <locked-flatpak@example.com>"
    );
    assert!(!env
        .store_head_commit_has_signature()
        .expect("inspect commit headers"));
}

#[cfg(any(feature = "fidostore", feature = "fidokey"))]
#[test]
fn integrated_backend_commits_pure_fido2_store_changes_without_signature() {
    let env = SystemBackendTestEnv::new();
    let credential = b"git-fido2-credential";
    let recipient = build_fido2_recipient_string(
        "0123456789abcdef0123456789abcdef01234567",
        "Git FIDO2 Key",
        credential,
    )
    .expect("build FIDO2 recipient");
    env.init_store_git_repository()
        .expect("initialize git repository");
    let store_root = env.store_root().to_string_lossy().to_string();

    save_store_recipients(
        &store_root,
        std::slice::from_ref(&recipient),
        StoreRecipientsPrivateKeyRequirement::AnyManagedKey,
    )
    .expect("save FIDO2 store recipients");

    let _guard = Fido2TransportGuard::install(Arc::new(SequentialOnlyFido2Transport::new(&[(
        credential.as_slice(),
        b"git-fido2-secret".as_slice(),
    )])));
    save_password_entry(
        &store_root,
        "team/service",
        "secret-value\nusername: alice",
        true,
    )
    .expect("save FIDO2 password entry");
    assert_eq!(
        git_commit_private_key_requiring_unlock_for_entry(&store_root, "team/service")
            .expect("skip FIDO2 entry signing"),
        None
    );

    let subjects = env
        .store_git_commit_subjects()
        .expect("read commit subjects");
    assert_eq!(subjects.len(), 2);
    assert_eq!(subjects[0], "Add password for team/service");
    assert_eq!(subjects[1], "Update password store recipients");
    assert_eq!(
        env.store_git_head_author().expect("read head author"),
        "Keycord <git@keycord.invalid>"
    );
    assert!(!env
        .store_head_commit_has_signature()
        .expect("inspect commit headers"));
}

#[test]
fn unreadable_entry_rename_commits_without_a_signature() {
    let env = SystemBackendTestEnv::new();
    let bytes_a = protected_cert_bytes("Entry Key <entry-unreadable@example.com>");
    let bytes_b = protected_cert_bytes("Missing Key <missing-unreadable@example.com>");
    let imported_a = import_ripasso_private_key_bytes(&bytes_a, Some("hunter2"))
        .expect("import first private key");
    let imported_b = import_ripasso_private_key_bytes(&bytes_b, Some("hunter2"))
        .expect("import second private key");
    Preferences::new()
        .set_ripasso_own_fingerprint(Some(&imported_a.fingerprint))
        .expect("select signing key");
    env.init_store_git_repository()
        .expect("initialize git repository");
    let store_root = env.store_root().to_string_lossy().to_string();

    save_store_recipients(
        &store_root,
        &[
            imported_a.fingerprint.clone(),
            imported_b.fingerprint.clone(),
        ],
        StoreRecipientsPrivateKeyRequirement::AllManagedKeys,
    )
    .expect("save store recipients");
    save_password_entry(
        &store_root,
        "team/service",
        "secret-value\nusername: alice",
        true,
    )
    .expect("save password entry");
    remove_ripasso_private_key(&imported_b.fingerprint).expect("remove second key");

    rename_password_entry(&store_root, "team/service", "team/renamed")
        .expect("rename unreadable entry");

    let subjects = env
        .store_git_commit_subjects()
        .expect("read commit subjects");
    assert_eq!(
        subjects[0],
        "Rename password from team/service to team/renamed"
    );
    assert_eq!(
        env.store_git_head_author().expect("read head author"),
        "Keycord <git@keycord.invalid>"
    );
    assert!(!env
        .store_head_commit_has_signature()
        .expect("inspect commit headers"));
}

#[test]
fn unreadable_entry_delete_commits_without_a_signature() {
    let env = SystemBackendTestEnv::new();
    let bytes_a = protected_cert_bytes("Entry Key <entry-delete@example.com>");
    let bytes_b = protected_cert_bytes("Missing Key <missing-delete@example.com>");
    let imported_a = import_ripasso_private_key_bytes(&bytes_a, Some("hunter2"))
        .expect("import first private key");
    let imported_b = import_ripasso_private_key_bytes(&bytes_b, Some("hunter2"))
        .expect("import second private key");
    Preferences::new()
        .set_ripasso_own_fingerprint(Some(&imported_a.fingerprint))
        .expect("select signing key");
    env.init_store_git_repository()
        .expect("initialize git repository");
    let store_root = env.store_root().to_string_lossy().to_string();

    save_store_recipients(
        &store_root,
        &[
            imported_a.fingerprint.clone(),
            imported_b.fingerprint.clone(),
        ],
        StoreRecipientsPrivateKeyRequirement::AllManagedKeys,
    )
    .expect("save store recipients");
    save_password_entry(
        &store_root,
        "team/service",
        "secret-value\nusername: alice",
        true,
    )
    .expect("save password entry");
    remove_ripasso_private_key(&imported_b.fingerprint).expect("remove second key");

    delete_password_entry(&store_root, "team/service").expect("delete unreadable entry");

    let subjects = env
        .store_git_commit_subjects()
        .expect("read commit subjects");
    assert_eq!(subjects[0], "Remove password for team/service");
    assert_eq!(
        env.store_git_head_author().expect("read head author"),
        "Keycord <git@keycord.invalid>"
    );
    assert!(!env
        .store_head_commit_has_signature()
        .expect("inspect commit headers"));
}

#[test]
fn integrated_backend_saves_entries_with_empty_password_lines() {
    let env = SystemBackendTestEnv::new();
    let (cert, bytes) = protected_cert("Empty Password <empty-password@example.com>");
    let imported =
        import_ripasso_private_key_bytes(&bytes, Some("hunter2")).expect("import private key");

    let mut public_bytes = Vec::new();
    cert.serialize(&mut public_bytes)
        .expect("serialize public certificate");
    SystemBackendTestEnv::import_public_key(&public_bytes).expect("import public key");
    SystemBackendTestEnv::trust_public_key(&imported.fingerprint).expect("trust public key");

    let store_root = env.store_root().to_string_lossy().to_string();
    save_store_recipients(
        &store_root,
        std::slice::from_ref(&imported.fingerprint),
        StoreRecipientsPrivateKeyRequirement::AnyManagedKey,
    )
    .expect("save store recipients");
    save_password_entry(
        &store_root,
        "team/empty-password",
        "\nusername: alice",
        true,
    )
    .expect("save password entry with empty first line");

    assert_eq!(
        read_password_entry(&store_root, "team/empty-password").expect("read saved entry"),
        "\nusername: alice"
    );
}

#[test]
fn integrated_backend_save_leaves_git_worktree_clean() {
    let env = SystemBackendTestEnv::new();
    let (cert, bytes) = protected_cert("Git Clean <git-clean@example.com>");
    let imported =
        import_ripasso_private_key_bytes(&bytes, Some("hunter2")).expect("import private key");

    let mut public_bytes = Vec::new();
    cert.serialize(&mut public_bytes)
        .expect("serialize public certificate");
    SystemBackendTestEnv::import_public_key(&public_bytes).expect("import public key");
    SystemBackendTestEnv::trust_public_key(&imported.fingerprint).expect("trust public key");

    env.init_store_git_repository()
        .expect("initialize git repository");
    let store_root = env.store_root().to_string_lossy().to_string();
    save_store_recipients(
        &store_root,
        std::slice::from_ref(&imported.fingerprint),
        StoreRecipientsPrivateKeyRequirement::AnyManagedKey,
    )
    .expect("save store recipients");
    save_password_entry(&store_root, "example/user", "secret\nusername: alice", true)
        .expect("save password entry");

    assert_eq!(
        env.store_git_status_porcelain()
            .expect("read store git status after integrated save"),
        ""
    );
}

#[test]
fn git_commit_unlock_helper_detects_a_locked_entry_signing_key() {
    let env = SystemBackendTestEnv::new();
    let bytes = protected_cert_bytes("Locked Signer <locked-entry@example.com>");
    let imported =
        import_ripasso_private_key_bytes(&bytes, Some("hunter2")).expect("import private key");
    env.init_store_git_repository()
        .expect("initialize git repository");
    let store_root = env.store_root().to_string_lossy().to_string();

    save_store_recipients(
        &store_root,
        std::slice::from_ref(&imported.fingerprint),
        StoreRecipientsPrivateKeyRequirement::AnyManagedKey,
    )
    .expect("save store recipients");
    save_password_entry(
        &store_root,
        "team/service",
        "secret-value\nusername: alice",
        true,
    )
    .expect("save password entry");
    clear_cached_unlocked_ripasso_private_keys();

    assert_eq!(
        git_commit_private_key_requiring_unlock_for_entry(&store_root, "team/service",)
            .expect("resolve locked signing key"),
        Some(imported.fingerprint)
    );
}

#[test]
fn git_commit_unlock_helper_detects_a_locked_recipients_signing_key() {
    let env = SystemBackendTestEnv::new();
    let bytes = protected_cert_bytes("Locked Signer <locked-store@example.com>");
    let imported =
        import_ripasso_private_key_bytes(&bytes, Some("hunter2")).expect("import private key");
    env.init_store_git_repository()
        .expect("initialize git repository");
    clear_cached_unlocked_ripasso_private_keys();
    let store_root = env.store_root().to_string_lossy().to_string();

    assert_eq!(
        git_commit_private_key_requiring_unlock_for_store_recipients(
            &store_root,
            std::slice::from_ref(&imported.fingerprint),
            StoreRecipientsPrivateKeyRequirement::AnyManagedKey,
        )
        .expect("resolve locked signing key"),
        Some(imported.fingerprint)
    );
}

#[cfg(any(feature = "fidostore", feature = "fidokey"))]
#[test]
fn git_commit_unlock_helper_skips_pure_fido2_store_signing() {
    let env = SystemBackendTestEnv::new();
    let store_root = env.store_root().to_string_lossy().to_string();
    let recipient = build_fido2_recipient_string(
        "0123456789abcdef0123456789abcdef01234567",
        "FIDO2 Key",
        b"unlock-helper-credential",
    )
    .expect("build FIDO2 recipient");
    env.init_store_git_repository()
        .expect("initialize git repository");

    assert_eq!(
        git_commit_private_key_requiring_unlock_for_store_recipients(
            &store_root,
            std::slice::from_ref(&recipient),
            StoreRecipientsPrivateKeyRequirement::AnyManagedKey,
        )
        .expect("skip FIDO2 store signing"),
        None
    );
}

#[test]
fn store_recipients_unlock_helper_detects_a_locked_entry_key() {
    let env = SystemBackendTestEnv::new();
    let bytes = protected_cert_bytes("Locked Store Entry <locked-entry@example.com>");
    let imported =
        import_ripasso_private_key_bytes(&bytes, Some("hunter2")).expect("import private key");
    let store_root = env.store_root().to_string_lossy().to_string();

    save_store_recipients(
        &store_root,
        std::slice::from_ref(&imported.fingerprint),
        StoreRecipientsPrivateKeyRequirement::AnyManagedKey,
    )
    .expect("save store recipients");
    save_password_entry(&store_root, "team/service", "secret\nusername: alice", true)
        .expect("save password entry");
    clear_cached_unlocked_ripasso_private_keys();

    assert_eq!(
        store_recipients_private_key_requiring_unlock(&store_root)
            .expect("resolve locked entry key"),
        Some(imported.fingerprint)
    );
}
