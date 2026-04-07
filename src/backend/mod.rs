mod command;
mod errors;
mod host;
mod host_errors;
mod integrated;
#[cfg(test)]
mod test_support;

#[cfg(feature = "audit")]
use sequoia_openpgp::Cert;

pub use self::errors::PasswordEntryError;
pub use self::errors::PrivateKeyError;
pub use self::errors::{PasswordEntryWriteError, StoreRecipientsError};
#[cfg(feature = "legacy-compat")]
pub(crate) use self::integrated::ManagedKeyStorageRecovery as ManagedKeyRecovery;
pub(crate) use self::integrated::ManagedKeyStorageStartup as StartupPreparation;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PasswordEntryProgress {
    pub current_step: usize,
    pub total_steps: usize,
}

pub type PasswordEntryReadProgress = PasswordEntryProgress;
pub type PasswordEntryWriteProgress = PasswordEntryProgress;

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct StoreRecipients {
    standard: Vec<String>,
    fido2: Vec<String>,
}

impl StoreRecipients {
    pub fn new(standard: Vec<String>, fido2: Vec<String>) -> Self {
        Self { standard, fido2 }
    }

    pub fn standard(&self) -> &[String] {
        &self.standard
    }

    pub fn fido2(&self) -> &[String] {
        &self.fido2
    }

    pub fn is_empty(&self) -> bool {
        self.standard.is_empty() && self.fido2.is_empty()
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StoreRecipientsSaveStage {
    ReadingExistingItems,
    WritingUpdatedItems,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StoreRecipientsSaveProgress {
    pub stage: StoreRecipientsSaveStage,
    pub current_item: usize,
    pub total_items: usize,
    pub current_touch: usize,
    pub total_touches: usize,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum StoreRecipientsPrivateKeyRequirement {
    #[default]
    AnyManagedKey,
    AllManagedKeys,
}

#[cfg(target_os = "linux")]
pub use self::host::{
    armored_host_gpg_private_key, delete_host_gpg_private_key, import_host_gpg_private_key_bytes,
    list_host_gpg_private_keys, HostGpgPrivateKeySummary,
};
#[cfg(test)]
pub use integrated::required_private_key_fingerprints_for_entry;
#[cfg(target_os = "linux")]
pub use integrated::store_ripasso_private_key_bytes;
pub use integrated::{
    armored_ripasso_private_key, armored_ripasso_public_key, create_fido2_store_recipient,
    discover_ripasso_hardware_keys, generate_fido2_private_key, generate_ripasso_hardware_key,
    generate_ripasso_private_key, import_ripasso_hardware_key_bytes,
    import_ripasso_private_key_bytes, is_ripasso_private_key_unlocked, list_ripasso_private_keys,
    preferred_ripasso_private_key_fingerprint_for_entry, remove_ripasso_private_key,
    ripasso_private_key_requires_passphrase, ripasso_private_key_requires_session_unlock,
    ripasso_private_key_title, set_fido2_security_key_pin,
    unlock_fido2_store_recipient_for_session, unlock_ripasso_private_key_for_session,
    ConnectedSmartcardKey, DiscoveredHardwareToken, ManagedRipassoHardwareKey,
    ManagedRipassoPrivateKey, ManagedRipassoPrivateKeyProtection, PrivateKeyUnlockKind,
    PrivateKeyUnlockRequest,
};
pub use integrated::{
    git_commit_private_key_requiring_unlock_for_entry,
    git_commit_private_key_requiring_unlock_for_store_recipients,
};

#[cfg(feature = "audit")]
pub fn available_standard_public_certs() -> Result<Vec<Cert>, String> {
    integrated::available_standard_public_certs()
}

use crate::preferences::Preferences;

fn dispatch_backend<T>(integrated: impl FnOnce() -> T, host: impl FnOnce() -> T) -> T {
    if Preferences::new().uses_integrated_backend() {
        integrated()
    } else {
        host()
    }
}

pub const fn supports_first_time_fido2_pin_setup() -> bool {
    cfg!(all(
        target_os = "linux",
        feature = "fidopin",
        any(feature = "fidostore", feature = "fidokey")
    ))
}

macro_rules! dispatch_backend_call {
    ($(fn $name:ident($($arg:ident: $arg_ty:ty),* $(,)?) -> $ret:ty;)+) => {
        $(
            pub fn $name($($arg: $arg_ty),*) -> $ret {
                dispatch_backend(
                    || integrated::$name($($arg),*),
                    || host::$name($($arg),*),
                )
            }
        )+
    };
}

dispatch_backend_call! {
    fn read_password_entry(store_root: &str, label: &str) -> Result<String, PasswordEntryError>;
    fn read_password_line(store_root: &str, label: &str) -> Result<String, PasswordEntryError>;
    fn save_password_entry(
        store_root: &str,
        label: &str,
        contents: &str,
        overwrite: bool,
    ) -> Result<(), PasswordEntryWriteError>;
    fn rename_password_entry(
        store_root: &str,
        old_label: &str,
        new_label: &str,
    ) -> Result<(), PasswordEntryWriteError>;
    fn delete_password_entry(store_root: &str, label: &str) -> Result<(), PasswordEntryWriteError>;
    fn save_store_recipients(
        store_root: &str,
        recipients: &StoreRecipients,
        private_key_requirement: StoreRecipientsPrivateKeyRequirement,
    ) -> Result<(), StoreRecipientsError>;
}

pub fn list_connected_smartcard_keys() -> Result<Vec<ConnectedSmartcardKey>, String> {
    dispatch_backend(integrated::list_connected_smartcard_keys, || Ok(Vec::new()))
}

pub fn save_password_entry_with_progress(
    store_root: &str,
    label: &str,
    contents: &str,
    overwrite: bool,
    report_progress: &mut dyn FnMut(PasswordEntryWriteProgress),
) -> Result<(), PasswordEntryWriteError> {
    if Preferences::new().uses_integrated_backend() {
        integrated::save_password_entry_with_progress(
            store_root,
            label,
            contents,
            overwrite,
            report_progress,
        )
    } else {
        host::save_password_entry_with_progress(store_root, label, contents, overwrite)
    }
}

pub fn save_store_recipients_with_progress(
    store_root: &str,
    recipients: &StoreRecipients,
    private_key_requirement: StoreRecipientsPrivateKeyRequirement,
    report_progress: &mut dyn FnMut(StoreRecipientsSaveProgress),
) -> Result<(), StoreRecipientsError> {
    if Preferences::new().uses_integrated_backend() {
        integrated::save_store_recipients_with_progress(
            store_root,
            recipients,
            private_key_requirement,
            report_progress,
        )
    } else {
        host::save_store_recipients_with_progress(store_root, recipients, private_key_requirement)
    }
}

pub fn save_store_recipients_for_relative_dir(
    store_root: &str,
    relative_dir: &str,
    recipients: &StoreRecipients,
    private_key_requirement: StoreRecipientsPrivateKeyRequirement,
) -> Result<(), StoreRecipientsError> {
    dispatch_backend(
        || {
            integrated::save_store_recipients_for_relative_dir(
                store_root,
                relative_dir,
                recipients,
                private_key_requirement,
            )
        },
        || {
            host::save_store_recipients_for_relative_dir(
                store_root,
                relative_dir,
                recipients,
                private_key_requirement,
            )
        },
    )
}

pub fn save_store_recipients_with_progress_for_relative_dir(
    store_root: &str,
    relative_dir: &str,
    recipients: &StoreRecipients,
    private_key_requirement: StoreRecipientsPrivateKeyRequirement,
    report_progress: &mut dyn FnMut(StoreRecipientsSaveProgress),
) -> Result<(), StoreRecipientsError> {
    if Preferences::new().uses_integrated_backend() {
        integrated::save_store_recipients_with_progress_for_relative_dir(
            store_root,
            relative_dir,
            recipients,
            private_key_requirement,
            report_progress,
        )
    } else {
        host::save_store_recipients_with_progress_for_relative_dir(
            store_root,
            relative_dir,
            recipients,
            private_key_requirement,
        )
    }
}

pub fn read_password_entry_with_progress(
    store_root: &str,
    label: &str,
    report_progress: &mut dyn FnMut(PasswordEntryReadProgress),
) -> Result<String, PasswordEntryError> {
    if Preferences::new().uses_integrated_backend() {
        integrated::read_password_entry_with_progress(store_root, label, report_progress)
    } else {
        host::read_password_entry_with_progress(store_root, label)
    }
}

pub fn password_entry_is_readable(store_root: &str, label: &str) -> bool {
    dispatch_backend(
        || integrated::password_entry_is_readable(store_root, label),
        || host::password_entry_is_readable(store_root, label),
    )
}

pub fn password_entry_fido2_recipient_count(store_root: &str, label: &str) -> usize {
    dispatch_backend(
        || integrated::password_entry_fido2_recipient_count(store_root, label),
        || host::password_entry_fido2_recipient_count(store_root, label),
    )
}

pub fn store_recipients_private_key_requiring_unlock(
    store_root: &str,
) -> Result<Option<String>, String> {
    dispatch_backend(
        || integrated::store_recipients_private_key_requiring_unlock(store_root),
        || host::store_recipients_private_key_requiring_unlock(store_root),
    )
}

pub fn store_recipients_private_key_requiring_unlock_for_relative_dir(
    store_root: &str,
    relative_dir: &str,
) -> Result<Option<String>, String> {
    dispatch_backend(
        || {
            integrated::store_recipients_private_key_requiring_unlock_for_relative_dir(
                store_root,
                relative_dir,
            )
        },
        || {
            host::store_recipients_private_key_requiring_unlock_for_relative_dir(
                store_root,
                relative_dir,
            )
        },
    )
}

pub fn clear_runtime_secret_state() {
    integrated::clear_integrated_runtime_secret_state();
}

pub(crate) fn prepare_startup() -> Result<StartupPreparation, String> {
    if Preferences::new().uses_integrated_backend() {
        integrated::prepare_managed_private_key_storage_for_startup()
    } else {
        Ok(StartupPreparation::Ready)
    }
}

#[cfg(feature = "legacy-compat")]
pub(crate) fn continue_after_startup_recovery(recovery: &ManagedKeyRecovery) -> Result<(), String> {
    if !Preferences::new().uses_integrated_backend() {
        return Ok(());
    }

    integrated::continue_after_managed_key_storage_recovery(recovery)
}
