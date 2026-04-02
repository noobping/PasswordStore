use super::crypto::IntegratedCryptoContext;
use super::entries::{read_password_entry, read_password_entry_with_progress};
use super::git::{maybe_commit_git_paths, password_entry_git_path};
use super::keys::clear_pending_fido2_enrollment;
use super::paths::{
    collect_password_entry_files, desired_entry_file_path, ensure_store_directory,
    fido2_recipients_file_for_recipients_path, label_from_entry_path, with_updated_recipient_files,
};
use super::recipients::{
    fido2_recipient_file_contents, preferred_ripasso_private_key_fingerprint_for_entry,
    standard_recipient_file_contents,
};
use crate::backend::{
    PasswordEntryError, PasswordEntryReadProgress, StoreRecipients, StoreRecipientsError,
    StoreRecipientsPrivateKeyRequirement, StoreRecipientsSaveProgress, StoreRecipientsSaveStage,
};
use crate::fido2_recipient::{parse_fido2_recipient_string, FIDO2_RECIPIENTS_FILE_NAME};
use crate::logging::log_error;
use crate::support::git::{ensure_store_git_repository, has_git_repository};
use crate::support::secure_fs::write_atomic_file;
use std::fs;
use std::path::{Path, PathBuf};

fn decrypted_store_entries_with_progress(
    store_dir: &Path,
    store_root: &str,
    mut report_progress: Option<&mut dyn FnMut(StoreRecipientsSaveProgress)>,
) -> Result<Vec<(PathBuf, String)>, String> {
    let mut decrypted = Vec::new();
    let entry_paths = collect_password_entry_files(store_dir)?;
    let total_items = entry_paths.len();

    for (index, entry_path) in entry_paths.into_iter().enumerate() {
        let current_item = index + 1;
        let label = label_from_entry_path(store_dir, &entry_path)?;
        let secret = if let Some(report_progress) = report_progress.as_deref_mut() {
            report_progress(StoreRecipientsSaveProgress {
                stage: StoreRecipientsSaveStage::ReadingExistingItems,
                current_item,
                total_items,
                current_touch: 0,
                total_touches: 0,
            });
            let mut emit_progress = |progress: PasswordEntryReadProgress| {
                report_progress(StoreRecipientsSaveProgress {
                    stage: StoreRecipientsSaveStage::ReadingExistingItems,
                    current_item,
                    total_items,
                    current_touch: progress.current_step,
                    total_touches: progress.total_steps,
                });
            };
            read_password_entry_with_progress(store_root, &label, &mut emit_progress)
                .map_err(|err| err.to_string())?
        } else {
            read_password_entry(store_root, &label).map_err(|err| err.to_string())?
        };
        decrypted.push((entry_path, secret));
    }

    Ok(decrypted)
}

fn clear_saved_fido2_enrollment_state(recipients: &StoreRecipients) {
    for recipient in recipients.fido2() {
        let Ok(Some(parsed)) = parse_fido2_recipient_string(recipient) else {
            continue;
        };
        if let Err(err) = clear_pending_fido2_enrollment(&parsed.id) {
            log_error(format!(
                "Failed to clear the pending FIDO2 enrollment cache for '{}': {err}",
                parsed.id
            ));
        }
    }
}

pub fn store_recipients_private_key_requiring_unlock(
    store_root: &str,
) -> Result<Option<String>, String> {
    let store_dir = ensure_store_directory(store_root)?;

    for entry_path in collect_password_entry_files(&store_dir)? {
        let label = label_from_entry_path(&store_dir, &entry_path)?;
        if !matches!(
            read_password_entry(store_root, &label),
            Err(PasswordEntryError::LockedPrivateKey(_))
        ) {
            continue;
        }

        return preferred_ripasso_private_key_fingerprint_for_entry(store_root, &label).map(Some);
    }

    Ok(None)
}

pub fn save_store_recipients(
    store_root: &str,
    recipients: &StoreRecipients,
    private_key_requirement: StoreRecipientsPrivateKeyRequirement,
) -> Result<(), StoreRecipientsError> {
    let store_dir =
        ensure_store_directory(store_root).map_err(StoreRecipientsError::from_store_message)?;
    let decrypted_entries = decrypted_store_entries_with_progress(&store_dir, store_root, None)
        .map_err(StoreRecipientsError::from_store_message)?;
    let recipients_contents =
        standard_recipient_file_contents(recipients.standard(), private_key_requirement);
    let fido2_recipients_contents = fido2_recipient_file_contents(recipients.fido2());
    let context = IntegratedCryptoContext::load_for_recipient_contents(
        &recipients_contents,
        &fido2_recipients_contents,
    )
    .map_err(StoreRecipientsError::from_store_message)?;
    let recipients_path = store_dir.join(".gpg-id");
    let fido2_recipients_path = fido2_recipients_file_for_recipients_path(&recipients_path);
    let should_initialize_git = !recipients_path.exists() && !has_git_repository(store_root);
    let had_fido2_recipients_path = fido2_recipients_path.exists();
    let mut committed_entry_paths = Vec::new();

    with_updated_recipient_files(
        &recipients_path,
        &recipients_contents,
        &fido2_recipients_path,
        &fido2_recipients_contents,
        || {
            for (entry_path, secret) in &decrypted_entries {
                let label = label_from_entry_path(&store_dir, entry_path)?;
                let updated_entry_path = desired_entry_file_path(store_root, &label)?;
                let previous_ciphertext = fs::read(entry_path).ok();
                let ciphertext = context
                    .encrypt_contents_with_existing(secret, previous_ciphertext.as_deref())?;
                write_atomic_file(&updated_entry_path, &ciphertext)
                    .map_err(|err| err.to_string())?;
                if updated_entry_path != *entry_path {
                    fs::remove_file(entry_path).map_err(|err| err.to_string())?;
                }
                committed_entry_paths
                    .push(password_entry_git_path(&store_dir, &updated_entry_path)?);
                if updated_entry_path != *entry_path {
                    committed_entry_paths.push(password_entry_git_path(&store_dir, entry_path)?);
                }
            }
            Ok(())
        },
    )
    .map_err(StoreRecipientsError::from_store_message)?;
    clear_saved_fido2_enrollment_state(recipients);

    if should_initialize_git {
        ensure_store_git_repository(store_root)
            .map_err(StoreRecipientsError::from_store_message)?;
    }

    maybe_commit_git_paths(
        store_root,
        "Update password store recipients",
        std::iter::once(".gpg-id".to_string())
            .chain(
                (!fido2_recipients_contents.trim().is_empty() || had_fido2_recipients_path)
                    .then(|| FIDO2_RECIPIENTS_FILE_NAME.to_string()),
            )
            .chain(committed_entry_paths),
        Some(context.fingerprint()),
    );

    Ok(())
}

pub fn save_store_recipients_with_progress(
    store_root: &str,
    recipients: &StoreRecipients,
    private_key_requirement: StoreRecipientsPrivateKeyRequirement,
    report_progress: &mut dyn FnMut(StoreRecipientsSaveProgress),
) -> Result<(), StoreRecipientsError> {
    let store_dir =
        ensure_store_directory(store_root).map_err(StoreRecipientsError::from_store_message)?;
    let decrypted_entries =
        decrypted_store_entries_with_progress(&store_dir, store_root, Some(report_progress))
            .map_err(StoreRecipientsError::from_store_message)?;
    let recipients_contents =
        standard_recipient_file_contents(recipients.standard(), private_key_requirement);
    let fido2_recipients_contents = fido2_recipient_file_contents(recipients.fido2());
    let context = IntegratedCryptoContext::load_for_recipient_contents(
        &recipients_contents,
        &fido2_recipients_contents,
    )
    .map_err(StoreRecipientsError::from_store_message)?;
    let recipients_path = store_dir.join(".gpg-id");
    let fido2_recipients_path = fido2_recipients_file_for_recipients_path(&recipients_path);
    let should_initialize_git = !recipients_path.exists() && !has_git_repository(store_root);
    let had_fido2_recipients_path = fido2_recipients_path.exists();
    let mut committed_entry_paths = Vec::new();

    with_updated_recipient_files(
        &recipients_path,
        &recipients_contents,
        &fido2_recipients_path,
        &fido2_recipients_contents,
        || {
            let total_items = decrypted_entries.len();
            for (index, (entry_path, secret)) in decrypted_entries.iter().enumerate() {
                let current_item = index + 1;
                let label = label_from_entry_path(&store_dir, entry_path)?;
                let updated_entry_path = desired_entry_file_path(store_root, &label)?;
                report_progress(StoreRecipientsSaveProgress {
                    stage: StoreRecipientsSaveStage::WritingUpdatedItems,
                    current_item,
                    total_items,
                    current_touch: 0,
                    total_touches: 0,
                });
                let previous_ciphertext = fs::read(entry_path).ok();
                let ciphertext = context.encrypt_contents_with_existing_and_progress(
                    secret,
                    previous_ciphertext.as_deref(),
                    Some(&mut |progress| {
                        report_progress(StoreRecipientsSaveProgress {
                            stage: StoreRecipientsSaveStage::WritingUpdatedItems,
                            current_item,
                            total_items,
                            current_touch: progress.current_step,
                            total_touches: progress.total_steps,
                        });
                    }),
                )?;
                write_atomic_file(&updated_entry_path, &ciphertext)
                    .map_err(|err| err.to_string())?;
                if updated_entry_path != *entry_path {
                    fs::remove_file(entry_path).map_err(|err| err.to_string())?;
                }
                committed_entry_paths
                    .push(password_entry_git_path(&store_dir, &updated_entry_path)?);
                if updated_entry_path != *entry_path {
                    committed_entry_paths.push(password_entry_git_path(&store_dir, entry_path)?);
                }
            }
            Ok(())
        },
    )
    .map_err(StoreRecipientsError::from_store_message)?;
    clear_saved_fido2_enrollment_state(recipients);

    if should_initialize_git {
        ensure_store_git_repository(store_root)
            .map_err(StoreRecipientsError::from_store_message)?;
    }

    maybe_commit_git_paths(
        store_root,
        "Update password store recipients",
        std::iter::once(".gpg-id".to_string())
            .chain(
                (!fido2_recipients_contents.trim().is_empty() || had_fido2_recipients_path)
                    .then(|| FIDO2_RECIPIENTS_FILE_NAME.to_string()),
            )
            .chain(committed_entry_paths),
        Some(context.fingerprint()),
    );

    Ok(())
}
