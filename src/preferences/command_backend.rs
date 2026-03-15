use super::{default_backend_kind, BackendKind, Preferences};
#[cfg(all(target_os = "linux", feature = "flatpak"))]
use crate::support::runtime::flatpak_has_host_override_permission;
use adw::gio::prelude::*;
use adw::glib::BoolError;

pub(super) const DEFAULT_CMD: &str = "pass";

pub(super) fn split_command_line(cmdline: &str) -> (String, Vec<String>) {
    if let Some(mut parts) = shlex::split(cmdline) {
        if parts.is_empty() {
            return (DEFAULT_CMD.to_string(), Vec::new());
        }
        let program = parts.remove(0);
        (program, parts)
    } else {
        (cmdline.to_string(), Vec::new())
    }
}

pub(super) fn stored_backend_kind(preferences: &Preferences) -> BackendKind {
    let stored = preferences.read_preference(
        |settings| BackendKind::from_stored(&settings.string("backend")),
        |cfg| {
            cfg.backend
                .as_deref()
                .map_or_else(default_backend_kind, BackendKind::from_stored)
        },
    );

    effective_backend_kind(stored)
}

#[cfg(all(target_os = "linux", feature = "flatpak"))]
fn effective_backend_kind(stored: BackendKind) -> BackendKind {
    if stored.uses_host_command() && !flatpak_has_host_override_permission() {
        BackendKind::Integrated
    } else {
        stored
    }
}

#[cfg(not(all(target_os = "linux", feature = "flatpak")))]
fn effective_backend_kind(stored: BackendKind) -> BackendKind {
    stored
}

impl Preferences {
    pub fn command_value(&self) -> String {
        self.read_preference(
            |settings| settings.string("pass-command").to_string(),
            |cfg| {
                cfg.pass_command
                    .clone()
                    .unwrap_or_else(|| DEFAULT_CMD.to_string())
            },
        )
    }

    pub fn set_command(&self, cmd: &str) -> Result<(), BoolError> {
        self.write_preference(
            |settings| settings.set_string("pass-command", cmd),
            |cfg| cfg.pass_command = Some(cmd.to_string()),
        )
    }

    pub fn set_backend_kind(&self, backend: BackendKind) -> Result<(), BoolError> {
        self.write_preference(
            |settings| settings.set_string("backend", backend.stored_value()),
            |cfg| cfg.backend = Some(backend.stored_value().to_string()),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::effective_backend_kind;
    use crate::preferences::BackendKind;

    #[test]
    fn non_host_backends_are_left_unchanged() {
        assert_eq!(
            effective_backend_kind(BackendKind::Integrated),
            BackendKind::Integrated
        );
    }

    #[cfg(not(all(target_os = "linux", feature = "flatpak")))]
    #[test]
    fn host_backend_is_left_unchanged_outside_flatpak_permission_checks() {
        assert_eq!(
            effective_backend_kind(BackendKind::HostCommand),
            BackendKind::HostCommand
        );
    }

    #[cfg(all(target_os = "linux", feature = "flatpak"))]
    #[test]
    fn host_backend_falls_back_to_integrated_without_permission() {
        assert_eq!(
            super::effective_backend_kind_for_host_permission(BackendKind::HostCommand, false),
            BackendKind::Integrated
        );
    }

    #[cfg(all(target_os = "linux", feature = "flatpak"))]
    #[test]
    fn host_backend_is_kept_with_permission() {
        assert_eq!(
            super::effective_backend_kind_for_host_permission(BackendKind::HostCommand, true),
            BackendKind::HostCommand
        );
    }
}
