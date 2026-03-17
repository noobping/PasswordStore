use super::command_backend::{split_command_line, stored_backend_kind};
use super::{BackendKind, Preferences};
use std::env;
use std::process::Command;

fn build_command(program: String, args: Vec<String>, envs: &[(&str, &str)]) -> Command {
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

pub(super) fn remote_git_command() -> Command {
    Command::new("git")
}

impl Preferences {
    pub fn git_command() -> Command {
        Command::new("git")
    }

    pub fn remote_git_command() -> Command {
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
        stored_backend_kind(self)
    }

    pub fn uses_integrated_backend(&self) -> bool {
        matches!(self.backend_kind(), BackendKind::Integrated)
    }

    pub fn uses_host_command_backend(&self) -> bool {
        self.backend_kind().uses_host_command()
    }
}

#[cfg(test)]
mod tests {
    use super::{build_command, remote_git_command};

    #[test]
    fn linux_host_command_sets_requested_environment_variables() {
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

    #[test]
    fn linux_remote_git_uses_system_git() {
        let cmd = remote_git_command();

        assert_eq!(cmd.get_program().to_string_lossy(), "git");
        assert_eq!(cmd.get_args().count(), 0);
    }
}
