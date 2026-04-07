#[cfg(feature = "hardening")]
use regex::{Captures, Regex};
use std::sync::{OnceLock, RwLock};
#[cfg(feature = "hardening")]
use url::Url;

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

fn push_log_entry(level: &str, message: &str, is_error: bool) {
    let message = sanitize_log_message(message.trim_end());
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
        state.text.push_str(&message);
        state.revision += 1;
        if is_error {
            state.error_revision = state.revision;
        }
    });
}

#[cfg(feature = "hardening")]
fn sanitize_log_message(message: &str) -> String {
    replace_embedded_nuls(&redact_scp_like_credentials(&redact_url_credentials(
        message,
    )))
}

#[cfg(not(feature = "hardening"))]
fn sanitize_log_message(message: &str) -> String {
    message.to_string()
}

#[cfg(feature = "hardening")]
fn replace_embedded_nuls(message: &str) -> String {
    message.replace('\0', "\u{FFFD}")
}

#[cfg(feature = "hardening")]
fn redact_url_credentials(message: &str) -> String {
    credential_url_regex()
        .replace_all(message, |captures: &Captures| {
            let prefix = captures.name("prefix").map_or("", |value| value.as_str());
            let url = captures.name("url").map_or("", |value| value.as_str());
            format!("{prefix}{}", redact_url_credential_value(url))
        })
        .into_owned()
}

#[cfg(feature = "hardening")]
fn redact_scp_like_credentials(message: &str) -> String {
    scp_remote_regex()
        .replace_all(message, |captures: &Captures| {
            let prefix = captures.name("prefix").map_or("", |value| value.as_str());
            let host = captures.name("host").map_or("", |value| value.as_str());
            let path = captures.name("path").map_or("", |value| value.as_str());
            format!("{prefix}redacted@{host}:{path}")
        })
        .into_owned()
}

#[cfg(feature = "hardening")]
fn redact_url_credential_value(url: &str) -> String {
    let (url, suffix) = split_trailing_punctuation(url);
    let Ok(mut parsed) = Url::parse(url) else {
        return format!("{url}{suffix}");
    };
    if parsed.username().is_empty() && parsed.password().is_none() {
        return format!("{url}{suffix}");
    }
    if parsed.set_username("redacted").is_err() || parsed.set_password(None).is_err() {
        return format!("{url}{suffix}");
    }

    format!("{}{suffix}", parsed.as_str())
}

#[cfg(feature = "hardening")]
fn split_trailing_punctuation(value: &str) -> (&str, &str) {
    let mut end = value.len();
    while end > 0 {
        let Some(ch) = value[..end].chars().next_back() else {
            break;
        };
        if !matches!(ch, '.' | ',' | ';' | ')' | ']' | '}') {
            break;
        }
        end -= ch.len_utf8();
    }

    value.split_at(end)
}

#[cfg(feature = "hardening")]
fn credential_url_regex() -> &'static Regex {
    static REGEX: OnceLock<Regex> = OnceLock::new();
    REGEX.get_or_init(|| {
        Regex::new(r#"(?P<prefix>^|[\s'"(])(?P<url>[A-Za-z][A-Za-z0-9+.-]*://[^\s'"<>]+)"#)
            .expect("credential URL regex should compile")
    })
}

#[cfg(feature = "hardening")]
fn scp_remote_regex() -> &'static Regex {
    static REGEX: OnceLock<Regex> = OnceLock::new();
    REGEX.get_or_init(|| {
        Regex::new(
            r#"(?P<prefix>^|[\s'"(])[^@\s'"/:]+@(?P<host>[A-Za-z0-9._-]+):(?P<path>[^\s'"<>]*/[^\s'"<>]+)"#,
        )
        .expect("scp remote regex should compile")
    })
}

pub fn log_info(message: impl Into<String>) {
    let message = message.into();
    push_log_entry("INFO", &message, false);
}

pub fn log_error(message: impl Into<String>) {
    let message = message.into();
    push_log_entry("ERROR", &message, true);
}

pub fn log_snapshot() -> (usize, usize, String) {
    with_log_state_read(|state| (state.revision, state.error_revision, state.text.clone()))
}

#[cfg(test)]
mod tests {
    use super::sanitize_log_message;

    #[cfg(feature = "hardening")]
    #[test]
    fn credentialed_urls_are_redacted() {
        let message =
            sanitize_log_message("git clone https://user:secret@example.test/private/repo.git");

        assert_eq!(
            message,
            "git clone https://redacted@example.test/private/repo.git".to_string()
        );
        assert!(!message.contains("secret"));
    }

    #[cfg(feature = "hardening")]
    #[test]
    fn scp_like_remotes_are_redacted() {
        let message = sanitize_log_message("git clone token@example.test:owner/repo.git");

        assert_eq!(
            message,
            "git clone redacted@example.test:owner/repo.git".to_string()
        );
        assert!(!message.contains("token@"));
    }

    #[cfg(feature = "hardening")]
    #[test]
    fn embedded_nuls_are_replaced() {
        assert_eq!(
            sanitize_log_message("alpha\0beta"),
            "alpha\u{FFFD}beta".to_string()
        );
    }

    #[cfg(not(feature = "hardening"))]
    #[test]
    fn messages_are_unchanged_without_hardening() {
        assert_eq!(
            sanitize_log_message("git clone https://user:secret@example.test/private/repo.git"),
            "git clone https://user:secret@example.test/private/repo.git".to_string()
        );
        assert_eq!(
            sanitize_log_message("alpha\0beta"),
            "alpha\0beta".to_string()
        );
    }
}
