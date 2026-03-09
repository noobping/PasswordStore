#![cfg_attr(feature = "flatpak", allow(dead_code))]

mod command;
mod store;

#[cfg(not(feature = "flatpak"))]
pub(crate) use command::{
    run_command_output, run_command_status, run_command_with_input, CommandControl,
    CommandLogOptions,
};
#[cfg(all(feature = "flatpak", test))]
pub(crate) use command::{run_command_output, CommandLogOptions};
#[cfg(not(feature = "flatpak"))]
pub(crate) use command::run_command_output_controlled;
#[cfg(any(test, not(feature = "flatpak")))]
pub(crate) use store::log_snapshot;
pub(crate) use store::log_error;

#[cfg(test)]
mod tests {
    use super::{log_error, log_snapshot, run_command_output, CommandLogOptions};
    use crate::logging::store::log_info;
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
}
