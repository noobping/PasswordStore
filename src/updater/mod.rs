#[cfg(any(
    target_os = "windows",
    all(
        target_os = "linux",
        feature = "linux-updater",
        not(feature = "flatpak")
    )
))]
mod common;
#[cfg(any(
    target_os = "windows",
    all(
        target_os = "linux",
        feature = "linux-updater",
        not(feature = "flatpak")
    )
))]
mod logic;
#[cfg(not(any(
    target_os = "windows",
    all(
        target_os = "linux",
        feature = "linux-updater",
        not(feature = "flatpak")
    )
)))]
#[path = "disabled.rs"]
mod platform;
#[cfg(all(
    target_os = "linux",
    feature = "linux-updater",
    not(feature = "flatpak")
))]
#[path = "linux.rs"]
mod platform;
#[cfg(target_os = "windows")]
#[path = "windows.rs"]
mod platform;

use adw::gtk::glib::ExitCode;
#[cfg(not(any(
    target_os = "windows",
    all(
        target_os = "linux",
        feature = "linux-updater",
        not(feature = "flatpak")
    )
)))]
use adw::{Application, ApplicationWindow, ToastOverlay};
use std::ffi::OsString;
use std::rc::Rc;

pub type DirtyProbe = Rc<dyn Fn() -> bool>;

#[cfg(any(
    target_os = "windows",
    all(
        target_os = "linux",
        feature = "linux-updater",
        not(feature = "flatpak")
    )
))]
pub use self::common::{after_window_presented, register_app_actions, register_window, shutdown};

#[cfg(not(any(
    target_os = "windows",
    all(
        target_os = "linux",
        feature = "linux-updater",
        not(feature = "flatpak")
    )
)))]
pub fn register_app_actions(_app: &Application) {}

#[cfg(not(any(
    target_os = "windows",
    all(
        target_os = "linux",
        feature = "linux-updater",
        not(feature = "flatpak")
    )
)))]
pub fn register_window(
    _app: &Application,
    _window: &ApplicationWindow,
    _overlay: &ToastOverlay,
    _has_unsaved_changes: DirtyProbe,
) {
}

#[cfg(not(any(
    target_os = "windows",
    all(
        target_os = "linux",
        feature = "linux-updater",
        not(feature = "flatpak")
    )
)))]
pub fn after_window_presented(_app: &Application, _window: &ApplicationWindow) {}

#[cfg(not(any(
    target_os = "windows",
    all(
        target_os = "linux",
        feature = "linux-updater",
        not(feature = "flatpak")
    )
)))]
pub fn shutdown(_app: &Application) {}

pub fn handle_special_command(args: &[OsString]) -> Option<ExitCode> {
    platform::handle_special_command(args)
}
