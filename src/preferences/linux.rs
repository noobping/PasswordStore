use super::command_backend::{split_command_line, stored_backend_kind};
use super::{BackendKind, Preferences};
use std::env;
use std::process::Command;

fn build_command(program: String, args: Vec<String>, envs: &[(&str, &str)]) -> Command {
    let mut cmd = if env::var("FLATPAK_ID").is_ok() {
        let mut cmd = Command::new("flatpak-spawn");
        cmd.arg("--host").arg(&program).args(&args);
        cmd.current_dir("/");
        cmd
    } else {
        let mut cmd = Command::new(&program);
        cmd.args(&args);
        cmd
    };

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
    build_command("git".to_string(), Vec::new(), &[])
}

fn local_git_command() -> Command {
    let mut cmd = Command::new("git");
    cmd.current_dir("/");
    cmd
}

impl Preferences {
    pub fn git_command() -> Command {
        local_git_command()
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

    pub fn host_program_command(&self, program: &str, args: &[&str]) -> Command {
        build_command(
            program.to_string(),
            args.iter().map(|arg| (*arg).to_string()).collect(),
            &[],
        )
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
    use super::{build_command, local_git_command, remote_git_command};

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

        #[cfg(feature = "flatpak")]
        {
            assert_eq!(cmd.get_program().to_string_lossy(), "flatpak-spawn");
            assert_eq!(
                cmd.get_args()
                    .map(|arg| arg.to_string_lossy().into_owned())
                    .collect::<Vec<_>>(),
                vec!["--host".to_string(), "git".to_string()]
            );
            assert_eq!(cmd.get_current_dir(), Some(std::path::Path::new("/")));
        }

        #[cfg(not(feature = "flatpak"))]
        {
            assert_eq!(cmd.get_program().to_string_lossy(), "git");
            assert_eq!(cmd.get_args().count(), 0);
        }
    }

    #[test]
    fn linux_local_git_uses_a_stable_working_directory() {
        let cmd = local_git_command();

        assert_eq!(cmd.get_program().to_string_lossy(), "git");
        assert_eq!(cmd.get_args().count(), 0);
        assert_eq!(cmd.get_current_dir(), Some(std::path::Path::new("/")));
    }

    #[test]
    fn linux_host_program_command_uses_requested_program_and_args() {
        let cmd =
            crate::preferences::Preferences::new().host_program_command("gpg", &["--version"]);

        #[cfg(feature = "flatpak")]
        {
            assert_eq!(cmd.get_program().to_string_lossy(), "flatpak-spawn");
            assert_eq!(
                cmd.get_args()
                    .map(|arg| arg.to_string_lossy().into_owned())
                    .collect::<Vec<_>>(),
                vec![
                    "--host".to_string(),
                    "gpg".to_string(),
                    "--version".to_string()
                ]
            );
            assert_eq!(cmd.get_current_dir(), Some(std::path::Path::new("/")));
        }

        #[cfg(not(feature = "flatpak"))]
        {
            assert_eq!(cmd.get_program().to_string_lossy(), "gpg");
            assert_eq!(
                cmd.get_args()
                    .map(|arg| arg.to_string_lossy().into_owned())
                    .collect::<Vec<_>>(),
                vec!["--version".to_string()]
            );
        }
    }
}
