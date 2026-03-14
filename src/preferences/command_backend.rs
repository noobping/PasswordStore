use super::{default_backend_kind, BackendKind, Preferences};
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
    preferences.read_preference(
        |settings| BackendKind::from_stored(&settings.string("backend")),
        |cfg| {
            cfg.backend
                .as_deref()
                .map(BackendKind::from_stored)
                .unwrap_or_else(default_backend_kind)
        },
    )
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
