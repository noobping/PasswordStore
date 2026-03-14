use super::export::copy_armored_private_key;
use super::generate::append_private_key_generate_row;
use super::import::{append_private_key_clipboard_import_row, append_private_key_import_row};
use super::{super::queue_store_recipients_autosave, StoreRecipientsPageState};
use crate::backend::{
    is_ripasso_private_key_unlocked, list_ripasso_private_keys, remove_ripasso_private_key,
    ripasso_private_key_requires_session_unlock, ManagedRipassoPrivateKey,
    StoreRecipientsPrivateKeyRequirement,
};
use crate::logging::log_error;
use crate::preferences::Preferences;
use crate::private_key::unlock::prompt_private_key_unlock_for_action;
use crate::support::ui::{
    append_info_row, clear_list_box, dim_label_icon, flat_icon_button_with_tooltip,
};
use adw::gtk::{CheckButton, Image};
use adw::prelude::*;
use adw::{ActionRow, Toast};
use std::rc::Rc;

fn inspect_private_key_lock_state(fingerprint: &str) -> (bool, bool) {
    let unlocked = match is_ripasso_private_key_unlocked(fingerprint) {
        Ok(unlocked) => unlocked,
        Err(err) => {
            log_error(format!(
                "Failed to inspect whether private key '{fingerprint}' is unlocked: {err}"
            ));
            false
        }
    };
    let requires_unlock = match ripasso_private_key_requires_session_unlock(fingerprint) {
        Ok(requires_unlock) => requires_unlock,
        Err(err) => {
            log_error(format!(
                "Failed to inspect whether private key '{fingerprint}' requires unlocking: {err}"
            ));
            false
        }
    };

    (unlocked, requires_unlock)
}

pub(super) fn recipient_matches_private_key(
    recipient: &str,
    key: &ManagedRipassoPrivateKey,
) -> bool {
    let recipient = recipient.trim();
    recipient.eq_ignore_ascii_case(&key.fingerprint)
        || key
            .user_ids
            .iter()
            .any(|user_id| user_id.eq_ignore_ascii_case(recipient))
}

fn set_private_key_recipient_enabled(
    state: &StoreRecipientsPageState,
    key: &ManagedRipassoPrivateKey,
    enabled: bool,
) -> bool {
    set_private_key_recipient_values(&mut state.recipients.borrow_mut(), key, enabled)
}

fn set_private_key_requirement(
    state: &StoreRecipientsPageState,
    private_key_requirement: StoreRecipientsPrivateKeyRequirement,
) -> bool {
    let changed = state.private_key_requirement.get() != private_key_requirement;
    if changed {
        state.private_key_requirement.set(private_key_requirement);
    }
    changed
}

fn set_private_key_recipient_values(
    recipients: &mut Vec<String>,
    key: &ManagedRipassoPrivateKey,
    enabled: bool,
) -> bool {
    let before = recipients.clone();
    recipients.retain(|value| !recipient_matches_private_key(value, key));
    if enabled {
        recipients.push(key.fingerprint.clone());
    }
    *recipients != before
}

fn unresolved_private_key_recipients(
    recipients: &[String],
    keys: &[ManagedRipassoPrivateKey],
) -> Vec<String> {
    let mut unresolved = Vec::new();

    for recipient in recipients {
        if keys
            .iter()
            .any(|key| recipient_matches_private_key(recipient, key))
        {
            continue;
        }
        if unresolved.iter().any(|existing| existing == recipient) {
            continue;
        }
        unresolved.push(recipient.clone());
    }

    unresolved
}

fn append_unresolved_private_key_rows(state: &StoreRecipientsPageState, recipients: &[String]) {
    if recipients.is_empty() {
        return;
    }

    for recipient in recipients {
        let row = ActionRow::builder()
            .title(recipient)
            .subtitle("This recipient is not available in the app.")
            .build();
        row.set_activatable(false);
        row.add_prefix(&dim_label_icon("dialog-warning-symbolic"));

        let delete_button =
            flat_icon_button_with_tooltip("user-trash-symbolic", "Remove recipient");
        row.add_suffix(&delete_button);
        state.list.append(&row);

        let page_state = state.clone();
        let recipient = recipient.clone();
        delete_button.connect_clicked(move |_| {
            let before = page_state.recipients.borrow().clone();
            page_state
                .recipients
                .borrow_mut()
                .retain(|value| value != &recipient);
            super::rebuild_store_recipients_list(&page_state);
            if *page_state.recipients.borrow() != before {
                queue_store_recipients_autosave(&page_state);
            }
        });
    }
}

fn append_private_key_requirement_row(state: &StoreRecipientsPageState) {
    let row = ActionRow::builder()
        .title("Require all selected private keys")
        .subtitle(
            "Uses layered encryption so every selected key must be unlocked. Other pass apps will not be able to read these items.",
        )
        .build();
    row.set_activatable(true);

    let toggle = CheckButton::new();
    toggle.set_active(matches!(
        state.private_key_requirement.get(),
        StoreRecipientsPrivateKeyRequirement::AllManagedKeys
    ));
    row.add_suffix(&toggle);
    state.list.append(&row);

    let toggle_for_row = toggle.clone();
    row.connect_activated(move |_| {
        toggle_for_row.set_active(!toggle_for_row.is_active());
    });

    let page_state = state.clone();
    toggle.connect_toggled(move |button| {
        let private_key_requirement = if button.is_active() {
            StoreRecipientsPrivateKeyRequirement::AllManagedKeys
        } else {
            StoreRecipientsPrivateKeyRequirement::AnyManagedKey
        };
        if set_private_key_requirement(&page_state, private_key_requirement) {
            queue_store_recipients_autosave(&page_state);
        }
    });
}

pub(super) fn rebuild_store_recipients_list(state: &StoreRecipientsPageState) {
    clear_list_box(&state.list);

    let keys = match list_ripasso_private_keys() {
        Ok(keys) => keys,
        Err(err) => {
            log_error(format!("Failed to load private keys for recipients: {err}"));
            append_info_row(
                &state.list,
                "Couldn't load private keys",
                "Try again from Preferences.",
            );
            append_private_key_generate_row(state);
            append_private_key_clipboard_import_row(state);
            append_private_key_import_row(state);
            return;
        }
    };

    let current_recipients = state.recipients.borrow().clone();
    let unresolved_recipients = unresolved_private_key_recipients(&current_recipients, &keys);

    if keys.is_empty() {
        if unresolved_recipients.is_empty() {
            append_info_row(
                &state.list,
                "No private keys yet",
                "Generate or import a private key first.",
            );
        } else {
            append_unresolved_private_key_rows(state, &unresolved_recipients);
        }
        append_private_key_generate_row(state);
        append_private_key_clipboard_import_row(state);
        append_private_key_import_row(state);
        return;
    }

    append_private_key_requirement_row(state);
    append_unresolved_private_key_rows(state, &unresolved_recipients);

    for key in keys {
        let active = state
            .recipients
            .borrow()
            .iter()
            .any(|recipient| recipient_matches_private_key(recipient, &key));
        let title = adw::glib::markup_escape_text(&key.title());
        let row = ActionRow::builder()
            .title(title.as_str())
            .subtitle(&key.fingerprint)
            .build();
        row.set_activatable(true);

        let key_icon = dim_label_icon("dialog-password-symbolic");
        row.add_prefix(&key_icon);

        let (unlocked, requires_unlock) = inspect_private_key_lock_state(&key.fingerprint);
        let toggle = CheckButton::new();
        toggle.set_active(active);
        row.add_suffix(&toggle);

        if requires_unlock {
            let unlock_button =
                flat_icon_button_with_tooltip("system-lock-screen-symbolic", "Unlock key");
            row.add_suffix(&unlock_button);

            let unlock_state = state.clone();
            let fingerprint = key.fingerprint.clone();
            unlock_button.connect_clicked(move |_| {
                let refresh_state = unlock_state.clone();
                prompt_private_key_unlock_for_action(
                    &unlock_state.platform.overlay,
                    fingerprint.clone(),
                    Rc::new(move || super::rebuild_store_recipients_list(&refresh_state)),
                );
            });
        } else if unlocked {
            let unlocked_icon = Image::from_icon_name("changes-allow-symbolic");
            unlocked_icon.add_css_class("accent");
            row.add_suffix(&unlocked_icon);
        }

        let copy_button =
            flat_icon_button_with_tooltip("edit-copy-symbolic", "Copy armored private key");
        row.add_suffix(&copy_button);

        let delete_button = flat_icon_button_with_tooltip("user-trash-symbolic", "Remove key");
        row.add_suffix(&delete_button);
        state.list.append(&row);

        let toggle_for_row = toggle.clone();
        row.connect_activated(move |_| {
            toggle_for_row.set_active(!toggle_for_row.is_active());
        });

        let page_state = state.clone();
        let key_for_toggle = key.clone();
        toggle.connect_toggled(move |button| {
            if set_private_key_recipient_enabled(&page_state, &key_for_toggle, button.is_active()) {
                queue_store_recipients_autosave(&page_state);
            }
        });

        let page_state = state.clone();
        let key_for_copy = key.clone();
        let copy_button_for_click = copy_button.clone();
        copy_button.connect_clicked(move |_| {
            copy_armored_private_key(
                &page_state,
                &key_for_copy.fingerprint,
                Some(&copy_button_for_click),
            );
        });

        let page_state = state.clone();
        let key_for_delete = key.clone();
        delete_button.connect_clicked(move |_| {
            if let Err(err) = remove_ripasso_private_key(&key_for_delete.fingerprint) {
                log_error(format!(
                    "Failed to remove private key '{}': {err}",
                    key_for_delete.fingerprint
                ));
                page_state
                    .platform
                    .overlay
                    .add_toast(Toast::new("Couldn't remove that key."));
                return;
            }

            if Preferences::new().ripasso_own_fingerprint().as_deref()
                == Some(key_for_delete.fingerprint.as_str())
            {
                let _ = Preferences::new().set_ripasso_own_fingerprint(None);
            }
            let recipients_changed =
                set_private_key_recipient_enabled(&page_state, &key_for_delete, false);
            super::rebuild_store_recipients_list(&page_state);
            if recipients_changed {
                queue_store_recipients_autosave(&page_state);
            }
        });
    }

    append_private_key_generate_row(state);
    append_private_key_clipboard_import_row(state);
    append_private_key_import_row(state);
}

#[cfg(test)]
mod tests {
    use super::{
        recipient_matches_private_key, set_private_key_recipient_values,
        unresolved_private_key_recipients,
    };
    use crate::backend::ManagedRipassoPrivateKey;

    #[test]
    fn imported_private_keys_match_existing_user_id_recipients() {
        let key = ManagedRipassoPrivateKey {
            fingerprint: "10F4487A3768155709168A8E3D00743E10EA9232".to_string(),
            user_ids: vec!["pass@store.local".to_string()],
        };

        assert!(recipient_matches_private_key("pass@store.local", &key));
        assert!(recipient_matches_private_key(
            "10F4487A3768155709168A8E3D00743E10EA9232",
            &key
        ));
        assert!(!recipient_matches_private_key("other@example.com", &key));
    }

    #[test]
    fn enabling_a_private_key_replaces_matching_user_ids_with_the_fingerprint() {
        let mut recipients = vec![
            "pass@store.local".to_string(),
            "other@example.com".to_string(),
        ];
        let key = ManagedRipassoPrivateKey {
            fingerprint: "10F4487A3768155709168A8E3D00743E10EA9232".to_string(),
            user_ids: vec!["pass@store.local".to_string()],
        };

        assert!(set_private_key_recipient_values(
            &mut recipients,
            &key,
            true
        ));
        assert_eq!(
            recipients,
            vec![
                "other@example.com".to_string(),
                "10F4487A3768155709168A8E3D00743E10EA9232".to_string(),
            ]
        );
    }

    #[test]
    fn disabling_a_private_key_removes_all_matching_recipients() {
        let mut recipients = vec![
            "pass@store.local",
            "10F4487A3768155709168A8E3D00743E10EA9232",
            "other@example.com",
        ]
        .into_iter()
        .map(str::to_string)
        .collect::<Vec<_>>();
        let key = ManagedRipassoPrivateKey {
            fingerprint: "10F4487A3768155709168A8E3D00743E10EA9232".to_string(),
            user_ids: vec!["pass@store.local".to_string()],
        };

        assert!(set_private_key_recipient_values(
            &mut recipients,
            &key,
            false
        ));
        assert_eq!(recipients, vec!["other@example.com".to_string()]);
    }

    #[test]
    fn unresolved_private_key_recipients_keep_values_not_available_in_the_app() {
        let recipients = vec![
            "pass@store.local".to_string(),
            "missing@example.com".to_string(),
            "10F4487A3768155709168A8E3D00743E10EA9232".to_string(),
        ];
        let keys = vec![ManagedRipassoPrivateKey {
            fingerprint: "10F4487A3768155709168A8E3D00743E10EA9232".to_string(),
            user_ids: vec!["pass@store.local".to_string()],
        }];

        assert_eq!(
            unresolved_private_key_recipients(&recipients, &keys),
            vec!["missing@example.com".to_string()]
        );
    }
}
