use crate::logging::log_info;
#[cfg(feature = "flatpak")]
use std::fs;
use std::sync::Once;
#[cfg(feature = "flatpak")]
use std::sync::OnceLock;

#[cfg(feature = "flatpak")]
const FLATPAK_INFO_PATH: &str = "/.flatpak-info";

pub(crate) fn git_network_operations_available() -> bool {
    #[cfg(feature = "flatpak")]
    {
        flatpak_runtime_state().has_network_permission
    }

    #[cfg(not(feature = "flatpak"))]
    {
        true
    }
}

#[cfg(feature = "flatpak")]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct FlatpakRuntimeState {
    flatpak_info_readable: bool,
    has_network_permission: bool,
}

pub(crate) fn log_runtime_capabilities_once() {
    static RUNTIME_LOGGED: Once = Once::new();

    RUNTIME_LOGGED.call_once(|| {
        log_info(format!(
            "App features: flatpak={}, setup={}, debug_assertions={}.",
            feature_status(cfg!(feature = "flatpak")),
            feature_status(cfg!(feature = "setup")),
            feature_status(cfg!(debug_assertions)),
        ));

        #[cfg(feature = "flatpak")]
        {
            let state = flatpak_runtime_state();
            log_info(format!(
                "Flatpak Git runtime: local commits enabled, network operations {} (/.flatpak-info {}, network permission {}).",
                feature_status(state.has_network_permission),
                readable_status(state.flatpak_info_readable),
                permission_status(state.has_network_permission),
            ));
        }

        #[cfg(not(feature = "flatpak"))]
        {
            log_info(
                "Standard runtime: host Git integration follows the current system configuration."
                    .to_string(),
            );
        }
    })
}

#[cfg(feature = "flatpak")]
fn flatpak_runtime_state() -> FlatpakRuntimeState {
    static RUNTIME_STATE: OnceLock<FlatpakRuntimeState> = OnceLock::new();

    *RUNTIME_STATE.get_or_init(|| {
        fs::read_to_string(FLATPAK_INFO_PATH)
            .ok()
            .map(|contents| FlatpakRuntimeState {
                flatpak_info_readable: true,
                has_network_permission: context_shared_permission(&contents, "network"),
            })
            .unwrap_or(FlatpakRuntimeState {
                flatpak_info_readable: false,
                has_network_permission: false,
            })
    })
}

fn feature_status(enabled: bool) -> &'static str {
    if enabled {
        "enabled"
    } else {
        "disabled"
    }
}

#[cfg(feature = "flatpak")]
fn readable_status(readable: bool) -> &'static str {
    if readable {
        "was readable"
    } else {
        "could not be read"
    }
}

#[cfg(feature = "flatpak")]
fn permission_status(enabled: bool) -> &'static str {
    if enabled {
        "granted"
    } else {
        "missing"
    }
}

#[cfg(feature = "flatpak")]
fn context_shared_permission(contents: &str, permission: &str) -> bool {
    let mut in_context = false;

    for line in contents.lines() {
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

        let Some(shared) = line.strip_prefix("shared=") else {
            continue;
        };

        return shared.split(';').any(|item| item.trim() == permission);
    }

    false
}

#[cfg(test)]
mod tests {
    #[cfg(feature = "flatpak")]
    use super::context_shared_permission;

    #[cfg(feature = "flatpak")]
    #[test]
    fn flatpak_context_detects_network_permission() {
        assert!(context_shared_permission(
            "[Application]\nname=example\n\n[Context]\nshared=network;ipc;\n",
            "network",
        ));
    }

    #[cfg(feature = "flatpak")]
    #[test]
    fn flatpak_context_requires_network_permission_for_git() {
        assert!(!context_shared_permission(
            "[Context]\nshared=ipc;\nfilesystems=host;\n",
            "network",
        ));
    }
}
