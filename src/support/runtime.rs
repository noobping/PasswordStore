use crate::logging::log_info;
#[cfg(feature = "flatpak")]
use std::process::{Command, Stdio};
use std::sync::Once;
#[cfg(feature = "flatpak")]
use std::sync::OnceLock;

pub(crate) fn git_network_operations_available() -> bool {
    #[cfg(feature = "flatpak")]
    {
        flatpak_runtime_state().host_command_execution_available
    }

    #[cfg(not(feature = "flatpak"))]
    {
        true
    }
}

pub(crate) fn host_command_execution_available() -> bool {
    #[cfg(feature = "flatpak")]
    {
        flatpak_runtime_state().host_command_execution_available
    }

    #[cfg(not(feature = "flatpak"))]
    {
        true
    }
}

#[cfg(feature = "flatpak")]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct FlatpakRuntimeState {
    host_command_execution_available: bool,
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
                "Flatpak host runtime: host command execution {} (flatpak-spawn probe {}).",
                feature_status(state.host_command_execution_available),
                probe_status(state.host_command_execution_available),
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

    *RUNTIME_STATE.get_or_init(|| FlatpakRuntimeState {
        host_command_execution_available: flatpak_spawn_host_probe(),
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
fn probe_status(success: bool) -> &'static str {
    if success {
        "succeeded"
    } else {
        "failed"
    }
}

#[cfg(feature = "flatpak")]
fn flatpak_spawn_host_probe() -> bool {
    Command::new("flatpak-spawn")
        .arg("--host")
        .arg("true")
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .is_ok_and(|status| status.success())
}
