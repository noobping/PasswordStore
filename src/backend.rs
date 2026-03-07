use ripasso::crypto::CryptoImpl;
use ripasso::pass::PasswordStore;
#[cfg(all(feature = "setup", not(feature = "flatpak")))]
use crate::logging::{run_command_output, run_command_with_input, CommandLogOptions};
#[cfg(feature = "flatpak")]
use crate::logging::log_error;
use crate::preferences::Preferences;
#[cfg(feature = "flatpak")]
use sequoia_openpgp::{Cert, Fingerprint, parse::Parse, serialize::Serialize};
use std::env;
#[cfg(feature = "flatpak")]
use std::fs::File;
use std::fs;
use std::path::PathBuf;
#[cfg(all(feature = "setup", not(feature = "flatpak")))]
use std::process::Output;

#[cfg(feature = "flatpak")]
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ManagedRipassoPrivateKey {
    pub fingerprint: String,
    pub user_ids: Vec<String>,
}

#[cfg(feature = "flatpak")]
impl ManagedRipassoPrivateKey {
    pub fn title(&self) -> String {
        self.user_ids
            .first()
            .cloned()
            .filter(|value| !value.trim().is_empty())
            .unwrap_or_else(|| "Unnamed private key".to_string())
    }
}

fn user_home() -> Option<PathBuf> {
    env::var_os("HOME").map(PathBuf::from)
}

fn open_store(store_root: &str) -> Result<PasswordStore, String> {
    #[cfg(feature = "flatpak")]
    let own_fingerprint = Some(resolve_ripasso_own_fingerprint_bytes()?);
    #[cfg(not(feature = "flatpak"))]
    let own_fingerprint = None;
    #[cfg(feature = "flatpak")]
    let crypto_impl = CryptoImpl::Sequoia;
    #[cfg(not(feature = "flatpak"))]
    let crypto_impl = CryptoImpl::GpgMe;

    PasswordStore::new(
        "default",
        &Some(PathBuf::from(store_root)),
        &None,
        &user_home(),
        &None,
        &crypto_impl,
        &own_fingerprint,
    )
    .map_err(|err| err.to_string())
}

#[cfg(feature = "flatpak")]
fn ripasso_config_path() -> Result<PathBuf, String> {
    let home = user_home().ok_or_else(|| "Could not determine the home folder.".to_string())?;
    Ok(home.join(".local"))
}

#[cfg(feature = "flatpak")]
fn ripasso_keys_dir() -> Result<PathBuf, String> {
    Ok(ripasso_config_path()?.join("share").join("ripasso").join("keys"))
}

#[cfg(feature = "flatpak")]
fn fingerprint_from_string(value: &str) -> Result<[u8; 20], String> {
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

#[cfg(feature = "flatpak")]
fn parse_managed_private_key_bytes(
    bytes: &[u8],
) -> Result<(Cert, ManagedRipassoPrivateKey), String> {
    let cert = Cert::from_bytes(bytes).map_err(|err| err.to_string())?;
    if !cert.is_tsk() {
        return Err("That OpenPGP key file does not include a private key.".to_string());
    }

    let key = ManagedRipassoPrivateKey {
        fingerprint: cert.fingerprint().to_hex(),
        user_ids: cert
            .userids()
            .map(|user_id| user_id.userid().to_string())
            .filter(|value| !value.trim().is_empty())
            .collect(),
    };

    Ok((cert, key))
}

#[cfg(feature = "flatpak")]
pub fn list_ripasso_private_keys() -> Result<Vec<ManagedRipassoPrivateKey>, String> {
    let keys_dir = ripasso_keys_dir()?;
    if !keys_dir.exists() {
        return Ok(Vec::new());
    }

    let mut keys = Vec::new();
    for entry in fs::read_dir(&keys_dir).map_err(|err| err.to_string())? {
        let entry = entry.map_err(|err| err.to_string())?;
        let path = entry.path();
        if !path.is_file() {
            continue;
        }

        let data = match fs::read(&path) {
            Ok(data) => data,
            Err(err) => {
                log_error(format!(
                    "Failed to read ripasso managed key '{}': {err}",
                    path.display()
                ));
                continue;
            }
        };

        match parse_managed_private_key_bytes(&data) {
            Ok((_, key)) => {
                if !keys
                    .iter()
                    .any(|existing: &ManagedRipassoPrivateKey| existing.fingerprint == key.fingerprint)
                {
                    keys.push(key);
                }
            }
            Err(err) => {
                log_error(format!(
                    "Failed to load ripasso managed key '{}': {err}",
                    path.display()
                ));
            }
        }
    }

    keys.sort_by(|left, right| {
        left.title()
            .to_ascii_lowercase()
            .cmp(&right.title().to_ascii_lowercase())
            .then_with(|| left.fingerprint.cmp(&right.fingerprint))
    });
    Ok(keys)
}

#[cfg(feature = "flatpak")]
pub fn import_ripasso_private_key_bytes(bytes: &[u8]) -> Result<ManagedRipassoPrivateKey, String> {
    let keys_dir = ripasso_keys_dir()?;
    fs::create_dir_all(&keys_dir).map_err(|err| err.to_string())?;

    let (cert, key) = parse_managed_private_key_bytes(bytes)?;
    let mut file = File::create(keys_dir.join(key.fingerprint.to_ascii_lowercase()))
        .map_err(|err| err.to_string())?;
    cert.as_tsk()
        .serialize(&mut file)
        .map_err(|err| err.to_string())?;

    Ok(key)
}

#[cfg(feature = "flatpak")]
pub fn remove_ripasso_private_key(fingerprint: &str) -> Result<(), String> {
    let requested = Fingerprint::from_hex(fingerprint)
        .map_err(|err| format!("Invalid private key fingerprint '{fingerprint}': {err}"))?
        .to_hex();
    let keys_dir = ripasso_keys_dir()?;
    let direct_path = keys_dir.join(requested.to_ascii_lowercase());
    if direct_path.exists() {
        fs::remove_file(direct_path).map_err(|err| err.to_string())?;
        return Ok(());
    }

    if !keys_dir.exists() {
        return Err("That private key is not stored in ripasso.".to_string());
    }

    for entry in fs::read_dir(&keys_dir).map_err(|err| err.to_string())? {
        let entry = entry.map_err(|err| err.to_string())?;
        let path = entry.path();
        if !path.is_file() {
            continue;
        }

        let data = match fs::read(&path) {
            Ok(data) => data,
            Err(err) => {
                log_error(format!(
                    "Failed to read ripasso managed key '{}': {err}",
                    path.display()
                ));
                continue;
            }
        };

        let Ok((_, key)) = parse_managed_private_key_bytes(&data) else {
            continue;
        };
        if key.fingerprint.eq_ignore_ascii_case(&requested) {
            fs::remove_file(&path).map_err(|err| err.to_string())?;
            return Ok(());
        }
    }

    Err("That private key is not stored in ripasso.".to_string())
}

#[cfg(feature = "flatpak")]
pub fn resolved_ripasso_own_fingerprint() -> Result<String, String> {
    let settings = Preferences::new();
    let configured = settings.ripasso_own_fingerprint();
    let keys = list_ripasso_private_keys()?;

    let resolved = configured
        .as_deref()
        .and_then(|fingerprint| {
            keys.iter()
                .find(|key| key.fingerprint.eq_ignore_ascii_case(fingerprint))
                .map(|key| key.fingerprint.clone())
        })
        .or_else(|| keys.first().map(|key| key.fingerprint.clone()))
        .ok_or_else(|| {
            "Import a private key in Preferences before using the password store.".to_string()
        })?;

    if configured.as_deref() != Some(resolved.as_str()) {
        let _ = settings.set_ripasso_own_fingerprint(Some(&resolved));
    }

    Ok(resolved)
}

#[cfg(feature = "flatpak")]
fn resolve_ripasso_own_fingerprint_bytes() -> Result<[u8; 20], String> {
    fingerprint_from_string(&resolved_ripasso_own_fingerprint()?)
}

fn load_store_entry(
    store_root: &str,
    label: &str,
) -> Result<(PasswordStore, ripasso::pass::PasswordEntry), String> {
    let mut store = open_store(store_root)?;
    store
        .reload_password_list()
        .map_err(|err| err.to_string())?;
    let entry = store
        .passwords
        .iter()
        .find(|entry| entry.name == label)
        .cloned()
        .ok_or_else(|| format!("Password entry '{label}' was not found."))?;
    Ok((store, entry))
}

fn read_password_entry_ripasso(store_root: &str, label: &str) -> Result<String, String> {
    let (store, entry) = load_store_entry(store_root, label)?;
    entry.secret(&store).map_err(|err| err.to_string())
}

fn read_password_line_ripasso(store_root: &str, label: &str) -> Result<String, String> {
    let (store, entry) = load_store_entry(store_root, label)?;
    entry.password(&store).map_err(|err| err.to_string())
}

fn read_otp_code_ripasso(store_root: &str, label: &str) -> Result<String, String> {
    let (store, entry) = load_store_entry(store_root, label)?;
    entry.mfa(&store).map_err(|err| err.to_string())
}

fn save_password_entry_ripasso(
    store_root: &str,
    label: &str,
    contents: &str,
    overwrite: bool,
) -> Result<(), String> {
    let mut store = open_store(store_root)?;
    store
        .reload_password_list()
        .map_err(|err| err.to_string())?;
    if let Some(entry) = store.passwords.iter().find(|entry| entry.name == label).cloned() {
        if !overwrite {
            return Err("That password entry already exists.".to_string());
        }
        entry
            .update(contents.to_string(), &store)
            .map_err(|err| err.to_string())
    } else {
        store
            .new_password_file(label, contents)
            .map(|_| ())
            .map_err(|err| err.to_string())
    }
}

fn rename_password_entry_ripasso(
    store_root: &str,
    old_label: &str,
    new_label: &str,
) -> Result<(), String> {
    let mut store = open_store(store_root)?;
    store
        .reload_password_list()
        .map_err(|err| err.to_string())?;
    store
        .rename_file(old_label, new_label)
        .map(|_| ())
        .map_err(|err| err.to_string())
}

fn delete_password_entry_ripasso(store_root: &str, label: &str) -> Result<(), String> {
    let (store, entry) = load_store_entry(store_root, label)?;
    entry.delete_file(&store).map_err(|err| err.to_string())
}

fn save_store_recipients_ripasso(store_root: &str, recipients: &[String]) -> Result<(), String> {
    let store_dir = PathBuf::from(store_root);
    if store_dir.exists() {
        if !store_dir.is_dir() {
            return Err("The selected password store path is not a folder.".to_string());
        }
    } else {
        fs::create_dir_all(&store_dir).map_err(|err| err.to_string())?;
    }

    let recipients_path = store_dir.join(".gpg-id");
    let previous_recipients = fs::read_to_string(&recipients_path).ok();
    let contents = format!("{}\n", recipients.join("\n"));

    fs::write(&recipients_path, contents).map_err(|err| err.to_string())?;

    let result = (|| {
        let store = open_store(store_root)?;
        let entries = store.all_passwords().map_err(|err| err.to_string())?;
        for entry in entries {
            let secret = entry.secret(&store).map_err(|err| err.to_string())?;
            entry.update(secret, &store).map_err(|err| err.to_string())?;
        }
        Ok(())
    })();

    if let Err(err) = result {
        match previous_recipients {
            Some(previous) => {
                let _ = fs::write(&recipients_path, previous);
            }
            None => {
                let _ = fs::remove_file(&recipients_path);
            }
        }
        return Err(err);
    }

    Ok(())
}

#[cfg(all(feature = "setup", not(feature = "flatpak")))]
fn use_ripasso_backend() -> bool {
    Preferences::new().uses_ripasso_backend()
}

#[cfg(all(feature = "setup", not(feature = "flatpak")))]
fn command_error(output: &Output, prefix: &str) -> String {
    let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
    if stderr.is_empty() {
        format!("{prefix}: {}", output.status)
    } else {
        stderr
    }
}

#[cfg(all(feature = "setup", not(feature = "flatpak")))]
fn read_password_entry_command(store_root: &str, label: &str) -> Result<String, String> {
    let settings = Preferences::new();
    let mut cmd = settings.command();
    cmd.env("PASSWORD_STORE_DIR", store_root).arg(label);
    let output = run_command_output(&mut cmd, "Read password entry", CommandLogOptions::SENSITIVE)
        .map_err(|err| format!("Failed to run pass: {err}"))?;
    if output.status.success() {
        Ok(String::from_utf8_lossy(&output.stdout).to_string())
    } else {
        Err(command_error(&output, "pass failed"))
    }
}

#[cfg(all(feature = "setup", not(feature = "flatpak")))]
fn read_password_line_command(store_root: &str, label: &str) -> Result<String, String> {
    let settings = Preferences::new();
    let mut cmd = settings.command();
    cmd.env("PASSWORD_STORE_DIR", store_root).arg(label);
    let output = run_command_output(
        &mut cmd,
        "Read password entry for clipboard copy",
        CommandLogOptions::SENSITIVE,
    )
    .map_err(|err| format!("Failed to run pass: {err}"))?;
    if output.status.success() {
        Ok(String::from_utf8_lossy(&output.stdout)
            .lines()
            .next()
            .unwrap_or_default()
            .to_string())
    } else {
        Err(command_error(&output, "pass failed"))
    }
}

#[cfg(all(feature = "setup", not(feature = "flatpak")))]
fn read_otp_code_command(store_root: &str, label: &str) -> Result<String, String> {
    let settings = Preferences::new();
    let mut cmd = settings.command();
    cmd.env("PASSWORD_STORE_DIR", store_root)
        .args(["otp", label]);
    let output = run_command_output(&mut cmd, "Read OTP code", CommandLogOptions::SENSITIVE)
        .map_err(|err| format!("Failed to run pass OTP: {err}"))?;
    if output.status.success() {
        Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
    } else {
        Err(command_error(&output, "pass otp failed"))
    }
}

#[cfg(all(feature = "setup", not(feature = "flatpak")))]
fn save_password_entry_command(
    store_root: &str,
    label: &str,
    contents: &str,
    overwrite: bool,
) -> Result<(), String> {
    let settings = Preferences::new();
    let mut cmd = settings.command();
    cmd.env("PASSWORD_STORE_DIR", store_root)
        .arg("insert")
        .arg("-m");
    if overwrite {
        cmd.arg("-f");
    }
    cmd.arg(label);

    let output = run_command_with_input(
        &mut cmd,
        "Save password entry",
        contents,
        CommandLogOptions::SENSITIVE,
    )?;
    if output.status.success() {
        Ok(())
    } else {
        Err(command_error(&output, "pass insert failed"))
    }
}

#[cfg(all(feature = "setup", not(feature = "flatpak")))]
fn rename_password_entry_command(
    store_root: &str,
    old_label: &str,
    new_label: &str,
) -> Result<(), String> {
    let settings = Preferences::new();
    let mut cmd = settings.command();
    cmd.env("PASSWORD_STORE_DIR", store_root)
        .arg("mv")
        .arg(old_label)
        .arg(new_label);
    let output = run_command_output(
        &mut cmd,
        "Rename password entry",
        CommandLogOptions::DEFAULT,
    )
    .map_err(|err| format!("Failed to run pass mv: {err}"))?;
    if output.status.success() {
        Ok(())
    } else {
        Err(command_error(&output, "pass mv failed"))
    }
}

#[cfg(all(feature = "setup", not(feature = "flatpak")))]
fn delete_password_entry_command(store_root: &str, label: &str) -> Result<(), String> {
    let settings = Preferences::new();
    let mut cmd = settings.command();
    cmd.env("PASSWORD_STORE_DIR", store_root)
        .arg("rm")
        .arg("-rf")
        .arg(label);
    let output = run_command_output(
        &mut cmd,
        "Delete password entry",
        CommandLogOptions::DEFAULT,
    )
    .map_err(|err| format!("Failed to run pass rm: {err}"))?;
    if output.status.success() {
        Ok(())
    } else {
        Err(command_error(&output, "pass rm failed"))
    }
}

#[cfg(all(feature = "setup", not(feature = "flatpak")))]
fn save_store_recipients_command(store_root: &str, recipients: &[String]) -> Result<(), String> {
    let settings = Preferences::new();
    let mut cmd = settings.command();
    cmd.env("PASSWORD_STORE_DIR", store_root)
        .arg("init")
        .args(recipients);
    let output = run_command_output(
        &mut cmd,
        "Save password store recipients",
        CommandLogOptions::DEFAULT,
    )
    .map_err(|err| format!("Failed to run pass init: {err}"))?;
    if output.status.success() {
        Ok(())
    } else {
        Err(command_error(&output, "pass init failed"))
    }
}

pub fn read_password_entry(store_root: &str, label: &str) -> Result<String, String> {
    #[cfg(feature = "flatpak")]
    {
        return read_password_entry_ripasso(store_root, label);
    }

    #[cfg(all(feature = "setup", not(feature = "flatpak")))]
    {
        if use_ripasso_backend() {
            read_password_entry_ripasso(store_root, label)
        } else {
            read_password_entry_command(store_root, label)
        }
    }
}

pub fn read_password_line(store_root: &str, label: &str) -> Result<String, String> {
    #[cfg(feature = "flatpak")]
    {
        return read_password_line_ripasso(store_root, label);
    }

    #[cfg(all(feature = "setup", not(feature = "flatpak")))]
    {
        if use_ripasso_backend() {
            read_password_line_ripasso(store_root, label)
        } else {
            read_password_line_command(store_root, label)
        }
    }
}

pub fn read_otp_code(store_root: &str, label: &str) -> Result<String, String> {
    #[cfg(feature = "flatpak")]
    {
        return read_otp_code_ripasso(store_root, label);
    }

    #[cfg(all(feature = "setup", not(feature = "flatpak")))]
    {
        if use_ripasso_backend() {
            read_otp_code_ripasso(store_root, label)
        } else {
            read_otp_code_command(store_root, label)
        }
    }
}

pub fn save_password_entry(
    store_root: &str,
    label: &str,
    contents: &str,
    overwrite: bool,
) -> Result<(), String> {
    #[cfg(feature = "flatpak")]
    {
        return save_password_entry_ripasso(store_root, label, contents, overwrite);
    }

    #[cfg(all(feature = "setup", not(feature = "flatpak")))]
    {
        if use_ripasso_backend() {
            save_password_entry_ripasso(store_root, label, contents, overwrite)
        } else {
            save_password_entry_command(store_root, label, contents, overwrite)
        }
    }
}

pub fn rename_password_entry(
    store_root: &str,
    old_label: &str,
    new_label: &str,
) -> Result<(), String> {
    #[cfg(feature = "flatpak")]
    {
        return rename_password_entry_ripasso(store_root, old_label, new_label);
    }

    #[cfg(all(feature = "setup", not(feature = "flatpak")))]
    {
        if use_ripasso_backend() {
            rename_password_entry_ripasso(store_root, old_label, new_label)
        } else {
            rename_password_entry_command(store_root, old_label, new_label)
        }
    }
}

pub fn delete_password_entry(store_root: &str, label: &str) -> Result<(), String> {
    #[cfg(feature = "flatpak")]
    {
        return delete_password_entry_ripasso(store_root, label);
    }

    #[cfg(all(feature = "setup", not(feature = "flatpak")))]
    {
        if use_ripasso_backend() {
            delete_password_entry_ripasso(store_root, label)
        } else {
            delete_password_entry_command(store_root, label)
        }
    }
}

pub fn save_store_recipients(store_root: &str, recipients: &[String]) -> Result<(), String> {
    #[cfg(feature = "flatpak")]
    {
        return save_store_recipients_ripasso(store_root, recipients);
    }

    #[cfg(all(feature = "setup", not(feature = "flatpak")))]
    {
        if use_ripasso_backend() {
            save_store_recipients_ripasso(store_root, recipients)
        } else {
            save_store_recipients_command(store_root, recipients)
        }
    }
}

#[cfg(all(test, feature = "flatpak"))]
mod tests {
    use super::parse_managed_private_key_bytes;
    use sequoia_openpgp::{
        cert::CertBuilder,
        serialize::Serialize,
    };

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
        assert!(err.contains("does not include a private key"));
    }
}
