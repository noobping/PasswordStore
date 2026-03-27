use crate::logging::log_info;
#[cfg(all(target_os = "linux", feature = "flatpak"))]
use std::env;
#[cfg(target_os = "windows")]
use std::path::{Path, PathBuf};
#[cfg(all(target_os = "linux", feature = "flatpak"))]
use std::process::Command;
use std::sync::Once;
#[cfg(all(target_os = "linux", feature = "flatpak"))]
use std::sync::OnceLock;

pub fn configure_process_environment() {
    #[cfg(target_os = "windows")]
    configure_windows_environment();
}

pub fn log_runtime_capabilities_once() {
    static RUNTIME_LOGGED: Once = Once::new();

    RUNTIME_LOGGED.call_once(|| {
        log_info(format!(
            "App runtime: debug={}, setup={}, flatpak={}, host-access={}, smartcard={}.",
            feature_status(cfg!(debug_assertions)),
            feature_status(cfg!(feature = "setup")),
            feature_status(cfg!(feature = "flatpak")),
            feature_status(has_host_permission()),
            feature_status(has_smartcard_permission()),
        ));
    });
}

#[cfg(target_os = "windows")]
fn configure_windows_environment() {
    let Some(exe_dir) = std::env::current_exe()
        .ok()
        .and_then(|path| path.parent().map(Path::to_path_buf))
    else {
        return;
    };

    let share_dir = exe_dir.join("share");
    if share_dir.is_dir() {
        prepend_env_path("XDG_DATA_DIRS", &share_dir);
    }

    let schema_dir = share_dir.join("glib-2.0").join("schemas");
    if schema_dir.is_dir() {
        std::env::set_var("GSETTINGS_SCHEMA_DIR", schema_dir);
    }
}

#[cfg(target_os = "windows")]
fn prepend_env_path(name: &str, path: &Path) {
    let mut paths = vec![PathBuf::from(path)];
    if let Some(existing) = std::env::var_os(name) {
        paths.extend(std::env::split_paths(&existing));
    }

    if let Ok(joined) = std::env::join_paths(paths) {
        std::env::set_var(name, joined);
    }
}

pub const HOST_COMMAND_FEATURES_UNSUPPORTED: &str =
    "Host command features are only available on Linux.";

const fn feature_status(enabled: bool) -> &'static str {
    if enabled {
        "enabled"
    } else {
        "disabled"
    }
}

pub const fn supports_host_command_features() -> bool {
    cfg!(target_os = "linux")
}

pub const fn supports_logging_features() -> bool {
    cfg!(target_os = "linux")
}

pub const fn supports_smartcard_features() -> bool {
    cfg!(target_os = "linux")
}

pub fn require_host_command_features() -> Result<(), String> {
    if supports_host_command_features() {
        Ok(())
    } else {
        Err(HOST_COMMAND_FEATURES_UNSUPPORTED.to_string())
    }
}

#[cfg(all(target_os = "linux", feature = "flatpak"))]
pub fn has_host_permission() -> bool {
    static HOST_PERMISSION: OnceLock<bool> = OnceLock::new();

    *HOST_PERMISSION.get_or_init(detect_host_permission)
}

#[cfg(not(all(target_os = "linux", feature = "flatpak")))]
pub fn has_host_permission() -> bool {
    supports_host_command_features()
}

#[cfg(all(target_os = "linux", feature = "flatpak"))]
pub fn has_smartcard_permission() -> bool {
    static SMARTCARD_PERMISSION: OnceLock<bool> = OnceLock::new();

    *SMARTCARD_PERMISSION.get_or_init(detect_smartcard_permission)
}

#[cfg(not(all(target_os = "linux", feature = "flatpak")))]
pub fn has_smartcard_permission() -> bool {
    supports_smartcard_features()
}

#[cfg(all(target_os = "linux", feature = "flatpak"))]
fn detect_host_permission() -> bool {
    detect_host_permission_with(flatpak_host_spawn_probe)
}

#[cfg(all(target_os = "linux", feature = "flatpak"))]
fn detect_smartcard_permission() -> bool {
    detect_smartcard_permission_with(flatpak_pcsc_socket_probe)
}

#[cfg(all(target_os = "linux", feature = "flatpak"))]
fn detect_host_permission_with(probe: impl FnOnce() -> bool) -> bool {
    probe()
}

#[cfg(all(target_os = "linux", feature = "flatpak"))]
fn detect_smartcard_permission_with(probe: impl FnOnce() -> bool) -> bool {
    probe()
}

#[cfg(all(target_os = "linux", feature = "flatpak"))]
fn flatpak_pcsc_socket_probe() -> bool {
    env::var_os("PCSCLITE_CSOCK_NAME").is_some()
}

#[cfg(all(target_os = "linux", feature = "flatpak"))]
fn flatpak_host_spawn_probe() -> bool {
    Command::new("flatpak-spawn")
        .args(["--host", "true"])
        .status()
        .is_ok_and(|status| status.success())
}

#[cfg(all(test, target_os = "linux", feature = "flatpak"))]
mod tests {
    use super::{detect_host_permission_with, detect_smartcard_permission_with};

    #[test]
    fn host_permission_is_available_when_probe_succeeds() {
        assert!(detect_host_permission_with(|| true));
    }

    #[test]
    fn host_permission_is_missing_when_probe_fails() {
        assert!(!detect_host_permission_with(|| false));
    }

    #[test]
    fn smartcard_permission_is_available_when_probe_succeeds() {
        assert!(detect_smartcard_permission_with(|| true));
    }

    #[test]
    fn smartcard_permission_is_missing_when_probe_fails() {
        assert!(!detect_smartcard_permission_with(|| false));
    }
}
