use crate::logging::{log_error, log_info, run_command_output, CommandLogOptions};
use crate::preferences::Preferences;
use crate::support::git::{has_git_repository, sync_store_repository};
use crate::support::runtime::require_host_command_features;

pub(super) enum GitOperationResult {
    Success,
    Failed(String),
}

fn git_operation_failed(message: &str) -> GitOperationResult {
    GitOperationResult::Failed(message.to_string())
}

fn sync_failure_toast(err: &str) -> &'static str {
    if err.contains("Local and remote commits are also waiting to sync") {
        return "Local changes found. Local and remote commits are also waiting to sync.";
    }
    if err.contains("Local commits are also waiting to sync") {
        return "Local changes found. Local commits are also waiting to sync.";
    }
    if err.contains("Remote commits are also waiting to sync") {
        return "Local changes found. Remote commits are also waiting to sync.";
    }
    if err.contains("Commit or discard local changes before syncing") {
        return "Local changes found. Commit or discard them first.";
    }
    if err.contains("Make an initial commit") {
        return "Make an initial commit before syncing.";
    }
    if err.contains("Check out a branch before syncing") {
        return "Check out a branch before syncing.";
    }

    "Couldn't sync stores."
}

fn syncable_store_roots(stores: &[String]) -> Vec<&str> {
    stores
        .iter()
        .map(String::as_str)
        .filter(|root| has_git_repository(root))
        .collect()
}

pub(super) fn run_clone_operation_at_root(url: &str, store_root: &str) -> GitOperationResult {
    if let Err(message) = require_host_command_features() {
        return git_operation_failed(&message);
    }

    let mut cmd = Preferences::remote_git_command();
    cmd.arg("clone").arg(url).arg(store_root);
    match run_command_output(
        &mut cmd,
        "Restore password store",
        CommandLogOptions::DEFAULT,
    ) {
        Ok(output) if output.status.success() => GitOperationResult::Success,
        Ok(_) => git_operation_failed("Couldn't restore the store."),
        Err(err) => {
            log_error(format!("Failed to start restore from Git: {err}"));
            git_operation_failed("Couldn't restore the store.")
        }
    }
}

pub(super) fn run_sync_operation() -> GitOperationResult {
    if let Err(message) = require_host_command_features() {
        return git_operation_failed(&message);
    }

    let settings = Preferences::new();
    let stores = settings.stores();
    let syncable_roots = syncable_store_roots(&stores);
    if syncable_roots.is_empty() {
        log_info("Git sync skipped: no Git-backed password stores are configured.".to_string());
        return GitOperationResult::Success;
    }

    for root in syncable_roots {
        if let Err(err) = sync_store_repository(root) {
            log_error(format!("Failed to sync password store '{root}': {err}"));
            return git_operation_failed(sync_failure_toast(&err));
        }
    }

    log_info("Git sync completed.".to_string());
    GitOperationResult::Success
}

#[cfg(test)]
mod tests {
    use super::{sync_failure_toast, syncable_store_roots};
    use crate::support::git::has_git_repository;
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
    fn sync_skips_store_roots_without_git_metadata() {
        let git_store = temp_dir_path("git");
        let plain_store = temp_dir_path("plain");
        fs::create_dir_all(git_store.join(".git")).expect("create git metadata");
        fs::create_dir_all(&plain_store).expect("create plain store");

        let stores = vec![
            git_store.to_string_lossy().to_string(),
            plain_store.to_string_lossy().to_string(),
        ];

        let expected = vec![stores[0].as_str()];
        assert_eq!(syncable_store_roots(&stores), expected);
        assert!(has_git_repository(git_store.to_string_lossy().as_ref()));
        assert!(!has_git_repository(plain_store.to_string_lossy().as_ref()));

        let _ = fs::remove_dir_all(&git_store);
        let _ = fs::remove_dir_all(&plain_store);
    }

    #[test]
    fn sync_failure_toast_reports_local_changes_concisely() {
        assert_eq!(
            sync_failure_toast("Commit or discard local changes before syncing this store."),
            "Local changes found. Commit or discard them first."
        );
    }

    #[test]
    fn sync_failure_toast_reports_dirty_and_outgoing_commits_concisely() {
        assert_eq!(
            sync_failure_toast(
                "Commit or discard local changes before syncing this store. Local commits are also waiting to sync."
            ),
            "Local changes found. Local commits are also waiting to sync."
        );
    }

    #[test]
    fn sync_failure_toast_reports_initial_commit_requirement_concisely() {
        assert_eq!(
            sync_failure_toast("Make an initial commit on 'main' before syncing this store."),
            "Make an initial commit before syncing."
        );
    }
}
