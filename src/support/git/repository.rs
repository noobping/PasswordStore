use super::command::git_command_error;
use crate::logging::{run_command_output, CommandLogOptions};
use crate::preferences::Preferences;
use crate::support::runtime::{has_host_permission, supports_host_command_features};
use std::path::Path;

pub fn has_git_repository(root: &str) -> bool {
    Path::new(root).join(".git").exists()
}

pub fn ensure_store_git_repository(root: &str) -> Result<(), String> {
    if has_git_repository(root) || !supports_host_command_features() {
        return Ok(());
    }

    let mut cmd = Preferences::git_command();
    cmd.arg("init").arg(root);
    let output = run_command_output(
        &mut cmd,
        "Initialize password store Git repository",
        CommandLogOptions::DEFAULT,
    )
    .map_err(|err| format!("Failed to run git command: {err}"))?;

    if output.status.success() {
        Ok(())
    } else {
        Err(git_command_error("git init", &output))
    }
}

pub fn password_store_git_state_summary(root: &str) -> String {
    if !has_git_repository(root) {
        return format!(
            "Password store Git state: {root} -> no Git repository detected, local commits disabled, network operations disabled."
        );
    }
    if !supports_host_command_features() {
        return format!(
            "Password store Git state: {root} -> Git repository detected, but Git commands are disabled in this build."
        );
    }
    if has_host_permission() {
        return format!(
            "Password store Git state: {root} -> Git repository detected, local commits enabled, network operations enabled."
        );
    }

    format!(
        "Password store Git state: {root} -> Git repository detected, local commits enabled, remote sync disabled in this backend."
    )
}
