use super::command_backend::stored_backend_kind;
use super::{BackendKind, Preferences};
use crate::support::runtime::HOST_COMMAND_FEATURES_UNSUPPORTED;
use std::process::Command;

fn unsupported_command() -> Command {
    panic!("{HOST_COMMAND_FEATURES_UNSUPPORTED}")
}

impl Preferences {
    pub fn git_command() -> Command {
        unsupported_command()
    }

    pub fn remote_git_command() -> Command {
        unsupported_command()
    }

    pub fn command(&self) -> Command {
        unsupported_command()
    }

    pub fn command_with_envs(&self, _envs: &[(&str, &str)]) -> Command {
        unsupported_command()
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
