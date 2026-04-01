use super::{FieldValueRequest, ToolReadMode, ToolsPageState};
use crate::backend::{
    read_password_entry, read_password_line, required_private_key_fingerprints_for_entry,
    ripasso_private_key_requires_session_unlock, PasswordEntryError,
};
use crate::fido2_recipient::{is_fido2_recipient_string, same_fido2_recipient};
use crate::i18n::gettext;
use crate::preferences::Preferences;
use crate::private_key::unlock::prompt_private_key_unlock_for_action;
use crate::support::background::spawn_result_task;
use adw::{Toast, ToastOverlay};
use std::rc::Rc;

impl ToolsPageState {
    pub(super) fn unlock_tool_keys_if_needed(
        &self,
        requests: Vec<FieldValueRequest>,
        read_mode: ToolReadMode,
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
            move || collect_locked_tool_fingerprints(&requests_for_unlock, read_mode),
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

fn collect_locked_tool_fingerprints(
    requests: &[FieldValueRequest],
    read_mode: ToolReadMode,
) -> Vec<String> {
    let mut fingerprints = Vec::new();
    for request in requests {
        let read_result = match read_mode {
            ToolReadMode::PasswordContents => {
                read_password_entry(&request.root, &request.label).map(|_| ())
            }
            ToolReadMode::PasswordLine => {
                read_password_line(&request.root, &request.label).map(|_| ())
            }
        };

        if !matches!(read_result, Err(PasswordEntryError::LockedPrivateKey(_))) {
            continue;
        }

        let Ok(required_fingerprints) =
            required_private_key_fingerprints_for_entry(&request.root, &request.label)
        else {
            continue;
        };
        append_unlockable_tool_fingerprints(&mut fingerprints, required_fingerprints);
    }

    fingerprints
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
