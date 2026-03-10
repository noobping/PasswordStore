mod run;
mod streams;

#[cfg(all(feature = "flatpak", test))]
pub(crate) use self::run::run_command_output;
#[cfg(not(feature = "flatpak"))]
pub(crate) use self::run::{run_command_output, run_command_status, run_command_with_input};

#[derive(Clone, Copy, Debug, Default)]
pub struct CommandLogOptions {
    pub redact_stdout: bool,
    pub redact_stdin: bool,
}

impl CommandLogOptions {
    pub const DEFAULT: Self = Self {
        redact_stdout: false,
        redact_stdin: false,
    };

    #[cfg(not(feature = "flatpak"))]
    pub const SENSITIVE: Self = Self {
        redact_stdout: true,
        redact_stdin: true,
    };
}
