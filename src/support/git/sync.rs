use super::command::{
    git_command_error, run_store_git_work_tree_command, run_store_remote_git_command,
};
use super::status::{remote_branch_exists, store_git_repository_status};
use super::types::{StoreGitHead, StoreGitRepositoryStatus};
use crate::logging::{log_error, CommandLogOptions};
use crate::support::runtime::require_host_command_features;

pub(super) fn sync_blocked_by_local_state(status: &StoreGitRepositoryStatus) -> Option<String> {
    if status.dirty && status.has_outgoing_commits && status.has_incoming_commits {
        return Some(
            "Commit or discard local changes before syncing this store. Local and remote commits are also waiting to sync."
                .to_string(),
        );
    }
    if status.dirty && status.has_outgoing_commits {
        return Some(
            "Commit or discard local changes before syncing this store. Local commits are also waiting to sync."
                .to_string(),
        );
    }
    if status.dirty && status.has_incoming_commits {
        return Some(
            "Commit or discard local changes before syncing this store. Remote commits are also waiting to sync."
                .to_string(),
        );
    }
    if status.dirty {
        return Some("Commit or discard local changes before syncing this store.".to_string());
    }

    None
}

fn fetch_store_git_remote(root: &str, remote: &str) -> Result<(), String> {
    let output = run_store_remote_git_command(
        root,
        &format!("Fetch password store Git remote {remote}"),
        |cmd| {
            cmd.args(["fetch", "--prune", remote]);
        },
        CommandLogOptions::DEFAULT,
    )?;
    if output.status.success() {
        Ok(())
    } else {
        Err(git_command_error("git fetch --prune", &output))
    }
}

fn abort_store_git_merge(root: &str) {
    let output = run_store_git_work_tree_command(
        root,
        "Abort password store Git merge",
        |cmd| {
            cmd.args(["merge", "--abort"]);
        },
        CommandLogOptions::DEFAULT,
    );

    match output {
        Ok(output) if output.status.success() => {}
        Ok(output) => {
            log_error(format!(
                "Failed to abort password store merge for {root}: {}",
                git_command_error("git merge --abort", &output)
            ));
        }
        Err(err) => {
            log_error(format!(
                "Failed to abort password store merge for {root}: {err}"
            ));
        }
    }
}

fn merge_store_git_remote_branch(root: &str, remote: &str, branch: &str) -> Result<(), String> {
    if !remote_branch_exists(root, remote, branch)? {
        return Ok(());
    }

    let target = format!("{remote}/{branch}");
    let output = run_store_git_work_tree_command(
        root,
        &format!("Merge password store Git branch {target}"),
        |cmd| {
            cmd.args(["merge", "--no-edit", &target]);
        },
        CommandLogOptions {
            accepted_exit_codes: &[1],
            ..CommandLogOptions::DEFAULT
        },
    )?;
    if output.status.success() {
        return Ok(());
    }

    abort_store_git_merge(root);
    Err(git_command_error("git merge --no-edit", &output))
}

fn push_store_git_remote_branch(root: &str, remote: &str, branch: &str) -> Result<(), String> {
    let refspec = format!("HEAD:refs/heads/{branch}");
    let output = run_store_remote_git_command(
        root,
        &format!("Push password store Git branch {branch} to {remote}"),
        |cmd| {
            cmd.args(["push", remote, &refspec]);
        },
        CommandLogOptions::DEFAULT,
    )?;
    if output.status.success() {
        Ok(())
    } else {
        Err(git_command_error("git push", &output))
    }
}

pub fn sync_store_repository(root: &str) -> Result<(), String> {
    require_host_command_features()?;
    let status = store_git_repository_status(root)?;
    if !status.has_repository || status.remotes.is_empty() {
        return Ok(());
    }
    if let Some(reason) = sync_blocked_by_local_state(&status) {
        return Err(reason);
    }

    let branch = match status.head {
        StoreGitHead::Branch(branch) => branch,
        StoreGitHead::UnbornBranch(branch) => {
            return Err(format!(
                "Make an initial commit on '{branch}' before syncing this store."
            ));
        }
        StoreGitHead::Detached => {
            return Err("Check out a branch before syncing this store.".to_string());
        }
    };

    for remote in &status.remotes {
        fetch_store_git_remote(root, &remote.name)?;
    }
    for remote in &status.remotes {
        merge_store_git_remote_branch(root, &remote.name, &branch)?;
    }
    for remote in &status.remotes {
        push_store_git_remote_branch(root, &remote.name, &branch)?;
    }

    Ok(())
}
