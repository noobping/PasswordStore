mod common;
mod logic;
#[cfg(not(any(
    target_os = "windows",
    all(target_os = "linux", feature = "setup", not(feature = "flatpak"))
)))]
#[path = "disabled.rs"]
mod platform;
#[cfg(all(target_os = "linux", feature = "setup", not(feature = "flatpak")))]
#[path = "linux.rs"]
mod platform;
#[cfg(target_os = "windows")]
#[path = "windows.rs"]
mod platform;

use adw::gtk::glib::ExitCode;
use std::ffi::OsString;
use std::rc::Rc;

pub type DirtyProbe = Rc<dyn Fn() -> bool>;

pub use self::common::{after_window_presented, register_app_actions, register_window, shutdown};

pub fn handle_special_command(args: &[OsString]) -> Option<ExitCode> {
    platform::handle_special_command(args)
}
