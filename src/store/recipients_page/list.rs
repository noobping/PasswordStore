use super::export::copy_armored_private_key;
use super::{queue_store_recipients_autosave, StoreRecipientsPageState};
use crate::backend::{
    is_ripasso_private_key_unlocked, list_ripasso_private_keys, remove_ripasso_private_key,
    ripasso_private_key_requires_session_unlock, ManagedRipassoPrivateKey,
    StoreRecipientsPrivateKeyRequirement,
};
use crate::logging::log_error;
use crate::preferences::Preferences;
use crate::private_key::unlock::prompt_private_key_unlock_for_action;
use crate::support::actions::activate_widget_action;
use crate::support::ui::{
    append_info_row, clear_list_box, dim_label_icon, flat_icon_button_with_tooltip,
};
use adw::gtk::Image;
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
            .any(|user_id: &String| user_id.eq_ignore_ascii_case(recipient))
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

fn private_key_delete_block_message(active: bool) -> Option<&'static str> {
    active.then_some("Uncheck this private key before removing it.")
}

fn sync_private_key_delete_button(delete_button: &adw::gtk::Button, active: bool) {
    delete_button.set_sensitive(!active);
    delete_button.set_tooltip_text(Some(
        private_key_delete_block_message(active).unwrap_or("Remove key"),
    ));
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

fn sync_private_key_requirement_row(state: &StoreRecipientsPageState, has_keys: bool) {
    state.platform.require_all_row.set_visible(has_keys);
    state.platform.require_all_check.set_active(matches!(
        state.private_key_requirement.get(),
        StoreRecipientsPrivateKeyRequirement::AllManagedKeys
    ));
}

pub(super) fn connect_private_key_requirement_control(state: &StoreRecipientsPageState) {
    let row = state.platform.require_all_row.clone();
    let check = state.platform.require_all_check.clone();
    let check_for_row = check.clone();
    row.connect_activated(move |_| {
        check_for_row.set_active(!check_for_row.is_active());
    });

    let page_state = state.clone();
    check.connect_toggled(move |button| {
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
            sync_private_key_requirement_row(state, false);
            append_info_row(
                &state.list,
                "Couldn't load private keys",
                "Try again from Preferences.",
            );
            return;
        }
    };

    let current_recipients = state.recipients.borrow().clone();
    let unresolved_recipients = unresolved_private_key_recipients(&current_recipients, &keys);
    sync_private_key_requirement_row(state, !keys.is_empty());

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
        return;
    }

    append_unresolved_private_key_rows(state, &unresolved_recipients);

    for key in keys {
        append_private_key_row(state, &key);
    }
}

fn append_private_key_row(state: &StoreRecipientsPageState, key: &ManagedRipassoPrivateKey) {
    let active = state
        .recipients
        .borrow()
        .iter()
        .any(|recipient| recipient_matches_private_key(recipient, key));
    let title = adw::glib::markup_escape_text(&key.title());
    let row = ActionRow::builder()
        .title(title.as_str())
        .subtitle(&key.fingerprint)
        .build();
    row.set_activatable(true);
    row.add_prefix(&dim_label_icon("dialog-password-symbolic"));

    let toggle = adw::gtk::CheckButton::new();
    toggle.set_active(active);
    row.add_suffix(&toggle);
    append_private_key_status_suffixes(state, key, &row);

    let copy_button =
        flat_icon_button_with_tooltip("edit-copy-symbolic", "Copy armored private key");
    row.add_suffix(&copy_button);

    let delete_button = flat_icon_button_with_tooltip("user-trash-symbolic", "Remove key");
    sync_private_key_delete_button(&delete_button, active);
    row.add_suffix(&delete_button);
    state.list.append(&row);

    let toggle_for_row = toggle.clone();
    row.connect_activated(move |_| {
        toggle_for_row.set_active(!toggle_for_row.is_active());
    });

    connect_private_key_row_actions(state, key, &toggle, &copy_button, &delete_button);
}

fn append_private_key_status_suffixes(
    state: &StoreRecipientsPageState,
    key: &ManagedRipassoPrivateKey,
    row: &ActionRow,
) {
    let (unlocked, requires_unlock) = inspect_private_key_lock_state(&key.fingerprint);
    if requires_unlock {
        let icon_name = if unlocked {
            "changes-allow-symbolic"
        } else {
            "changes-prevent-symbolic"
        };
        let tooltip = if unlocked {
            "Key is unlocked for this session"
        } else {
            "Key requires unlocking before use"
        };
        let icon = Image::from_icon_name(icon_name);
        icon.set_tooltip_text(Some(tooltip));
        row.add_suffix(&icon);
    }

    if !Preferences::new().uses_integrated_backend() {
        return;
    }

    if !unlocked && requires_unlock {
        let unlock_button = flat_icon_button_with_tooltip("dialog-password-symbolic", "Unlock key");
        row.add_suffix(&unlock_button);
        let state = state.clone();
        let fingerprint = key.fingerprint.clone();
        unlock_button.connect_clicked(move |_| {
            let after_unlock: Rc<dyn Fn()> = Rc::new({
                let state = state.clone();
                move || {
                    super::rebuild_store_recipients_list(&state);
                    activate_widget_action(&state.window, "win.reload-password-list");
                }
            });
            let on_finish: Rc<dyn Fn(bool)> = Rc::new(|_| {});
            prompt_private_key_unlock_for_action(
                &state.platform.overlay,
                fingerprint.clone(),
                after_unlock,
                on_finish,
            );
        });
    }
}

fn connect_private_key_row_actions(
    state: &StoreRecipientsPageState,
    key: &ManagedRipassoPrivateKey,
    toggle: &adw::gtk::CheckButton,
    copy_button: &adw::gtk::Button,
    delete_button: &adw::gtk::Button,
) {
    let state_for_toggle = state.clone();
    let key_for_toggle = key.clone();
    let delete_for_toggle = delete_button.clone();
    toggle.connect_toggled(move |button| {
        if set_private_key_recipient_enabled(&state_for_toggle, &key_for_toggle, button.is_active())
        {
            sync_private_key_delete_button(&delete_for_toggle, button.is_active());
            queue_store_recipients_autosave(&state_for_toggle);
        }
    });

    let state_for_copy = state.clone();
    let fingerprint_for_copy = key.fingerprint.clone();
    let copy_button_for_click = copy_button.clone();
    copy_button.connect_clicked(move |_| {
        copy_armored_private_key(
            &state_for_copy,
            &fingerprint_for_copy,
            Some(&copy_button_for_click),
        );
    });

    let state_for_delete = state.clone();
    let key_for_delete = key.clone();
    delete_button.connect_clicked(move |_| {
        if let Err(err) = remove_ripasso_private_key(&key_for_delete.fingerprint) {
            log_error(format!(
                "Failed to remove private key '{}': {err}",
                key_for_delete.fingerprint
            ));
            state_for_delete
                .platform
                .overlay
                .add_toast(Toast::new("Couldn't remove that key."));
            return;
        }

        state_for_delete
            .recipients
            .borrow_mut()
            .retain(|value| !recipient_matches_private_key(value, &key_for_delete));
        super::rebuild_store_recipients_list(&state_for_delete);
        queue_store_recipients_autosave(&state_for_delete);
        activate_widget_action(&state_for_delete.window, "win.reload-password-list");
        state_for_delete
            .platform
            .overlay
            .add_toast(Toast::new("Key removed."));
    });
}
