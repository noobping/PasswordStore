#[cfg(keycord_linux)]
mod command;
mod store;

#[cfg(keycord_standard_linux)]
pub(crate) use command::run_command_status;
#[cfg(keycord_linux)]
pub(crate) use command::run_command_with_input;
#[cfg(keycord_linux)]
pub(crate) use command::{run_command_output, CommandLogOptions};
pub(crate) use store::log_error;
pub(crate) use store::log_info;
pub(crate) use store::log_snapshot;

#[cfg(test)]
mod tests {
    use super::{log_error, log_snapshot};
    #[cfg(keycord_linux)]
    use super::{run_command_output, CommandLogOptions};
    use crate::logging::store::log_info;
    #[cfg(keycord_linux)]
    use std::process::Command;

    #[test]
    fn log_snapshot_tracks_revisions() {
        let (before_rev, before_err, _) = log_snapshot();
        log_info("first log line");
        let (rev, err, text) = log_snapshot();
        assert!(rev > before_rev);
        assert_eq!(err, before_err);
        assert!(text.contains("first log line"));

        log_error("second log line");
        let (rev_after, err_after, text_after) = log_snapshot();
        assert!(rev_after > rev);
        assert_eq!(err_after, rev_after);
        assert!(text_after.contains("second log line"));
    }

    #[cfg(keycord_linux)]
    #[test]
    fn run_command_output_logs_streams() {
        let marker = format!("stream-log-test-{}", std::process::id());
        let mut cmd = Command::new("sh");
        cmd.args([
            "-lc",
            &format!("printf '{marker} stdout'; printf '{marker} stderr' >&2"),
        ]);

        let output = run_command_output(&mut cmd, &marker, CommandLogOptions::DEFAULT)
            .expect("command should run");

        assert!(output.status.success());

        let (_, _, text) = log_snapshot();
        assert!(text.contains(&marker));
        assert!(text.contains(&format!("stdout:\n{marker} stdout")));
        assert!(text.contains(&format!("stderr:\n{marker} stderr")));
    }

    #[cfg(keycord_linux)]
    #[test]
    fn run_command_output_can_accept_expected_non_zero_exit_codes() {
        let marker = format!("expected-exit-{}", std::process::id());
        let mut cmd = Command::new("sh");
        cmd.args(["-lc", "exit 1"]);

        let output = run_command_output(
            &mut cmd,
            &marker,
            CommandLogOptions {
                accepted_exit_codes: &[1],
                ..CommandLogOptions::DEFAULT
            },
        )
        .expect("command should run");

        assert_eq!(output.status.code(), Some(1));

        let (_, _, text) = log_snapshot();
        assert!(text.contains(&format!(
            "[INFO] {marker}\n$ sh -lc 'exit 1'\nstatus: exit status: 1"
        )));
        assert!(!text.contains(&format!(
            "[ERROR] {marker}\n$ sh -lc 'exit 1'\nstatus: exit status: 1"
        )));
        assert!(text.contains(&marker));
    }
}
