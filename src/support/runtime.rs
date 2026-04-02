use crate::logging::log_info;
#[cfg(all(target_os = "linux", feature = "flatpak"))]
use std::env;
use std::ffi::OsString;
#[cfg(all(target_os = "linux", feature = "flatpak"))]
use std::fs;
#[cfg(all(target_os = "linux", feature = "flatpak"))]
use std::process::Command;
use std::sync::Once;
#[cfg(all(target_os = "linux", feature = "flatpak"))]
use std::sync::OnceLock;

pub fn log_runtime_capabilities_once() {
    static RUNTIME_LOGGED: Once = Once::new();

    RUNTIME_LOGGED.call_once(|| {
        log_info(format!(
            "App runtime: debug={}, setup={}, flatpak={}, docs={}, logging={}, host-access={}, smartcard={}, fidostore={}, fidokey={}.",
            feature_status(cfg!(debug_assertions)),
            feature_status(cfg!(feature = "setup")),
            feature_status(cfg!(feature = "flatpak")),
            feature_status(supports_docs_features()),
            feature_status(supports_logging_features()),
            feature_status(has_host_permission()),
            feature_status(has_smartcard_permission()),
            feature_status(supports_fidostore_features() && has_fido2_permission()),
            feature_status(supports_fidokey_features() && has_fido2_permission()),
        ));
    });
}

pub const HOST_COMMAND_FEATURES_UNSUPPORTED: &str =
    "Host command features are only available on Linux.";
pub const UNSUPPORTED_HOST_COMMAND_ARG: &str = "--unsupported-host-command";

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
    cfg!(all(target_os = "linux", feature = "logging"))
}

pub const fn supports_docs_features() -> bool {
    cfg!(feature = "docs")
}

pub const fn supports_smartcard_features() -> bool {
    cfg!(target_os = "linux")
}

pub const fn supports_fidostore_features() -> bool {
    cfg!(feature = "fidostore") && cfg!(any(target_os = "linux", target_os = "windows"))
}

pub const fn supports_fidokey_features() -> bool {
    cfg!(feature = "fidokey") && cfg!(any(target_os = "linux", target_os = "windows"))
}

pub fn require_host_command_features() -> Result<(), String> {
    if supports_host_command_features() {
        Ok(())
    } else {
        Err(HOST_COMMAND_FEATURES_UNSUPPORTED.to_string())
    }
}

pub fn handle_unsupported_host_command_invocation(args: &[OsString]) -> bool {
    if !args
        .get(1)
        .is_some_and(|arg| arg == UNSUPPORTED_HOST_COMMAND_ARG)
    {
        return false;
    }

    eprintln!("{HOST_COMMAND_FEATURES_UNSUPPORTED}");
    true
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
pub fn has_fido2_permission() -> bool {
    static FIDO2_PERMISSION: OnceLock<bool> = OnceLock::new();

    *FIDO2_PERMISSION.get_or_init(detect_fido2_permission)
}

#[cfg(not(all(target_os = "linux", feature = "flatpak")))]
pub fn has_fido2_permission() -> bool {
    (supports_fidostore_features() || supports_fidokey_features())
        && cfg!(any(target_os = "linux", target_os = "windows"))
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
fn detect_fido2_permission() -> bool {
    detect_fido2_permission_with(flatpak_usb_device_probe)
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
fn detect_fido2_permission_with(probe: impl FnOnce() -> bool) -> bool {
    probe()
}

#[cfg(all(target_os = "linux", feature = "flatpak"))]
fn flatpak_pcsc_socket_probe() -> bool {
    env::var_os("PCSCLITE_CSOCK_NAME").is_some()
}

#[cfg(all(target_os = "linux", feature = "flatpak"))]
fn flatpak_usb_device_probe() -> bool {
    flatpak_context_list("/.flatpak-info", "Context", "devices")
        .is_some_and(|devices| devices.iter().any(|entry| entry == "all"))
}

#[cfg(all(target_os = "linux", feature = "flatpak"))]
fn flatpak_host_spawn_probe() -> bool {
    Command::new("flatpak-spawn")
        .args(["--host", "true"])
        .status()
        .is_ok_and(|status| status.success())
}

#[cfg(all(target_os = "linux", feature = "flatpak"))]
fn flatpak_context_list(path: &str, section: &str, key: &str) -> Option<Vec<String>> {
    let contents = fs::read_to_string(path).ok()?;
    parse_flatpak_context_list(&contents, section, key)
}

#[cfg(all(target_os = "linux", feature = "flatpak"))]
fn parse_flatpak_context_list(contents: &str, section: &str, key: &str) -> Option<Vec<String>> {
    let mut in_requested_section = false;
    for raw_line in contents.lines() {
        let line = raw_line.trim();
        if line.is_empty() || line.starts_with('#') || line.starts_with(';') {
            continue;
        }

        if line.starts_with('[') && line.ends_with(']') {
            in_requested_section = &line[1..line.len() - 1] == section;
            continue;
        }

        if !in_requested_section {
            continue;
        }

        let Some((found_key, value)) = line.split_once('=') else {
            continue;
        };
        if found_key.trim() != key {
            continue;
        }

        let values = value
            .split(';')
            .map(str::trim)
            .filter(|entry| !entry.is_empty())
            .map(ToOwned::to_owned)
            .collect::<Vec<_>>();
        return Some(values);
    }

    None
}

#[cfg(test)]
mod tests {
    use super::{
        handle_unsupported_host_command_invocation, OsString, UNSUPPORTED_HOST_COMMAND_ARG,
    };

    #[test]
    fn unsupported_host_command_flag_is_detected() {
        assert!(handle_unsupported_host_command_invocation(&[
            OsString::from("keycord"),
            OsString::from(UNSUPPORTED_HOST_COMMAND_ARG),
        ]));
    }

    #[test]
    fn regular_arguments_do_not_trigger_the_unsupported_host_command_handler() {
        assert!(!handle_unsupported_host_command_invocation(&[
            OsString::from("keycord"),
            OsString::from("--query"),
        ]));
    }

    #[cfg(all(target_os = "linux", feature = "flatpak"))]
    use super::{
        detect_fido2_permission_with, detect_host_permission_with,
        detect_smartcard_permission_with, parse_flatpak_context_list,
    };

    #[cfg(all(target_os = "linux", feature = "flatpak"))]
    #[test]
    fn host_permission_is_available_when_probe_succeeds() {
        assert!(detect_host_permission_with(|| true));
    }

    #[cfg(all(target_os = "linux", feature = "flatpak"))]
    #[test]
    fn host_permission_is_missing_when_probe_fails() {
        assert!(!detect_host_permission_with(|| false));
    }

    #[cfg(all(target_os = "linux", feature = "flatpak"))]
    #[test]
    fn smartcard_permission_is_available_when_probe_succeeds() {
        assert!(detect_smartcard_permission_with(|| true));
    }

    #[cfg(all(target_os = "linux", feature = "flatpak"))]
    #[test]
    fn smartcard_permission_is_missing_when_probe_fails() {
        assert!(!detect_smartcard_permission_with(|| false));
    }

    #[cfg(all(target_os = "linux", feature = "flatpak"))]
    #[test]
    fn fido2_permission_is_available_when_probe_succeeds() {
        assert!(detect_fido2_permission_with(|| true));
    }

    #[cfg(all(target_os = "linux", feature = "flatpak"))]
    #[test]
    fn fido2_permission_is_missing_when_probe_fails() {
        assert!(!detect_fido2_permission_with(|| false));
    }

    #[cfg(all(target_os = "linux", feature = "flatpak"))]
    #[test]
    fn flatpak_context_list_reads_device_permissions() {
        let contents = "[Context]\ndevices=dri;all;\n";
        assert_eq!(
            parse_flatpak_context_list(contents, "Context", "devices"),
            Some(vec!["dri".to_string(), "all".to_string()])
        );
    }

    #[cfg(all(target_os = "linux", feature = "flatpak"))]
    #[test]
    fn flatpak_context_list_returns_none_for_missing_keys() {
        let contents = "[Context]\nsockets=wayland;\n";
        assert_eq!(
            parse_flatpak_context_list(contents, "Context", "devices"),
            None
        );
    }
}
