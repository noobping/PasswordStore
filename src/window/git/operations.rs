use crate::logging::{log_error, run_command_output, CommandLogOptions};
use crate::preferences::Preferences;
use crate::support::git::has_git_repository;

pub(super) enum GitOperationResult {
    Success,
    Failed(String),
}

fn git_operation_failed(message: &str) -> GitOperationResult {
    GitOperationResult::Failed(message.to_string())
}

fn syncable_store_roots(stores: &[String]) -> Vec<&str> {
    stores
        .iter()
        .map(String::as_str)
        .filter(|root| has_git_repository(root))
        .collect()
}

pub(super) fn run_clone_operation_at_root(url: &str, store_root: &str) -> GitOperationResult {
    let settings = Preferences::new();
    let mut cmd = settings.git_command();
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
    let settings = Preferences::new();
    let stores = settings.stores();
    for root in syncable_store_roots(&stores) {
        for args in [&["fetch", "--all"][..], &["pull"][..], &["push"][..]] {
            let mut cmd = settings.git_command();
            cmd.arg("-C").arg(root).args(args);
            match run_command_output(
                &mut cmd,
                &format!("Synchronize password store {root}"),
                CommandLogOptions::DEFAULT,
            ) {
                Ok(output) if output.status.success() => {}
                Ok(output) => {
                    let stderr = String::from_utf8_lossy(&output.stderr);
                    let fatal_line = stderr
                        .lines()
                        .rev()
                        .find(|line| line.contains("fatal:"))
                        .unwrap_or(stderr.trim());
                    log_error(format!(
                        "Password store sync failed for {root}: {fatal_line}"
                    ));
                    return git_operation_failed("Couldn't sync a store.");
                }
                Err(err) => {
                    log_error(format!("Password store sync failed for {root}: {err}"));
                    return git_operation_failed("Couldn't sync a store.");
                }
            }
        }
    }

    GitOperationResult::Success
}

#[cfg(test)]
mod tests {
    use super::syncable_store_roots;
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
}
