use crate::logging::{log_error, run_command_output, CommandLogOptions};
use crate::preferences::Preferences;

pub(super) enum GitOperationResult {
    Success,
    Failed(String),
}

fn git_operation_failed(message: &str) -> GitOperationResult {
    GitOperationResult::Failed(message.to_string())
}

pub(super) fn run_clone_operation(url: &str) -> GitOperationResult {
    let settings = Preferences::new();
    let store_root = settings.store();
    if store_root.is_empty() {
        return GitOperationResult::Failed("Add a store folder in Preferences first.".to_string());
    }

    let mut cmd = settings.git_command();
    cmd.arg("clone").arg(url).arg(&store_root);
    match run_command_output(&mut cmd, "Clone password store", CommandLogOptions::DEFAULT) {
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
    for root in settings.stores() {
        for args in [&["fetch", "--all"][..], &["pull"][..], &["push"][..]] {
            let mut cmd = settings.git_command();
            cmd.arg("-C").arg(&root).args(args);
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
