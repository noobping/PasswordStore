mod command;
mod remotes;
mod repository;
mod status;
mod sync;
mod types;

pub use remotes::{
    add_store_git_remote, list_store_git_remotes, remove_store_git_remote, rename_store_git_remote,
    set_store_git_remote_url,
};
pub use repository::{
    ensure_store_git_repository, has_git_repository, password_store_git_state_summary,
};
pub use status::store_git_repository_status;
pub use sync::sync_store_repository;
#[cfg(test)]
pub use types::GitRemote;
pub use types::{StoreGitHead, StoreGitRepositoryStatus};

#[cfg(test)]
mod tests;
