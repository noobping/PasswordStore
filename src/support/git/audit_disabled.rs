pub const STORE_GIT_AUDIT_PAGE_SIZE: usize = 100;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StoreGitAuditCatalog {
    pub stores: Vec<StoreGitAuditStore>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StoreGitAuditStore {
    pub store_root: String,
    pub branches: Vec<StoreGitAuditBranchRef>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StoreGitAuditBranchRef {
    pub full_ref: String,
    pub name: String,
    pub remote: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StoreGitAuditCommitPage {
    pub commits: Vec<StoreGitAuditCommit>,
    pub has_more: bool,
    pub next_page: usize,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StoreGitAuditCommit {
    pub oid: String,
    pub short_oid: String,
    pub subject: String,
    pub author: String,
    pub authored_at: String,
    pub committer: String,
    pub committed_at: String,
    pub message: String,
    pub changed_paths: Vec<StoreGitAuditPathChange>,
    pub verification: StoreGitAuditVerification,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StoreGitAuditPathChange {
    pub status: String,
    pub path: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StoreGitAuditVerificationState {
    Verified,
    Unverified,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StoreGitAuditVerificationMode {
    BranchTipRecipients,
    CommitHistoryRecipients,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StoreGitAuditVerificationMethod {
    KeycordOpenPgp,
    HostGitGpg,
    HostGitSsh,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StoreGitAuditUnverifiedReason {
    NoSignature,
    MalformedSignature,
    InvalidSignature,
    SigningKeyUnavailable,
    SignerNotAuthorized,
    NoResolvableStandardRecipients,
    OnlyFido2Recipients,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StoreGitAuditVerification {
    pub state: StoreGitAuditVerificationState,
    pub mode: StoreGitAuditVerificationMode,
    pub method: Option<StoreGitAuditVerificationMethod>,
    pub used_commit_history_fallback: bool,
    pub reason: Option<StoreGitAuditUnverifiedReason>,
    pub signer_fingerprint: Option<String>,
    pub signer_label: Option<String>,
}

pub fn discover_store_git_audit_catalog(
    _store_roots: &[String],
) -> Result<StoreGitAuditCatalog, String> {
    touch_disabled_audit_types();
    Err("Audit features are disabled in this build.".to_string())
}

pub fn load_store_git_audit_commit_page(
    _store_root: &str,
    _full_ref: &str,
    _use_commit_history_recipients: bool,
    _page: usize,
) -> Result<StoreGitAuditCommitPage, String> {
    touch_disabled_audit_types();
    Err("Audit features are disabled in this build.".to_string())
}

pub fn audit_unverified_reason_message(reason: StoreGitAuditUnverifiedReason) -> &'static str {
    match reason {
        StoreGitAuditUnverifiedReason::NoSignature => "No signature",
        StoreGitAuditUnverifiedReason::MalformedSignature => "Malformed or unsupported signature",
        StoreGitAuditUnverifiedReason::InvalidSignature => "Cryptographically invalid signature",
        StoreGitAuditUnverifiedReason::SigningKeyUnavailable => {
            "Signing key not available in Keycord"
        }
        StoreGitAuditUnverifiedReason::SignerNotAuthorized => {
            "Signer not in the branch recipient set"
        }
        StoreGitAuditUnverifiedReason::NoResolvableStandardRecipients => {
            "No resolvable standard recipient keys"
        }
        StoreGitAuditUnverifiedReason::OnlyFido2Recipients => "Only FIDO2 recipients are available",
    }
}

fn touch_disabled_audit_types() {
    let _ = StoreGitAuditVerificationState::Verified;
    let _ = StoreGitAuditVerificationState::Unverified;
    let _ = StoreGitAuditVerificationMode::BranchTipRecipients;
    let _ = StoreGitAuditVerificationMode::CommitHistoryRecipients;
    let _ = [
        StoreGitAuditVerificationMethod::KeycordOpenPgp,
        StoreGitAuditVerificationMethod::HostGitGpg,
        StoreGitAuditVerificationMethod::HostGitSsh,
    ];
    let _ = [
        StoreGitAuditUnverifiedReason::NoSignature,
        StoreGitAuditUnverifiedReason::MalformedSignature,
        StoreGitAuditUnverifiedReason::InvalidSignature,
        StoreGitAuditUnverifiedReason::SigningKeyUnavailable,
        StoreGitAuditUnverifiedReason::SignerNotAuthorized,
        StoreGitAuditUnverifiedReason::NoResolvableStandardRecipients,
        StoreGitAuditUnverifiedReason::OnlyFido2Recipients,
    ];
}
