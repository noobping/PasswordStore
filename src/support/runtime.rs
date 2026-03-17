use crate::logging::log_info;
#[cfg(all(target_os = "linux", feature = "flatpak"))]
use std::process::Command;
#[cfg(all(target_os = "linux", feature = "flatpak"))]
use std::sync::OnceLock;
use std::sync::Once;

pub fn log_runtime_capabilities_once() {
    static RUNTIME_LOGGED: Once = Once::new();

    RUNTIME_LOGGED.call_once(|| {
        log_info(format!(
            "App runtime: debug={}, setup={}, flatpak={}, network={}.",
            feature_status(cfg!(debug_assertions)),
            feature_status(cfg!(feature = "setup")),
            feature_status(cfg!(feature = "flatpak")),
            feature_status(has_host_permission()),
        ));
    });
}

const fn feature_status(enabled: bool) -> &'static str {
    if enabled {
        "enabled"
    } else {
        "disabled"
    }
}

#[cfg(all(target_os = "linux", feature = "flatpak"))]
pub fn has_host_permission() -> bool {
    static HOST_PERMISSION: OnceLock<bool> = OnceLock::new();

    *HOST_PERMISSION.get_or_init(detect_host_permission)
}

#[cfg(not(all(target_os = "linux", feature = "flatpak")))]
pub fn has_host_permission() -> bool {
    true
}

#[cfg(all(target_os = "linux", feature = "flatpak"))]
fn detect_host_permission() -> bool {
    detect_host_permission_with(flatpak_host_spawn_probe)
}

#[cfg(all(target_os = "linux", feature = "flatpak"))]
fn detect_host_permission_with(probe: impl FnOnce() -> bool) -> bool {
    probe()
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
    use super::detect_host_permission_with;

    #[test]
    fn host_permission_is_available_when_probe_succeeds() {
        assert!(detect_host_permission_with(|| true));
    }

    #[test]
    fn host_permission_is_missing_when_probe_fails() {
        assert!(!detect_host_permission_with(|| false));
    }
}
