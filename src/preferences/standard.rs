use super::{default_backend_kind, BackendKind, Preferences};
use crate::support::runtime::host_command_execution_available;
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

fn split_command_line(cmdline: &str) -> (String, Vec<String>) {
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

fn build_command(program: String, args: Vec<String>, envs: &[(&str, &str)]) -> Command {
    #[cfg(feature = "flatpak")]
    {
        let mut cmd = Command::new("flatpak-spawn");
        cmd.current_dir("/");
        cmd.arg("--host");
        if !envs.is_empty() {
            cmd.arg("env");
            for (key, value) in envs {
                cmd.arg(format!("{key}={value}"));
            }
        }
        cmd.arg(&program);
        cmd.args(args);
        cmd
    }

    #[cfg(not(feature = "flatpak"))]
    {
        let mut cmd = Command::new(program);
        cmd.args(args);
        for (key, value) in envs {
            cmd.env(key, value);
        }

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
}

pub(super) fn remote_git_command() -> Command {
    #[cfg(feature = "flatpak")]
    {
        build_command("git".to_string(), Vec::new(), &[])
    }

    #[cfg(not(feature = "flatpak"))]
    {
        Command::new("git")
    }
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

    #[cfg_attr(feature = "flatpak", allow(dead_code))]
    pub fn command(&self) -> Command {
        self.command_with_envs(&[])
    }

    pub fn command_with_envs(&self, envs: &[(&str, &str)]) -> Command {
        let (program, args) = split_command_line(&self.command_value());
        build_command(program, args, envs)
    }

    fn stored_backend_kind(&self) -> BackendKind {
        self.read_preference(
            |settings| BackendKind::from_stored(&settings.string("backend")),
            |cfg| {
                cfg.backend
                    .as_deref()
                    .map(BackendKind::from_stored)
                    .unwrap_or_else(default_backend_kind)
            },
        )
    }

    pub fn backend_kind(&self) -> BackendKind {
        let backend = self.stored_backend_kind();
        if backend.uses_host_command() && !host_command_execution_available() {
            BackendKind::Integrated
        } else {
            backend
        }
    }

    pub fn uses_integrated_backend(&self) -> bool {
        matches!(self.backend_kind(), BackendKind::Integrated)
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

#[cfg_attr(feature = "flatpak", allow(dead_code))]
pub(super) fn default_store_dirs() -> Vec<String> {
    if let Ok(home) = env::var("HOME") {
        vec![format!("{home}/.password-store")]
    } else {
        Vec::new()
    }
}

#[cfg(test)]
mod tests {
    use super::build_command;
    #[cfg(feature = "flatpak")]
    use super::remote_git_command;

    #[cfg(feature = "flatpak")]
    #[test]
    fn flatpak_host_command_uses_flatpak_spawn_with_env_wrapper() {
        let cmd = build_command(
            "pass".to_string(),
            vec!["show".to_string(), "team/demo".to_string()],
            &[("PASSWORD_STORE_DIR", "/tmp/store")],
        );

        assert_eq!(cmd.get_program().to_string_lossy(), "flatpak-spawn");
        assert_eq!(cmd.get_current_dir(), Some(std::path::Path::new("/")));
        assert_eq!(
            cmd.get_args()
                .map(|arg| arg.to_string_lossy().into_owned())
                .collect::<Vec<_>>(),
            vec![
                "--host".to_string(),
                "env".to_string(),
                "PASSWORD_STORE_DIR=/tmp/store".to_string(),
                "pass".to_string(),
                "show".to_string(),
                "team/demo".to_string(),
            ]
        );
    }

    #[cfg(feature = "flatpak")]
    #[test]
    fn flatpak_remote_git_routes_through_host_execution() {
        let cmd = remote_git_command();

        assert_eq!(cmd.get_program().to_string_lossy(), "flatpak-spawn");
        assert_eq!(cmd.get_current_dir(), Some(std::path::Path::new("/")));
        assert_eq!(
            cmd.get_args()
                .map(|arg| arg.to_string_lossy().into_owned())
                .collect::<Vec<_>>(),
            vec!["--host".to_string(), "git".to_string()]
        );
    }

    #[cfg(not(feature = "flatpak"))]
    #[test]
    fn standard_host_command_sets_requested_environment_variables() {
        let cmd = build_command(
            "pass".to_string(),
            vec!["show".to_string(), "team/demo".to_string()],
            &[("PASSWORD_STORE_DIR", "/tmp/store")],
        );

        assert_eq!(cmd.get_program().to_string_lossy(), "pass");
        assert_eq!(
            cmd.get_args()
                .map(|arg| arg.to_string_lossy().into_owned())
                .collect::<Vec<_>>(),
            vec!["show".to_string(), "team/demo".to_string()]
        );
        assert!(cmd
            .get_envs()
            .filter_map(|(key, value)| value.map(|value| (key, value)))
            .any(|(key, value)| {
                key.to_string_lossy() == "PASSWORD_STORE_DIR"
                    && value.to_string_lossy() == "/tmp/store"
            }));
    }
}
