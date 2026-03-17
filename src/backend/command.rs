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
    Preferences::new().command_with_envs(&[("PASSWORD_STORE_DIR", store_root)])
}

fn host_program_command(program: &str, args: &[&str]) -> Command {
    Preferences::new().host_program_command(program, args)
}

pub(super) fn run_store_command_output(
    store_root: &str,
    action: &str,
    log_options: CommandLogOptions,
    configure: impl FnOnce(&mut Command),
) -> Result<Output, String> {
    let mut cmd = store_command(store_root);
    configure(&mut cmd);
    run_command_output(&mut cmd, action, log_options)
        .map_err(|err| format!("Failed to run the host backend command: {err}"))
}

pub(super) fn run_store_command_with_input(
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

pub(super) fn run_host_program_output(
    program: &str,
    args: &[&str],
    action: &str,
    log_options: CommandLogOptions,
) -> Result<Output, String> {
    let mut cmd = host_program_command(program, args);
    run_command_output(&mut cmd, action, log_options)
        .map_err(|err| format!("Failed to run host program '{program}': {err}"))
}

pub(super) fn run_host_program_with_input(
    program: &str,
    args: &[&str],
    input: &str,
    action: &str,
    log_options: CommandLogOptions,
) -> Result<Output, String> {
    let mut cmd = host_program_command(program, args);
    run_command_with_input(&mut cmd, action, input, log_options)
        .map_err(|err| format!("Failed to run host program '{program}': {err}"))
}

pub(super) fn ensure_success(output: Output, prefix: &str) -> Result<Output, String> {
    if output.status.success() {
        Ok(output)
    } else {
        Err(command_error(&output, prefix))
    }
}
