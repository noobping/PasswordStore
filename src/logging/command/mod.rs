mod run;
mod streams;

pub use self::run::run_command_output;
pub use self::run::run_command_status;
pub use self::run::run_command_with_input;

#[derive(Clone, Copy, Debug, Default)]
pub struct CommandLogOptions {
    pub redact_stdout: bool,
    pub redact_stderr: bool,
    pub redact_stdin: bool,
    pub accepted_exit_codes: &'static [i32],
}

impl CommandLogOptions {
    pub const DEFAULT: Self = Self {
        redact_stdout: false,
        redact_stderr: false,
        redact_stdin: false,
        accepted_exit_codes: &[],
    };

    pub const SENSITIVE: Self = Self {
        redact_stdout: true,
        redact_stderr: true,
        redact_stdin: true,
        accepted_exit_codes: &[],
    };
}
