use super::import::append_private_key_import_row;
use super::{super::queue_store_recipients_autosave, StoreRecipientsPageState};
use crate::backend::{
    is_ripasso_private_key_unlocked, list_ripasso_private_keys, remove_ripasso_private_key,
    ripasso_private_key_requires_session_unlock, ManagedRipassoPrivateKey,
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
    let mut recipients = state.recipients.borrow_mut();
    let before = recipients.clone();
    recipients.retain(|value| !recipient_matches_private_key(value, key));
    if enabled {
        recipients.push(key.fingerprint.clone());
    }
    *recipients != before
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
            append_private_key_import_row(state);
            return;
        }
    };

    if keys.is_empty() {
        append_info_row(
            &state.list,
            "No private keys yet",
            "Import a private key first.",
        );
        append_private_key_import_row(state);
        return;
    }

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

    append_private_key_import_row(state);
}

#[cfg(test)]
mod tests {
    use super::recipient_matches_private_key;
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
}
