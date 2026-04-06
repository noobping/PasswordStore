use super::{FieldValueRequest, ToolsPageState};
use crate::backend::{
    list_connected_smartcard_keys, list_ripasso_private_keys,
    ripasso_private_key_requires_session_unlock,
};
use crate::fido2_recipient::{is_fido2_recipient_string, same_fido2_recipient};
use crate::i18n::gettext;
use crate::preferences::Preferences;
use crate::private_key::unlock::prompt_private_key_unlock_for_action;
use crate::store::recipients::{
    read_store_fido2_recipients_for_scope, read_store_standard_recipients_for_scope,
    relevant_store_recipient_scopes, ROOT_STORE_RECIPIENTS_SCOPE,
};
use crate::support::background::spawn_result_task;
use adw::{Toast, ToastOverlay};
use std::rc::Rc;

#[derive(Clone, Debug, PartialEq, Eq)]
struct AvailableToolKey {
    fingerprint: String,
    user_ids: Vec<String>,
}

impl ToolsPageState {
    pub(super) fn unlock_tool_keys_if_needed(
        &self,
        requests: Vec<FieldValueRequest>,
        on_ready: Rc<dyn Fn(Vec<FieldValueRequest>)>,
        on_abort: Rc<dyn Fn()>,
    ) {
        if !Preferences::new().uses_integrated_backend() {
            on_ready(requests);
            return;
        }

        let requests_for_unlock = requests.clone();
        let on_ready_for_result = on_ready.clone();
        let on_abort_for_result = on_abort.clone();
        let overlay_for_result = self.overlay.clone();
        let overlay_for_disconnect = self.overlay.clone();
        spawn_result_task(
            move || collect_locked_tool_fingerprints(&requests_for_unlock),
            move |fingerprints| {
                if fingerprints.is_empty() {
                    on_ready_for_result(requests);
                    return;
                }

                let on_abort_for_unlock = on_abort_for_result.clone();
                prompt_tool_unlock_sequence(
                    &overlay_for_result,
                    fingerprints,
                    Rc::new(move |success| {
                        if success {
                            on_ready(requests.clone());
                        } else {
                            on_abort_for_unlock();
                        }
                    }),
                );
            },
            move || {
                on_abort();
                overlay_for_disconnect
                    .add_toast(Toast::new(&gettext("Couldn't prepare tool access.")));
            },
        );
    }
}

fn collect_locked_tool_fingerprints(requests: &[FieldValueRequest]) -> Vec<String> {
    let mut fingerprints = collect_unlockable_standard_tool_fingerprints(requests);
    append_unlockable_tool_fingerprints(
        &mut fingerprints,
        collect_available_tool_fido2_recipients(requests),
    );
    fingerprints
}

fn collect_unlockable_standard_tool_fingerprints(requests: &[FieldValueRequest]) -> Vec<String> {
    let Ok(keys) = available_tool_keys() else {
        return Vec::new();
    };
    let recipients = collect_tool_standard_recipients(requests);
    let mut fingerprints = Vec::new();

    for key in keys {
        if recipients
            .iter()
            .any(|recipient| tool_recipient_matches_key(recipient, &key))
        {
            append_unlockable_tool_fingerprints(&mut fingerprints, vec![key.fingerprint]);
        }
    }

    fingerprints
}

fn collect_available_tool_fido2_recipients(requests: &[FieldValueRequest]) -> Vec<String> {
    let mut recipients = Vec::new();
    for store_root in tool_request_store_roots(requests) {
        for scope in tool_request_store_scopes(&store_root) {
            append_unlockable_tool_fingerprints(
                &mut recipients,
                read_store_fido2_recipients_for_scope(&store_root, &scope),
            );
        }
    }
    recipients
}

fn available_tool_keys() -> Result<Vec<AvailableToolKey>, String> {
    let mut keys = Vec::new();

    for key in list_ripasso_private_keys()? {
        push_unique_available_tool_key(&mut keys, key.fingerprint, key.user_ids);
    }

    for key in list_connected_smartcard_keys()? {
        push_unique_available_tool_key(&mut keys, key.fingerprint, key.user_ids);
    }

    Ok(keys)
}

fn push_unique_available_tool_key(
    keys: &mut Vec<AvailableToolKey>,
    fingerprint: String,
    user_ids: Vec<String>,
) {
    if keys
        .iter()
        .any(|existing| existing.fingerprint.eq_ignore_ascii_case(&fingerprint))
    {
        return;
    }

    keys.push(AvailableToolKey {
        fingerprint,
        user_ids,
    });
}

fn collect_tool_standard_recipients(requests: &[FieldValueRequest]) -> Vec<String> {
    let mut recipients = Vec::new();
    for store_root in tool_request_store_roots(requests) {
        for scope in tool_request_store_scopes(&store_root) {
            for recipient in read_store_standard_recipients_for_scope(&store_root, &scope) {
                push_unique_standard_tool_recipient(&mut recipients, recipient);
            }
        }
    }
    recipients
}

fn push_unique_standard_tool_recipient(recipients: &mut Vec<String>, candidate: String) {
    let candidate = candidate.trim();
    if candidate.is_empty()
        || recipients
            .iter()
            .any(|existing| existing.eq_ignore_ascii_case(candidate))
    {
        return;
    }

    recipients.push(candidate.to_string());
}

fn tool_request_store_roots(requests: &[FieldValueRequest]) -> Vec<String> {
    let mut store_roots = Vec::new();
    for request in requests {
        if !store_roots.iter().any(|existing| existing == &request.root) {
            store_roots.push(request.root.clone());
        }
    }
    store_roots
}

fn tool_request_store_scopes(store_root: &str) -> Vec<String> {
    let mut scopes = vec![ROOT_STORE_RECIPIENTS_SCOPE.to_string()];
    for scope in relevant_store_recipient_scopes(store_root) {
        if !scopes.iter().any(|existing| existing == &scope) {
            scopes.push(scope);
        }
    }
    scopes
}

fn tool_recipient_matches_key(recipient: &str, key: &AvailableToolKey) -> bool {
    let recipient = recipient.trim();
    recipient.eq_ignore_ascii_case(&key.fingerprint)
        || key
            .user_ids
            .iter()
            .any(|user_id| user_id.eq_ignore_ascii_case(recipient))
}

fn append_unlockable_tool_fingerprints(fingerprints: &mut Vec<String>, candidates: Vec<String>) {
    for candidate in candidates {
        let unlockable = if is_fido2_recipient_string(&candidate) {
            true
        } else {
            matches!(
                ripasso_private_key_requires_session_unlock(&candidate),
                Ok(true)
            )
        };
        if !unlockable {
            continue;
        }

        let duplicate = fingerprints.iter().any(|existing| {
            if is_fido2_recipient_string(existing) && is_fido2_recipient_string(&candidate) {
                same_fido2_recipient(existing, &candidate)
            } else {
                existing.eq_ignore_ascii_case(&candidate)
            }
        });
        if !duplicate {
            fingerprints.push(candidate);
        }
    }
}

fn prompt_tool_unlock_sequence(
    overlay: &ToastOverlay,
    fingerprints: Vec<String>,
    on_finish: Rc<dyn Fn(bool)>,
) {
    if fingerprints.is_empty() {
        on_finish(true);
        return;
    }

    prompt_tool_unlock_at_index(overlay.clone(), Rc::new(fingerprints), 0, on_finish);
}

fn prompt_tool_unlock_at_index(
    overlay: ToastOverlay,
    fingerprints: Rc<Vec<String>>,
    index: usize,
    on_finish: Rc<dyn Fn(bool)>,
) {
    let Some(fingerprint) = fingerprints.get(index).cloned() else {
        on_finish(true);
        return;
    };

    let overlay_for_next = overlay.clone();
    let fingerprints_for_next = fingerprints.clone();
    let on_finish_for_next = on_finish.clone();
    let on_finish_for_result = on_finish.clone();
    prompt_private_key_unlock_for_action(
        &overlay,
        fingerprint,
        Rc::new(move || {
            prompt_tool_unlock_at_index(
                overlay_for_next.clone(),
                fingerprints_for_next.clone(),
                index + 1,
                on_finish_for_next.clone(),
            );
        }),
        Rc::new(move |success| {
            if !success {
                on_finish_for_result(false);
            }
        }),
    );
}

#[cfg(test)]
mod tests {
    use super::append_unlockable_tool_fingerprints;

    #[test]
    fn unlockable_tool_fingerprints_keep_unique_standard_and_fido_keys() {
        let mut fingerprints = vec!["ABCDEF0123456789".to_string()];
        append_unlockable_tool_fingerprints(
            &mut fingerprints,
            vec![
                "abcdef0123456789".to_string(),
                "keycord-fido2-recipient-v1=0123456789abcdef0123456789abcdef01234567:4465736b204b6579:63726564"
                    .to_string(),
                "keycord-fido2-recipient-v1=0123456789abcdef0123456789abcdef01234567:4261636b7570204b6579:63726564"
                    .to_string(),
            ],
        );

        assert_eq!(
            fingerprints,
            vec![
                "ABCDEF0123456789".to_string(),
                "keycord-fido2-recipient-v1=0123456789abcdef0123456789abcdef01234567:4465736b204b6579:63726564"
                    .to_string(),
            ]
        );
    }
}
