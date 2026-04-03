use crate::logging::{run_command_output, CommandLogOptions};
use crate::preferences::Preferences;
use crate::support::runtime::require_host_command_features;
use std::path::Path;
use std::process::{Command, Output};

#[derive(Clone, Copy)]
enum GitCommandTransport {
    Local,
    Remote,
}

#[derive(Clone, Copy)]
enum GitCommandScope {
    Repository,
    WorkTree,
}

pub(super) fn git_command_error(action: &str, output: &Output) -> String {
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

pub(super) fn git_output_text(output: &Output) -> Result<String, String> {
    String::from_utf8(output.stdout.clone())
        .map_err(|err| err.to_string())
        .map(|text| text.trim().to_string())
}

pub(super) fn configure_store_git_repo_command(cmd: &mut Command, root: &str) {
    cmd.arg("--git-dir").arg(Path::new(root).join(".git"));
}

fn configure_store_git_work_tree_command(cmd: &mut Command, root: &str) {
    configure_store_git_repo_command(cmd, root);
    cmd.arg("--work-tree").arg(root);
}

fn command_failure_label(transport: GitCommandTransport, scope: GitCommandScope) -> &'static str {
    match (transport, scope) {
        (GitCommandTransport::Local, GitCommandScope::Repository) => "git command",
        (GitCommandTransport::Remote, GitCommandScope::Repository) => "remote git command",
        (GitCommandTransport::Local, GitCommandScope::WorkTree) => "work-tree git command",
        (GitCommandTransport::Remote, GitCommandScope::WorkTree) => "remote work-tree git command",
    }
}

fn run_store_git_command_with(
    root: &str,
    context: &str,
    transport: GitCommandTransport,
    scope: GitCommandScope,
    configure: impl FnOnce(&mut Command),
    options: CommandLogOptions,
) -> Result<Output, String> {
    require_host_command_features()?;
    let mut cmd = match transport {
        GitCommandTransport::Local => Preferences::git_command(),
        GitCommandTransport::Remote => Preferences::remote_git_command(),
    };
    match scope {
        GitCommandScope::Repository => configure_store_git_repo_command(&mut cmd, root),
        GitCommandScope::WorkTree => configure_store_git_work_tree_command(&mut cmd, root),
    }
    configure(&mut cmd);
    run_command_output(&mut cmd, context, options).map_err(|err| {
        format!(
            "Failed to run {}: {err}",
            command_failure_label(transport, scope)
        )
    })
}

pub(super) fn run_store_git_command(
    root: &str,
    context: &str,
    configure: impl FnOnce(&mut Command),
    options: CommandLogOptions,
) -> Result<Output, String> {
    run_store_git_command_with(
        root,
        context,
        GitCommandTransport::Local,
        GitCommandScope::Repository,
        configure,
        options,
    )
}

pub(super) fn run_store_remote_git_command(
    root: &str,
    context: &str,
    configure: impl FnOnce(&mut Command),
    options: CommandLogOptions,
) -> Result<Output, String> {
    run_store_git_command_with(
        root,
        context,
        GitCommandTransport::Remote,
        GitCommandScope::Repository,
        configure,
        options,
    )
}

pub(super) fn run_store_git_work_tree_command(
    root: &str,
    context: &str,
    configure: impl FnOnce(&mut Command),
    options: CommandLogOptions,
) -> Result<Output, String> {
    run_store_git_command_with(
        root,
        context,
        GitCommandTransport::Local,
        GitCommandScope::WorkTree,
        configure,
        options,
    )
}

pub(super) fn run_store_remote_git_work_tree_command(
    root: &str,
    context: &str,
    configure: impl FnOnce(&mut Command),
    options: CommandLogOptions,
) -> Result<Output, String> {
    run_store_git_command_with(
        root,
        context,
        GitCommandTransport::Remote,
        GitCommandScope::WorkTree,
        configure,
        options,
    )
}
