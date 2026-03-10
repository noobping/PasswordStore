use std::sync::{OnceLock, RwLock};

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

#[cfg(any(test, not(feature = "flatpak")))]
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

#[cfg(any(test, not(feature = "flatpak")))]
pub fn log_info(message: impl Into<String>) {
    push_log_entry("INFO", message.into(), false);
}

pub fn log_error(message: impl Into<String>) {
    push_log_entry("ERROR", message.into(), true);
}

#[cfg(any(test, not(feature = "flatpak")))]
pub fn log_snapshot() -> (usize, usize, String) {
    with_log_state_read(|state| (state.revision, state.error_revision, state.text.clone()))
}
