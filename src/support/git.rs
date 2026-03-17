use crate::logging::{log_error, run_command_output, CommandLogOptions};
use crate::preferences::Preferences;
use crate::support::runtime::has_host_permission;
use std::path::Path;
use std::process::{Command, Output};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GitRemote {
    pub name: String,
    pub url: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum StoreGitHead {
    Branch(String),
    UnbornBranch(String),
    Detached,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StoreGitRepositoryStatus {
    pub has_repository: bool,
    pub head: StoreGitHead,
    pub dirty: bool,
    pub remotes: Vec<GitRemote>,
}

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
    options: CommandLogOptions,
) -> Result<Output, String> {
    let mut cmd = Preferences::git_command();
    cmd.arg("-C").arg(root);
    configure(&mut cmd);
    run_command_output(&mut cmd, context, options)
        .map_err(|err| format!("Failed to run git command: {err}"))
}

pub fn ensure_store_git_repository(root: &str) -> Result<(), String> {
    if has_git_repository(root) {
        return Ok(());
    }

    let output = run_store_git_command(
        root,
        "Initialize password store Git repository",
        |cmd| {
            cmd.arg("init");
        },
        CommandLogOptions::DEFAULT,
    )?;

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

    if has_host_permission() {
        return format!(
            "Password store Git state: {root} -> Git repository detected, local commits enabled, network operations enabled."
        );
    }

    format!(
        "Password store Git state: {root} -> Git repository detected, local commits enabled, remote sync disabled in this backend."
    )
}

fn git_output_text(output: &Output) -> Result<String, String> {
    String::from_utf8(output.stdout.clone())
        .map_err(|err| err.to_string())
        .map(|text| text.trim().to_string())
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

fn working_tree_is_dirty(root: &str) -> Result<bool, String> {
    let output = run_store_git_command(
        root,
        "Inspect password store Git status",
        |cmd| {
            cmd.args(["status", "--short"]);
        },
        CommandLogOptions::DEFAULT,
    )?;
    if !output.status.success() {
        return Err(git_command_error("git status --short", &output));
    }

    Ok(!git_output_text(&output)?.is_empty())
}

pub fn list_store_git_remotes(root: &str) -> Result<Vec<GitRemote>, String> {
    if !has_git_repository(root) {
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

    let mut remotes = Vec::new();
    let names = git_output_text(&output)?;
    for name in names.lines().filter(|line| !line.trim().is_empty()) {
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

        remotes.push(GitRemote {
            name: name.to_string(),
            url: git_output_text(&output)?,
        });
    }

    Ok(remotes)
}

pub fn store_git_repository_status(root: &str) -> Result<StoreGitRepositoryStatus, String> {
    if !has_git_repository(root) {
        return Ok(StoreGitRepositoryStatus {
            has_repository: false,
            head: StoreGitHead::Detached,
            dirty: false,
            remotes: Vec::new(),
        });
    }

    let branch = symbolic_head_branch(root)?;
    let has_commit = head_has_commit(root)?;
    let dirty = working_tree_is_dirty(root)?;
    let remotes = list_store_git_remotes(root)?;
    let head = match branch {
        Some(branch) if has_commit => StoreGitHead::Branch(branch),
        Some(branch) => StoreGitHead::UnbornBranch(branch),
        None => StoreGitHead::Detached,
    };

    Ok(StoreGitRepositoryStatus {
        has_repository: true,
        head,
        dirty,
        remotes,
    })
}

pub fn add_store_git_remote(root: &str, name: &str, url: &str) -> Result<(), String> {
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

fn fetch_store_git_remote(root: &str, remote: &str) -> Result<(), String> {
    let output = run_store_git_command(
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

fn remote_branch_exists(root: &str, remote: &str, branch: &str) -> Result<bool, String> {
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

fn abort_store_git_merge(root: &str) {
    let output = run_store_git_command(
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
    let output = run_store_git_command(
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
    let output = run_store_git_command(
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
    let status = store_git_repository_status(root)?;
    if !status.has_repository || status.remotes.is_empty() {
        return Ok(());
    }
    if status.dirty {
        return Err("Commit or discard local changes before syncing this store.".to_string());
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

#[cfg(test)]
mod tests {
    use super::{
        add_store_git_remote, has_git_repository, list_store_git_remotes,
        password_store_git_state_summary, remove_store_git_remote, rename_store_git_remote,
        set_store_git_remote_url, store_git_repository_status, sync_store_repository, GitRemote,
        StoreGitHead,
    };
    use std::fs::{self, File};
    use std::io::Write;
    use std::path::Path;
    use std::process::{Command, Output};
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

    fn command_error(action: &str, output: &Output) -> String {
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

    fn git(path: &Path, args: &[&str]) -> Result<String, String> {
        let output = Command::new("git")
            .arg("-C")
            .arg(path)
            .args(args)
            .output()
            .map_err(|err| format!("Failed to start git command: {err}"))?;
        if !output.status.success() {
            return Err(command_error("git", &output));
        }

        Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
    }

    fn git_output(path: &Path, args: &[&str]) -> Result<Output, String> {
        Command::new("git")
            .arg("-C")
            .arg(path)
            .args(args)
            .output()
            .map_err(|err| format!("Failed to start git command: {err}"))
    }

    fn init_repo(path: &Path) -> Result<(), String> {
        fs::create_dir_all(path).map_err(|err| err.to_string())?;
        git(path, &["init"])?;
        git(path, &["config", "user.name", "Keycord Tests"])?;
        git(path, &["config", "user.email", "tests@example.com"])?;
        git(path, &["branch", "-M", "main"])?;
        Ok(())
    }

    fn init_bare_repo(path: &Path) -> Result<(), String> {
        fs::create_dir_all(path).map_err(|err| err.to_string())?;
        let output = Command::new("git")
            .arg("init")
            .arg("--bare")
            .arg(path)
            .output()
            .map_err(|err| format!("Failed to start bare git init: {err}"))?;
        if output.status.success() {
            let head_output = Command::new("git")
                .arg("--git-dir")
                .arg(path)
                .args(["symbolic-ref", "HEAD", "refs/heads/main"])
                .output()
                .map_err(|err| format!("Failed to start git symbolic-ref: {err}"))?;
            if !head_output.status.success() {
                return Err(command_error(
                    "git symbolic-ref HEAD refs/heads/main",
                    &head_output,
                ));
            }
            Ok(())
        } else {
            Err(command_error("git init --bare", &output))
        }
    }

    fn write_file(path: &Path, value: &str) -> Result<(), String> {
        let mut file = File::create(path).map_err(|err| err.to_string())?;
        file.write_all(value.as_bytes())
            .map_err(|err| err.to_string())
    }

    fn commit_file(path: &Path, file_name: &str, value: &str, message: &str) -> Result<(), String> {
        write_file(&path.join(file_name), value)?;
        git(path, &["add", file_name])?;
        git(path, &["commit", "-m", message])?;
        Ok(())
    }

    fn clone_repo(source: &Path, target: &Path) -> Result<(), String> {
        let output = Command::new("git")
            .arg("clone")
            .args(["--branch", "main"])
            .arg(source)
            .arg(target)
            .output()
            .map_err(|err| format!("Failed to start git clone: {err}"))?;
        if output.status.success() {
            git(target, &["config", "user.name", "Keycord Tests"])?;
            git(target, &["config", "user.email", "tests@example.com"])?;
            Ok(())
        } else {
            Err(command_error("git clone", &output))
        }
    }

    fn head_oid(path: &Path) -> Result<String, String> {
        git(path, &["rev-parse", "HEAD"])
    }

    fn branch_head_oid(path: &Path, branch: &str) -> Result<String, String> {
        let output = Command::new("git")
            .arg("--git-dir")
            .arg(path)
            .args(["rev-parse", branch])
            .output()
            .map_err(|err| format!("Failed to start git rev-parse: {err}"))?;
        if output.status.success() {
            Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
        } else {
            Err(command_error("git rev-parse", &output))
        }
    }

    #[test]
    fn git_remote_crud_preserves_existing_history() {
        let repo = temp_dir_path("remote-crud");
        let remote_a = temp_dir_path("remote-a.git");
        let remote_b = temp_dir_path("remote-b.git");
        init_repo(&repo).expect("initialize repo");
        commit_file(&repo, "secret.txt", "one\n", "Initial commit").expect("create commit");
        init_bare_repo(&remote_a).expect("initialize first bare repo");
        init_bare_repo(&remote_b).expect("initialize second bare repo");
        let head_before = head_oid(&repo).expect("read head before remote edit");

        add_store_git_remote(
            repo.to_string_lossy().as_ref(),
            "origin",
            remote_a.to_string_lossy().as_ref(),
        )
        .expect("add remote");
        assert_eq!(
            list_store_git_remotes(repo.to_string_lossy().as_ref()).expect("list remotes"),
            vec![GitRemote {
                name: "origin".to_string(),
                url: remote_a.to_string_lossy().to_string(),
            }]
        );

        rename_store_git_remote(repo.to_string_lossy().as_ref(), "origin", "backup")
            .expect("rename remote");
        set_store_git_remote_url(
            repo.to_string_lossy().as_ref(),
            "backup",
            remote_b.to_string_lossy().as_ref(),
        )
        .expect("change remote url");
        assert_eq!(
            list_store_git_remotes(repo.to_string_lossy().as_ref()).expect("list remotes"),
            vec![GitRemote {
                name: "backup".to_string(),
                url: remote_b.to_string_lossy().to_string(),
            }]
        );

        remove_store_git_remote(repo.to_string_lossy().as_ref(), "backup").expect("remove remote");
        assert!(list_store_git_remotes(repo.to_string_lossy().as_ref())
            .expect("list remotes after removal")
            .is_empty());
        assert_eq!(
            head_oid(&repo).expect("read head after remote edit"),
            head_before
        );

        let _ = fs::remove_dir_all(&repo);
        let _ = fs::remove_dir_all(&remote_a);
        let _ = fs::remove_dir_all(&remote_b);
    }

    #[test]
    fn git_status_reports_branch_dirty_state_and_remotes() {
        let repo = temp_dir_path("status");
        let remote = temp_dir_path("status-remote.git");
        init_repo(&repo).expect("initialize repo");
        commit_file(&repo, "secret.txt", "one\n", "Initial commit").expect("create commit");
        init_bare_repo(&remote).expect("initialize bare repo");
        add_store_git_remote(
            repo.to_string_lossy().as_ref(),
            "origin",
            remote.to_string_lossy().as_ref(),
        )
        .expect("add remote");
        write_file(&repo.join("dirty.txt"), "pending\n").expect("write untracked file");

        let status =
            store_git_repository_status(repo.to_string_lossy().as_ref()).expect("read git status");
        assert!(status.has_repository);
        assert_eq!(status.head, StoreGitHead::Branch("main".to_string()));
        assert!(status.dirty);
        assert_eq!(
            status.remotes,
            vec![GitRemote {
                name: "origin".to_string(),
                url: remote.to_string_lossy().to_string(),
            }]
        );

        let _ = fs::remove_dir_all(&repo);
        let _ = fs::remove_dir_all(&remote);
    }

    #[test]
    fn sync_store_repository_merges_and_pushes_all_remotes() {
        let repo = temp_dir_path("sync-local");
        let remote_a = temp_dir_path("sync-remote-a.git");
        let remote_b = temp_dir_path("sync-remote-b.git");
        let clone = temp_dir_path("sync-clone");
        init_repo(&repo).expect("initialize repo");
        commit_file(&repo, "secret.txt", "one\n", "Initial commit").expect("create local commit");
        init_bare_repo(&remote_a).expect("initialize first bare repo");
        init_bare_repo(&remote_b).expect("initialize second bare repo");
        add_store_git_remote(
            repo.to_string_lossy().as_ref(),
            "origin",
            remote_a.to_string_lossy().as_ref(),
        )
        .expect("add origin");
        add_store_git_remote(
            repo.to_string_lossy().as_ref(),
            "backup",
            remote_b.to_string_lossy().as_ref(),
        )
        .expect("add backup");
        git(&repo, &["push", "origin", "HEAD:refs/heads/main"]).expect("push main to origin");
        git(&repo, &["push", "backup", "HEAD:refs/heads/main"]).expect("push main to backup");

        clone_repo(&remote_a, &clone).expect("clone remote");
        commit_file(&clone, "secret.txt", "one\nremote\n", "Remote commit")
            .expect("create remote commit");
        git(&clone, &["push", "origin", "HEAD:refs/heads/main"]).expect("push remote commit");

        sync_store_repository(repo.to_string_lossy().as_ref()).expect("sync local repository");

        let local_log = git(&repo, &["log", "--format=%s", "-3"]).expect("read local log");
        assert!(local_log.lines().any(|line| line == "Remote commit"));
        assert_eq!(
            branch_head_oid(&remote_a, "main").expect("read origin head"),
            branch_head_oid(&remote_b, "main").expect("read backup head")
        );

        let _ = fs::remove_dir_all(&repo);
        let _ = fs::remove_dir_all(&remote_a);
        let _ = fs::remove_dir_all(&remote_b);
        let _ = fs::remove_dir_all(&clone);
    }

    #[test]
    fn sync_store_repository_aborts_conflicted_merges() {
        let repo = temp_dir_path("sync-conflict-local");
        let remote = temp_dir_path("sync-conflict-remote.git");
        let clone = temp_dir_path("sync-conflict-clone");
        init_repo(&repo).expect("initialize repo");
        commit_file(&repo, "secret.txt", "one\n", "Initial commit").expect("create initial commit");
        init_bare_repo(&remote).expect("initialize bare repo");
        add_store_git_remote(
            repo.to_string_lossy().as_ref(),
            "origin",
            remote.to_string_lossy().as_ref(),
        )
        .expect("add origin");
        git(&repo, &["push", "origin", "HEAD:refs/heads/main"]).expect("push local branch");

        clone_repo(&remote, &clone).expect("clone remote");
        commit_file(&clone, "secret.txt", "remote\n", "Remote change")
            .expect("create remote change");
        git(&clone, &["push", "origin", "HEAD:refs/heads/main"]).expect("push remote change");

        commit_file(&repo, "secret.txt", "local\n", "Local change").expect("create local change");

        let error =
            sync_store_repository(repo.to_string_lossy().as_ref()).expect_err("sync should fail");
        assert!(error.contains("git merge --no-edit"));
        assert!(
            !repo.join(".git").join("MERGE_HEAD").exists(),
            "merge state should be aborted"
        );
        assert!(git(&repo, &["status", "--short"])
            .expect("read repo status")
            .is_empty());

        let _ = fs::remove_dir_all(&repo);
        let _ = fs::remove_dir_all(&remote);
        let _ = fs::remove_dir_all(&clone);
    }

    #[test]
    fn sync_store_repository_rejects_dirty_worktrees() {
        let repo = temp_dir_path("sync-dirty");
        let remote = temp_dir_path("sync-dirty-remote.git");
        init_repo(&repo).expect("initialize repo");
        commit_file(&repo, "secret.txt", "one\n", "Initial commit").expect("create initial commit");
        init_bare_repo(&remote).expect("initialize bare repo");
        add_store_git_remote(
            repo.to_string_lossy().as_ref(),
            "origin",
            remote.to_string_lossy().as_ref(),
        )
        .expect("add origin");
        write_file(&repo.join("dirty.txt"), "pending\n").expect("write dirty file");

        let error =
            sync_store_repository(repo.to_string_lossy().as_ref()).expect_err("sync should fail");
        assert!(error.contains("Commit or discard local changes"));

        let _ = fs::remove_dir_all(&repo);
        let _ = fs::remove_dir_all(&remote);
    }

    #[test]
    fn sync_store_repository_rejects_detached_head() {
        let repo = temp_dir_path("sync-detached");
        let remote = temp_dir_path("sync-detached-remote.git");
        init_repo(&repo).expect("initialize repo");
        commit_file(&repo, "secret.txt", "one\n", "Initial commit").expect("create initial commit");
        init_bare_repo(&remote).expect("initialize bare repo");
        add_store_git_remote(
            repo.to_string_lossy().as_ref(),
            "origin",
            remote.to_string_lossy().as_ref(),
        )
        .expect("add origin");
        git(&repo, &["checkout", "--detach"]).expect("detach head");

        let error =
            sync_store_repository(repo.to_string_lossy().as_ref()).expect_err("sync should fail");
        assert!(error.contains("Check out a branch"));

        let _ = fs::remove_dir_all(&repo);
        let _ = fs::remove_dir_all(&remote);
    }

    #[test]
    fn add_store_git_remote_initializes_missing_repository() {
        let repo = temp_dir_path("init-on-add");
        let remote = temp_dir_path("init-on-add-remote.git");
        fs::create_dir_all(&repo).expect("create local directory");
        init_bare_repo(&remote).expect("initialize bare repo");

        add_store_git_remote(
            repo.to_string_lossy().as_ref(),
            "origin",
            remote.to_string_lossy().as_ref(),
        )
        .expect("add remote");

        assert!(has_git_repository(repo.to_string_lossy().as_ref()));
        assert_eq!(
            list_store_git_remotes(repo.to_string_lossy().as_ref()).expect("list remotes"),
            vec![GitRemote {
                name: "origin".to_string(),
                url: remote.to_string_lossy().to_string(),
            }]
        );

        let _ = fs::remove_dir_all(&repo);
        let _ = fs::remove_dir_all(&remote);
    }

    #[test]
    fn sync_store_repository_skips_missing_remote_branch() {
        let repo = temp_dir_path("sync-skip-missing-branch");
        let remote = temp_dir_path("sync-skip-missing-branch-remote.git");
        init_repo(&repo).expect("initialize repo");
        commit_file(&repo, "secret.txt", "one\n", "Initial commit").expect("create initial commit");
        init_bare_repo(&remote).expect("initialize bare repo");
        add_store_git_remote(
            repo.to_string_lossy().as_ref(),
            "origin",
            remote.to_string_lossy().as_ref(),
        )
        .expect("add remote");

        sync_store_repository(repo.to_string_lossy().as_ref()).expect("sync local repository");
        let push_output = git_output(
            &repo,
            &[
                "ls-remote",
                "--heads",
                remote.to_string_lossy().as_ref(),
                "main",
            ],
        )
        .expect("run ls-remote");
        assert!(push_output.status.success());
        assert!(!String::from_utf8_lossy(&push_output.stdout)
            .trim()
            .is_empty());

        let _ = fs::remove_dir_all(&repo);
        let _ = fs::remove_dir_all(&remote);
    }
}
