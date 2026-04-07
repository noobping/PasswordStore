use super::command::git_command_error;
use crate::logging::{run_command_output, CommandLogOptions};
use crate::preferences::Preferences;
use crate::support::runtime::{has_host_permission, supports_host_command_features};
use std::path::Path;
#[cfg(any(test, not(feature = "flatpak")))]
use std::process::Stdio;
#[cfg(not(feature = "flatpak"))]
use std::sync::OnceLock;

pub fn has_git_repository(root: &str) -> bool {
    Path::new(root).join(".git").exists()
}

#[cfg(feature = "flatpak")]
pub fn git_command_available() -> bool {
    return true;
}

#[cfg(not(feature = "flatpak"))]
pub fn git_command_available() -> bool {
    static AVAILABLE: OnceLock<bool> = OnceLock::new();

    *AVAILABLE.get_or_init(|| git_command_available_with(Preferences::git_command))
}

#[cfg(any(test, not(feature = "flatpak")))]
fn git_command_available_with(build: impl FnOnce() -> std::process::Command) -> bool {
    if !supports_host_command_features() {
        return false;
    }

    let mut cmd = build();
    cmd.arg("--version")
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    cmd.status().is_ok_and(|status| status.success())
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

#[cfg(test)]
mod tests {
    use super::git_command_available_with;
    use std::process::Command;

    #[test]
    fn git_command_probe_accepts_successful_commands() {
        assert!(git_command_available_with(|| Command::new("true")));
    }

    #[test]
    fn git_command_probe_rejects_failing_commands() {
        assert!(!git_command_available_with(|| Command::new("false")));
    }

    #[test]
    fn git_command_probe_rejects_missing_commands() {
        assert!(!git_command_available_with(|| {
            Command::new("keycord-command-that-does-not-exist")
        }));
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
