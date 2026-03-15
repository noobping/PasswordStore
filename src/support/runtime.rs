use crate::logging::log_info;
use std::sync::Once;
#[cfg(all(target_os = "linux", feature = "flatpak"))]
use std::fs;

pub const fn git_network_operations_available() -> bool {
    true
}

pub fn log_runtime_capabilities_once() {
    static RUNTIME_LOGGED: Once = Once::new();

    RUNTIME_LOGGED.call_once(|| {
        log_info(format!(
            "App runtime: flatpak={}, setup={}, debug_assertions={}.",
            feature_status(cfg!(feature = "setup")),
            feature_status(cfg!(feature = "flatpak")),
            feature_status(cfg!(debug_assertions)),
        ));
        log_platform_runtime_details();
    });
}

fn log_platform_runtime_details() {
    log_info(format!(
        "Linux runtime: integrated key management {}, host execution {}, Git network operations {}.",
        feature_status(true),
        feature_status(flatpak_has_host_override_permission()),
        feature_status(git_network_operations_available()),
    ));
}

const fn feature_status(enabled: bool) -> &'static str {
    if enabled {
        "enabled"
    } else {
        "disabled"
    }
}

#[cfg(all(target_os = "linux", feature = "flatpak"))]
pub fn flatpak_has_host_override_permission() -> bool {
    let Ok(info) = fs::read_to_string("/.flatpak-info") else {
        return true;
    };

    flatpak_context_has_talk_name(&info, "org.freedesktop.Flatpak")
}

#[cfg(not(all(target_os = "linux", feature = "flatpak")))]
pub fn flatpak_has_host_override_permission() -> bool {
    true
}

#[cfg(all(target_os = "linux", feature = "flatpak"))]
fn flatpak_context_has_talk_name(info: &str, bus_name: &str) -> bool {
    let mut in_context = false;

    for line in info.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') || line.starts_with(';') {
            continue;
        }

        if line.starts_with('[') && line.ends_with(']') {
            in_context = line == "[Context]";
            continue;
        }

        if !in_context {
            continue;
        }

        let Some(value) = line.strip_prefix("session-bus-policy=") else {
            continue;
        };

        if flatpak_policy_allows_talk_name(value, bus_name) {
            return true;
        }
    }

    false
}

#[cfg(all(target_os = "linux", feature = "flatpak"))]
fn flatpak_policy_allows_talk_name(policy: &str, bus_name: &str) -> bool {
    policy.split(';').any(|entry| {
        let mut parts = entry.trim().splitn(2, '=');
        let name = parts.next().unwrap_or("").trim();
        let permission = parts.next().unwrap_or("").trim();
        name == bus_name && permission.eq_ignore_ascii_case("talk")
    })
}

#[cfg(all(test, target_os = "linux", feature = "flatpak"))]
mod tests {
    use super::flatpak_context_has_talk_name;

    #[test]
    fn flatpak_context_detects_required_talk_name() {
        let info = "\
[Application]
name=io.github.noobping.keycord
[Context]
session-bus-policy=org.freedesktop.Flatpak=talk;org.gtk.vfs.*=talk;
";

        assert!(flatpak_context_has_talk_name(
            info,
            "org.freedesktop.Flatpak"
        ));
    }

    #[test]
    fn flatpak_context_reports_missing_talk_name() {
        let info = "\
[Context]
session-bus-policy=org.gtk.vfs.*=talk;
";

        assert!(!flatpak_context_has_talk_name(
            info,
            "org.freedesktop.Flatpak"
        ));
    }
}
