use crate::logging::log_info;
use std::sync::Once;

pub const fn git_network_operations_available() -> bool {
    true
}

pub const fn host_command_execution_available() -> bool {
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
        feature_status(host_command_execution_available()),
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
