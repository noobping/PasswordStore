use super::super::store::{log_error, log_info};
use super::streams::{join_stream_logger, spawn_stream_logger};
use super::CommandLogOptions;
use std::ffi::OsStr;
use std::io;
use std::io::Write;
#[cfg(unix)]
use std::os::unix::process::ExitStatusExt;
use std::process::{Command, ExitStatus, Output, Stdio};
use std::thread;

fn redact_stderr(options: CommandLogOptions) -> bool {
    #[cfg(feature = "hardening")]
    {
        options.redact_stderr
    }

    #[cfg(not(feature = "hardening"))]
    {
        let _ = options;
        false
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

fn format_exit_status(status: ExitStatus) -> String {
    #[cfg(unix)]
    if let Some(signal) = status.signal() {
        return format!("signal {signal}");
    }

    status.to_string()
}

fn exit_status_is_error(status: ExitStatus, options: CommandLogOptions) -> bool {
    #[cfg(unix)]
    if status.signal().is_some() {
        return true;
    }

    match status.code() {
        Some(0) => false,
        Some(code) => !options.accepted_exit_codes.contains(&code),
        None => true,
    }
}

fn run_command_output_inner(
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
                    redact_stderr(options),
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
                &format_exit_status(output.status),
                false,
                options.redact_stdin,
                exit_status_is_error(output.status, options),
            );
            Ok(output)
        }
        Err(err) => {
            log_error(format!("{context}\n$ {command}\nfailed to start: {err}"));
            Err(err)
        }
    }
}

fn spawn_input_writer(
    mut stdin: impl Write + Send + 'static,
    input: String,
) -> thread::JoinHandle<Result<(), String>> {
    thread::spawn(move || {
        stdin
            .write_all(input.as_bytes())
            .map_err(|err| format!("Failed to write command input: {err}"))
    })
}

fn join_input_writer(handle: thread::JoinHandle<Result<(), String>>) -> Result<(), String> {
    handle
        .join()
        .unwrap_or_else(|_| Err("Command input writer panicked.".to_string()))
}

pub fn run_command_output(
    cmd: &mut Command,
    context: &str,
    options: CommandLogOptions,
) -> io::Result<Output> {
    run_command_output_inner(cmd, context, options)
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

    log_command_state(
        context,
        &command,
        "running",
        true,
        options.redact_stdin,
        false,
    );

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
            redact_stderr(options),
        )
    });

    let Some(stdin) = child.stdin.take() else {
        let message = format!("{context}\n$ {command}\nfailed to open stdin");
        log_error(message);
        return Err("Failed to open stdin for command".to_string());
    };
    let input_writer = spawn_input_writer(stdin, input.to_string());

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

    if let Err(err) = join_input_writer(input_writer) {
        log_error(format!(
            "{context}\n$ {command}\nfailed to write stdin: {err}"
        ));
        return Err(err);
    }

    log_command_state(
        context,
        &command,
        &format_exit_status(output.status),
        true,
        options.redact_stdin,
        exit_status_is_error(output.status, options),
    );

    Ok(output)
}
