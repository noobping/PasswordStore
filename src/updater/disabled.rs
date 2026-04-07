use adw::gtk::glib::ExitCode;
use std::ffi::OsString;

pub fn handle_special_command(_args: &[OsString]) -> Option<ExitCode> {
    None
}
