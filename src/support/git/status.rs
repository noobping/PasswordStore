use super::command::{
    git_command_error, git_output_text, run_store_git_command, run_store_git_work_tree_command,
    run_store_remote_git_work_tree_command,
};
use super::remotes::list_store_git_remotes;
use super::repository::has_git_repository;
use super::types::{GitRemote, StoreGitHead, StoreGitRepositoryStatus};
use crate::logging::{log_error, CommandLogOptions};
use crate::preferences::Preferences;
use crate::support::runtime::supports_host_command_features;
use std::process::{Command, Output};

fn empty_git_status(has_repository: bool) -> StoreGitRepositoryStatus {
    StoreGitRepositoryStatus {
        has_repository,
        head: StoreGitHead::Detached,
        dirty: false,
        has_outgoing_commits: false,
        has_incoming_commits: false,
        remotes: Vec::new(),
    }
}

fn head_has_commit(root: &str) -> Result<bool, String> {
    let output = run_store_git_command(
        root,
        "Inspect password store Git HEAD",
        |cmd| {
            cmd.args(["rev-parse", "-q", "--verify", "HEAD^{commit}"]);
        },
        CommandLogOptions {
            accepted_exit_codes: &[1],
            ..CommandLogOptions::DEFAULT
        },
    )?;

    match output.status.code() {
        Some(0) => Ok(true),
        Some(1) => Ok(false),
        _ => Err(git_command_error(
            "git rev-parse -q --verify HEAD^{commit}",
            &output,
        )),
    }
}

fn symbolic_head_branch(root: &str) -> Result<Option<String>, String> {
    let output = run_store_git_command(
        root,
        "Inspect password store Git branch",
        |cmd| {
            cmd.args(["symbolic-ref", "--quiet", "--short", "HEAD"]);
        },
        CommandLogOptions {
            accepted_exit_codes: &[1],
            ..CommandLogOptions::DEFAULT
        },
    )?;

    match output.status.code() {
        Some(0) => {
            let branch = git_output_text(&output)?;
            if branch.is_empty() {
                Ok(None)
            } else {
                Ok(Some(branch))
            }
        }
        Some(1) => Ok(None),
        _ => Err(git_command_error(
            "git symbolic-ref --quiet --short HEAD",
            &output,
        )),
    }
}

fn working_tree_is_dirty(root: &str, has_commit: bool) -> Result<bool, String> {
    let local_dirty = working_tree_is_dirty_with(root, has_commit, false)?;
    if !Preferences::new().uses_host_command_backend() {
        return Ok(local_dirty);
    }

    match working_tree_is_dirty_with(root, has_commit, true) {
        Ok(host_dirty) => Ok(host_dirty),
        Err(err) => {
            log_error(format!(
                "Failed to inspect host Git work-tree state for '{root}', falling back to local Git state: {err}"
            ));
            Ok(local_dirty)
        }
    }
}

fn working_tree_is_dirty_with(
    root: &str,
    has_commit: bool,
    use_host_git: bool,
) -> Result<bool, String> {
    if !has_commit {
        return files_exist_in_work_tree_with(
            || {
                run_git_work_tree_command(
                    root,
                    use_host_git,
                    "Inspect password store Git files before the first commit",
                    |cmd| {
                        cmd.args(["ls-files", "--cached", "--others", "--exclude-standard"]);
                    },
                )
            },
            "git ls-files --cached --others --exclude-standard",
        );
    }

    let tracked_changes = tracked_changes_are_present_with(
        || {
            run_git_work_tree_command(
                root,
                use_host_git,
                "Inspect password store Git tracked changes",
                |cmd| {
                    cmd.args(["diff", "--quiet", "HEAD", "--"]);
                },
            )
        },
        "git diff --quiet HEAD --",
    )?;
    let untracked_files = files_exist_in_work_tree_with(
        || {
            run_git_work_tree_command(
                root,
                use_host_git,
                "Inspect password store Git untracked files",
                |cmd| {
                    cmd.args(["ls-files", "--others", "--exclude-standard"]);
                },
            )
        },
        "git ls-files --others --exclude-standard",
    )?;

    Ok(tracked_changes || untracked_files)
}

fn run_git_work_tree_command(
    root: &str,
    use_host_git: bool,
    context: &str,
    configure: impl FnOnce(&mut Command),
) -> Result<Output, String> {
    if use_host_git {
        run_store_remote_git_work_tree_command(root, context, configure, CommandLogOptions::DEFAULT)
    } else {
        run_store_git_work_tree_command(root, context, configure, CommandLogOptions::DEFAULT)
    }
}

fn tracked_changes_are_present_with(
    run_diff: impl FnOnce() -> Result<Output, String>,
    action: &str,
) -> Result<bool, String> {
    let output = run_diff()?;
    match output.status.code() {
        Some(0) => Ok(false),
        Some(1) => Ok(true),
        _ => Err(git_command_error(action, &output)),
    }
}

fn files_exist_in_work_tree_with(
    run_list: impl FnOnce() -> Result<Output, String>,
    action: &str,
) -> Result<bool, String> {
    let output = run_list()?;
    if !output.status.success() {
        return Err(git_command_error(action, &output));
    }

    Ok(!git_output_text(&output)?.is_empty())
}

pub fn store_git_repository_status(root: &str) -> Result<StoreGitRepositoryStatus, String> {
    if !has_git_repository(root) {
        return Ok(empty_git_status(false));
    }
    if !supports_host_command_features() {
        return Ok(empty_git_status(true));
    }

    let branch = symbolic_head_branch(root)?;
    let has_commit = head_has_commit(root)?;
    let dirty = working_tree_is_dirty(root, has_commit)?;
    let remotes = list_store_git_remotes(root)?;
    let (has_outgoing_commits, has_incoming_commits) =
        branch_sync_state(root, branch.as_deref(), has_commit, &remotes)?;
    let head = match branch {
        Some(branch) if has_commit => StoreGitHead::Branch(branch),
        Some(branch) => StoreGitHead::UnbornBranch(branch),
        None => StoreGitHead::Detached,
    };

    Ok(StoreGitRepositoryStatus {
        has_repository: true,
        head,
        dirty,
        has_outgoing_commits,
        has_incoming_commits,
        remotes,
    })
}

fn branch_sync_state(
    root: &str,
    branch: Option<&str>,
    has_commit: bool,
    remotes: &[GitRemote],
) -> Result<(bool, bool), String> {
    let Some(branch) = branch else {
        return Ok((false, false));
    };
    if !has_commit || remotes.is_empty() {
        return Ok((false, false));
    }

    let mut has_outgoing_commits = false;
    let mut has_incoming_commits = false;

    for remote in remotes {
        let remote_ref = format!("refs/remotes/{}/{}", remote.name, branch);
        if !remote_branch_exists(root, &remote.name, branch)? {
            has_outgoing_commits = true;
            continue;
        }

        if ref_has_unique_commits(root, &remote_ref, "HEAD")? {
            has_outgoing_commits = true;
        }
        if ref_has_unique_commits(root, "HEAD", &remote_ref)? {
            has_incoming_commits = true;
        }
    }

    Ok((has_outgoing_commits, has_incoming_commits))
}

fn ref_has_unique_commits(root: &str, from: &str, to: &str) -> Result<bool, String> {
    let range = format!("{from}..{to}");
    let output = run_store_git_command(
        root,
        &format!("Inspect password store Git revision range {range}"),
        |cmd| {
            cmd.args(["rev-list", "--count", &range]);
        },
        CommandLogOptions::DEFAULT,
    )?;
    if !output.status.success() {
        return Err(git_command_error("git rev-list --count", &output));
    }

    let count = git_output_text(&output)?;
    count
        .parse::<u64>()
        .map(|count| count > 0)
        .map_err(|err| err.to_string())
}

pub(super) fn remote_branch_exists(root: &str, remote: &str, branch: &str) -> Result<bool, String> {
    let reference = format!("refs/remotes/{remote}/{branch}^{{commit}}");
    let output = run_store_git_command(
        root,
        &format!("Inspect password store Git remote branch {remote}/{branch}"),
        |cmd| {
            cmd.args(["rev-parse", "-q", "--verify", &reference]);
        },
        CommandLogOptions {
            accepted_exit_codes: &[1],
            ..CommandLogOptions::DEFAULT
        },
    )?;

    match output.status.code() {
        Some(0) => Ok(true),
        Some(1) => Ok(false),
        _ => Err(git_command_error(
            "git rev-parse -q --verify refs/remotes/<remote>/<branch>^{commit}",
            &output,
        )),
    }
}
