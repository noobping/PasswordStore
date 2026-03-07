use std::ffi::OsStr;
use std::io::{self, Read, Write};
use std::process::{Command, Output, Stdio};
use std::sync::{OnceLock, RwLock};
use std::thread;

#[derive(Clone, Copy, Debug, Default)]
pub struct CommandLogOptions {
    pub redact_stdout: bool,
    pub redact_stdin: bool,
}

impl CommandLogOptions {
    pub const DEFAULT: Self = Self {
        redact_stdout: false,
        redact_stdin: false,
    };

    pub const SENSITIVE: Self = Self {
        redact_stdout: true,
        redact_stdin: true,
    };
}

#[derive(Debug, Default)]
struct LogState {
    text: String,
    revision: usize,
    error_revision: usize,
}

fn global_log_state() -> &'static RwLock<LogState> {
    static LOG_STATE: OnceLock<RwLock<LogState>> = OnceLock::new();
    LOG_STATE.get_or_init(|| RwLock::new(LogState::default()))
}

fn with_log_state_read<T>(f: impl FnOnce(&LogState) -> T) -> T {
    match global_log_state().read() {
        Ok(state) => f(&state),
        Err(poisoned) => {
            let state = poisoned.into_inner();
            f(&state)
        }
    }
}

fn with_log_state_write<T>(f: impl FnOnce(&mut LogState) -> T) -> T {
    match global_log_state().write() {
        Ok(mut state) => f(&mut state),
        Err(poisoned) => {
            let mut state = poisoned.into_inner();
            f(&mut state)
        }
    }
}

fn push_log_entry(level: &str, message: String, is_error: bool) {
    let message = message.trim_end();
    if message.is_empty() {
        return;
    }

    with_log_state_write(|state| {
        if !state.text.is_empty() {
            state.text.push_str("\n\n");
        }
        state.text.push('[');
        state.text.push_str(level);
        state.text.push_str("] ");
        state.text.push_str(message);
        state.revision += 1;
        if is_error {
            state.error_revision = state.revision;
        }
    });
}

pub fn log_info(message: impl Into<String>) {
    push_log_entry("INFO", message.into(), false);
}

pub fn log_error(message: impl Into<String>) {
    push_log_entry("ERROR", message.into(), true);
}

pub fn log_snapshot() -> (usize, usize, String) {
    with_log_state_read(|state| (state.revision, state.error_revision, state.text.clone()))
}

fn shell_quote(value: &OsStr) -> String {
    let text = value.to_string_lossy();
    if text.is_empty() {
        return "''".to_string();
    }
    if text
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || matches!(c, '/' | '.' | '_' | '-' | ':' | '='))
    {
        return text.into_owned();
    }

    let escaped = text.replace('\'', "'\"'\"'");
    format!("'{escaped}'")
}

fn describe_command(cmd: &Command) -> String {
    let mut parts = Vec::new();
    for (key, value) in cmd.get_envs() {
        let key_text = key.to_string_lossy();
        if !key_text.starts_with("PASSWORD_STORE_") {
            continue;
        }
        if let Some(value) = value {
            parts.push(format!("{key_text}={}", shell_quote(value)));
        }
    }
    parts.push(shell_quote(cmd.get_program()));
    for arg in cmd.get_args() {
        parts.push(shell_quote(arg));
    }
    parts.join(" ")
}

fn log_command_state(
    context: &str,
    command: &str,
    status: &str,
    stdin_was_provided: bool,
    redact_stdin: bool,
    is_error: bool,
) {
    let mut message = format!("{context}\n$ {command}\nstatus: {status}");
    if stdin_was_provided {
        message.push('\n');
        if redact_stdin {
            message.push_str("stdin: [redacted]");
        } else {
            message.push_str("stdin: provided");
        }
    }

    if is_error {
        log_error(message);
    } else {
        log_info(message);
    }
}

fn log_command_stream(
    context: &str,
    command: &str,
    label: &str,
    bytes: &[u8],
    redacted: bool,
) {
    if bytes.is_empty() {
        return;
    }

    let mut message = format!("{context}\n$ {command}\n{label}:");
    if redacted {
        message.push_str(" [redacted]");
        log_info(message);
        return;
    }

    let text = String::from_utf8_lossy(bytes);
    let text = text.trim_end_matches(['\n', '\r']);
    if text.is_empty() {
        return;
    }

    message.push('\n');
    message.push_str(text);
    log_info(message);
}

fn spawn_stream_logger<R>(
    mut reader: R,
    context: String,
    command: String,
    label: &'static str,
    redacted: bool,
) -> thread::JoinHandle<io::Result<Vec<u8>>>
where
    R: Read + Send + 'static,
{
    thread::spawn(move || {
        let mut bytes = Vec::new();
        let mut buf = [0u8; 4096];
        let mut logged_redaction = false;

        loop {
            match reader.read(&mut buf) {
                Ok(0) => return Ok(bytes),
                Ok(n) => {
                    let chunk = &buf[..n];
                    bytes.extend_from_slice(chunk);
                    if redacted {
                        if !logged_redaction {
                            log_command_stream(&context, &command, label, chunk, true);
                            logged_redaction = true;
                        }
                    } else {
                        log_command_stream(&context, &command, label, chunk, false);
                    }
                }
                Err(err) => {
                    log_error(format!(
                        "{context}\n$ {command}\nfailed to read {label}: {err}"
                    ));
                    return Err(err);
                }
            }
        }
    })
}

fn join_stream_logger(
    handle: Option<thread::JoinHandle<io::Result<Vec<u8>>>>,
    context: &str,
    command: &str,
    label: &str,
) -> io::Result<Vec<u8>> {
    let Some(handle) = handle else {
        return Ok(Vec::new());
    };

    match handle.join() {
        Ok(result) => result,
        Err(_) => {
            let err = io::Error::new(
                io::ErrorKind::Other,
                format!("stream logger panicked while reading {label}"),
            );
            log_error(format!("{context}\n$ {command}\n{err}"));
            Err(err)
        }
    }
}

pub fn run_command_output(
    cmd: &mut Command,
    context: &str,
    options: CommandLogOptions,
) -> io::Result<Output> {
    cmd.stdout(Stdio::piped());
    cmd.stderr(Stdio::piped());
    let command = describe_command(cmd);

    match cmd.spawn() {
        Ok(mut child) => {
            log_command_state(context, &command, "running", false, false, false);

            let stdout_handle = child.stdout.take().map(|stdout| {
                spawn_stream_logger(
                    stdout,
                    context.to_string(),
                    command.clone(),
                    "stdout",
                    options.redact_stdout,
                )
            });
            let stderr_handle = child.stderr.take().map(|stderr| {
                spawn_stream_logger(
                    stderr,
                    context.to_string(),
                    command.clone(),
                    "stderr",
                    false,
                )
            });

            let status = match child.wait() {
                Ok(status) => status,
                Err(err) => {
                    log_error(format!("{context}\n$ {command}\nfailed to wait: {err}"));
                    return Err(err);
                }
            };

            let stdout = join_stream_logger(stdout_handle, context, &command, "stdout")?;
            let stderr = join_stream_logger(stderr_handle, context, &command, "stderr")?;
            let output = Output {
                status,
                stdout,
                stderr,
            };

            log_command_state(
                context,
                &command,
                &output.status.to_string(),
                false,
                false,
                !output.status.success(),
            );
            Ok(output)
        }
        Err(err) => {
            log_error(format!("{context}\n$ {command}\nfailed to start: {err}"));
            Err(err)
        }
    }
}

pub fn run_command_status(
    cmd: &mut Command,
    context: &str,
    options: CommandLogOptions,
) -> io::Result<std::process::ExitStatus> {
    run_command_output(cmd, context, options).map(|output| output.status)
}

pub fn run_command_with_input(
    cmd: &mut Command,
    context: &str,
    input: &str,
    options: CommandLogOptions,
) -> Result<Output, String> {
    cmd.stdin(Stdio::piped());
    cmd.stdout(Stdio::piped());
    cmd.stderr(Stdio::piped());
    let command = describe_command(cmd);

    let mut child = match cmd.spawn() {
        Ok(child) => child,
        Err(err) => {
            log_error(format!("{context}\n$ {command}\nfailed to start: {err}"));
            return Err(format!("Failed to run command: {err}"));
        }
    };

    log_command_state(context, &command, "running", true, options.redact_stdin, false);

    let stdout_handle = child.stdout.take().map(|stdout| {
        spawn_stream_logger(
            stdout,
            context.to_string(),
            command.clone(),
            "stdout",
            options.redact_stdout,
        )
    });
    let stderr_handle = child.stderr.take().map(|stderr| {
        spawn_stream_logger(
            stderr,
            context.to_string(),
            command.clone(),
            "stderr",
            false,
        )
    });

    let Some(mut stdin) = child.stdin.take() else {
        let message = format!("{context}\n$ {command}\nfailed to open stdin");
        log_error(message.clone());
        return Err("Failed to open stdin for command".to_string());
    };

    if let Err(err) = stdin.write_all(input.as_bytes()) {
        log_error(format!("{context}\n$ {command}\nfailed to write stdin: {err}"));
        return Err(format!("Failed to write command input: {err}"));
    }
    drop(stdin);

    let status = match child.wait() {
        Ok(status) => status,
        Err(err) => {
            log_error(format!("{context}\n$ {command}\nfailed to wait: {err}"));
            return Err(format!("Failed to wait for command: {err}"));
        }
    };

    let stdout = join_stream_logger(stdout_handle, context, &command, "stdout")
        .map_err(|err| format!("Failed to read command stdout: {err}"))?;
    let stderr = join_stream_logger(stderr_handle, context, &command, "stderr")
        .map_err(|err| format!("Failed to read command stderr: {err}"))?;
    let output = Output {
        status,
        stdout,
        stderr,
    };

    log_command_state(
        context,
        &command,
        &output.status.to_string(),
        true,
        options.redact_stdin,
        !output.status.success(),
    );

    Ok(output)
}

#[cfg(test)]
mod tests {
    use super::{log_error, log_info, log_snapshot, run_command_output, CommandLogOptions};
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

        let output =
            run_command_output(&mut cmd, &marker, CommandLogOptions::DEFAULT).expect("command should run");

        assert!(output.status.success());

        let (_, _, text) = log_snapshot();
        assert!(text.contains(&marker));
        assert!(text.contains(&format!("stdout:\n{marker} stdout")));
        assert!(text.contains(&format!("stderr:\n{marker} stderr")));
    }
}
