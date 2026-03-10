use super::super::store::{log_error, log_info};
use std::io::{self, Read};
use std::thread;

fn log_command_stream(context: &str, command: &str, label: &str, bytes: &[u8], redacted: bool) {
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

pub(super) fn spawn_stream_logger<R>(
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

pub(super) fn join_stream_logger(
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
            let err = io::Error::other(format!("stream logger panicked while reading {label}"));
            log_error(format!("{context}\n$ {command}\n{err}"));
            Err(err)
        }
    }
}
