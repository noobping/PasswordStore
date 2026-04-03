use super::export::copy_managed_key_material;
use super::mode::{
    current_selection_mode, show_standard_private_key_choice, sync_store_recipients_mode_controls,
    StoreRecipientsSelectionMode,
};
use super::sync::{sync_private_keys_from_host_if_enabled, sync_private_keys_to_host_if_enabled};
use super::{queue_store_recipients_autosave, StoreRecipientsPageState};
use crate::backend::{
    is_ripasso_private_key_unlocked, list_ripasso_private_keys, remove_ripasso_private_key,
    ripasso_private_key_requires_session_unlock, ManagedRipassoPrivateKey,
    ManagedRipassoPrivateKeyProtection, StoreRecipientsPrivateKeyRequirement,
};
#[cfg(target_os = "linux")]
use crate::backend::{list_host_gpg_private_keys, HostGpgPrivateKeySummary};
use crate::clipboard::set_clipboard_text;
use crate::fido2_recipient::{
    fido2_recipient_subtitle, fido2_recipient_title, is_fido2_recipient_string,
    same_fido2_recipient,
};
use crate::i18n::gettext;
use crate::logging::log_error;
use crate::preferences::Preferences;
use crate::private_key::unlock::prompt_private_key_unlock_for_action;
use crate::store::git_page::rebuild_store_recipients_git_row;
use crate::support::actions::activate_widget_action;
use crate::support::ui::{
    add_persistent_hide_button, append_info_row, clear_list_box, dim_label_icon,
    flat_icon_button_with_tooltip,
};
use adw::prelude::*;
use adw::{ActionRow, Toast};
use std::collections::HashSet;
use std::rc::Rc;

#[cfg(not(target_os = "linux"))]
#[derive(Clone, Debug, PartialEq, Eq)]
struct HostGpgPrivateKeySummary {
    fingerprint: String,
    user_ids: Vec<String>,
}

#[cfg(not(target_os = "linux"))]
impl HostGpgPrivateKeySummary {
    fn title(&self) -> String {
        self.user_ids
            .first()
            .cloned()
            .filter(|value| !value.trim().is_empty())
            .unwrap_or_else(|| gettext("Unnamed host private key"))
    }
}

#[cfg(not(target_os = "linux"))]
fn list_host_gpg_private_keys() -> Result<Vec<HostGpgPrivateKeySummary>, String> {
    Ok(Vec::new())
}

#[derive(Clone, Debug, PartialEq, Eq)]
enum AvailablePrivateKey {
    Managed(ManagedRipassoPrivateKey),
    HostOnly(HostGpgPrivateKeySummary),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum PrivateKeyVerificationWarning {
    HostInspectionFailed,
    SyncDisabled,
}

const HOST_GPG_WARNING_NOTICE_ID: &str = "store-recipients-host-gpg-warning";
const ALL_FIDO2_KEYS_REQUIRED_NOTICE_ID: &str = "store-recipients-all-fido2-keys-required";

impl PrivateKeyVerificationWarning {
    const fn title(self) -> &'static str {
        match self {
            Self::HostInspectionFailed => "Couldn't inspect host GPG keys",
            Self::SyncDisabled => "Private keys can't be verified",
        }
    }

    const fn subtitle(self) -> &'static str {
        match self {
            Self::HostInspectionFailed => "Valid host keys may appear unavailable here.",
            Self::SyncDisabled => {
                "Valid host keys may appear unavailable here while private-key sync is off."
            }
        }
    }
}

impl AvailablePrivateKey {
    fn fingerprint(&self) -> &str {
        match self {
            Self::Managed(key) => &key.fingerprint,
            Self::HostOnly(key) => &key.fingerprint,
        }
    }

    fn user_ids(&self) -> &[String] {
        match self {
            Self::Managed(key) => &key.user_ids,
            Self::HostOnly(key) => &key.user_ids,
        }
    }

    fn title(&self) -> String {
        match self {
            Self::Managed(key) => key.title(),
            Self::HostOnly(key) => key.title(),
        }
    }
}

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

fn recipient_matches_parts(recipient: &str, fingerprint: &str, user_ids: &[String]) -> bool {
    let recipient = recipient.trim();
    if is_fido2_recipient_string(recipient) && is_fido2_recipient_string(fingerprint) {
        return same_fido2_recipient(recipient, fingerprint);
    }
    recipient.eq_ignore_ascii_case(fingerprint)
        || user_ids
            .iter()
            .any(|user_id| user_id.eq_ignore_ascii_case(recipient))
}

pub(super) fn recipient_matches_private_key(
    recipient: &str,
    key: &ManagedRipassoPrivateKey,
) -> bool {
    recipient_matches_parts(recipient, &key.fingerprint, &key.user_ids)
}

fn recipient_matches_available_private_key(recipient: &str, key: &AvailablePrivateKey) -> bool {
    recipient_matches_parts(recipient, key.fingerprint(), key.user_ids())
}

fn set_private_key_recipient_enabled(
    state: &StoreRecipientsPageState,
    fingerprint: &str,
    user_ids: &[String],
    enabled: bool,
) -> bool {
    set_private_key_recipient_values(
        &mut state.recipients.borrow_mut(),
        fingerprint,
        user_ids,
        enabled,
    )
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
    fingerprint: &str,
    user_ids: &[String],
    enabled: bool,
) -> bool {
    let before = recipients.clone();
    recipients.retain(|value| !recipient_matches_parts(value, fingerprint, user_ids));
    if enabled {
        recipients.push(fingerprint.to_string());
    }
    *recipients != before
}

fn selected_available_private_key_count(
    recipients: &[String],
    keys: &[AvailablePrivateKey],
    count_fido2_recipients: bool,
) -> usize {
    let available_private_keys = keys
        .iter()
        .filter(|key| {
            recipients
                .iter()
                .any(|recipient| recipient_matches_available_private_key(recipient, key))
        })
        .count();
    let available_fido2 = if count_fido2_recipients {
        recipients
            .iter()
            .filter(|recipient| is_fido2_recipient_string(recipient))
            .filter(|recipient| {
                !keys
                    .iter()
                    .any(|key| recipient_matches_available_private_key(recipient, key))
            })
            .count()
    } else {
        0
    };

    available_private_keys + available_fido2
}

fn private_key_is_currently_usable(key: &AvailablePrivateKey) -> bool {
    match key {
        AvailablePrivateKey::Managed(key) => {
            let (unlocked, requires_unlock) = inspect_private_key_lock_state(&key.fingerprint);
            unlocked || !requires_unlock
        }
        AvailablePrivateKey::HostOnly(_) => true,
    }
}

fn selected_usable_private_key_count(
    recipients: &[String],
    keys: &[AvailablePrivateKey],
    count_fido2_recipients: bool,
) -> usize {
    let usable_private_keys = keys
        .iter()
        .filter(|key| {
            recipients
                .iter()
                .any(|recipient| recipient_matches_available_private_key(recipient, key))
                && private_key_is_currently_usable(key)
        })
        .count();
    let usable_fido2 = if count_fido2_recipients {
        recipients
            .iter()
            .filter(|recipient| is_fido2_recipient_string(recipient))
            .filter(|recipient| {
                !keys
                    .iter()
                    .any(|key| recipient_matches_available_private_key(recipient, key))
            })
            .count()
    } else {
        0
    };

    usable_private_keys + usable_fido2
}

fn private_key_delete_block_message(
    active: bool,
    require_all_selected_keys: bool,
    selected_available_keys: usize,
) -> Option<&'static str> {
    if !active {
        None
    } else if require_all_selected_keys {
        Some("This selected key is required while all selected private keys are required.")
    } else if selected_available_keys <= 1 {
        Some("Keep another selected private key available before removing this key.")
    } else {
        None
    }
}

fn private_key_toggle_block_message(
    active: bool,
    usable: bool,
    require_all_selected_keys: bool,
    selected_available_keys: usize,
    selected_usable_keys: usize,
) -> Option<&'static str> {
    if !active {
        None
    } else if require_all_selected_keys {
        Some("Keep this key selected while all selected private keys are required.")
    } else if selected_available_keys <= 1 {
        Some("Keep at least one selected private key available.")
    } else if usable && selected_usable_keys <= 1 {
        Some("Unlock another selected private key before clearing this one.")
    } else {
        None
    }
}

fn sync_private_key_delete_button(delete_button: &adw::gtk::Button, blocked_message: Option<&str>) {
    delete_button.set_sensitive(blocked_message.is_none());
    let tooltip = gettext(blocked_message.unwrap_or("Remove key file"));
    delete_button.set_tooltip_text(Some(&tooltip));
}

fn sync_recipient_remove_button(button: &adw::gtk::Button, blocked_message: Option<&str>) {
    button.set_sensitive(blocked_message.is_none());
    let tooltip = gettext(blocked_message.unwrap_or("Remove recipient"));
    button.set_tooltip_text(Some(&tooltip));
}

fn sync_private_key_toggle_button(toggle: &adw::gtk::CheckButton, blocked_message: Option<&str>) {
    toggle.set_sensitive(blocked_message.is_none());
    let tooltip = blocked_message.map(gettext);
    toggle.set_tooltip_text(tooltip.as_deref());
}

fn unresolved_private_key_recipients(
    recipients: &[String],
    keys: &[AvailablePrivateKey],
) -> Vec<String> {
    let mut unresolved = Vec::new();

    for recipient in recipients {
        if is_fido2_recipient_string(recipient) {
            continue;
        }
        if keys
            .iter()
            .any(|key| recipient_matches_available_private_key(recipient, key))
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
            .subtitle(gettext("This recipient is not available in the app."))
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

fn show_require_all_private_keys_option(
    selection_mode: StoreRecipientsSelectionMode,
    has_keys: bool,
) -> bool {
    has_keys && !matches!(selection_mode, StoreRecipientsSelectionMode::Fido2Only)
}

fn show_all_fido2_keys_required_info(
    selection_mode: StoreRecipientsSelectionMode,
    selected_fido2_keys: usize,
) -> bool {
    matches!(selection_mode, StoreRecipientsSelectionMode::Fido2Only) && selected_fido2_keys > 1
}

fn show_store_options_title_above_git_row(show_require_all: bool, show_git: bool) -> bool {
    show_git && !show_require_all
}

fn sync_private_key_requirement_row(
    state: &StoreRecipientsPageState,
    selection_mode: StoreRecipientsSelectionMode,
    has_keys: bool,
    selected_fido2_keys: usize,
) {
    let preferences = Preferences::new();
    let uses_integrated_backend = preferences.uses_integrated_backend();
    let show_require_all = show_require_all_private_keys_option(selection_mode, has_keys);
    let show_all_fido2_required =
        show_all_fido2_keys_required_info(selection_mode, selected_fido2_keys)
            && !preferences.is_notice_hidden(ALL_FIDO2_KEYS_REQUIRED_NOTICE_ID);
    let show_store_options_title = show_store_options_title_above_git_row(
        show_require_all,
        state.platform.git_group.is_visible(),
    );
    let git_group_title = if show_store_options_title {
        gettext("Store options")
    } else {
        String::new()
    };

    state.platform.options_group.set_visible(show_require_all);
    state.platform.git_group.set_title(&git_group_title);
    state
        .platform
        .fido2_info_group
        .set_visible(show_all_fido2_required);
    state.platform.require_all_row.set_visible(show_require_all);
    state
        .platform
        .all_fido2_keys_required_row
        .set_visible(show_all_fido2_required);
    state
        .platform
        .require_all_row
        .set_sensitive(show_require_all && uses_integrated_backend);
    state
        .platform
        .require_all_check
        .set_sensitive(show_require_all && uses_integrated_backend);
    state.platform.require_all_check.set_active(matches!(
        state.private_key_requirement.get(),
        StoreRecipientsPrivateKeyRequirement::AllManagedKeys
    ));
}

fn selected_fido2_recipients(recipients: &[String], keys: &[AvailablePrivateKey]) -> Vec<String> {
    let mut selected = Vec::<String>::new();

    for recipient in recipients {
        if !is_fido2_recipient_string(recipient) {
            continue;
        }
        if keys
            .iter()
            .any(|key| recipient_matches_available_private_key(recipient, key))
        {
            continue;
        }
        if selected
            .iter()
            .any(|existing| same_fido2_recipient(existing, recipient))
        {
            continue;
        }
        selected.push(recipient.clone());
    }

    selected
}

fn selected_fido2_key_count(recipients: &[String]) -> usize {
    let mut selected = Vec::<String>::new();

    for recipient in recipients {
        if !is_fido2_recipient_string(recipient) {
            continue;
        }
        if selected
            .iter()
            .any(|existing| same_fido2_recipient(existing, recipient))
        {
            continue;
        }
        selected.push(recipient.clone());
    }

    selected.len()
}

fn fido2_recipient_remove_block_message(
    uses_integrated_backend: bool,
    require_all_selected_keys: bool,
    selected_available_keys: usize,
    selected_usable_keys: usize,
) -> Option<&'static str> {
    if !uses_integrated_backend {
        None
    } else {
        private_key_toggle_block_message(
            true,
            true,
            require_all_selected_keys,
            selected_available_keys,
            selected_usable_keys,
        )
    }
}

fn effective_private_key_verification_warning(
    selection_mode: StoreRecipientsSelectionMode,
    warning: Option<PrivateKeyVerificationWarning>,
) -> Option<PrivateKeyVerificationWarning> {
    if matches!(selection_mode, StoreRecipientsSelectionMode::Fido2Only) {
        None
    } else {
        warning
    }
}

fn sync_private_key_verification_warning(
    state: &StoreRecipientsPageState,
    selection_mode: StoreRecipientsSelectionMode,
    warning: Option<PrivateKeyVerificationWarning>,
) {
    let warning = effective_private_key_verification_warning(selection_mode, warning);
    let show_warning =
        warning.is_some() && !Preferences::new().is_notice_hidden(HOST_GPG_WARNING_NOTICE_ID);

    if let Some(warning) = warning {
        state
            .platform
            .host_gpg_warning_row
            .set_title(&gettext(warning.title()));
        state
            .platform
            .host_gpg_warning_row
            .set_subtitle(&gettext(warning.subtitle()));
    }
    state
        .platform
        .host_gpg_warning_group
        .set_visible(show_warning);
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
            super::rebuild_store_recipients_list(&page_state);
            queue_store_recipients_autosave(&page_state);
        }
    });
}

pub(super) fn connect_dismissible_notice_controls(state: &StoreRecipientsPageState) {
    let host_warning_group = state.platform.host_gpg_warning_group.clone();
    add_persistent_hide_button(
        &state.platform.host_gpg_warning_row,
        HOST_GPG_WARNING_NOTICE_ID,
        move || host_warning_group.set_visible(false),
    );

    let fido2_info_group = state.platform.fido2_info_group.clone();
    add_persistent_hide_button(
        &state.platform.all_fido2_keys_required_row,
        ALL_FIDO2_KEYS_REQUIRED_NOTICE_ID,
        move || fido2_info_group.set_visible(false),
    );
}

fn merge_available_private_keys(
    managed_keys: Vec<ManagedRipassoPrivateKey>,
    host_keys: Vec<HostGpgPrivateKeySummary>,
) -> Vec<AvailablePrivateKey> {
    let mut seen_fingerprints: HashSet<String> = managed_keys
        .iter()
        .map(|key| key.fingerprint.to_ascii_lowercase())
        .collect();
    let mut keys: Vec<AvailablePrivateKey> = managed_keys
        .into_iter()
        .map(AvailablePrivateKey::Managed)
        .collect();

    for key in host_keys {
        if seen_fingerprints.insert(key.fingerprint.to_ascii_lowercase()) {
            keys.push(AvailablePrivateKey::HostOnly(key));
        }
    }

    keys.sort_by(|left, right| {
        left.title()
            .to_ascii_lowercase()
            .cmp(&right.title().to_ascii_lowercase())
            .then_with(|| left.fingerprint().cmp(right.fingerprint()))
    });
    keys
}

fn private_key_verification_warning(
    uses_host_backend: bool,
    sync_enabled: bool,
    host_key_inspection_failed: bool,
) -> Option<PrivateKeyVerificationWarning> {
    if !cfg!(target_os = "linux") {
        return None;
    }

    if uses_host_backend && host_key_inspection_failed {
        Some(PrivateKeyVerificationWarning::HostInspectionFailed)
    } else if !uses_host_backend && !sync_enabled {
        Some(PrivateKeyVerificationWarning::SyncDisabled)
    } else {
        None
    }
}

fn load_available_private_keys(
    managed_keys: Vec<ManagedRipassoPrivateKey>,
    uses_host_backend: bool,
) -> (Vec<AvailablePrivateKey>, bool) {
    if !uses_host_backend {
        return (
            managed_keys
                .into_iter()
                .map(AvailablePrivateKey::Managed)
                .collect(),
            false,
        );
    }

    let host_keys = list_host_gpg_private_keys();
    match host_keys {
        Ok(host_keys) => (merge_available_private_keys(managed_keys, host_keys), false),
        Err(err) => {
            log_error(format!(
                "Failed to inspect host GPG private keys for recipients: {err}"
            ));
            (
                managed_keys
                    .into_iter()
                    .map(AvailablePrivateKey::Managed)
                    .collect(),
                true,
            )
        }
    }
}

fn show_available_private_key_choice(
    selection_mode: StoreRecipientsSelectionMode,
    key: &AvailablePrivateKey,
    active: bool,
) -> bool {
    match key {
        AvailablePrivateKey::Managed(key) => match key.protection {
            ManagedRipassoPrivateKeyProtection::Password
            | ManagedRipassoPrivateKeyProtection::HardwareOpenPgpCard => {
                show_standard_private_key_choice(selection_mode, active)
            }
            #[cfg(feature = "fidokey")]
            ManagedRipassoPrivateKeyProtection::Fido2HmacSecret => {
                show_standard_private_key_choice(selection_mode, active)
            }
        },
        AvailablePrivateKey::HostOnly(_) => {
            show_standard_private_key_choice(selection_mode, active)
        }
    }
}

pub(super) fn rebuild_store_recipients_list(state: &StoreRecipientsPageState) {
    clear_list_box(&state.list);
    rebuild_store_recipients_git_row(state);
    sync_private_key_verification_warning(state, StoreRecipientsSelectionMode::Empty, None);
    let _ = sync_private_keys_from_host_if_enabled(state);
    let current_recipients = state.recipients.borrow().clone();

    let preferences = Preferences::new();
    let uses_host_backend = preferences.uses_host_command_backend();
    let uses_integrated_backend = preferences.uses_integrated_backend();
    let sync_enabled = preferences.sync_private_keys_with_host();
    let selection_mode = current_selection_mode(state);
    sync_store_recipients_mode_controls(state, selection_mode, uses_integrated_backend);

    let managed_keys = match list_ripasso_private_keys() {
        Ok(keys) => keys,
        Err(err) => {
            log_error(format!("Failed to load private keys for recipients: {err}"));
            sync_private_key_requirement_row(state, selection_mode, false, 0);
            append_info_row(
                &state.list,
                "Couldn't load private keys",
                "Try again from Preferences.",
            );
            return;
        }
    };

    let managed_key_count = managed_keys.len();
    let (keys, host_key_inspection_failed) =
        load_available_private_keys(managed_keys, uses_host_backend);
    let fido2_recipients = selected_fido2_recipients(&current_recipients, &keys);
    let selected_fido2_keys = selected_fido2_key_count(&current_recipients);
    let unresolved_recipients = unresolved_private_key_recipients(&current_recipients, &keys);
    let selected_available_keys =
        selected_available_private_key_count(&current_recipients, &keys, uses_integrated_backend);
    let selected_usable_keys =
        selected_usable_private_key_count(&current_recipients, &keys, uses_integrated_backend);
    sync_private_key_requirement_row(
        state,
        selection_mode,
        managed_key_count > 0 || (uses_integrated_backend && selected_fido2_keys > 0),
        selected_fido2_keys,
    );
    sync_private_key_verification_warning(
        state,
        selection_mode,
        private_key_verification_warning(
            uses_host_backend,
            sync_enabled,
            host_key_inspection_failed,
        ),
    );

    if keys.is_empty() && fido2_recipients.is_empty() {
        if unresolved_recipients.is_empty() {
            append_info_row(
                &state.list,
                "No recipients yet",
                if uses_integrated_backend {
                    "Generate or import a private key, or add a FIDO2 security key."
                } else {
                    "Generate or import a private key."
                },
            );
        } else {
            append_unresolved_private_key_rows(state, &unresolved_recipients);
        }
        return;
    }

    append_unresolved_private_key_rows(state, &unresolved_recipients);
    for recipient in &fido2_recipients {
        append_fido2_recipient_row(
            state,
            recipient,
            uses_integrated_backend,
            selected_available_keys,
            selected_usable_keys,
        );
    }

    for key in keys {
        let active = current_recipients
            .iter()
            .any(|recipient| recipient_matches_available_private_key(recipient, &key));
        if !show_available_private_key_choice(selection_mode, &key, active) {
            continue;
        }

        match key {
            AvailablePrivateKey::Managed(key) => append_managed_private_key_row(
                state,
                &key,
                selected_available_keys,
                selected_usable_keys,
            ),
            AvailablePrivateKey::HostOnly(key) => append_host_private_key_row(
                state,
                &key,
                selected_available_keys,
                selected_usable_keys,
            ),
        }
    }
}

fn append_fido2_recipient_row(
    state: &StoreRecipientsPageState,
    recipient: &str,
    uses_integrated_backend: bool,
    selected_available_keys: usize,
    selected_usable_keys: usize,
) {
    let require_all_selected_keys = matches!(
        state.private_key_requirement.get(),
        StoreRecipientsPrivateKeyRequirement::AllManagedKeys
    );
    let remove_blocked_message = fido2_recipient_remove_block_message(
        uses_integrated_backend,
        require_all_selected_keys,
        selected_available_keys,
        selected_usable_keys,
    );
    let title = fido2_recipient_title(recipient).unwrap_or_else(|| gettext("FIDO2 security key"));
    let subtitle =
        fido2_recipient_subtitle(recipient).unwrap_or_else(|| gettext("FIDO2 recipient"));
    let row = ActionRow::builder().title(title).subtitle(subtitle).build();
    row.set_activatable(false);
    row.add_prefix(&dim_label_icon("dialog-password-symbolic"));
    let remove_button = flat_icon_button_with_tooltip("user-trash-symbolic", "Remove recipient");
    sync_recipient_remove_button(&remove_button, remove_blocked_message);
    row.add_suffix(&remove_button);
    state.list.append(&row);

    let state_for_remove = state.clone();
    let recipient_for_remove = recipient.to_string();
    remove_button.connect_clicked(move |_| {
        let before = state_for_remove.recipients.borrow().clone();
        state_for_remove
            .recipients
            .borrow_mut()
            .retain(|value| !same_fido2_recipient(value, &recipient_for_remove));
        super::rebuild_store_recipients_list(&state_for_remove);
        if *state_for_remove.recipients.borrow() != before {
            queue_store_recipients_autosave(&state_for_remove);
        }
    });
}

fn append_private_key_row_shell(
    title: &str,
    subtitle: &str,
    active: bool,
    toggle_blocked_message: Option<&str>,
) -> (ActionRow, adw::gtk::CheckButton) {
    let title = adw::glib::markup_escape_text(title);
    let row = ActionRow::builder()
        .title(title.as_str())
        .subtitle(subtitle)
        .build();
    row.set_activatable(false);
    row.add_prefix(&dim_label_icon("dialog-password-symbolic"));

    let toggle = adw::gtk::CheckButton::new();
    toggle.set_active(active);
    sync_private_key_toggle_button(&toggle, toggle_blocked_message);
    row.add_suffix(&toggle);

    (row, toggle)
}

fn append_managed_private_key_row(
    state: &StoreRecipientsPageState,
    key: &ManagedRipassoPrivateKey,
    selected_available_keys: usize,
    selected_usable_keys: usize,
) {
    let active = state
        .recipients
        .borrow()
        .iter()
        .any(|recipient| recipient_matches_private_key(recipient, key));
    let require_all_selected_keys = matches!(
        state.private_key_requirement.get(),
        StoreRecipientsPrivateKeyRequirement::AllManagedKeys
    );
    let (unlocked, requires_unlock) = inspect_private_key_lock_state(&key.fingerprint);
    let usable = unlocked || !requires_unlock;
    let toggle_blocked_message = private_key_toggle_block_message(
        active,
        usable,
        require_all_selected_keys,
        selected_available_keys,
        selected_usable_keys,
    );
    let delete_blocked_message = private_key_delete_block_message(
        active,
        require_all_selected_keys,
        selected_available_keys,
    );
    let subtitle = match key.protection {
        ManagedRipassoPrivateKeyProtection::Password => {
            gettext("{fingerprint} - Password protected").replace("{fingerprint}", &key.fingerprint)
        }
        ManagedRipassoPrivateKeyProtection::HardwareOpenPgpCard => {
            gettext("{fingerprint} - Hardware key").replace("{fingerprint}", &key.fingerprint)
        }
        #[cfg(feature = "fidokey")]
        ManagedRipassoPrivateKeyProtection::Fido2HmacSecret => {
            gettext("{fingerprint} - Security key protected")
                .replace("{fingerprint}", &key.fingerprint)
        }
    };
    let (row, toggle) =
        append_private_key_row_shell(&key.title(), &subtitle, active, toggle_blocked_message);
    append_private_key_status_suffixes(state, key, &row, unlocked, requires_unlock);

    let copy_button = flat_icon_button_with_tooltip(
        "edit-copy-symbolic",
        match key.protection {
            ManagedRipassoPrivateKeyProtection::Password => "Copy armored private key",
            ManagedRipassoPrivateKeyProtection::HardwareOpenPgpCard => "Copy armored public key",
            #[cfg(feature = "fidokey")]
            ManagedRipassoPrivateKeyProtection::Fido2HmacSecret => {
                "Copy FIDO2-protected private key"
            }
        },
    );
    row.add_suffix(&copy_button);

    let delete_button = flat_icon_button_with_tooltip("user-trash-symbolic", "Remove key");
    sync_private_key_delete_button(&delete_button, delete_blocked_message);
    row.add_suffix(&delete_button);
    state.list.append(&row);

    connect_managed_private_key_row_actions(state, key, &toggle, &copy_button, &delete_button);
}

fn append_host_private_key_row(
    state: &StoreRecipientsPageState,
    key: &HostGpgPrivateKeySummary,
    selected_available_keys: usize,
    selected_usable_keys: usize,
) {
    let active = state
        .recipients
        .borrow()
        .iter()
        .any(|recipient| recipient_matches_parts(recipient, &key.fingerprint, &key.user_ids));
    let toggle_blocked_message = private_key_toggle_block_message(
        active,
        true,
        matches!(
            state.private_key_requirement.get(),
            StoreRecipientsPrivateKeyRequirement::AllManagedKeys
        ),
        selected_available_keys,
        selected_usable_keys,
    );
    let (row, toggle) = append_private_key_row_shell(
        &key.title(),
        &key.fingerprint,
        active,
        toggle_blocked_message,
    );

    let copy_button = flat_icon_button_with_tooltip("edit-copy-symbolic", "Copy fingerprint");
    row.add_suffix(&copy_button);
    state.list.append(&row);

    let state_for_toggle = state.clone();
    let fingerprint_for_toggle = key.fingerprint.clone();
    let user_ids_for_toggle = key.user_ids.clone();
    toggle.connect_toggled(move |button| {
        if set_private_key_recipient_enabled(
            &state_for_toggle,
            &fingerprint_for_toggle,
            &user_ids_for_toggle,
            button.is_active(),
        ) {
            super::rebuild_store_recipients_list(&state_for_toggle);
            queue_store_recipients_autosave(&state_for_toggle);
        }
    });

    let overlay = state.platform.overlay.clone();
    let fingerprint_for_copy = key.fingerprint.clone();
    let copy_button_for_click = copy_button.clone();
    copy_button.connect_clicked(move |_| {
        let _ = set_clipboard_text(
            &fingerprint_for_copy,
            &overlay,
            Some(&copy_button_for_click),
        );
    });
}

fn append_private_key_status_suffixes(
    state: &StoreRecipientsPageState,
    key: &ManagedRipassoPrivateKey,
    row: &ActionRow,
    unlocked: bool,
    requires_unlock: bool,
) {
    if !Preferences::new().uses_integrated_backend() {
        return;
    }

    if !unlocked && requires_unlock {
        let unlock_button = flat_icon_button_with_tooltip("changes-prevent-symbolic", "Unlock key");
        row.add_suffix(&unlock_button);
        let state = state.clone();
        let fingerprint = key.fingerprint.clone();
        let finish_button = unlock_button.clone();
        unlock_button.connect_clicked(move |_| {
            finish_button.set_sensitive(false);
            let after_unlock: Rc<dyn Fn()> = Rc::new({
                let state = state.clone();
                move || {
                    super::rebuild_store_recipients_list(&state);
                    activate_widget_action(&state.window, "win.reload-password-list");
                }
            });
            let on_finish: Rc<dyn Fn(bool)> = Rc::new({
                let finish_button = finish_button.clone();
                move |success| {
                    if !success {
                        finish_button.set_sensitive(true);
                    }
                }
            });
            prompt_private_key_unlock_for_action(
                &state.platform.overlay,
                fingerprint.clone(),
                after_unlock,
                on_finish,
            );
        });
    }
}

fn connect_managed_private_key_row_actions(
    state: &StoreRecipientsPageState,
    key: &ManagedRipassoPrivateKey,
    toggle: &adw::gtk::CheckButton,
    copy_button: &adw::gtk::Button,
    delete_button: &adw::gtk::Button,
) {
    let state_for_toggle = state.clone();
    let fingerprint_for_toggle = key.fingerprint.clone();
    let user_ids_for_toggle = key.user_ids.clone();
    toggle.connect_toggled(move |button| {
        if set_private_key_recipient_enabled(
            &state_for_toggle,
            &fingerprint_for_toggle,
            &user_ids_for_toggle,
            button.is_active(),
        ) {
            super::rebuild_store_recipients_list(&state_for_toggle);
            queue_store_recipients_autosave(&state_for_toggle);
        }
    });

    let state_for_copy = state.clone();
    let key_for_copy = key.clone();
    let copy_button_for_click = copy_button.clone();
    copy_button.connect_clicked(move |_| {
        copy_managed_key_material(&state_for_copy, &key_for_copy, Some(&copy_button_for_click));
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
                .add_toast(Toast::new(&gettext("Couldn't remove that key.")));
            return;
        }

        let _ = sync_private_keys_to_host_if_enabled(&state_for_delete);
        super::rebuild_store_recipients_list(&state_for_delete);
        activate_widget_action(&state_for_delete.window, "win.reload-password-list");
        state_for_delete
            .platform
            .overlay
            .add_toast(Toast::new(&gettext("Key file removed.")));
    });
}

#[cfg(test)]
mod tests {
    use super::{
        effective_private_key_verification_warning, fido2_recipient_remove_block_message,
        merge_available_private_keys, private_key_delete_block_message,
        private_key_toggle_block_message, private_key_verification_warning,
        selected_available_private_key_count, show_all_fido2_keys_required_info,
        show_require_all_private_keys_option, show_store_options_title_above_git_row,
        unresolved_private_key_recipients, AvailablePrivateKey, HostGpgPrivateKeySummary,
        PrivateKeyVerificationWarning,
    };
    use crate::backend::{ManagedRipassoPrivateKey, ManagedRipassoPrivateKeyProtection};
    use crate::fido2_recipient::{build_fido2_recipient_string, derived_fido2_recipient_id};
    use crate::store::recipients_page::mode::StoreRecipientsSelectionMode;

    fn test_fido2_recipient(label: &str, credential_id: &[u8]) -> String {
        build_fido2_recipient_string(
            &derived_fido2_recipient_id(credential_id),
            label,
            credential_id,
        )
        .expect("build recipient")
    }

    #[test]
    fn merged_private_keys_prefer_managed_duplicates() {
        let managed = ManagedRipassoPrivateKey {
            fingerprint: "AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA".to_string(),
            user_ids: vec!["Managed User <managed@example.com>".to_string()],
            protection: ManagedRipassoPrivateKeyProtection::Password,
            hardware: None,
        };
        let merged = merge_available_private_keys(
            vec![managed.clone()],
            vec![
                HostGpgPrivateKeySummary {
                    fingerprint: managed.fingerprint.clone(),
                    user_ids: vec!["Host Duplicate <host@example.com>".to_string()],
                },
                HostGpgPrivateKeySummary {
                    fingerprint: "BBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBB".to_string(),
                    user_ids: vec!["Host Only <host-only@example.com>".to_string()],
                },
            ],
        );

        assert_eq!(merged.len(), 2);
        assert!(merged.iter().any(|key| matches!(
            key,
            AvailablePrivateKey::Managed(found) if found == &managed
        )));
        assert!(merged.iter().any(|key| matches!(
            key,
            AvailablePrivateKey::HostOnly(found)
                if found.fingerprint == "BBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBB"
        )));
    }

    #[test]
    fn unresolved_recipients_consider_host_only_keys() {
        let unresolved = unresolved_private_key_recipients(
            &[
                "Host User <host@example.com>".to_string(),
                "missing@example.com".to_string(),
            ],
            &[AvailablePrivateKey::HostOnly(HostGpgPrivateKeySummary {
                fingerprint: "AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA".to_string(),
                user_ids: vec!["Host User <host@example.com>".to_string()],
            })],
        );

        assert_eq!(unresolved, vec!["missing@example.com".to_string()]);
    }

    #[test]
    fn private_key_verification_warning_matches_backend_sync_and_inspection_state() {
        #[cfg(target_os = "linux")]
        {
            assert_eq!(
                private_key_verification_warning(true, false, true),
                Some(PrivateKeyVerificationWarning::HostInspectionFailed)
            );
            assert_eq!(
                private_key_verification_warning(false, false, false),
                Some(PrivateKeyVerificationWarning::SyncDisabled)
            );
            assert_eq!(private_key_verification_warning(true, false, false), None);
            assert_eq!(private_key_verification_warning(false, true, false), None);
        }

        #[cfg(not(target_os = "linux"))]
        {
            assert_eq!(private_key_verification_warning(true, false, true), None);
            assert_eq!(private_key_verification_warning(false, false, false), None);
            assert_eq!(private_key_verification_warning(true, false, false), None);
            assert_eq!(private_key_verification_warning(false, true, false), None);
        }
    }

    #[test]
    fn fido_only_stores_hide_private_key_verification_warnings() {
        assert_eq!(
            effective_private_key_verification_warning(
                StoreRecipientsSelectionMode::Fido2Only,
                Some(PrivateKeyVerificationWarning::SyncDisabled),
            ),
            None
        );
        assert_eq!(
            effective_private_key_verification_warning(
                StoreRecipientsSelectionMode::StandardOnly,
                Some(PrivateKeyVerificationWarning::SyncDisabled),
            ),
            Some(PrivateKeyVerificationWarning::SyncDisabled)
        );
    }

    #[test]
    fn fido_only_stores_show_info_instead_of_require_all_toggle() {
        assert!(!show_require_all_private_keys_option(
            StoreRecipientsSelectionMode::Fido2Only,
            true
        ));
        assert!(show_require_all_private_keys_option(
            StoreRecipientsSelectionMode::StandardOnly,
            true
        ));
        assert!(show_require_all_private_keys_option(
            StoreRecipientsSelectionMode::Mixed,
            true
        ));
        assert!(!show_all_fido2_keys_required_info(
            StoreRecipientsSelectionMode::Fido2Only,
            1
        ));
        assert!(show_all_fido2_keys_required_info(
            StoreRecipientsSelectionMode::Fido2Only,
            2
        ));
        assert!(!show_all_fido2_keys_required_info(
            StoreRecipientsSelectionMode::StandardOnly,
            2
        ));
    }

    #[test]
    fn git_row_shows_store_options_title_when_it_is_the_only_option() {
        assert!(show_store_options_title_above_git_row(false, true));
        assert!(!show_store_options_title_above_git_row(true, true));
        assert!(!show_store_options_title_above_git_row(false, false));
    }

    #[test]
    fn selected_available_private_key_count_only_tracks_matching_keys() {
        let keys = vec![
            AvailablePrivateKey::Managed(ManagedRipassoPrivateKey {
                fingerprint: "AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA".to_string(),
                user_ids: vec!["Alice <alice@example.com>".to_string()],
                protection: ManagedRipassoPrivateKeyProtection::Password,
                hardware: None,
            }),
            AvailablePrivateKey::HostOnly(HostGpgPrivateKeySummary {
                fingerprint: "BBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBB".to_string(),
                user_ids: vec!["Bob <bob@example.com>".to_string()],
            }),
        ];

        assert_eq!(
            selected_available_private_key_count(
                &[
                    "AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA".to_string(),
                    "Bob <bob@example.com>".to_string(),
                    "missing@example.com".to_string(),
                ],
                &keys,
                true,
            ),
            2
        );
    }

    #[test]
    fn selected_available_private_key_count_ignores_fido2_when_backend_cannot_use_it() {
        let recipient = test_fido2_recipient("Desk Key", b"cred");
        assert_eq!(
            selected_available_private_key_count(
                &[
                    "AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA".to_string(),
                    recipient,
                ],
                &[AvailablePrivateKey::Managed(ManagedRipassoPrivateKey {
                    fingerprint: "AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA".to_string(),
                    user_ids: vec!["Alice <alice@example.com>".to_string()],
                    protection: ManagedRipassoPrivateKeyProtection::Password,
                    hardware: None,
                })],
                false,
            ),
            1
        );
    }

    #[cfg(feature = "fidokey")]
    #[test]
    fn selected_available_private_key_count_does_not_double_count_managed_fido2_keys() {
        let recipient = "BBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBB".to_string();

        assert_eq!(
            selected_available_private_key_count(
                std::slice::from_ref(&recipient),
                &[AvailablePrivateKey::Managed(ManagedRipassoPrivateKey {
                    fingerprint: recipient.clone(),
                    user_ids: vec!["Desk Key".to_string()],
                    protection: ManagedRipassoPrivateKeyProtection::Fido2HmacSecret,
                    hardware: None,
                })],
                true,
            ),
            1
        );
    }

    #[test]
    fn delete_rules_require_another_selected_key_and_disabled_require_all() {
        assert_eq!(
            private_key_delete_block_message(true, true, 2),
            Some("This selected key is required while all selected private keys are required.")
        );
        assert_eq!(
            private_key_delete_block_message(true, false, 1),
            Some("Keep another selected private key available before removing this key.")
        );
        assert_eq!(private_key_delete_block_message(true, false, 2), None);
        assert_eq!(private_key_delete_block_message(false, false, 0), None);
    }

    #[test]
    fn locked_checked_keys_only_block_unchecking_when_they_are_required() {
        assert_eq!(
            private_key_toggle_block_message(true, true, true, 2, 2),
            Some("Keep this key selected while all selected private keys are required.")
        );
        assert_eq!(
            private_key_toggle_block_message(true, true, false, 1, 1),
            Some("Keep at least one selected private key available.")
        );
        assert_eq!(
            private_key_toggle_block_message(true, true, false, 2, 1),
            Some("Unlock another selected private key before clearing this one.")
        );
        assert_eq!(
            private_key_toggle_block_message(true, true, false, 2, 2),
            None
        );
        assert_eq!(
            private_key_toggle_block_message(true, false, false, 1, 0),
            Some("Keep at least one selected private key available.")
        );
        assert_eq!(
            private_key_toggle_block_message(true, false, false, 2, 0),
            None
        );
    }

    #[test]
    fn host_backend_still_allows_removing_selected_fido2_recipients() {
        assert_eq!(
            fido2_recipient_remove_block_message(false, false, 0, 0),
            None
        );
    }
}
