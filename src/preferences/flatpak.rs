use super::command_backend::{split_command_line, stored_backend_kind};
use super::{BackendKind, Preferences};
use crate::support::runtime::host_command_execution_available;
use std::process::Command;

fn build_command(program: String, args: Vec<String>, envs: &[(&str, &str)]) -> Command {
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

pub(super) fn remote_git_command() -> Command {
    build_command("git".to_string(), Vec::new(), &[])
}

fn resolved_backend_kind(backend: BackendKind) -> BackendKind {
    if backend.uses_host_command() && !host_command_execution_available() {
        BackendKind::Integrated
    } else {
        backend
    }
}

impl Preferences {
    pub fn git_command(&self) -> Command {
        Command::new("git")
    }

    pub fn remote_git_command(&self) -> Command {
        remote_git_command()
    }

    pub fn command(&self) -> Command {
        self.command_with_envs(&[])
    }

    pub fn command_with_envs(&self, envs: &[(&str, &str)]) -> Command {
        let (program, args) = split_command_line(&self.command_value());
        build_command(program, args, envs)
    }

    pub fn backend_kind(&self) -> BackendKind {
        resolved_backend_kind(stored_backend_kind(self))
    }

    pub fn uses_integrated_backend(&self) -> bool {
        matches!(self.backend_kind(), BackendKind::Integrated)
    }
}

#[cfg(test)]
mod tests {
    use super::{build_command, remote_git_command};

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
}
