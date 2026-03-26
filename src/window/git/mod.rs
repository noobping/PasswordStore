#[cfg(not(target_os = "linux"))]
mod disabled;
#[cfg(target_os = "linux")]
mod enabled;

#[cfg(not(target_os = "linux"))]
use self::disabled as imp;
#[cfg(target_os = "linux")]
use self::enabled as imp;

pub use self::imp::{
    clone_store_repository, handle_git_busy_back, register_open_git_action,
    register_synchronize_action, set_git_action_availability, GitActionState,
};
