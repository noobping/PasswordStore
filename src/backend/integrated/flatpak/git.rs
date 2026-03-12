use super::super::keys::{
    cached_unlocked_ripasso_private_key, list_ripasso_private_keys, ManagedRipassoPrivateKey,
};
use crate::logging::{log_error, run_command_output, run_command_with_input, CommandLogOptions};
use crate::preferences::Preferences;
use crate::support::git::has_git_repository;
use sequoia_openpgp::policy::StandardPolicy;
use sequoia_openpgp::serialize::stream::{Armorer, Message, Signer};
use sequoia_openpgp::{self as openpgp, Cert};
use std::io::Cursor;
use std::process::{Command, Output};
use std::sync::Arc;

#[derive(Clone, Debug, PartialEq, Eq)]
struct CommitIdentity {
    name: String,
    email: String,
    signing_fingerprint: Option<String>,
}

fn git_command_error(action: &str, output: &Output) -> String {
    let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
    let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
    if !stderr.is_empty() {
        format!("{action} failed: {stderr}")
    } else if !stdout.is_empty() {
        format!("{action} failed: {stdout}")
    } else {
        format!("{action} failed: {}", output.status)
    }
}

fn run_store_git_command(
    store_root: &str,
    context: &str,
    configure: impl FnOnce(&mut Command),
) -> Result<Output, String> {
    let settings = Preferences::new();
    let mut cmd = settings.git_command();
    cmd.arg("-C").arg(store_root);
    configure(&mut cmd);
    run_command_output(&mut cmd, context, CommandLogOptions::DEFAULT)
        .map_err(|err| format!("Failed to run git command: {err}"))
}

fn run_store_git_command_with_input(
    store_root: &str,
    context: &str,
    input: &str,
    configure: impl FnOnce(&mut Command),
) -> Result<Output, String> {
    let settings = Preferences::new();
    let mut cmd = settings.git_command();
    cmd.arg("-C").arg(store_root);
    configure(&mut cmd);
    run_command_with_input(&mut cmd, context, input, CommandLogOptions::DEFAULT)
        .map_err(|err| format!("Failed to run git command: {err}"))
}

fn stage_git_paths(store_root: &str, paths: &[String]) -> Result<(), String> {
    let output = run_store_git_command(store_root, "Stage password store Git changes", |cmd| {
        cmd.arg("add").arg("-A").arg("--").args(paths);
    })?;
    if output.status.success() {
        Ok(())
    } else {
        Err(git_command_error("git add", &output))
    }
}

fn staged_git_paths_have_changes(store_root: &str, paths: &[String]) -> Result<bool, String> {
    let output = run_store_git_command(
        store_root,
        "Check staged password store Git changes",
        |cmd| {
            cmd.args(["diff", "--cached", "--quiet", "--exit-code", "--"])
                .args(paths);
        },
    )?;
    match output.status.code() {
        Some(0) => Ok(false),
        Some(1) => Ok(true),
        _ => Err(git_command_error("git diff --cached", &output)),
    }
}

fn git_output_text(output: &Output) -> Result<String, String> {
    let text = String::from_utf8(output.stdout.clone()).map_err(|err| err.to_string())?;
    let trimmed = text.trim().to_string();
    if trimmed.is_empty() {
        Err("Git command did not return output.".to_string())
    } else {
        Ok(trimmed)
    }
}

fn write_git_tree(store_root: &str) -> Result<String, String> {
    let output = run_store_git_command(store_root, "Write password store Git tree", |cmd| {
        cmd.arg("write-tree");
    })?;
    if output.status.success() {
        git_output_text(&output)
    } else {
        Err(git_command_error("git write-tree", &output))
    }
}

fn head_oid(store_root: &str) -> Result<Option<String>, String> {
    let output = run_store_git_command(store_root, "Read password store Git HEAD", |cmd| {
        cmd.args(["rev-parse", "--verify", "HEAD"]);
    })?;
    if output.status.success() {
        git_output_text(&output).map(Some)
    } else {
        Ok(None)
    }
}

fn git_ident(store_root: &str, role: &str, identity: &CommitIdentity) -> Result<String, String> {
    let env_prefix = role.to_ascii_uppercase();
    let output =
        run_store_git_command(store_root, &format!("Resolve Git {role} identity"), |cmd| {
            cmd.env(format!("GIT_{env_prefix}_NAME"), &identity.name)
                .env(format!("GIT_{env_prefix}_EMAIL"), &identity.email)
                .arg("var")
                .arg(format!("GIT_{env_prefix}_IDENT"));
        })?;
    if output.status.success() {
        git_output_text(&output)
    } else {
        Err(git_command_error("git var", &output))
    }
}

fn preferred_commit_private_key(
    explicit_fingerprint: Option<&str>,
) -> Result<Option<ManagedRipassoPrivateKey>, String> {
    let Some(explicit_fingerprint) = explicit_fingerprint else {
        return Ok(None);
    };
    let keys = list_ripasso_private_keys()?;
    Ok(preferred_commit_private_key_from_values(
        explicit_fingerprint,
        &keys,
    ))
}

fn preferred_commit_private_key_from_values(
    explicit: &str,
    keys: &[ManagedRipassoPrivateKey],
) -> Option<ManagedRipassoPrivateKey> {
    keys.iter()
        .find(|key| key.fingerprint.eq_ignore_ascii_case(explicit))
        .cloned()
}

fn synthetic_commit_email(fingerprint: &str) -> String {
    format!("git+{}@keycord.invalid", fingerprint.to_ascii_lowercase())
}

fn parse_private_key_user_id(user_id: &str, fingerprint: &str) -> (String, String) {
    let trimmed = user_id.trim();
    if trimmed.is_empty() {
        return (fingerprint.to_string(), synthetic_commit_email(fingerprint));
    }

    let email = if let Some(start) = trimmed.rfind('<') {
        trimmed[start + 1..]
            .find('>')
            .map(|offset| trimmed[start + 1..start + 1 + offset].trim().to_string())
            .filter(|value| !value.is_empty())
    } else if trimmed.contains('@') && !trimmed.contains(' ') {
        Some(trimmed.to_string())
    } else {
        None
    };

    let name = if let Some(start) = trimmed.rfind('<') {
        trimmed[..start].trim().trim_matches('"').to_string()
    } else {
        trimmed.to_string()
    };

    let email = email.unwrap_or_else(|| synthetic_commit_email(fingerprint));
    let name = if name.is_empty() { email.clone() } else { name };

    (name, email)
}

fn commit_identity(explicit_fingerprint: Option<&str>) -> Result<CommitIdentity, String> {
    if let Some(key) = preferred_commit_private_key(explicit_fingerprint)? {
        return Ok(commit_identity_from_private_key(&key));
    }

    Ok(CommitIdentity {
        name: "Keycord".to_string(),
        email: "git@keycord.invalid".to_string(),
        signing_fingerprint: None,
    })
}

fn commit_identity_from_private_key(key: &ManagedRipassoPrivateKey) -> CommitIdentity {
    let (name, email) = key
        .user_ids
        .iter()
        .find(|user_id| !user_id.trim().is_empty())
        .map(|user_id| parse_private_key_user_id(user_id, &key.fingerprint))
        .unwrap_or_else(|| {
            (
                key.fingerprint.clone(),
                synthetic_commit_email(&key.fingerprint),
            )
        });

    CommitIdentity {
        name,
        email,
        signing_fingerprint: Some(key.fingerprint.clone()),
    }
}

fn unlocked_signing_cert(fingerprint: &str) -> Result<Option<Arc<Cert>>, String> {
    cached_unlocked_ripasso_private_key(fingerprint)
}

fn sign_commit_buffer(commit_buffer: &str, cert: &Cert) -> Result<String, String> {
    let policy = StandardPolicy::new();
    let signing_key = cert
        .keys()
        .with_policy(&policy, None)
        .alive()
        .revoked(false)
        .for_signing()
        .secret()
        .next()
        .ok_or_else(|| "That private key cannot sign Git commits.".to_string())?;
    let keypair = signing_key
        .key()
        .clone()
        .into_keypair()
        .map_err(|err| err.to_string())?;

    let mut sink = Vec::new();
    let message = Message::new(&mut sink);
    let message = Armorer::new(message)
        .kind(openpgp::armor::Kind::Signature)
        .build()
        .map_err(|err| err.to_string())?;
    let mut signer = Signer::new(message, keypair)
        .map_err(|err| err.to_string())?
        .detached()
        .build()
        .map_err(|err| err.to_string())?;
    std::io::copy(&mut Cursor::new(commit_buffer.as_bytes()), &mut signer)
        .map_err(|err| err.to_string())?;
    signer.finalize().map_err(|err| err.to_string())?;

    String::from_utf8(sink).map_err(|err| err.to_string())
}

fn build_commit_headers(
    tree_oid: &str,
    parent_oid: Option<&str>,
    author_ident: &str,
    committer_ident: &str,
) -> String {
    let mut headers = String::new();
    headers.push_str("tree ");
    headers.push_str(tree_oid);
    headers.push('\n');
    if let Some(parent_oid) = parent_oid {
        headers.push_str("parent ");
        headers.push_str(parent_oid);
        headers.push('\n');
    }
    headers.push_str("author ");
    headers.push_str(author_ident);
    headers.push('\n');
    headers.push_str("committer ");
    headers.push_str(committer_ident);
    headers.push('\n');
    headers
}

fn build_signed_commit_buffer(headers: &str, message: &str, signature: Option<&str>) -> String {
    let mut buffer = String::new();
    buffer.push_str(headers);

    if let Some(signature) = signature {
        let mut lines = signature.trim_end_matches('\n').lines();
        if let Some(first) = lines.next() {
            buffer.push_str("gpgsig ");
            buffer.push_str(first);
            buffer.push('\n');
            for line in lines {
                buffer.push(' ');
                buffer.push_str(line);
                buffer.push('\n');
            }
        }
    }

    buffer.push('\n');
    buffer.push_str(message);
    if !message.ends_with('\n') {
        buffer.push('\n');
    }
    buffer
}

fn write_commit_object(store_root: &str, commit_buffer: &str) -> Result<String, String> {
    let output = run_store_git_command_with_input(
        store_root,
        "Write password store Git commit object",
        commit_buffer,
        |cmd| {
            cmd.args(["hash-object", "-t", "commit", "-w", "--stdin"]);
        },
    )?;
    if output.status.success() {
        git_output_text(&output)
    } else {
        Err(git_command_error("git hash-object", &output))
    }
}

fn update_git_head(
    store_root: &str,
    new_oid: &str,
    previous_oid: Option<&str>,
) -> Result<(), String> {
    let output = run_store_git_command(store_root, "Update password store Git HEAD", |cmd| {
        cmd.arg("update-ref").arg("HEAD").arg(new_oid);
        if let Some(previous_oid) = previous_oid {
            cmd.arg(previous_oid);
        }
    })?;
    if output.status.success() {
        Ok(())
    } else {
        Err(git_command_error("git update-ref", &output))
    }
}

fn commit_git_paths(
    store_root: &str,
    message: &str,
    paths: &[String],
    explicit_fingerprint: Option<&str>,
) -> Result<(), String> {
    if paths.is_empty() || !has_git_repository(store_root) {
        return Ok(());
    }

    stage_git_paths(store_root, paths)?;
    if !staged_git_paths_have_changes(store_root, paths)? {
        return Ok(());
    }

    let tree_oid = write_git_tree(store_root)?;
    let parent_oid = head_oid(store_root)?;
    let identity = commit_identity(explicit_fingerprint)?;
    let author_ident = git_ident(store_root, "author", &identity)?;
    let committer_ident = git_ident(store_root, "committer", &identity)?;
    let headers = build_commit_headers(
        &tree_oid,
        parent_oid.as_deref(),
        &author_ident,
        &committer_ident,
    );
    let unsigned_commit = build_signed_commit_buffer(&headers, message, None);
    let signature = identity
        .signing_fingerprint
        .as_deref()
        .map(unlocked_signing_cert)
        .transpose()?
        .flatten()
        .map(|cert| sign_commit_buffer(&unsigned_commit, &cert))
        .transpose();

    let commit_buffer = match signature {
        Ok(signature) => build_signed_commit_buffer(&headers, message, signature.as_deref()),
        Err(err) => {
            log_error(format!(
                "Git commit signing unavailable for {store_root}: {err}. Falling back to an unsigned commit."
            ));
            unsigned_commit
        }
    };

    let commit_oid = write_commit_object(store_root, &commit_buffer)?;
    update_git_head(store_root, &commit_oid, parent_oid.as_deref())
}

pub(super) fn password_entry_git_path(label: &str) -> String {
    format!("{label}.gpg")
}

pub(super) fn maybe_commit_git_paths(
    store_root: &str,
    message: &str,
    paths: impl IntoIterator<Item = String>,
    explicit_fingerprint: Option<&str>,
) {
    let mut paths = paths.into_iter().collect::<Vec<_>>();
    paths.sort();
    paths.dedup();
    if let Err(err) = commit_git_paths(store_root, message, &paths, explicit_fingerprint) {
        log_error(format!(
            "Flatpak backend Git commit failed for {store_root}: {err}"
        ));
    }
}

#[cfg(test)]
mod tests {
    use super::{
        build_commit_headers, build_signed_commit_buffer, commit_identity,
        commit_identity_from_private_key, parse_private_key_user_id,
        preferred_commit_private_key_from_values, CommitIdentity,
    };
    use crate::backend::ManagedRipassoPrivateKey;

    #[test]
    fn private_key_user_ids_map_to_git_identity() {
        assert_eq!(
            parse_private_key_user_id("Alice Example <alice@example.com>", "ABC123"),
            ("Alice Example".to_string(), "alice@example.com".to_string(),)
        );
        assert_eq!(
            parse_private_key_user_id("alice@example.com", "ABC123"),
            (
                "alice@example.com".to_string(),
                "alice@example.com".to_string(),
            )
        );
        assert_eq!(
            parse_private_key_user_id("Alice Example", "ABC123"),
            (
                "Alice Example".to_string(),
                "git+abc123@keycord.invalid".to_string(),
            )
        );
    }

    #[test]
    fn signed_commit_buffer_inserts_indented_gpgsig_header() {
        let headers = build_commit_headers(
            "deadbeef",
            Some("cafebabe"),
            "Alice <alice@example.com> 1 +0000",
            "Alice <alice@example.com> 1 +0000",
        );
        let commit = build_signed_commit_buffer(
            &headers,
            "Test commit",
            Some("-----BEGIN PGP SIGNATURE-----\nline-two\n-----END PGP SIGNATURE-----\n"),
        );

        assert!(commit.contains(
            "gpgsig -----BEGIN PGP SIGNATURE-----\n line-two\n -----END PGP SIGNATURE-----\n\nTest commit\n"
        ));
    }

    #[test]
    fn commit_identity_uses_the_explicit_private_key() {
        let key_a = ManagedRipassoPrivateKey {
            fingerprint: "AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA".to_string(),
            user_ids: vec!["Key A <a@example.com>".to_string()],
        };
        let explicit = key_a.fingerprint.clone();
        let selected =
            preferred_commit_private_key_from_values(&explicit, std::slice::from_ref(&key_a))
                .expect("resolve explicit key");

        assert_eq!(
            commit_identity_from_private_key(&selected),
            CommitIdentity {
                name: "Key A".to_string(),
                email: "a@example.com".to_string(),
                signing_fingerprint: Some(key_a.fingerprint),
            }
        );
    }

    #[test]
    fn commit_identity_has_no_fallback_private_key_when_the_explicit_key_is_missing() {
        let key_a = ManagedRipassoPrivateKey {
            fingerprint: "AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA".to_string(),
            user_ids: vec!["Key A <a@example.com>".to_string()],
        };
        let key_b = ManagedRipassoPrivateKey {
            fingerprint: "BBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBB".to_string(),
            user_ids: vec!["Key B <b@example.com>".to_string()],
        };
        assert_eq!(
            preferred_commit_private_key_from_values(
                "CCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCC",
                &[key_a, key_b]
            ),
            None
        );
    }

    #[test]
    fn commit_identity_is_generic_and_unsigned_without_an_explicit_key() {
        assert_eq!(
            commit_identity(None).expect("build generic identity"),
            CommitIdentity {
                name: "Keycord".to_string(),
                email: "git@keycord.invalid".to_string(),
                signing_fingerprint: None,
            }
        );
    }
}
