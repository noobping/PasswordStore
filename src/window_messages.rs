#[cfg(not(feature = "flatpak"))]
pub(crate) fn with_logs_hint(message: &str) -> String {
    format!("{message} Check Logs for details.")
}

#[cfg(feature = "flatpak")]
pub(crate) fn with_logs_hint(message: &str) -> String {
    message.to_string()
}
