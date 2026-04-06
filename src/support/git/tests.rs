use super::command::{configure_store_git_repo_command, git_command_error};
use super::sync::sync_blocked_by_local_state;
use super::{
    add_store_git_remote, has_git_repository, list_store_git_remotes,
    password_store_git_state_summary, remove_store_git_remote, rename_store_git_remote,
    set_store_git_remote_url, store_git_repository_status, sync_store_repository, GitRemote,
    StoreGitHead, StoreGitRepositoryStatus,
};
use crate::preferences::Preferences;
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

fn git(path: &Path, args: &[&str]) -> Result<String, String> {
    let output = Command::new("git")
        .arg("-C")
        .arg(path)
        .args(args)
        .output()
        .map_err(|err| format!("Failed to start git command: {err}"))?;
    if !output.status.success() {
        return Err(git_command_error("git", &output));
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
    if !output.status.success() {
        return Err(git_command_error("git init --bare", &output));
    }

    let head_output = Command::new("git")
        .arg("--git-dir")
        .arg(path)
        .args(["symbolic-ref", "HEAD", "refs/heads/main"])
        .output()
        .map_err(|err| format!("Failed to start git symbolic-ref: {err}"))?;
    if !head_output.status.success() {
        return Err(git_command_error(
            "git symbolic-ref HEAD refs/heads/main",
            &head_output,
        ));
    }

    Ok(())
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
    if !output.status.success() {
        return Err(git_command_error("git clone", &output));
    }

    git(target, &["config", "user.name", "Keycord Tests"])?;
    git(target, &["config", "user.email", "tests@example.com"])?;
    Ok(())
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
        Err(git_command_error("git rev-parse", &output))
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
    assert!(status.has_outgoing_commits);
    assert!(!status.has_incoming_commits);
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
fn local_git_commands_target_the_store_git_dir() {
    let repo = temp_dir_path("current-dir");
    init_repo(&repo).expect("initialize repo");

    let mut cmd = Command::new("git");
    configure_store_git_repo_command(&mut cmd, repo.to_string_lossy().as_ref());

    assert_eq!(cmd.get_current_dir(), None);
    assert_eq!(
        cmd.get_args()
            .map(|arg| arg.to_string_lossy().into_owned())
            .collect::<Vec<_>>(),
        vec![
            "--git-dir".to_string(),
            repo.join(".git").to_string_lossy().into_owned()
        ]
    );

    let _ = fs::remove_dir_all(&repo);
}

#[test]
fn remote_git_commands_target_the_store_git_dir_without_dash_c() {
    let repo = temp_dir_path("remote-current-dir");
    init_repo(&repo).expect("initialize repo");

    let mut cmd = Preferences::remote_git_command();
    configure_store_git_repo_command(&mut cmd, repo.to_string_lossy().as_ref());

    let args = cmd
        .get_args()
        .map(|arg| arg.to_string_lossy().into_owned())
        .collect::<Vec<_>>();

    assert!(args.iter().any(|arg| arg == "--git-dir"));
    assert!(!args.iter().any(|arg| arg == "-C"));

    let _ = fs::remove_dir_all(&repo);
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
    commit_file(&clone, "secret.txt", "remote\n", "Remote change").expect("create remote change");
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
fn sync_block_reason_mentions_outgoing_commits_when_dirty() {
    let reason = sync_blocked_by_local_state(&StoreGitRepositoryStatus {
        has_repository: true,
        head: StoreGitHead::Branch("main".to_string()),
        dirty: true,
        has_outgoing_commits: true,
        has_incoming_commits: false,
        remotes: vec![GitRemote {
            name: "origin".to_string(),
            url: "ssh://example.test/repo.git".to_string(),
        }],
    })
    .expect("dirty repository should block sync");

    assert_eq!(
        reason,
        "Commit or discard local changes before syncing this store. Local commits are also waiting to sync."
    );
}

#[test]
fn committed_staged_entries_do_not_make_the_repository_dirty() {
    let repo = temp_dir_path("committed-add");
    init_repo(&repo).expect("initialize repo");
    write_file(&repo.join("example-user.gpg"), "secret\n").expect("write staged file");
    git(&repo, &["add", "example-user.gpg"]).expect("stage file");
    git(&repo, &["commit", "-m", "Add file"]).expect("commit file");

    let status =
        store_git_repository_status(repo.to_string_lossy().as_ref()).expect("read git status");
    assert!(!status.dirty);

    let _ = fs::remove_dir_all(&repo);
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
