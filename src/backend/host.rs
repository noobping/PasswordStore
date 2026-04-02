mod legacy;

use self::legacy::{
    password_entry_error_from_host_message, password_entry_write_error_from_host_message,
    store_recipients_error_from_host_message,
};
#[cfg(target_os = "linux")]
use crate::backend::command::{run_host_program_output, run_host_program_with_input};
use crate::backend::{
    command::{ensure_success, run_store_command_output, run_store_command_with_input},
    PasswordEntryError, PasswordEntryWriteError, StoreRecipients, StoreRecipientsError,
    StoreRecipientsPrivateKeyRequirement,
};
use crate::logging::CommandLogOptions;
use crate::support::git::{ensure_store_git_repository, has_git_repository};
use std::path::Path;
use std::process::Output;

#[cfg(target_os = "linux")]
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HostGpgPrivateKeySummary {
    pub fingerprint: String,
    pub user_ids: Vec<String>,
}

#[cfg(target_os = "linux")]
impl HostGpgPrivateKeySummary {
    pub fn title(&self) -> String {
        self.user_ids
            .first()
            .cloned()
            .filter(|value| !value.trim().is_empty())
            .unwrap_or_else(|| "Unnamed host private key".to_string())
    }
}

fn read_entry_output(store_root: &str, label: &str, action: &str) -> Result<Output, String> {
    let output =
        run_store_command_output(store_root, action, CommandLogOptions::SENSITIVE, |cmd| {
            cmd.arg(label);
        })?;
    ensure_success(output, "pass failed")
}

pub(super) fn read_password_entry(
    store_root: &str,
    label: &str,
) -> Result<String, PasswordEntryError> {
    read_password_entry_with_progress(store_root, label)
}

pub(super) fn read_password_entry_with_progress(
    store_root: &str,
    label: &str,
) -> Result<String, PasswordEntryError> {
    let output = read_entry_output(store_root, label, "Read password entry")
        .map_err(password_entry_error_from_host_message)?;
    Ok(String::from_utf8_lossy(&output.stdout).to_string())
}

pub(super) fn read_password_line(
    store_root: &str,
    label: &str,
) -> Result<String, PasswordEntryError> {
    let output = read_entry_output(store_root, label, "Read password entry for clipboard copy")
        .map_err(password_entry_error_from_host_message)?;
    Ok(String::from_utf8_lossy(&output.stdout)
        .lines()
        .next()
        .unwrap_or_default()
        .to_string())
}

pub(super) const fn password_entry_is_readable(_store_root: &str, _label: &str) -> bool {
    true
}

pub(super) const fn password_entry_fido2_recipient_count(_store_root: &str, _label: &str) -> usize {
    0
}

pub(super) fn save_password_entry(
    store_root: &str,
    label: &str,
    contents: &str,
    overwrite: bool,
) -> Result<(), PasswordEntryWriteError> {
    save_password_entry_with_progress(store_root, label, contents, overwrite)
}

pub(super) fn save_password_entry_with_progress(
    store_root: &str,
    label: &str,
    contents: &str,
    overwrite: bool,
) -> Result<(), PasswordEntryWriteError> {
    let output = run_store_command_with_input(
        store_root,
        "Save password entry",
        contents,
        CommandLogOptions::SENSITIVE,
        |cmd| {
            cmd.arg("insert").arg("-m");
            if overwrite {
                cmd.arg("-f");
            }
            cmd.arg(label);
        },
    )
    .map_err(password_entry_write_error_from_host_message)?;
    ensure_success(output, "pass insert failed")
        .map(|_| ())
        .map_err(password_entry_write_error_from_host_message)
}

pub(super) fn rename_password_entry(
    store_root: &str,
    old_label: &str,
    new_label: &str,
) -> Result<(), PasswordEntryWriteError> {
    let output = run_store_command_output(
        store_root,
        "Rename password entry",
        CommandLogOptions::DEFAULT,
        |cmd| {
            cmd.arg("mv").arg(old_label).arg(new_label);
        },
    )
    .map_err(password_entry_write_error_from_host_message)?;
    ensure_success(output, "pass mv failed")
        .map(|_| ())
        .map_err(password_entry_write_error_from_host_message)
}

pub(super) fn delete_password_entry(
    store_root: &str,
    label: &str,
) -> Result<(), PasswordEntryWriteError> {
    let output = run_store_command_output(
        store_root,
        "Delete password entry",
        CommandLogOptions::DEFAULT,
        |cmd| {
            cmd.arg("rm").arg("-rf").arg(label);
        },
    )
    .map_err(password_entry_write_error_from_host_message)?;
    ensure_success(output, "pass rm failed")
        .map(|_| ())
        .map_err(password_entry_write_error_from_host_message)
}

pub(super) fn save_store_recipients(
    store_root: &str,
    recipients: &StoreRecipients,
    private_key_requirement: StoreRecipientsPrivateKeyRequirement,
) -> Result<(), StoreRecipientsError> {
    save_store_recipients_with_progress(store_root, recipients, private_key_requirement)
}

pub(super) fn save_store_recipients_with_progress(
    store_root: &str,
    recipients: &StoreRecipients,
    _private_key_requirement: StoreRecipientsPrivateKeyRequirement,
) -> Result<(), StoreRecipientsError> {
    if !recipients.fido2().is_empty() {
        return Err(StoreRecipientsError::other(
            "FIDO2 recipients require the Integrated backend.",
        ));
    }

    let should_initialize_git =
        !Path::new(store_root).join(".gpg-id").exists() && !has_git_repository(store_root);
    let output = run_store_command_output(
        store_root,
        "Save password store recipients",
        CommandLogOptions::DEFAULT,
        |cmd| {
            cmd.arg("init").args(recipients.standard());
        },
    )
    .map_err(store_recipients_error_from_host_message)?;
    ensure_success(output, "pass init failed").map_err(store_recipients_error_from_host_message)?;
    if should_initialize_git {
        ensure_store_git_repository(store_root)
            .map_err(store_recipients_error_from_host_message)?;
    }
    Ok(())
}

pub(super) fn store_recipients_private_key_requiring_unlock(
    _store_root: &str,
) -> Result<Option<String>, String> {
    Ok(None)
}

#[cfg(target_os = "linux")]
pub fn list_host_gpg_private_keys() -> Result<Vec<HostGpgPrivateKeySummary>, String> {
    let output = run_host_program_output(
        "gpg",
        &[
            "--batch",
            "--with-colons",
            "--fingerprint",
            "--list-secret-keys",
        ],
        "Inspect host GPG private keys",
        CommandLogOptions::DEFAULT,
    )?;
    let output = ensure_success(output, "gpg --list-secret-keys failed")?;
    Ok(parse_host_gpg_private_keys(&String::from_utf8_lossy(
        &output.stdout,
    )))
}

#[cfg(target_os = "linux")]
pub fn armored_host_gpg_private_key(fingerprint: &str) -> Result<String, String> {
    let output = run_host_program_output(
        "gpg",
        &[
            "--batch",
            "--yes",
            "--armor",
            "--export-secret-keys",
            fingerprint,
        ],
        "Export host GPG private key",
        CommandLogOptions::SENSITIVE,
    )?;
    let output = ensure_success(output, "gpg --export-secret-keys failed")?;
    String::from_utf8(output.stdout).map_err(|err| err.to_string())
}

#[cfg(target_os = "linux")]
pub fn import_host_gpg_private_key_bytes(bytes: &[u8]) -> Result<(), String> {
    let input = std::str::from_utf8(bytes).map_err(|err| err.to_string())?;
    let output = run_host_program_with_input(
        "gpg",
        &["--batch", "--yes", "--import"],
        input,
        "Import host GPG private key",
        CommandLogOptions::SENSITIVE,
    )?;
    ensure_success(output, "gpg --import failed").map(|_| ())
}

#[cfg(target_os = "linux")]
pub fn delete_host_gpg_private_key(fingerprint: &str) -> Result<(), String> {
    let output = run_host_program_output(
        "gpg",
        &["--batch", "--yes", "--delete-secret-keys", fingerprint],
        "Delete host GPG private key",
        CommandLogOptions::DEFAULT,
    )?;
    ensure_success(output, "gpg --delete-secret-keys failed").map(|_| ())
}

#[cfg(target_os = "linux")]
fn parse_host_gpg_private_keys(output: &str) -> Vec<HostGpgPrivateKeySummary> {
    #[derive(Default)]
    struct PartialHostKey {
        fingerprint: Option<String>,
        user_ids: Vec<String>,
        awaiting_primary_fpr: bool,
    }

    fn finish_key(
        partial: Option<PartialHostKey>,
        keys: &mut Vec<HostGpgPrivateKeySummary>,
    ) -> Option<PartialHostKey> {
        let Some(partial) = partial else {
            return None;
        };
        let Some(fingerprint) = partial.fingerprint.filter(|value| !value.trim().is_empty()) else {
            return None;
        };
        if keys
            .iter()
            .any(|existing| existing.fingerprint.eq_ignore_ascii_case(&fingerprint))
        {
            return None;
        }

        keys.push(HostGpgPrivateKeySummary {
            fingerprint,
            user_ids: partial
                .user_ids
                .into_iter()
                .filter(|value| !value.trim().is_empty())
                .collect(),
        });
        None
    }

    fn colon_field(line: &str, index: usize) -> Option<&str> {
        line.split(':').nth(index).map(str::trim)
    }

    fn user_id_field(line: &str) -> &str {
        colon_field(line, 9)
            .filter(|value| !value.is_empty())
            .or_else(|| colon_field(line, 7).filter(|value| !value.is_empty()))
            .unwrap_or_default()
    }

    let mut keys = Vec::new();
    let mut current = None;

    for line in output.lines() {
        let mut fields = line.split(':');
        let Some(record_type) = fields.next() else {
            continue;
        };

        match record_type {
            "sec" => {
                let _ = finish_key(current.take(), &mut keys);
                current = Some(PartialHostKey {
                    fingerprint: None,
                    user_ids: Vec::new(),
                    awaiting_primary_fpr: true,
                });
            }
            "fpr" => {
                let Some(current) = current.as_mut() else {
                    continue;
                };
                if !current.awaiting_primary_fpr {
                    continue;
                }
                let fingerprint = colon_field(line, 9).unwrap_or_default().to_string();
                if fingerprint.is_empty() {
                    continue;
                }
                current.fingerprint = Some(fingerprint);
                current.awaiting_primary_fpr = false;
            }
            "uid" => {
                let Some(current) = current.as_mut() else {
                    continue;
                };
                let user_id = user_id_field(line).to_string();
                if !user_id.is_empty() {
                    current.user_ids.push(user_id);
                }
            }
            _ => {}
        }
    }

    finish_key(current, &mut keys);
    keys.sort_by(|left, right| {
        left.title()
            .to_ascii_lowercase()
            .cmp(&right.title().to_ascii_lowercase())
            .then_with(|| left.fingerprint.cmp(&right.fingerprint))
    });
    keys
}

#[cfg(all(test, target_os = "linux"))]
mod tests {
    use super::{
        list_host_gpg_private_keys, parse_host_gpg_private_keys, save_password_entry,
        save_store_recipients,
    };
    use crate::backend::test_support::assert_entry_is_encrypted_for_each_recipient;
    use crate::backend::test_support::SystemBackendTestEnv;
    use crate::backend::{
        StoreRecipients, StoreRecipientsError, StoreRecipientsPrivateKeyRequirement,
    };
    use crate::support::git::has_git_repository;
    use sequoia_openpgp::serialize::Serialize;
    use std::io::Write;
    use std::process::{Command, Stdio};

    fn import_secret_key(bytes: &[u8]) -> Result<(), String> {
        let mut child = Command::new("gpg")
            .args(["--batch", "--yes", "--import"])
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|err| format!("Failed to start gpg secret-key import: {err}"))?;

        {
            let stdin = child
                .stdin
                .as_mut()
                .ok_or_else(|| "gpg secret-key import did not provide stdin".to_string())?;
            stdin
                .write_all(bytes)
                .map_err(|err| format!("Failed to write imported secret key bytes: {err}"))?;
        }

        let output = child
            .wait_with_output()
            .map_err(|err| format!("Failed to wait for gpg secret-key import: {err}"))?;
        if output.status.success() {
            Ok(())
        } else {
            Err(String::from_utf8_lossy(&output.stderr).trim().to_string())
        }
    }

    #[test]
    fn host_backend_encrypts_entries_for_all_store_recipients() {
        assert_entry_is_encrypted_for_each_recipient(
            |store_root, recipients| {
                save_store_recipients(
                    store_root,
                    &StoreRecipients::new(recipients.to_vec(), Vec::new()),
                    StoreRecipientsPrivateKeyRequirement::AnyManagedKey,
                )
                .map_err(|err| err.to_string())
            },
            |store_root, label, contents| {
                save_password_entry(store_root, label, contents, true)
                    .map_err(|err| err.to_string())
            },
        );
    }

    #[test]
    fn host_backend_rejects_fido2_store_recipients() {
        let err = save_store_recipients(
            "/tmp/unused",
            &StoreRecipients::new(
                Vec::new(),
                vec![String::from(
                "keycord-fido2-recipient-v1=0123456789abcdef0123456789abcdef01234567:4465736b204b6579:63726564",
                )],
            ),
            StoreRecipientsPrivateKeyRequirement::AnyManagedKey,
        )
        .expect_err("host backend should reject FIDO2 recipients");

        assert!(matches!(err, StoreRecipientsError::Other(_)));
    }

    #[test]
    #[expect(
        clippy::significant_drop_tightening,
        reason = "SystemBackendTestEnv must stay alive for the full test to keep the temp store and env vars in place."
    )]
    fn host_backend_initializes_git_for_new_stores() {
        let env = SystemBackendTestEnv::new();

        let key = SystemBackendTestEnv::generate_secret_key("Recipient <host-create@example.com>")
            .expect("generate host recipient key");
        SystemBackendTestEnv::import_public_key(&key.public_key_bytes)
            .expect("import host recipient key");
        SystemBackendTestEnv::trust_public_key(&key.fingerprint_hex)
            .expect("trust host recipient key");

        let store_root = env.store_root().to_string_lossy().to_string();
        save_store_recipients(
            &store_root,
            &StoreRecipients::new(vec![key.fingerprint_hex.clone()], Vec::new()),
            StoreRecipientsPrivateKeyRequirement::AnyManagedKey,
        )
        .expect("save store recipients");

        assert!(has_git_repository(&store_root));
    }

    #[test]
    #[expect(
        clippy::significant_drop_tightening,
        reason = "SystemBackendTestEnv must stay alive for the full test to keep the temp store and env vars in place."
    )]
    fn host_backend_saves_entries_with_empty_password_lines() {
        let env = SystemBackendTestEnv::new();

        let key = SystemBackendTestEnv::generate_secret_key("Recipient <host-empty@example.com>")
            .expect("generate host recipient key");
        let mut secret_key_bytes = Vec::new();
        key.cert
            .as_tsk()
            .serialize(&mut secret_key_bytes)
            .expect("serialize secret key");
        import_secret_key(&secret_key_bytes).expect("import host secret key");
        SystemBackendTestEnv::import_public_key(&key.public_key_bytes)
            .expect("import host recipient key");
        SystemBackendTestEnv::trust_public_key(&key.fingerprint_hex)
            .expect("trust host recipient key");

        let store_root = env.store_root().to_string_lossy().to_string();
        save_store_recipients(
            &store_root,
            &StoreRecipients::new(vec![key.fingerprint_hex.clone()], Vec::new()),
            StoreRecipientsPrivateKeyRequirement::AnyManagedKey,
        )
        .expect("save store recipients");
        save_password_entry(
            &store_root,
            "team/empty-password",
            "\nusername: alice",
            true,
        )
        .expect("save password entry with empty first line");

        assert_eq!(
            super::read_password_entry(&store_root, "team/empty-password").expect("read entry"),
            "\nusername: alice"
        );
    }

    #[test]
    #[expect(
        clippy::significant_drop_tightening,
        reason = "SystemBackendTestEnv must stay alive for the full test to keep the temp store and env vars in place."
    )]
    fn host_backend_save_leaves_git_worktree_clean() {
        let env = SystemBackendTestEnv::new();
        env.init_store_git_repository()
            .expect("initialize store git repository");

        let key =
            SystemBackendTestEnv::generate_secret_key("Recipient <host-git-clean@example.com>")
                .expect("generate host recipient key");
        let mut secret_key_bytes = Vec::new();
        key.cert
            .as_tsk()
            .serialize(&mut secret_key_bytes)
            .expect("serialize secret key");
        import_secret_key(&secret_key_bytes).expect("import host secret key");
        SystemBackendTestEnv::import_public_key(&key.public_key_bytes)
            .expect("import host recipient key");
        SystemBackendTestEnv::trust_public_key(&key.fingerprint_hex)
            .expect("trust host recipient key");

        let store_root = env.store_root().to_string_lossy().to_string();
        save_store_recipients(
            &store_root,
            &StoreRecipients::new(vec![key.fingerprint_hex.clone()], Vec::new()),
            StoreRecipientsPrivateKeyRequirement::AnyManagedKey,
        )
        .expect("save store recipients");
        save_password_entry(&store_root, "example/user", "secret\nusername: alice", true)
            .expect("save password entry");

        assert_eq!(
            env.store_git_status_porcelain()
                .expect("read store git status after host save"),
            ""
        );
    }

    #[test]
    fn host_gpg_parser_keeps_primary_fingerprint_and_user_ids() {
        let parsed = parse_host_gpg_private_keys(
            "\
sec:u:255:22:PRIMARY:1:::::::scESC:::+:::23::0:\n\
fpr:::::::::AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA:\n\
grp:::::::::group:\n\
uid:u::::1::Alice Example <alice@example.com>::::::::::0:\n\
uid:u::::1::Alice Work <alice@work.example>::::::::::0:\n\
ssb:u:255:18:SUB:1:::::::e:::+:::23:\n\
fpr:::::::::BBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBB:\n",
        );

        assert_eq!(parsed.len(), 1);
        assert_eq!(
            parsed[0].fingerprint,
            "AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA"
        );
        assert_eq!(
            parsed[0].user_ids,
            vec![
                "Alice Example <alice@example.com>".to_string(),
                "Alice Work <alice@work.example>".to_string()
            ]
        );
    }

    #[test]
    fn host_gpg_parser_ignores_duplicate_or_incomplete_blocks() {
        let parsed = parse_host_gpg_private_keys(
            "\
sec:u:::::::\n\
uid:u::::1::Missing Fingerprint:::::::\n\
sec:u:255:22:PRIMARY:1:::::::scESC:::+:::23::0:\n\
fpr:::::::::AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA:\n\
uid:u::::1::Alice Example <alice@example.com>::::::::::0:\n\
sec:u:255:22:PRIMARY:1:::::::scESC:::+:::23::0:\n\
fpr:::::::::AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA:\n\
uid:u::::1::Duplicate Alice <alice@example.com>::::::::::0:\n",
        );

        assert_eq!(parsed.len(), 1);
        assert_eq!(
            parsed[0].user_ids,
            vec!["Alice Example <alice@example.com>".to_string()]
        );
    }

    #[test]
    #[expect(
        clippy::significant_drop_tightening,
        reason = "SystemBackendTestEnv must stay alive for the full test to keep the temp gpg home in place."
    )]
    fn host_gpg_discovery_lists_imported_secret_keys() {
        let env = SystemBackendTestEnv::new();
        env.activate_profile("host-gpg");

        let key = SystemBackendTestEnv::generate_secret_key("Host User <host-user@example.com>")
            .expect("generate secret key");
        let mut bytes = Vec::new();
        key.cert
            .as_tsk()
            .serialize(&mut bytes)
            .expect("serialize secret key");
        import_secret_key(&bytes).expect("import host secret key");

        let keys = list_host_gpg_private_keys().expect("list host gpg private keys");
        assert!(keys.iter().any(|found| {
            found.fingerprint.eq_ignore_ascii_case(&key.fingerprint_hex)
                && found
                    .user_ids
                    .iter()
                    .any(|user_id| user_id == "Host User <host-user@example.com>")
        }));
    }
}
