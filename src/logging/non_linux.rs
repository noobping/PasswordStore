mod store {
    pub fn log_info(_message: impl Into<String>) {}

    pub fn log_error(_message: impl Into<String>) {}

    pub fn log_snapshot() -> (usize, usize, String) {
        (0, 0, String::new())
    }
}

#[path = "command/mod.rs"]
mod command;

pub use command::run_command_status;
pub use command::run_command_with_input;
pub use command::{run_command_output, CommandLogOptions};
pub use store::{log_error, log_info, log_snapshot};
