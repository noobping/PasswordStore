use crate::support::background::spawn_worker;
use std::io::{self, Write};
use std::process::{Command, ExitStatus, Output, Stdio};

pub fn log_info(message: impl Into<String>) {
    let _ = sanitize_diagnostic_message(&message.into());
}

pub fn log_error(message: impl Into<String>) {
    let _ = sanitize_diagnostic_message(&message.into());
}

pub fn log_snapshot() -> (usize, usize, String) {
    (0, 0, String::new())
}

pub(crate) fn sanitize_diagnostic_message(message: &str) -> String {
    message.to_string()
}

#[derive(Clone, Copy, Debug, Default)]
pub struct CommandLogOptions {
    pub redact_stdout: bool,
    pub redact_stderr: bool,
    pub redact_stdin: bool,
    pub accepted_exit_codes: &'static [i32],
}

impl CommandLogOptions {
    pub const DEFAULT: Self = Self {
        redact_stdout: false,
        redact_stderr: false,
        redact_stdin: false,
        accepted_exit_codes: &[],
    };

    pub const SENSITIVE: Self = Self {
        redact_stdout: true,
        redact_stderr: true,
        redact_stdin: true,
        accepted_exit_codes: &[],
    };
}

fn consume_command_log_options(options: CommandLogOptions) {
    let _ = (
        options.redact_stdout,
        options.redact_stderr,
        options.redact_stdin,
        options.accepted_exit_codes,
    );
}

pub fn run_command_output(
    cmd: &mut Command,
    _context: &str,
    options: CommandLogOptions,
) -> io::Result<Output> {
    consume_command_log_options(options);
    cmd.output()
}

pub fn run_command_status(
    cmd: &mut Command,
    _context: &str,
    options: CommandLogOptions,
) -> io::Result<ExitStatus> {
    consume_command_log_options(options);
    cmd.status()
}

pub fn run_command_with_input(
    cmd: &mut Command,
    _context: &str,
    input: &str,
    options: CommandLogOptions,
) -> Result<Output, String> {
    consume_command_log_options(options);
    cmd.stdin(Stdio::piped());
    cmd.stdout(Stdio::piped());
    cmd.stderr(Stdio::piped());

    let mut child = cmd
        .spawn()
        .map_err(|err| format!("Failed to run command: {err}"))?;

    let Some(mut stdin) = child.stdin.take() else {
        return Err("Failed to open stdin for command".to_string());
    };

    let input = input.to_string();
    let writer = spawn_worker("disabled-command-stdin-writer", move || {
        stdin.write_all(input.as_bytes())
    })
    .map_err(|err| format!("Failed to spawn command input writer: {err}"))?;

    let output = child
        .wait_with_output()
        .map_err(|err| format!("Failed to wait for command: {err}"))?;
    match writer.join() {
        Ok(Ok(())) => Ok(output),
        Ok(Err(err)) => Err(format!("Failed to write command input: {err}")),
        Err(_) => Err("Command input writer panicked.".to_string()),
    }
}

#[cfg(all(test, unix))]
mod tests {
    use super::{log_snapshot, run_command_output, run_command_with_input, CommandLogOptions};
    use std::process::Command;

    #[test]
    fn disabled_logging_keeps_log_snapshot_empty() {
        assert_eq!(log_snapshot(), (0, 0, String::new()));
    }

    #[test]
    fn disabled_logging_still_collects_command_output() {
        let mut cmd = Command::new("sh");
        cmd.args(["-lc", "printf 'stdout'; printf 'stderr' >&2; exit 3"]);

        let output = run_command_output(&mut cmd, "disabled logging", CommandLogOptions::DEFAULT)
            .expect("command should run");

        assert_eq!(output.status.code(), Some(3));
        assert_eq!(String::from_utf8_lossy(&output.stdout), "stdout");
        assert_eq!(String::from_utf8_lossy(&output.stderr), "stderr");
    }

    #[test]
    fn disabled_logging_still_passes_stdin() {
        let mut cmd = Command::new("sh");
        cmd.args(["-lc", "cat"]);

        let output = run_command_with_input(
            &mut cmd,
            "disabled logging stdin",
            "secret input",
            CommandLogOptions::SENSITIVE,
        )
        .expect("command should run");

        assert!(output.status.success());
        assert_eq!(String::from_utf8_lossy(&output.stdout), "secret input");
        assert!(output.stderr.is_empty());
    }

    #[test]
    fn disabled_logging_handles_large_input_when_stdout_fills_first() {
        let mut cmd = Command::new("sh");
        cmd.args([
            "-lc",
            "dd if=/dev/zero bs=65536 count=2 2>/dev/null; cat >/dev/null",
        ]);

        let output = run_command_with_input(
            &mut cmd,
            "disabled logging large stdin",
            &"x".repeat(262_144),
            CommandLogOptions::SENSITIVE,
        )
        .expect("command should run");

        assert!(output.status.success());
        assert_eq!(output.stdout.len(), 131_072);
    }
}
