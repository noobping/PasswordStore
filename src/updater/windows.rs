use super::common::{sanitize_filename, DownloadedUpdate};
use super::logic::{
    select_update_release as select_windows_update_release, ReleaseCandidate, SelectedRelease,
};
use std::path::PathBuf;

pub fn supports_updater() -> bool {
    true
}

pub fn update_check_body() -> &'static str {
    "Looking for a newer Windows installer on GitHub Releases."
}

pub fn update_available_description() -> &'static str {
    "A newer Windows release is available."
}

pub fn ready_status() -> &'static str {
    "The installer is ready to run."
}

pub fn install_failed_toast() -> &'static str {
    "Couldn't start the installer."
}

pub fn select_update_release(
    current_version: &str,
    releases: &[ReleaseCandidate],
) -> Result<Option<SelectedRelease>, String> {
    Ok(select_windows_update_release(current_version, releases))
}

pub fn download_target(release: &SelectedRelease) -> Result<DownloadedUpdate, String> {
    Ok(DownloadedUpdate {
        path: cached_download_path(release),
        size: release.asset.size,
        cleanup_dir: None,
    })
}

pub fn cleanup_download(_download: &DownloadedUpdate) {}

pub fn launch_update(download: &DownloadedUpdate) -> Result<(), String> {
    std::process::Command::new("msiexec")
        .arg("/i")
        .arg(&download.path)
        .arg("/norestart")
        .spawn()
        .map(|_| ())
        .map_err(|error| format!("Failed to start msiexec for update install: {error}"))
}

pub fn handle_special_command(_args: &[std::ffi::OsString]) -> Option<adw::gtk::glib::ExitCode> {
    None
}

fn cached_download_path(release: &SelectedRelease) -> PathBuf {
    let base = dirs_next::cache_dir()
        .or_else(dirs_next::data_local_dir)
        .unwrap_or_else(std::env::temp_dir);
    base.join(env!("CARGO_PKG_NAME"))
        .join("updates")
        .join(format!(
            "{}-{}",
            release.version,
            sanitize_filename(&release.asset.name)
        ))
}
