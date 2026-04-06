use super::command::{configure_store_git_repo_command, git_command_error, run_store_git_command};
use super::repository::has_git_repository;
use crate::backend::available_standard_public_certs;
use crate::fido2_recipient::FIDO2_RECIPIENTS_FILE_NAME;
use crate::logging::{run_command_with_input, CommandLogOptions};
use crate::preferences::Preferences;
use crate::store::recipients::{
    normalize_standard_recipient, parse_fido2_recipients, parse_standard_recipients,
};
use sequoia_openpgp::parse::stream::{
    DetachedVerifierBuilder, MessageLayer, MessageStructure, VerificationError, VerificationHelper,
};
use sequoia_openpgp::parse::Parse;
use sequoia_openpgp::policy::StandardPolicy;
use sequoia_openpgp::{Cert, Fingerprint, Result as OpenPgpResult};
use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
use std::path::Path;
use std::process::Command;
use std::rc::Rc;

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
    pub used_commit_history_fallback: bool,
    pub reason: Option<StoreGitAuditUnverifiedReason>,
    pub signer_fingerprint: Option<String>,
    pub signer_label: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct CommitSummary {
    oid: String,
    short_oid: String,
    subject: String,
    author: String,
    authored_at: String,
    committer: String,
    committed_at: String,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
struct TreeRecipientContext {
    resolved_standard_fingerprints: HashSet<String>,
    standard_recipient_count: usize,
    fido2_recipient_count: usize,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct ParsedCommitObject {
    unsigned_bytes: Vec<u8>,
    signature: Option<String>,
    message: String,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
struct SignatureVerificationState {
    signer_fingerprint: Option<String>,
    signer_label: Option<String>,
    missing_key: bool,
    invalid_signature: bool,
    malformed_signature: bool,
}

#[derive(Clone)]
struct SignatureVerificationHelper {
    certs: Vec<Cert>,
    state: Rc<RefCell<SignatureVerificationState>>,
}

impl VerificationHelper for SignatureVerificationHelper {
    fn get_certs(&mut self, _ids: &[sequoia_openpgp::KeyHandle]) -> OpenPgpResult<Vec<Cert>> {
        Ok(self.certs.clone())
    }

    fn check(&mut self, structure: MessageStructure) -> OpenPgpResult<()> {
        for layer in structure.into_iter() {
            let MessageLayer::SignatureGroup { results } = layer else {
                continue;
            };

            for result in results {
                match result {
                    Ok(good) => {
                        let mut state = self.state.borrow_mut();
                        state.signer_fingerprint = Some(good.ka.cert().fingerprint().to_hex());
                        state.signer_label = cert_primary_user_id(good.ka.cert());
                    }
                    Err(VerificationError::MissingKey { .. }) => {
                        self.state.borrow_mut().missing_key = true;
                    }
                    Err(
                        VerificationError::BadKey { .. }
                        | VerificationError::BadSignature { .. }
                        | VerificationError::UnboundKey { .. },
                    ) => {
                        self.state.borrow_mut().invalid_signature = true;
                    }
                    Err(
                        VerificationError::MalformedSignature { .. }
                        | VerificationError::UnknownSignature { .. },
                    ) => {
                        self.state.borrow_mut().malformed_signature = true;
                    }
                    Err(_) => {
                        self.state.borrow_mut().invalid_signature = true;
                    }
                }
            }
        }

        Ok(())
    }
}

pub fn discover_store_git_audit_catalog(
    store_roots: &[String],
) -> Result<StoreGitAuditCatalog, String> {
    let mut stores = Vec::new();
    for store_root in store_roots {
        if !has_git_repository(store_root) {
            continue;
        }

        let mut branches = list_audit_refs_for_prefix(store_root, "refs/heads", false)?;
        let mut remote_branches = list_audit_refs_for_prefix(store_root, "refs/remotes", true)?;
        sort_audit_refs(&mut branches);
        sort_audit_refs(&mut remote_branches);
        branches.extend(remote_branches);
        stores.push(StoreGitAuditStore {
            store_root: store_root.clone(),
            branches,
        });
    }

    Ok(StoreGitAuditCatalog { stores })
}

pub fn load_store_git_audit_commit_page(
    store_root: &str,
    full_ref: &str,
    use_commit_history_recipients: bool,
    page: usize,
) -> Result<StoreGitAuditCommitPage, String> {
    let all_certs = available_standard_public_certs()?;
    let branch_tip_context = load_tree_recipient_context(store_root, full_ref, &all_certs)?;
    let summaries = read_commit_summaries(store_root, full_ref, page)?;
    let has_more = summaries.len() > STORE_GIT_AUDIT_PAGE_SIZE;
    let summaries = summaries
        .into_iter()
        .take(STORE_GIT_AUDIT_PAGE_SIZE)
        .collect::<Vec<_>>();
    let raw_commits = read_raw_commits(
        store_root,
        &summaries
            .iter()
            .map(|summary| summary.oid.clone())
            .collect::<Vec<_>>(),
    )?;

    let mut commits = Vec::with_capacity(summaries.len());
    for summary in summaries {
        let raw_commit = raw_commits
            .get(&summary.oid)
            .ok_or_else(|| format!("Missing raw commit data for {}.", summary.oid))?;
        let parsed = parse_commit_object(raw_commit)?;
        let changed_paths = read_commit_changed_paths(store_root, &summary.oid)?;
        let verification = verify_commit(
            store_root,
            full_ref,
            &summary.oid,
            &parsed,
            &branch_tip_context,
            &all_certs,
            use_commit_history_recipients,
        )?;

        commits.push(StoreGitAuditCommit {
            oid: summary.oid,
            short_oid: summary.short_oid,
            subject: summary.subject,
            author: summary.author,
            authored_at: summary.authored_at,
            committer: summary.committer,
            committed_at: summary.committed_at,
            message: parsed.message,
            changed_paths,
            verification,
        });
    }

    Ok(StoreGitAuditCommitPage {
        commits,
        has_more,
        next_page: page + 1,
    })
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

fn list_audit_refs_for_prefix(
    store_root: &str,
    prefix: &str,
    remote: bool,
) -> Result<Vec<StoreGitAuditBranchRef>, String> {
    let output = run_store_git_command(
        store_root,
        &format!("List password store Git audit refs under {prefix}"),
        |cmd| {
            cmd.arg("for-each-ref")
                .arg("--format=%(refname)%00%(refname:short)%00")
                .arg(prefix);
        },
        CommandLogOptions::DEFAULT,
    )?;
    if !output.status.success() {
        return Err(git_command_error("git for-each-ref", &output));
    }

    Ok(parse_audit_refs(&output.stdout, remote))
}

fn parse_audit_refs(output: &[u8], remote: bool) -> Vec<StoreGitAuditBranchRef> {
    output
        .split(|byte| *byte == 0)
        .collect::<Vec<_>>()
        .chunks_exact(2)
        .filter_map(|chunk| {
            let full_ref = String::from_utf8_lossy(chunk[0]).trim().to_string();
            let name = String::from_utf8_lossy(chunk[1]).trim().to_string();
            if full_ref.is_empty() || name.is_empty() || full_ref.ends_with("/HEAD") {
                return None;
            }

            Some(StoreGitAuditBranchRef {
                full_ref,
                name,
                remote,
            })
        })
        .collect()
}

fn sort_audit_refs(refs: &mut [StoreGitAuditBranchRef]) {
    refs.sort_by(|left, right| left.name.cmp(&right.name));
}

fn read_commit_summaries(
    store_root: &str,
    full_ref: &str,
    page: usize,
) -> Result<Vec<CommitSummary>, String> {
    let skip = page.saturating_mul(STORE_GIT_AUDIT_PAGE_SIZE);
    let limit = STORE_GIT_AUDIT_PAGE_SIZE + 1;
    let output = run_store_git_command(
        store_root,
        &format!("Read password store Git audit log for {full_ref}"),
        |cmd| {
            cmd.arg("log")
                .arg(full_ref)
                .arg(format!("--skip={skip}"))
                .arg(format!("-n{limit}"))
                .arg("--format=%H%x00%h%x00%s%x00%an <%ae>%x00%aI%x00%cn <%ce>%x00%cI%x00");
        },
        CommandLogOptions::DEFAULT,
    )?;
    if !output.status.success() {
        return Err(git_command_error("git log", &output));
    }

    Ok(parse_commit_summaries(&output.stdout))
}

fn parse_commit_summaries(output: &[u8]) -> Vec<CommitSummary> {
    output
        .split(|byte| *byte == 0)
        .collect::<Vec<_>>()
        .chunks_exact(7)
        .filter_map(|chunk| {
            let oid = String::from_utf8_lossy(chunk[0]).trim().to_string();
            if oid.is_empty() {
                return None;
            }

            Some(CommitSummary {
                oid,
                short_oid: String::from_utf8_lossy(chunk[1]).trim().to_string(),
                subject: String::from_utf8_lossy(chunk[2]).trim().to_string(),
                author: String::from_utf8_lossy(chunk[3]).trim().to_string(),
                authored_at: String::from_utf8_lossy(chunk[4]).trim().to_string(),
                committer: String::from_utf8_lossy(chunk[5]).trim().to_string(),
                committed_at: String::from_utf8_lossy(chunk[6]).trim().to_string(),
            })
        })
        .collect()
}

fn run_store_git_command_with_input(
    store_root: &str,
    context: &str,
    input: &str,
    configure: impl FnOnce(&mut Command),
) -> Result<std::process::Output, String> {
    let mut cmd = Preferences::git_command();
    configure_store_git_repo_command(&mut cmd, store_root);
    configure(&mut cmd);
    run_command_with_input(&mut cmd, context, input, CommandLogOptions::DEFAULT)
        .map_err(|err| format!("Failed to run git command: {err}"))
}

fn read_raw_commits(store_root: &str, oids: &[String]) -> Result<HashMap<String, Vec<u8>>, String> {
    if oids.is_empty() {
        return Ok(HashMap::new());
    }

    let input = format!("{}\n", oids.join("\n"));
    let output = run_store_git_command_with_input(
        store_root,
        "Read raw password store Git commits",
        &input,
        |cmd| {
            cmd.arg("cat-file").arg("--batch");
        },
    )?;
    if !output.status.success() {
        return Err(git_command_error("git cat-file --batch", &output));
    }

    parse_raw_commit_batch(&output.stdout)
}

fn parse_raw_commit_batch(output: &[u8]) -> Result<HashMap<String, Vec<u8>>, String> {
    let mut commits = HashMap::new();
    let mut cursor = 0;

    while cursor < output.len() {
        let Some(header_end) = output[cursor..].iter().position(|byte| *byte == b'\n') else {
            return Err("Invalid git cat-file batch output.".to_string());
        };
        let header = &output[cursor..cursor + header_end];
        cursor += header_end + 1;
        if header.is_empty() {
            continue;
        }

        let header_text = String::from_utf8_lossy(header).to_string();
        let mut parts = header_text.split_whitespace();
        let oid = parts
            .next()
            .ok_or_else(|| "Invalid git cat-file header.".to_string())?
            .to_string();
        let object_type = parts
            .next()
            .ok_or_else(|| "Invalid git cat-file header.".to_string())?;
        if object_type != "commit" {
            return Err(format!(
                "Unexpected git cat-file object type '{object_type}'."
            ));
        }
        let size = parts
            .next()
            .ok_or_else(|| "Invalid git cat-file header.".to_string())?
            .parse::<usize>()
            .map_err(|err| err.to_string())?;
        if cursor + size > output.len() {
            return Err("Truncated git cat-file batch output.".to_string());
        }

        let contents = output[cursor..cursor + size].to_vec();
        cursor += size;
        if cursor < output.len() && output[cursor] == b'\n' {
            cursor += 1;
        }

        commits.insert(oid, contents);
    }

    Ok(commits)
}

fn read_commit_changed_paths(
    store_root: &str,
    oid: &str,
) -> Result<Vec<StoreGitAuditPathChange>, String> {
    let output = run_store_git_command(
        store_root,
        &format!("Read changed paths for password store Git commit {oid}"),
        |cmd| {
            cmd.arg("show")
                .arg("--name-status")
                .arg("--format=")
                .arg("--no-ext-diff")
                .arg("--no-color")
                .arg(oid);
        },
        CommandLogOptions::DEFAULT,
    )?;
    if !output.status.success() {
        return Err(git_command_error("git show --name-status", &output));
    }

    Ok(String::from_utf8_lossy(&output.stdout)
        .lines()
        .filter_map(parse_changed_path_line)
        .collect())
}

fn parse_changed_path_line(line: &str) -> Option<StoreGitAuditPathChange> {
    let trimmed = line.trim();
    if trimmed.is_empty() {
        return None;
    }

    let mut parts = trimmed.split('\t');
    let status = parts.next()?.trim().to_string();
    let paths = parts.collect::<Vec<_>>();
    if paths.is_empty() {
        return None;
    }

    let path = if paths.len() == 1 {
        paths[0].to_string()
    } else {
        format!("{} -> {}", paths[0], paths[paths.len() - 1])
    };

    Some(StoreGitAuditPathChange { status, path })
}

fn parse_commit_object(raw_commit: &[u8]) -> Result<ParsedCommitObject, String> {
    let Some(separator) = raw_commit.windows(2).position(|window| window == b"\n\n") else {
        return Err("Malformed Git commit object.".to_string());
    };
    let headers = &raw_commit[..separator];
    let message = &raw_commit[separator + 2..];
    let mut unsigned = Vec::new();
    let mut signature = None;
    let lines = headers.split(|byte| *byte == b'\n').collect::<Vec<_>>();
    let mut index = 0;

    while index < lines.len() {
        let line = lines[index];
        if line.is_empty() {
            index += 1;
            continue;
        }
        if line.starts_with(b" ") {
            return Err("Malformed Git commit headers.".to_string());
        }

        let mut block = vec![line];
        index += 1;
        while index < lines.len() && lines[index].starts_with(b" ") {
            block.push(lines[index]);
            index += 1;
        }

        if let Some(first_line) = line.strip_prefix(b"gpgsig ") {
            let mut signature_bytes = Vec::new();
            signature_bytes.extend_from_slice(first_line);
            signature_bytes.push(b'\n');
            for continuation in block.iter().skip(1) {
                let Some(content) = continuation.strip_prefix(b" ") else {
                    return Err("Malformed Git signature header.".to_string());
                };
                signature_bytes.extend_from_slice(content);
                signature_bytes.push(b'\n');
            }
            signature = Some(String::from_utf8(signature_bytes).map_err(|err| err.to_string())?);
            continue;
        }

        unsigned.extend_from_slice(line);
        unsigned.push(b'\n');
        for continuation in block.iter().skip(1) {
            unsigned.extend_from_slice(continuation);
            unsigned.push(b'\n');
        }
    }

    unsigned.push(b'\n');
    unsigned.extend_from_slice(message);

    Ok(ParsedCommitObject {
        unsigned_bytes: unsigned,
        signature,
        message: String::from_utf8_lossy(message).to_string(),
    })
}

fn verify_commit(
    store_root: &str,
    _full_ref: &str,
    oid: &str,
    parsed: &ParsedCommitObject,
    branch_tip_context: &TreeRecipientContext,
    all_certs: &[Cert],
    use_commit_history_recipients: bool,
) -> Result<StoreGitAuditVerification, String> {
    let Some(signature) = parsed.signature.as_ref() else {
        return Ok(unverified(
            StoreGitAuditVerificationMode::BranchTipRecipients,
            false,
            StoreGitAuditUnverifiedReason::NoSignature,
            None,
            None,
        ));
    };

    let crypto = verify_commit_signature(&parsed.unsigned_bytes, signature, all_certs);
    let CryptoVerification {
        signer_fingerprint,
        signer_label,
        reason,
    } = crypto;

    let Some(signer_fingerprint) = signer_fingerprint else {
        let reason = reason.unwrap_or(StoreGitAuditUnverifiedReason::InvalidSignature);
        return Ok(unverified(
            StoreGitAuditVerificationMode::BranchTipRecipients,
            false,
            reason,
            None,
            signer_label,
        ));
    };

    if let Some(reason) = authorize_signer(branch_tip_context, &signer_fingerprint) {
        if should_retry_with_commit_history_recipients(reason, use_commit_history_recipients) {
            let historical_context = load_tree_recipient_context(store_root, oid, all_certs)?;
            if authorize_signer(&historical_context, &signer_fingerprint).is_none() {
                return Ok(StoreGitAuditVerification {
                    state: StoreGitAuditVerificationState::Verified,
                    mode: StoreGitAuditVerificationMode::CommitHistoryRecipients,
                    used_commit_history_fallback: true,
                    reason: None,
                    signer_fingerprint: Some(signer_fingerprint),
                    signer_label,
                });
            }
        }

        return Ok(unverified(
            StoreGitAuditVerificationMode::BranchTipRecipients,
            false,
            reason,
            Some(signer_fingerprint),
            signer_label,
        ));
    }

    Ok(StoreGitAuditVerification {
        state: StoreGitAuditVerificationState::Verified,
        mode: StoreGitAuditVerificationMode::BranchTipRecipients,
        used_commit_history_fallback: false,
        reason: None,
        signer_fingerprint: Some(signer_fingerprint),
        signer_label,
    })
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
struct CryptoVerification {
    signer_fingerprint: Option<String>,
    signer_label: Option<String>,
    reason: Option<StoreGitAuditUnverifiedReason>,
}

fn verify_commit_signature(
    unsigned_bytes: &[u8],
    signature: &str,
    all_certs: &[Cert],
) -> CryptoVerification {
    let state = Rc::new(RefCell::new(SignatureVerificationState::default()));
    let helper = SignatureVerificationHelper {
        certs: all_certs.to_vec(),
        state: state.clone(),
    };
    let policy = StandardPolicy::new();

    let Ok(builder) = DetachedVerifierBuilder::from_bytes(signature.as_bytes()) else {
        return CryptoVerification {
            reason: Some(StoreGitAuditUnverifiedReason::MalformedSignature),
            ..CryptoVerification::default()
        };
    };
    let Ok(mut verifier) = builder.with_policy(&policy, None, helper) else {
        return CryptoVerification {
            reason: Some(StoreGitAuditUnverifiedReason::MalformedSignature),
            ..CryptoVerification::default()
        };
    };

    let verify_result = verifier.verify_bytes(unsigned_bytes);
    let state = state.borrow().clone();
    if let Some(signer_fingerprint) = state.signer_fingerprint {
        return CryptoVerification {
            signer_fingerprint: Some(signer_fingerprint),
            signer_label: state.signer_label,
            reason: None,
        };
    }
    if state.missing_key {
        return CryptoVerification {
            reason: Some(StoreGitAuditUnverifiedReason::SigningKeyUnavailable),
            signer_label: state.signer_label,
            ..CryptoVerification::default()
        };
    }
    if state.malformed_signature {
        return CryptoVerification {
            reason: Some(StoreGitAuditUnverifiedReason::MalformedSignature),
            signer_label: state.signer_label,
            ..CryptoVerification::default()
        };
    }
    if state.invalid_signature || verify_result.is_err() {
        return CryptoVerification {
            reason: Some(StoreGitAuditUnverifiedReason::InvalidSignature),
            signer_label: state.signer_label,
            ..CryptoVerification::default()
        };
    }

    CryptoVerification {
        reason: Some(StoreGitAuditUnverifiedReason::InvalidSignature),
        ..CryptoVerification::default()
    }
}

fn unverified(
    mode: StoreGitAuditVerificationMode,
    used_commit_history_fallback: bool,
    reason: StoreGitAuditUnverifiedReason,
    signer_fingerprint: Option<String>,
    signer_label: Option<String>,
) -> StoreGitAuditVerification {
    StoreGitAuditVerification {
        state: StoreGitAuditVerificationState::Unverified,
        mode,
        used_commit_history_fallback,
        reason: Some(reason),
        signer_fingerprint,
        signer_label,
    }
}

fn authorize_signer(
    context: &TreeRecipientContext,
    signer_fingerprint: &str,
) -> Option<StoreGitAuditUnverifiedReason> {
    if context
        .resolved_standard_fingerprints
        .contains(&signer_fingerprint.to_ascii_uppercase())
    {
        return None;
    }
    if context.standard_recipient_count == 0 && context.fido2_recipient_count > 0 {
        return Some(StoreGitAuditUnverifiedReason::OnlyFido2Recipients);
    }
    if context.resolved_standard_fingerprints.is_empty() {
        return Some(StoreGitAuditUnverifiedReason::NoResolvableStandardRecipients);
    }

    Some(StoreGitAuditUnverifiedReason::SignerNotAuthorized)
}

fn should_retry_with_commit_history_recipients(
    reason: StoreGitAuditUnverifiedReason,
    enabled: bool,
) -> bool {
    enabled
        && matches!(
            reason,
            StoreGitAuditUnverifiedReason::SignerNotAuthorized
                | StoreGitAuditUnverifiedReason::NoResolvableStandardRecipients
                | StoreGitAuditUnverifiedReason::OnlyFido2Recipients
        )
}

fn load_tree_recipient_context(
    store_root: &str,
    object: &str,
    certs: &[Cert],
) -> Result<TreeRecipientContext, String> {
    let recipient_paths = list_tree_recipient_paths(store_root, object)?;
    let mut standard_recipients = Vec::new();
    let mut fido2_recipients = Vec::new();

    for path in recipient_paths {
        let contents = read_tree_file(store_root, object, &path)?;
        let file_name = Path::new(&path)
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or_default();
        if file_name == ".gpg-id" {
            standard_recipients.extend(parse_standard_recipients(&contents));
        } else if file_name == FIDO2_RECIPIENTS_FILE_NAME {
            fido2_recipients.extend(parse_fido2_recipients(&contents));
        }
    }

    standard_recipients.sort();
    standard_recipients.dedup();
    fido2_recipients.sort();
    fido2_recipients.dedup();

    let resolved_standard_fingerprints = standard_recipients
        .iter()
        .filter_map(|recipient| resolve_standard_recipient_fingerprint(recipient, certs))
        .collect::<HashSet<_>>();

    Ok(TreeRecipientContext {
        resolved_standard_fingerprints,
        standard_recipient_count: standard_recipients.len(),
        fido2_recipient_count: fido2_recipients.len(),
    })
}

fn list_tree_recipient_paths(store_root: &str, object: &str) -> Result<Vec<String>, String> {
    let output = run_store_git_command(
        store_root,
        &format!("List recipient files for password store Git tree {object}"),
        |cmd| {
            cmd.arg("ls-tree")
                .arg("-r")
                .arg("-z")
                .arg("--name-only")
                .arg(object);
        },
        CommandLogOptions::DEFAULT,
    )?;
    if !output.status.success() {
        return Err(git_command_error("git ls-tree", &output));
    }

    Ok(output
        .stdout
        .split(|byte| *byte == 0)
        .filter_map(|entry| {
            let path = String::from_utf8_lossy(entry).trim().to_string();
            if path.is_empty() {
                return None;
            }
            let file_name = Path::new(&path)
                .file_name()
                .and_then(|name| name.to_str())
                .unwrap_or_default();
            if file_name == ".gpg-id" || file_name == FIDO2_RECIPIENTS_FILE_NAME {
                Some(path)
            } else {
                None
            }
        })
        .collect())
}

fn read_tree_file(store_root: &str, object: &str, path: &str) -> Result<String, String> {
    let object_path = format!("{object}:{path}");
    let output = run_store_git_command(
        store_root,
        &format!("Read password store Git tree file {object_path}"),
        |cmd| {
            cmd.arg("show").arg(&object_path);
        },
        CommandLogOptions::DEFAULT,
    )?;
    if !output.status.success() {
        return Err(git_command_error("git show", &output));
    }

    Ok(String::from_utf8_lossy(&output.stdout).to_string())
}

fn resolve_standard_recipient_fingerprint(recipient: &str, certs: &[Cert]) -> Option<String> {
    let normalized = normalize_standard_recipient(recipient);
    if normalized.is_empty() {
        return None;
    }

    if let Ok(expected) = Fingerprint::from_hex(&normalized) {
        let matches = certs
            .iter()
            .filter(|cert| cert.fingerprint() == expected)
            .map(|cert| cert.fingerprint().to_hex())
            .collect::<Vec<_>>();
        if matches.len() == 1 {
            return matches.into_iter().next();
        }
    }

    let needle = normalized.to_ascii_lowercase();
    let matches = certs
        .iter()
        .filter(|cert| {
            cert.userids().any(|user_id| {
                standard_recipient_matches_user_id(&needle, &user_id.userid().to_string())
            })
        })
        .map(|cert| cert.fingerprint().to_hex())
        .collect::<Vec<_>>();
    if matches.len() == 1 {
        matches.into_iter().next()
    } else {
        None
    }
}

fn standard_recipient_matches_user_id(needle: &str, user_id: &str) -> bool {
    user_id.trim().eq_ignore_ascii_case(needle)
        || extracted_user_id_email(user_id).is_some_and(|email| email.eq_ignore_ascii_case(needle))
}

fn extracted_user_id_email(user_id: &str) -> Option<&str> {
    let trimmed = user_id.trim();
    let start = trimmed.rfind('<')?;
    let after_start = &trimmed[start + 1..];
    let end = after_start.find('>')?;
    let email = after_start[..end].trim();
    if email.is_empty() {
        None
    } else {
        Some(email)
    }
}

fn cert_primary_user_id(cert: &Cert) -> Option<String> {
    cert.userids()
        .next()
        .map(|user_id| user_id.userid().to_string())
        .filter(|value| !value.trim().is_empty())
}

#[cfg(test)]
mod tests {
    use super::{
        audit_unverified_reason_message, authorize_signer, parse_audit_refs,
        parse_changed_path_line, parse_commit_object, parse_commit_summaries,
        resolve_standard_recipient_fingerprint, should_retry_with_commit_history_recipients,
        sort_audit_refs, StoreGitAuditUnverifiedReason, TreeRecipientContext,
    };
    use sequoia_openpgp::{cert::CertBuilder, Cert};

    fn test_cert(user_id: &str) -> Cert {
        let (cert, _) = CertBuilder::new()
            .add_userid(user_id)
            .generate()
            .expect("generate cert");
        cert
    }

    #[test]
    fn ref_parser_skips_remote_head_and_keeps_names() {
        let refs = parse_audit_refs(
            b"refs/heads/main\0main\0refs/remotes/origin/main\0origin/main\0refs/remotes/origin/HEAD\0origin/HEAD\0",
            true,
        );

        assert_eq!(
            refs,
            vec![
                super::StoreGitAuditBranchRef {
                    full_ref: "refs/heads/main".to_string(),
                    name: "main".to_string(),
                    remote: true,
                },
                super::StoreGitAuditBranchRef {
                    full_ref: "refs/remotes/origin/main".to_string(),
                    name: "origin/main".to_string(),
                    remote: true,
                },
            ]
        );
    }

    #[test]
    fn ref_sorting_keeps_names_alphabetical() {
        let mut refs = vec![
            super::StoreGitAuditBranchRef {
                full_ref: "refs/heads/zeta".to_string(),
                name: "zeta".to_string(),
                remote: false,
            },
            super::StoreGitAuditBranchRef {
                full_ref: "refs/heads/main".to_string(),
                name: "main".to_string(),
                remote: false,
            },
        ];

        sort_audit_refs(&mut refs);

        assert_eq!(refs[0].name, "main");
        assert_eq!(refs[1].name, "zeta");
    }

    #[test]
    fn commit_summary_parser_reads_fixed_fields() {
        let summaries = parse_commit_summaries(
            b"abc\0abc\0Subject\0Alice <a@example.com>\02026-01-01T00:00:00+00:00\0Bob <b@example.com>\02026-01-01T00:00:00+00:00\0",
        );
        assert_eq!(summaries.len(), 1);
        assert_eq!(summaries[0].oid, "abc");
        assert_eq!(summaries[0].subject, "Subject");
    }

    #[test]
    fn commit_parser_removes_gpgsig_block_from_unsigned_payload() {
        let raw = b"tree deadbeef\nauthor Alice <alice@example.com> 1 +0000\ncommitter Alice <alice@example.com> 1 +0000\ngpgsig -----BEGIN PGP SIGNATURE-----\n line-two\n -----END PGP SIGNATURE-----\n\nSubject\nBody\n";
        let parsed = parse_commit_object(raw).expect("parse commit");

        assert_eq!(
            parsed.signature.as_deref(),
            Some("-----BEGIN PGP SIGNATURE-----\nline-two\n-----END PGP SIGNATURE-----\n")
        );
        assert_eq!(
            String::from_utf8_lossy(&parsed.unsigned_bytes),
            "tree deadbeef\nauthor Alice <alice@example.com> 1 +0000\ncommitter Alice <alice@example.com> 1 +0000\n\nSubject\nBody\n"
        );
        assert_eq!(parsed.message, "Subject\nBody\n".to_string());
    }

    #[test]
    fn recipient_resolution_matches_fingerprint_user_id_and_email() {
        let cert = test_cert("Alice Example <alice@example.com>");
        let fingerprint = cert.fingerprint().to_hex();
        let certs = vec![cert];

        assert_eq!(
            resolve_standard_recipient_fingerprint(&fingerprint, &certs),
            Some(fingerprint.clone())
        );
        assert_eq!(
            resolve_standard_recipient_fingerprint("Alice Example <alice@example.com>", &certs),
            Some(fingerprint.clone())
        );
        assert_eq!(
            resolve_standard_recipient_fingerprint("alice@example.com", &certs),
            Some(fingerprint)
        );
    }

    #[test]
    fn authorization_prefers_expected_unverified_reasons() {
        assert_eq!(
            authorize_signer(
                &TreeRecipientContext {
                    standard_recipient_count: 0,
                    fido2_recipient_count: 1,
                    ..TreeRecipientContext::default()
                },
                "ABC"
            ),
            Some(StoreGitAuditUnverifiedReason::OnlyFido2Recipients)
        );
        assert_eq!(
            authorize_signer(
                &TreeRecipientContext {
                    standard_recipient_count: 1,
                    ..TreeRecipientContext::default()
                },
                "ABC"
            ),
            Some(StoreGitAuditUnverifiedReason::NoResolvableStandardRecipients)
        );
    }

    #[test]
    fn historical_retry_only_applies_to_authorization_failures() {
        assert!(should_retry_with_commit_history_recipients(
            StoreGitAuditUnverifiedReason::SignerNotAuthorized,
            true,
        ));
        assert!(should_retry_with_commit_history_recipients(
            StoreGitAuditUnverifiedReason::NoResolvableStandardRecipients,
            true,
        ));
        assert!(should_retry_with_commit_history_recipients(
            StoreGitAuditUnverifiedReason::OnlyFido2Recipients,
            true,
        ));
        assert!(!should_retry_with_commit_history_recipients(
            StoreGitAuditUnverifiedReason::InvalidSignature,
            true,
        ));
        assert!(!should_retry_with_commit_history_recipients(
            StoreGitAuditUnverifiedReason::SignerNotAuthorized,
            false,
        ));
    }

    #[test]
    fn changed_path_parser_handles_rename_lines() {
        assert_eq!(
            parse_changed_path_line("R100\told.txt\tnew.txt"),
            Some(super::StoreGitAuditPathChange {
                status: "R100".to_string(),
                path: "old.txt -> new.txt".to_string(),
            })
        );
    }

    #[test]
    fn reason_messages_are_readable() {
        assert_eq!(
            audit_unverified_reason_message(StoreGitAuditUnverifiedReason::NoSignature),
            "No signature"
        );
    }
}
