#[cfg(keycord_linux)]
mod enabled;
#[cfg(not(keycord_linux))]
mod non_linux;

#[cfg(keycord_linux)]
use self::enabled as imp;
#[cfg(not(keycord_linux))]
use self::non_linux as imp;

pub(crate) use self::imp::{
    clone_store_repository, handle_git_busy_back, register_open_git_action,
    register_synchronize_action, set_git_action_availability, GitActionState,
};
