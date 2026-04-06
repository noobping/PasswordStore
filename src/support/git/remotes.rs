use super::command::{git_command_error, git_output_text, run_store_git_command};
use super::repository::{ensure_store_git_repository, has_git_repository};
use super::types::GitRemote;
use crate::logging::CommandLogOptions;
use crate::support::runtime::{require_host_command_features, supports_host_command_features};

pub fn list_store_git_remotes(root: &str) -> Result<Vec<GitRemote>, String> {
    if !has_git_repository(root) || !supports_host_command_features() {
        return Ok(Vec::new());
    }

    let output = run_store_git_command(
        root,
        "List password store Git remotes",
        |cmd| {
            cmd.arg("remote");
        },
        CommandLogOptions::DEFAULT,
    )?;
    if !output.status.success() {
        return Err(git_command_error("git remote", &output));
    }

    let names = git_output_text(&output)?;
    names
        .lines()
        .filter(|line| !line.trim().is_empty())
        .map(|name| {
            let output = run_store_git_command(
                root,
                &format!("Read password store Git remote URL for {name}"),
                |cmd| {
                    cmd.args(["remote", "get-url", name]);
                },
                CommandLogOptions::DEFAULT,
            )?;
            if !output.status.success() {
                return Err(git_command_error("git remote get-url", &output));
            }

            Ok(GitRemote {
                name: name.to_string(),
                url: git_output_text(&output)?,
            })
        })
        .collect()
}

pub fn add_store_git_remote(root: &str, name: &str, url: &str) -> Result<(), String> {
    require_host_command_features()?;
    ensure_store_git_repository(root)?;
    let output = run_store_git_command(
        root,
        "Add password store Git remote",
        |cmd| {
            cmd.args(["remote", "add", name, url]);
        },
        CommandLogOptions::DEFAULT,
    )?;
    if output.status.success() {
        Ok(())
    } else {
        Err(git_command_error("git remote add", &output))
    }
}

pub fn rename_store_git_remote(
    root: &str,
    current_name: &str,
    new_name: &str,
) -> Result<(), String> {
    require_host_command_features()?;
    let output = run_store_git_command(
        root,
        "Rename password store Git remote",
        |cmd| {
            cmd.args(["remote", "rename", current_name, new_name]);
        },
        CommandLogOptions::DEFAULT,
    )?;
    if output.status.success() {
        Ok(())
    } else {
        Err(git_command_error("git remote rename", &output))
    }
}

pub fn set_store_git_remote_url(root: &str, name: &str, url: &str) -> Result<(), String> {
    require_host_command_features()?;
    let output = run_store_git_command(
        root,
        "Update password store Git remote URL",
        |cmd| {
            cmd.args(["remote", "set-url", name, url]);
        },
        CommandLogOptions::DEFAULT,
    )?;
    if output.status.success() {
        Ok(())
    } else {
        Err(git_command_error("git remote set-url", &output))
    }
}

pub fn remove_store_git_remote(root: &str, name: &str) -> Result<(), String> {
    require_host_command_features()?;
    let output = run_store_git_command(
        root,
        "Remove password store Git remote",
        |cmd| {
            cmd.args(["remote", "remove", name]);
        },
        CommandLogOptions::DEFAULT,
    )?;
    if output.status.success() {
        Ok(())
    } else {
        Err(git_command_error("git remote remove", &output))
    }
}
