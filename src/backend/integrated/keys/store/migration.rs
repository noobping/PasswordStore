use super::paths::{ripasso_keys_dir, ripasso_keys_v2_dir};
use super::storage::{read_hardware_private_key_entry, read_password_private_key_entry};
#[cfg(feature = "fidokey")]
use super::{paths::ripasso_fido_keys_dir, storage::read_fido2_private_key_entry};
use std::ffi::OsStr;
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum ManagedKeyStorageStartup {
    Ready,
    RecoveryRequired(ManagedKeyStorageRecovery),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct ManagedKeyStorageRecovery {
    pub(crate) incompatible_paths: Vec<PathBuf>,
    pub(crate) detail: String,
}

impl ManagedKeyStorageRecovery {
    pub(crate) fn detail(&self) -> &str {
        &self.detail
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct IncompatibleManagedKeyArtifact {
    path: PathBuf,
    reason: String,
}

impl IncompatibleManagedKeyArtifact {
    fn new(path: PathBuf, reason: impl Into<String>) -> Self {
        Self {
            path,
            reason: reason.into(),
        }
    }
}

pub(crate) fn prepare_managed_private_key_storage_for_startup(
) -> Result<ManagedKeyStorageStartup, String> {
    let mut incompatible = Vec::new();
    migrate_password_key_names(&mut incompatible)?;
    migrate_hardware_key_names(&mut incompatible)?;
    #[cfg(feature = "fidokey")]
    migrate_fido2_key_names(&mut incompatible)?;

    if incompatible.is_empty() {
        Ok(ManagedKeyStorageStartup::Ready)
    } else {
        Ok(ManagedKeyStorageStartup::RecoveryRequired(
            build_recovery_report(incompatible),
        ))
    }
}

pub(crate) fn continue_after_managed_key_storage_recovery(
    recovery: &ManagedKeyStorageRecovery,
) -> Result<(), String> {
    for path in &recovery.incompatible_paths {
        remove_incompatible_managed_key_artifact(path)?;
    }

    match prepare_managed_private_key_storage_for_startup()? {
        ManagedKeyStorageStartup::Ready => Ok(()),
        ManagedKeyStorageStartup::RecoveryRequired(report) => Err(format!(
            "Managed private-key recovery is still blocked.\n{}",
            report.detail()
        )),
    }
}

fn build_recovery_report(
    incompatible: Vec<IncompatibleManagedKeyArtifact>,
) -> ManagedKeyStorageRecovery {
    let detail = incompatible
        .iter()
        .map(|artifact| format!("{}: {}", artifact.path.display(), artifact.reason))
        .collect::<Vec<_>>()
        .join("\n");

    ManagedKeyStorageRecovery {
        incompatible_paths: incompatible
            .into_iter()
            .map(|artifact| artifact.path)
            .collect(),
        detail,
    }
}

fn migrate_password_key_names(
    incompatible: &mut Vec<IncompatibleManagedKeyArtifact>,
) -> Result<(), String> {
    let keys_dir = ripasso_keys_dir()?;
    for path in directory_entries(&keys_dir)? {
        if !path.is_file() {
            incompatible.push(IncompatibleManagedKeyArtifact::new(
                path,
                "Expected a private-key file in the managed key folder.",
            ));
            continue;
        }

        match read_password_private_key_entry(&path) {
            Ok(entry) => normalize_managed_key_path(
                &path,
                &keys_dir.join(entry.key.fingerprint.to_ascii_lowercase()),
                incompatible,
            )?,
            Err(err) => incompatible.push(IncompatibleManagedKeyArtifact::new(path, err)),
        }
    }
    Ok(())
}

fn migrate_hardware_key_names(
    incompatible: &mut Vec<IncompatibleManagedKeyArtifact>,
) -> Result<(), String> {
    let keys_dir = ripasso_keys_v2_dir()?;
    for path in directory_entries(&keys_dir)? {
        if !path.is_dir() {
            incompatible.push(IncompatibleManagedKeyArtifact::new(
                path,
                "Expected a hardware-key folder in the managed hardware key folder.",
            ));
            continue;
        }

        match read_hardware_private_key_entry(&path) {
            Ok(entry) => normalize_managed_key_path(
                &path,
                &keys_dir.join(entry.key.fingerprint.to_ascii_lowercase()),
                incompatible,
            )?,
            Err(err) => incompatible.push(IncompatibleManagedKeyArtifact::new(path, err)),
        }
    }
    Ok(())
}

#[cfg(feature = "fidokey")]
fn migrate_fido2_key_names(
    incompatible: &mut Vec<IncompatibleManagedKeyArtifact>,
) -> Result<(), String> {
    let keys_dir = ripasso_fido_keys_dir()?;
    for path in directory_entries(&keys_dir)? {
        if !path.is_file() {
            incompatible.push(IncompatibleManagedKeyArtifact::new(
                path,
                "Expected a FIDO2 key file in the managed FIDO2 key folder.",
            ));
            continue;
        }

        match read_fido2_private_key_entry(&path) {
            Ok(entry) => normalize_managed_key_path(
                &path,
                &keys_dir.join(entry.key.fingerprint.to_ascii_lowercase()),
                incompatible,
            )?,
            Err(err) => incompatible.push(IncompatibleManagedKeyArtifact::new(path, err)),
        }
    }
    Ok(())
}

fn directory_entries(dir: &Path) -> Result<Vec<PathBuf>, String> {
    if !dir.exists() {
        return Ok(Vec::new());
    }

    let mut entries = Vec::new();
    for entry in fs::read_dir(dir).map_err(|err| err.to_string())? {
        let entry = entry.map_err(|err| err.to_string())?;
        entries.push(entry.path());
    }
    Ok(entries)
}

fn normalize_managed_key_path(
    current: &Path,
    canonical: &Path,
    incompatible: &mut Vec<IncompatibleManagedKeyArtifact>,
) -> Result<(), String> {
    if current == canonical {
        return Ok(());
    }

    if canonical.exists() {
        incompatible.push(IncompatibleManagedKeyArtifact::new(
            current.to_path_buf(),
            format!("Conflicts with canonical path '{}'.", canonical.display()),
        ));
        return Ok(());
    }

    rename_managed_key_path(current, canonical)
}

fn rename_managed_key_path(current: &Path, canonical: &Path) -> Result<(), String> {
    let is_case_only_rename = current.parent() == canonical.parent()
        && current
            .file_name()
            .zip(canonical.file_name())
            .is_some_and(|(left, right)| {
                left != right
                    && left
                        .to_string_lossy()
                        .eq_ignore_ascii_case(&right.to_string_lossy())
            });

    if !is_case_only_rename {
        return fs::rename(current, canonical).map_err(|err| {
            format!(
                "Failed to rename managed private-key data '{}' to '{}': {err}",
                current.display(),
                canonical.display()
            )
        });
    }

    let temp = unique_sibling_path(current, ".keycord-migrate");
    fs::rename(current, &temp).map_err(|err| {
        format!(
            "Failed to stage managed private-key data '{}' for rename: {err}",
            current.display()
        )
    })?;
    if let Err(err) = fs::rename(&temp, canonical) {
        let _ = fs::rename(&temp, current);
        return Err(format!(
            "Failed to rename managed private-key data '{}' to '{}': {err}",
            current.display(),
            canonical.display()
        ));
    }
    Ok(())
}

fn unique_sibling_path(path: &Path, suffix: &str) -> PathBuf {
    let parent = path.parent().unwrap_or_else(|| Path::new("."));
    let stem = path
        .file_name()
        .unwrap_or_else(|| OsStr::new("managed-key"));

    for attempt in 0.. {
        let candidate = parent.join(format!("{}{}{attempt}", stem.to_string_lossy(), suffix));
        if !candidate.exists() {
            return candidate;
        }
    }

    unreachable!("infinite candidate space exhausted")
}

fn remove_incompatible_managed_key_artifact(path: &Path) -> Result<(), String> {
    let metadata = fs::symlink_metadata(path).map_err(|err| {
        format!(
            "Failed to inspect incompatible managed private-key data '{}': {err}",
            path.display()
        )
    })?;

    if metadata.file_type().is_dir() {
        fs::remove_dir_all(path).map_err(|err| {
            format!(
                "Failed to remove incompatible managed private-key folder '{}': {err}",
                path.display()
            )
        })?;
    } else {
        fs::remove_file(path).map_err(|err| {
            format!(
                "Failed to remove incompatible managed private-key file '{}': {err}",
                path.display()
            )
        })?;
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{
        continue_after_managed_key_storage_recovery,
        prepare_managed_private_key_storage_for_startup, ManagedKeyStorageStartup,
    };
    use crate::backend::integrated::keys::cert::{
        parse_hardware_public_key_bytes, parse_managed_private_key_bytes, ManagedRipassoHardwareKey,
    };
    use crate::backend::test_support::SystemBackendTestEnv;
    use sequoia_openpgp::{
        cert::CertBuilder,
        parse::Parse,
        serialize::{Serialize, SerializeInto},
        Cert,
    };
    use std::fs;

    fn protected_cert_bytes(email: &str) -> Vec<u8> {
        let password: sequoia_openpgp::crypto::Password = "hunter2".into();
        let (cert, _) = CertBuilder::general_purpose(Some(email))
            .set_password(Some(password))
            .generate()
            .expect("generate protected cert");
        let mut bytes = Vec::new();
        cert.as_tsk()
            .serialize(&mut bytes)
            .expect("serialize protected cert");
        bytes
    }

    fn public_cert_bytes(email: &str) -> Vec<u8> {
        let (cert, _) = CertBuilder::general_purpose(Some(email))
            .generate()
            .expect("generate public cert");
        let mut bytes = Vec::new();
        cert.strip_secret_key_material()
            .serialize(&mut bytes)
            .expect("serialize public cert");
        bytes
    }

    fn app_data_dir(env: &SystemBackendTestEnv) -> std::path::PathBuf {
        env.root_dir().join("base/.local/share/keycord")
    }

    #[test]
    fn startup_migration_canonicalizes_password_key_filenames() {
        let env = SystemBackendTestEnv::new();
        let bytes = protected_cert_bytes("migrate-password@example.com");
        let (_, key) = parse_managed_private_key_bytes(&bytes).expect("parse key");
        let keys_dir = app_data_dir(&env).join("keys");
        fs::create_dir_all(&keys_dir).expect("create keys dir");
        let legacy_path = keys_dir.join(key.fingerprint.to_ascii_uppercase());
        fs::write(&legacy_path, &bytes).expect("write legacy key");

        let result = prepare_managed_private_key_storage_for_startup().expect("migrate keys");

        assert_eq!(result, ManagedKeyStorageStartup::Ready);
        assert!(!legacy_path.exists());
        assert!(keys_dir.join(key.fingerprint.to_ascii_lowercase()).exists());
    }

    #[test]
    fn startup_migration_canonicalizes_hardware_key_directories() {
        let env = SystemBackendTestEnv::new();
        let cert_bytes = public_cert_bytes("migrate-hardware@example.com");
        let hardware = ManagedRipassoHardwareKey {
            ident: "card-ident".to_string(),
            signing_fingerprint: None,
            decryption_fingerprint: None,
            reader_hint: None,
        };
        let (_cert, key) =
            parse_hardware_public_key_bytes(&cert_bytes, hardware.clone()).expect("parse key");
        let keys_dir = app_data_dir(&env).join("keys-v2");
        let legacy_dir = keys_dir.join(key.fingerprint.to_ascii_uppercase());
        fs::create_dir_all(&legacy_dir).expect("create hardware dir");
        fs::write(
            legacy_dir.join("manifest.toml"),
            toml::to_string_pretty(
                &super::super::manifest::HardwarePrivateKeyManifest::from_key(&key, &hardware),
            )
            .expect("serialize manifest"),
        )
        .expect("write manifest");
        let cert = Cert::from_bytes(&cert_bytes).expect("parse public cert");
        fs::write(
            legacy_dir.join("public.asc"),
            cert.armored().to_vec().expect("armor public cert"),
        )
        .expect("write public key");

        let result = prepare_managed_private_key_storage_for_startup().expect("migrate keys");

        assert_eq!(result, ManagedKeyStorageStartup::Ready);
        assert!(!legacy_dir.exists());
        assert!(keys_dir.join(key.fingerprint.to_ascii_lowercase()).exists());
    }

    #[test]
    fn startup_migration_reports_incompatible_items_without_removing_them() {
        let env = SystemBackendTestEnv::new();
        let keys_dir = app_data_dir(&env).join("keys");
        fs::create_dir_all(&keys_dir).expect("create keys dir");
        let incompatible = keys_dir.join("BROKEN");
        fs::write(&incompatible, b"not a key").expect("write invalid key");

        let result = prepare_managed_private_key_storage_for_startup().expect("inspect keys");

        let ManagedKeyStorageStartup::RecoveryRequired(report) = result else {
            panic!("expected recovery report");
        };
        assert!(report.detail().contains("BROKEN"));
        assert!(incompatible.exists());
    }

    #[test]
    fn continue_recovery_removes_only_incompatible_items_and_finishes_migration() {
        let env = SystemBackendTestEnv::new();
        let bytes = protected_cert_bytes("continue-migrate@example.com");
        let (_, key) = parse_managed_private_key_bytes(&bytes).expect("parse key");
        let keys_dir = app_data_dir(&env).join("keys");
        fs::create_dir_all(&keys_dir).expect("create keys dir");

        let legacy_path = keys_dir.join(key.fingerprint.to_ascii_uppercase());
        let incompatible = keys_dir.join("BROKEN");
        fs::write(&legacy_path, &bytes).expect("write legacy key");
        fs::write(&incompatible, b"not a key").expect("write invalid key");

        let result = prepare_managed_private_key_storage_for_startup().expect("inspect keys");
        let ManagedKeyStorageStartup::RecoveryRequired(report) = result else {
            panic!("expected recovery report");
        };

        continue_after_managed_key_storage_recovery(&report).expect("continue recovery");

        assert!(!incompatible.exists());
        assert!(!legacy_path.exists());
        assert!(keys_dir.join(key.fingerprint.to_ascii_lowercase()).exists());
    }

    #[test]
    fn continue_recovery_fails_if_selected_artifact_can_no_longer_be_removed() {
        let env = SystemBackendTestEnv::new();
        let keys_dir = app_data_dir(&env).join("keys");
        fs::create_dir_all(&keys_dir).expect("create keys dir");
        let incompatible = keys_dir.join("BROKEN");
        fs::write(&incompatible, b"not a key").expect("write invalid key");

        let result = prepare_managed_private_key_storage_for_startup().expect("inspect keys");
        let ManagedKeyStorageStartup::RecoveryRequired(report) = result else {
            panic!("expected recovery report");
        };

        fs::remove_file(&incompatible).expect("remove incompatible file externally");
        let err =
            continue_after_managed_key_storage_recovery(&report).expect_err("removal should fail");
        assert!(err.contains("Failed to inspect incompatible managed private-key data"));
    }
}
