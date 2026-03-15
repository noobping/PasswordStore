use crate::logging::{run_command_output, CommandLogOptions};
use crate::preferences::Preferences;
use crate::support::runtime::git_network_operations_available;
use std::path::Path;
use std::process::{Command, Output};

pub fn has_git_repository(root: &str) -> bool {
    Path::new(root).join(".git").exists()
}

fn git_command_error(action: &str, output: &Output) -> String {
    let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
    let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
    if !stderr.is_empty() {
        format!("{action} failed: {stderr}")
    } else if !stdout.is_empty() {
        format!("{action} failed: {stdout}")
    } else {
        format!("{action} failed: {}", output.status)
    }
}

fn run_store_git_command(
    root: &str,
    context: &str,
    configure: impl FnOnce(&mut Command),
) -> Result<Output, String> {
    let mut cmd = Preferences::git_command();
    cmd.arg("-C").arg(root);
    configure(&mut cmd);
    run_command_output(&mut cmd, context, CommandLogOptions::DEFAULT)
        .map_err(|err| format!("Failed to run git command: {err}"))
}

pub fn ensure_store_git_repository(root: &str) -> Result<(), String> {
    if has_git_repository(root) {
        return Ok(());
    }

    let output = run_store_git_command(root, "Initialize password store Git repository", |cmd| {
        cmd.arg("init");
    })?;

    if output.status.success() {
        Ok(())
    } else {
        Err(git_command_error("git init", &output))
    }
}

fn password_store_without_repository_summary(root: &str) -> String {
    format!(
        "Password store Git state: {root} -> no Git repository detected, local commits disabled, network operations disabled."
    )
}

pub fn password_store_git_state_summary(root: &str) -> String {
    if !has_git_repository(root) {
        return password_store_without_repository_summary(root);
    }

    if git_network_operations_available() {
        return password_store_git_state_summary_with_network(root);
    }

    password_store_git_state_summary_without_network(root)
}

fn password_store_git_state_summary_with_network(root: &str) -> String {
    format!(
        "Password store Git state: {root} -> Git repository detected, local commits enabled, network operations enabled."
    )
}

fn password_store_git_state_summary_without_network(root: &str) -> String {
    format!(
        "Password store Git state: {root} -> Git repository detected, local commits enabled, network operations disabled because host execution is unavailable."
    )
}

#[cfg(test)]
mod tests {
    use super::{has_git_repository, password_store_git_state_summary};
    use std::fs;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn temp_dir_path(name: &str) -> std::path::PathBuf {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system time before unix epoch")
            .as_nanos();
        std::env::temp_dir().join(format!("passwordstore-git-{name}-{nanos}"))
    }

    #[test]
    fn git_repository_detection_checks_for_dot_git_metadata() {
        let git_store = temp_dir_path("git");
        let plain_store = temp_dir_path("plain");
        fs::create_dir_all(git_store.join(".git")).expect("create git metadata");
        fs::create_dir_all(&plain_store).expect("create plain store");

        assert!(has_git_repository(git_store.to_string_lossy().as_ref()));
        assert!(!has_git_repository(plain_store.to_string_lossy().as_ref()));

        let _ = fs::remove_dir_all(&git_store);
        let _ = fs::remove_dir_all(&plain_store);
    }

    #[test]
    fn plain_store_summary_reports_git_disabled() {
        let plain_store = temp_dir_path("plain-summary");
        fs::create_dir_all(&plain_store).expect("create plain store");

        let summary = password_store_git_state_summary(plain_store.to_string_lossy().as_ref());

        assert!(summary.contains("no Git repository detected"));
        assert!(summary.contains("local commits disabled"));

        let _ = fs::remove_dir_all(&plain_store);
    }
}
