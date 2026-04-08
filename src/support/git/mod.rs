#[cfg(feature = "audit")]
mod audit;
#[cfg(not(feature = "audit"))]
#[path = "audit_disabled.rs"]
mod audit;
mod command;
mod remotes;
mod repository;
mod status;
mod sync;
mod types;

#[cfg(test)]
pub use audit::StoreGitAuditUnverifiedReason;
pub use audit::{
    audit_unverified_reason_message, discover_store_git_audit_catalog,
    load_store_git_audit_commit_page, StoreGitAuditBranchRef, StoreGitAuditCatalog,
    StoreGitAuditCommit, StoreGitAuditCommitPage, StoreGitAuditPathChange, StoreGitAuditStore,
    StoreGitAuditVerification, StoreGitAuditVerificationMethod, StoreGitAuditVerificationMode,
    StoreGitAuditVerificationState, STORE_GIT_AUDIT_PAGE_SIZE,
};
pub use remotes::{
    add_store_git_remote, list_store_git_remotes, remove_store_git_remote, rename_store_git_remote,
    set_store_git_remote_url,
};
pub use repository::{
    ensure_store_git_repository, git_command_available, has_git_repository,
    password_store_git_state_summary,
};
pub use status::store_git_repository_status;
pub use sync::sync_store_repository;
#[cfg(test)]
pub use types::GitRemote;
pub use types::{StoreGitHead, StoreGitRepositoryStatus};

#[cfg(test)]
mod tests;
