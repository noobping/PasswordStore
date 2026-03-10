use super::{default_backend_kind, load_file_prefs, save_file_prefs, BackendKind, Preferences};
use adw::gio::prelude::*;
use adw::glib::BoolError;
use std::env;
use std::process::Command;

const DEFAULT_CMD: &str = "pass";

impl BackendKind {
    pub fn stored_value(self) -> &'static str {
        match self {
            Self::Integrated => "integrated",
            Self::HostCommand => "host-command",
        }
    }

    pub fn from_stored(value: &str) -> Self {
        match value.trim().to_ascii_lowercase().as_str() {
            "integrated" | "ripasso" => Self::Integrated,
            "host-command" | "host command" | "pass" | "pass-command" | "pass command" => {
                Self::HostCommand
            }
            _ => default_backend_kind(),
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Integrated => "Integrated",
            Self::HostCommand => "Host command",
        }
    }

    pub fn combo_position(self) -> u32 {
        match self {
            Self::Integrated => 0,
            Self::HostCommand => 1,
        }
    }

    pub fn from_combo_position(position: u32) -> Self {
        match position {
            1 => Self::HostCommand,
            _ => Self::Integrated,
        }
    }

    pub fn uses_host_command(self) -> bool {
        matches!(self, Self::HostCommand)
    }
}

impl Preferences {
    pub fn command_value(&self) -> String {
        if let Some(s) = &self.settings {
            s.string("pass-command").to_string()
        } else {
            let cfg = load_file_prefs();
            cfg.pass_command.unwrap_or_else(|| DEFAULT_CMD.to_string())
        }
    }

    pub fn command(&self) -> Command {
        let (program, args) = self.command_parts();
        let mut cmd = Command::new(program);
        cmd.args(args);

        if let Ok(appdir) = env::var("APPDIR") {
            cmd.env(
                "PATH",
                format!("{appdir}/usr/bin:{}", env::var("PATH").unwrap_or_default()),
            );
            cmd.env(
                "LD_LIBRARY_PATH",
                format!("{appdir}/usr/lib/x86_64-linux-gnu:{appdir}/usr/lib"),
            );
            cmd.env("PASSWORD_STORE_ENABLE_EXTENSIONS", "true");
            cmd.env(
                "PASSWORD_STORE_EXTENSIONS_DIR",
                format!("{appdir}/usr/lib/password-store/extensions"),
            );
        }

        cmd
    }

    pub fn git_command(&self) -> Command {
        Command::new("git")
    }

    fn command_parts(&self) -> (String, Vec<String>) {
        let cmdline = self.command_value();
        if let Some(mut parts) = shlex::split(&cmdline) {
            if parts.is_empty() {
                return (DEFAULT_CMD.to_string(), Vec::new());
            }
            let program = parts.remove(0);
            (program, parts)
        } else {
            (cmdline, Vec::new())
        }
    }

    pub fn backend_kind(&self) -> BackendKind {
        if let Some(s) = &self.settings {
            BackendKind::from_stored(&s.string("backend"))
        } else {
            let cfg = load_file_prefs();
            cfg.backend
                .as_deref()
                .map(BackendKind::from_stored)
                .unwrap_or_else(default_backend_kind)
        }
    }

    pub fn uses_integrated_backend(&self) -> bool {
        matches!(self.backend_kind(), BackendKind::Integrated)
    }

    pub fn set_command(&self, cmd: &str) -> Result<(), BoolError> {
        if let Some(s) = &self.settings {
            s.set_string("pass-command", cmd)
        } else {
            let mut cfg = load_file_prefs();
            cfg.pass_command = Some(cmd.to_string());
            save_file_prefs(&cfg)
        }
    }

    pub fn set_backend_kind(&self, backend: BackendKind) -> Result<(), BoolError> {
        if let Some(s) = &self.settings {
            s.set_string("backend", backend.stored_value())
        } else {
            let mut cfg = load_file_prefs();
            cfg.backend = Some(backend.stored_value().to_string());
            save_file_prefs(&cfg)
        }
    }
}

pub(super) fn default_store_dirs() -> Vec<String> {
    if let Ok(home) = std::env::var("HOME") {
        vec![format!("{home}/.password-store")]
    } else {
        Vec::new()
    }
}
