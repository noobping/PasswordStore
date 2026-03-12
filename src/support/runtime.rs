#[cfg(feature = "flatpak")]
use std::fs;
#[cfg(feature = "flatpak")]
use std::sync::OnceLock;

#[cfg(feature = "flatpak")]
const FLATPAK_INFO_PATH: &str = "/.flatpak-info";

pub(crate) fn git_network_operations_available() -> bool {
    #[cfg(feature = "flatpak")]
    {
        flatpak_has_network_permission()
    }

    #[cfg(not(feature = "flatpak"))]
    {
        true
    }
}

#[cfg(feature = "flatpak")]
fn flatpak_has_network_permission() -> bool {
    static HAS_NETWORK_PERMISSION: OnceLock<bool> = OnceLock::new();

    *HAS_NETWORK_PERMISSION.get_or_init(|| {
        fs::read_to_string(FLATPAK_INFO_PATH)
            .ok()
            .map(|contents| context_shared_permission(&contents, "network"))
            .unwrap_or(false)
    })
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
