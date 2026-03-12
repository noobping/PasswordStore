use crate::logging::{log_error, run_command_output, CommandLogOptions};
use crate::preferences::Preferences;
use crate::support::git::has_git_repository;
use std::process::{Command, Output};

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
    store_root: &str,
    context: &str,
    configure: impl FnOnce(&mut Command),
) -> Result<Output, String> {
    let settings = Preferences::new();
    let mut cmd = settings.git_command();
    cmd.arg("-C").arg(store_root);
    configure(&mut cmd);
    run_command_output(&mut cmd, context, CommandLogOptions::DEFAULT)
        .map_err(|err| format!("Failed to run git command: {err}"))
}

fn stage_git_paths(store_root: &str, paths: &[String]) -> Result<(), String> {
    let output = run_store_git_command(store_root, "Stage password store Git changes", |cmd| {
        cmd.arg("add").arg("-A").arg("--").args(paths);
    })?;
    if output.status.success() {
        Ok(())
    } else {
        Err(git_command_error("git add", &output))
    }
}

fn staged_git_paths_have_changes(store_root: &str, paths: &[String]) -> Result<bool, String> {
    let output = run_store_git_command(
        store_root,
        "Check staged password store Git changes",
        |cmd| {
            cmd.args(["diff", "--cached", "--quiet", "--exit-code", "--"])
                .args(paths);
        },
    )?;
    match output.status.code() {
        Some(0) => Ok(false),
        Some(1) => Ok(true),
        _ => Err(git_command_error("git diff --cached", &output)),
    }
}

fn commit_git_paths(store_root: &str, message: &str, paths: &[String]) -> Result<(), String> {
    if paths.is_empty() || !has_git_repository(store_root) {
        return Ok(());
    }
    stage_git_paths(store_root, paths)?;
    if !staged_git_paths_have_changes(store_root, paths)? {
        return Ok(());
    }

    let output = run_store_git_command(store_root, "Commit password store Git changes", |cmd| {
        cmd.arg("commit")
            .arg("-m")
            .arg(message)
            .arg("--")
            .args(paths);
    })?;
    if output.status.success() {
        Ok(())
    } else {
        Err(git_command_error("git commit", &output))
    }
}

pub(super) fn maybe_commit_git_paths(
    store_root: &str,
    message: &str,
    paths: impl IntoIterator<Item = String>,
) {
    let mut paths = paths.into_iter().collect::<Vec<_>>();
    paths.sort();
    paths.dedup();
    if let Err(err) = commit_git_paths(store_root, message, &paths) {
        log_error(format!(
            "Integrated backend Git commit failed for {store_root}: {err}"
        ));
    }
}
