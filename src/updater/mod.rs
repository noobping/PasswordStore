mod logic;
mod windows;

use adw::{Application, ApplicationWindow, ToastOverlay};
use std::rc::Rc;

pub type DirtyProbe = Rc<dyn Fn() -> bool>;

pub use self::windows::{after_window_presented, register_app_actions, register_window};
