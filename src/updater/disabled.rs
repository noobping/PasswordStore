use super::common::DownloadedUpdate;
use super::logic::{ReleaseCandidate, SelectedRelease};
use adw::gtk::glib::ExitCode;
use std::ffi::OsString;

pub fn supports_updater() -> bool {
    false
}

pub fn update_check_body() -> &'static str {
    "Looking for updates."
}

pub fn update_available_description() -> &'static str {
    "A newer release is available."
}

pub fn ready_status() -> &'static str {
    "The update is ready to run."
}

pub fn install_failed_toast() -> &'static str {
    "Couldn't start the installer."
}

pub fn select_update_release(
    _current_version: &str,
    _releases: &[ReleaseCandidate],
) -> Result<Option<SelectedRelease>, String> {
    Ok(None)
}

pub fn download_target(_release: &SelectedRelease) -> Result<DownloadedUpdate, String> {
    Err("Updates are not supported in this build.".to_string())
}

pub fn cleanup_download(_download: &DownloadedUpdate) {}

pub fn launch_update(_download: &DownloadedUpdate) -> Result<(), String> {
    Err("Updates are not supported in this build.".to_string())
}

pub fn handle_special_command(_args: &[OsString]) -> Option<ExitCode> {
    None
}
