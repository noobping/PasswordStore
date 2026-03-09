use super::store::{log_error, log_info};
use std::ffi::OsStr;
use std::io::{self, Read, Write};
#[cfg(unix)]
use std::os::unix::process::{CommandExt, ExitStatusExt};
use std::process::{Child, Command, ExitStatus, Output, Stdio};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

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

#[derive(Clone, Default)]
pub struct CommandControl {
    child: Arc<Mutex<Option<Child>>>,
}

impl CommandControl {
    fn set_child(&self, child: Child) {
        match self.child.lock() {
            Ok(mut slot) => *slot = Some(child),
            Err(poisoned) => {
                let mut slot = poisoned.into_inner();
                *slot = Some(child);
            }
        }
    }

    fn clear(&self) {
        match self.child.lock() {
            Ok(mut slot) => {
                slot.take();
            }
            Err(poisoned) => {
                let mut slot = poisoned.into_inner();
                slot.take();
            }
        }
    }

    fn wait(&self, context: &str, command: &str) -> io::Result<ExitStatus> {
        loop {
            let status = match self.child.lock() {
                Ok(mut slot) => Self::try_wait_locked(&mut slot),
                Err(poisoned) => {
                    let mut slot = poisoned.into_inner();
                    Self::try_wait_locked(&mut slot)
                }
            };

            match status {
                Ok(Some(status)) => {
                    self.clear();
                    return Ok(status);
                }
                Ok(None) => thread::sleep(Duration::from_millis(50)),
                Err(err) => {
                    self.clear();
                    log_error(format!("{context}\n$ {command}\nfailed to wait: {err}"));
                    return Err(err);
                }
            }
        }
    }

    fn try_wait_locked(child: &mut Option<Child>) -> io::Result<Option<ExitStatus>> {
        let Some(child) = child.as_mut() else {
            return Err(io::Error::new(
                io::ErrorKind::BrokenPipe,
                "command handle missing child process",
            ));
        };
        child.try_wait()
    }
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

fn format_exit_status(status: &ExitStatus) -> String {
    #[cfg(unix)]
    if let Some(signal) = status.signal() {
        return format!("signal {signal}");
    }

    status.to_string()
}

fn run_command_output_inner(
    cmd: &mut Command,
    context: &str,
    options: CommandLogOptions,
    control: Option<&CommandControl>,
) -> io::Result<Output> {
    cmd.stdout(Stdio::piped());
    cmd.stderr(Stdio::piped());
    let command = describe_command(cmd);

    #[cfg(unix)]
    if control.is_some() {
        cmd.process_group(0);
    }

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

            let status = if let Some(control) = control {
                control.set_child(child);
                control.wait(context, &command)?
            } else {
                match child.wait() {
                    Ok(status) => status,
                    Err(err) => {
                        log_error(format!("{context}\n$ {command}\nfailed to wait: {err}"));
                        return Err(err);
                    }
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
                &format_exit_status(&output.status),
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

pub fn run_command_output(
    cmd: &mut Command,
    context: &str,
    options: CommandLogOptions,
) -> io::Result<Output> {
    run_command_output_inner(cmd, context, options, None)
}

#[cfg(not(feature = "flatpak"))]
pub fn run_command_output_controlled(
    cmd: &mut Command,
    context: &str,
    options: CommandLogOptions,
    control: &CommandControl,
) -> io::Result<Output> {
    run_command_output_inner(cmd, context, options, Some(control))
}

pub fn run_command_status(
    cmd: &mut Command,
    context: &str,
    options: CommandLogOptions,
) -> io::Result<ExitStatus> {
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
        log_error(message);
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
        &format_exit_status(&output.status),
        true,
        options.redact_stdin,
        !output.status.success(),
    );

    Ok(output)
}
