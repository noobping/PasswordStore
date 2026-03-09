use crate::logging::{run_command_output, run_command_with_input, CommandLogOptions};
use crate::preferences::Preferences;
use std::process::{Command, Output};

fn command_error(output: &Output, prefix: &str) -> String {
    let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
    if stderr.is_empty() {
        format!("{prefix}: {}", output.status)
    } else {
        stderr
    }
}

fn store_command(store_root: &str) -> Command {
    let settings = Preferences::new();
    let mut cmd = settings.command();
    cmd.env("PASSWORD_STORE_DIR", store_root);
    cmd
}

fn run_store_command_output(
    store_root: &str,
    action: &str,
    log_options: CommandLogOptions,
    configure: impl FnOnce(&mut Command),
) -> Result<Output, String> {
    let mut cmd = store_command(store_root);
    configure(&mut cmd);
    run_command_output(&mut cmd, action, log_options)
        .map_err(|err| format!("Failed to run the host command: {err}"))
}

fn run_store_command_with_input(
    store_root: &str,
    action: &str,
    input: &str,
    log_options: CommandLogOptions,
    configure: impl FnOnce(&mut Command),
) -> Result<Output, String> {
    let mut cmd = store_command(store_root);
    configure(&mut cmd);
    run_command_with_input(&mut cmd, action, input, log_options)
}

fn ensure_success(output: Output, prefix: &str) -> Result<Output, String> {
    if output.status.success() {
        Ok(output)
    } else {
        Err(command_error(&output, prefix))
    }
}

pub(super) fn read_password_entry(store_root: &str, label: &str) -> Result<String, String> {
    let output = run_store_command_output(
        store_root,
        "Read password entry",
        CommandLogOptions::SENSITIVE,
        |cmd| {
            cmd.arg(label);
        },
    )?;
    let output = ensure_success(output, "pass failed")?;
    Ok(String::from_utf8_lossy(&output.stdout).to_string())
}

pub(super) fn read_password_line(store_root: &str, label: &str) -> Result<String, String> {
    let output = run_store_command_output(
        store_root,
        "Read password entry for clipboard copy",
        CommandLogOptions::SENSITIVE,
        |cmd| {
            cmd.arg(label);
        },
    )?;
    let output = ensure_success(output, "pass failed")?;
    Ok(String::from_utf8_lossy(&output.stdout)
        .lines()
        .next()
        .unwrap_or_default()
        .to_string())
}

pub(super) fn read_otp_code(store_root: &str, label: &str) -> Result<String, String> {
    let output = run_store_command_output(
        store_root,
        "Read OTP code",
        CommandLogOptions::SENSITIVE,
        |cmd| {
            cmd.args(["otp", label]);
        },
    )?;
    let output = ensure_success(output, "pass otp failed")?;
    Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
}

pub(super) fn save_password_entry(
    store_root: &str,
    label: &str,
    contents: &str,
    overwrite: bool,
) -> Result<(), String> {
    let output = run_store_command_with_input(
        store_root,
        "Save password entry",
        contents,
        CommandLogOptions::SENSITIVE,
        |cmd| {
            cmd.arg("insert").arg("-m");
            if overwrite {
                cmd.arg("-f");
            }
            cmd.arg(label);
        },
    )?;
    ensure_success(output, "pass insert failed").map(|_| ())
}

pub(super) fn rename_password_entry(
    store_root: &str,
    old_label: &str,
    new_label: &str,
) -> Result<(), String> {
    let output = run_store_command_output(
        store_root,
        "Rename password entry",
        CommandLogOptions::DEFAULT,
        |cmd| {
            cmd.arg("mv").arg(old_label).arg(new_label);
        },
    )?;
    ensure_success(output, "pass mv failed").map(|_| ())
}

pub(super) fn delete_password_entry(store_root: &str, label: &str) -> Result<(), String> {
    let output = run_store_command_output(
        store_root,
        "Delete password entry",
        CommandLogOptions::DEFAULT,
        |cmd| {
            cmd.arg("rm").arg("-rf").arg(label);
        },
    )?;
    ensure_success(output, "pass rm failed").map(|_| ())
}

pub(super) fn save_store_recipients(
    store_root: &str,
    recipients: &[String],
) -> Result<(), String> {
    let output = run_store_command_output(
        store_root,
        "Save password store recipients",
        CommandLogOptions::DEFAULT,
        |cmd| {
            cmd.arg("init").args(recipients);
        },
    )?;
    ensure_success(output, "pass init failed").map(|_| ())
}
