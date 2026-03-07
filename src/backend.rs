use ripasso::crypto::CryptoImpl;
use ripasso::pass::PasswordStore;
#[cfg(all(feature = "setup", not(feature = "flatpak")))]
use crate::logging::{run_command_output, run_command_with_input, CommandLogOptions};
#[cfg(all(feature = "setup", not(feature = "flatpak")))]
use crate::preferences::Preferences;
use std::env;
use std::fs;
use std::path::PathBuf;
#[cfg(all(feature = "setup", not(feature = "flatpak")))]
use std::process::Output;

fn user_home() -> Option<PathBuf> {
    env::var_os("HOME").map(PathBuf::from)
}

fn open_store(store_root: &str) -> Result<PasswordStore, String> {
    PasswordStore::new(
        "default",
        &Some(PathBuf::from(store_root)),
        &None,
        &user_home(),
        &None,
        &CryptoImpl::GpgMe,
        &None,
    )
    .map_err(|err| err.to_string())
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
