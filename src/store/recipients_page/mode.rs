use super::StoreRecipientsPageState;
use crate::fido2_recipient::is_fido2_recipient_string;
use crate::i18n::gettext;
use crate::support::runtime::{
    supports_fidokey_features, supports_fidostore_features, supports_smartcard_features,
};
use crate::window::host_access::{
    append_optional_fido2_access_row, append_optional_smartcard_access_row,
};
use adw::prelude::*;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum StoreRecipientsSelectionMode {
    Empty,
    StandardOnly,
    Fido2Only,
    Mixed,
}

impl StoreRecipientsSelectionMode {
    const fn allows_standard_recipients(self) -> bool {
        matches!(self, Self::Empty | Self::StandardOnly)
    }

    const fn allows_fido2_recipients(self) -> bool {
        matches!(self, Self::Empty | Self::Fido2Only)
    }

    const fn shows_standard_recipient_choice(self, active: bool) -> bool {
        matches!(self, Self::Empty | Self::StandardOnly) || active
    }

    const fn standard_action_block_message(self) -> Option<&'static str> {
        match self {
            Self::Empty | Self::StandardOnly => None,
            Self::Fido2Only => {
                Some("This store already uses a FIDO2 security key. Remove it before adding normal keys.")
            }
            Self::Mixed => Some(
                "This store already mixes FIDO2 and normal keys. Remove one type before adding more.",
            ),
        }
    }

    const fn fido2_action_block_message(self) -> Option<&'static str> {
        match self {
            Self::Empty | Self::Fido2Only => None,
            Self::StandardOnly => {
                Some("This store already uses normal keys. Remove them before adding a FIDO2 security key.")
            }
            Self::Mixed => Some(
                "This store already mixes FIDO2 and normal keys. Remove one type before adding more.",
            ),
        }
    }
}

pub(super) fn store_recipients_selection_mode(
    recipients: &[String],
) -> StoreRecipientsSelectionMode {
    let has_fido2 = recipients
        .iter()
        .any(|recipient| is_fido2_recipient_string(recipient));
    let has_standard = recipients
        .iter()
        .any(|recipient| !is_fido2_recipient_string(recipient));

    match (has_standard, has_fido2) {
        (false, false) => StoreRecipientsSelectionMode::Empty,
        (true, false) => StoreRecipientsSelectionMode::StandardOnly,
        (false, true) => StoreRecipientsSelectionMode::Fido2Only,
        (true, true) => StoreRecipientsSelectionMode::Mixed,
    }
}

pub(super) fn current_selection_mode(
    state: &StoreRecipientsPageState,
) -> StoreRecipientsSelectionMode {
    store_recipients_selection_mode(&state.recipients.borrow())
}

pub(super) fn sync_store_recipients_mode_controls(
    state: &StoreRecipientsPageState,
    selection_mode: StoreRecipientsSelectionMode,
    uses_integrated_backend: bool,
) {
    let show_standard_rows = selection_mode.allows_standard_recipients();
    let show_fido2_rows = selection_mode.allows_fido2_recipients();
    let smartcard_supported = supports_smartcard_features();
    let fidostore_supported = supports_fidostore_features();
    let fidokey_supported = supports_fidokey_features();
    let show_generic_import_rows = show_standard_rows;

    state
        .platform
        .generate_key_row
        .set_visible(show_standard_rows);
    state
        .platform
        .generate_fido2_key_row
        .set_visible(show_standard_rows && fidokey_supported);
    state
        .platform
        .import_clipboard_row
        .set_visible(show_generic_import_rows);
    state
        .platform
        .import_file_row
        .set_visible(show_generic_import_rows);
    state
        .platform
        .add_hardware_key_row
        .set_visible(show_standard_rows && smartcard_supported);
    state
        .platform
        .import_hardware_key_row
        .set_visible(show_standard_rows && smartcard_supported);
    state
        .platform
        .add_fido2_key_row
        .set_visible(show_fido2_rows && fidostore_supported);

    append_optional_smartcard_access_row(
        &state.platform.add_list,
        &state.platform.overlay,
        &[
            &state.platform.add_hardware_key_row,
            &state.platform.import_hardware_key_row,
        ],
        show_standard_rows && smartcard_supported,
    );
    append_optional_fido2_access_row(
        &state.platform.create_list,
        &state.platform.overlay,
        &[
            &state.platform.generate_fido2_key_row,
            &state.platform.add_fido2_key_row,
        ],
        uses_integrated_backend
            && (state.platform.generate_fido2_key_row.is_visible()
                || state.platform.add_fido2_key_row.is_visible()),
    );

    state.platform.create_group.set_visible(
        state.platform.generate_key_row.is_visible()
            || state.platform.generate_fido2_key_row.is_visible()
            || state.platform.add_fido2_key_row.is_visible(),
    );
    state.platform.add_group.set_visible(
        state.platform.add_hardware_key_row.is_visible()
            || state.platform.import_hardware_key_row.is_visible()
            || state.platform.import_clipboard_row.is_visible()
            || state.platform.import_file_row.is_visible(),
    );
}

pub(super) fn show_standard_private_key_choice(
    selection_mode: StoreRecipientsSelectionMode,
    active: bool,
) -> bool {
    selection_mode.shows_standard_recipient_choice(active)
}

fn toast_blocked_action(state: &StoreRecipientsPageState, message: Option<&'static str>) -> bool {
    let Some(message) = message else {
        return true;
    };

    state
        .platform
        .overlay
        .add_toast(adw::Toast::new(&gettext(message)));
    false
}

pub(super) fn ensure_standard_recipient_actions_allowed(state: &StoreRecipientsPageState) -> bool {
    let selection_mode = current_selection_mode(state);
    toast_blocked_action(state, selection_mode.standard_action_block_message())
}

pub(super) fn ensure_fido2_recipient_actions_allowed(state: &StoreRecipientsPageState) -> bool {
    let selection_mode = current_selection_mode(state);
    toast_blocked_action(state, selection_mode.fido2_action_block_message())
}

#[cfg(test)]
mod tests {
    use super::{
        show_standard_private_key_choice, store_recipients_selection_mode,
        StoreRecipientsSelectionMode,
    };

    #[test]
    fn recipients_selection_mode_distinguishes_standard_fido2_and_mixed_stores() {
        assert_eq!(
            store_recipients_selection_mode(&[]),
            StoreRecipientsSelectionMode::Empty
        );
        assert_eq!(
            store_recipients_selection_mode(&["alice@example.com".to_string()]),
            StoreRecipientsSelectionMode::StandardOnly
        );
        assert_eq!(
            store_recipients_selection_mode(&[
                "keycord-fido2-recipient-v1=0123456789abcdef0123456789abcdef01234567:4465736b204b6579:63726564"
                    .to_string(),
            ]),
            StoreRecipientsSelectionMode::Fido2Only
        );
        assert_eq!(
            store_recipients_selection_mode(&[
                "alice@example.com".to_string(),
                "keycord-fido2-recipient-v1=0123456789abcdef0123456789abcdef01234567:4465736b204b6579:63726564"
                    .to_string(),
            ]),
            StoreRecipientsSelectionMode::Mixed
        );
    }

    #[test]
    fn fido2_or_mixed_stores_hide_inactive_standard_key_choices() {
        assert!(!show_standard_private_key_choice(
            StoreRecipientsSelectionMode::Fido2Only,
            false
        ));
        assert!(!show_standard_private_key_choice(
            StoreRecipientsSelectionMode::Mixed,
            false
        ));
        assert!(show_standard_private_key_choice(
            StoreRecipientsSelectionMode::Mixed,
            true
        ));
        assert!(show_standard_private_key_choice(
            StoreRecipientsSelectionMode::StandardOnly,
            false
        ));
    }

    #[test]
    fn block_messages_match_the_selection_mode() {
        assert_eq!(
            StoreRecipientsSelectionMode::Fido2Only.standard_action_block_message(),
            Some("This store already uses a FIDO2 security key. Remove it before adding normal keys.")
        );
        assert_eq!(
            StoreRecipientsSelectionMode::StandardOnly.fido2_action_block_message(),
            Some("This store already uses normal keys. Remove them before adding a FIDO2 security key.")
        );
        assert_eq!(
            StoreRecipientsSelectionMode::Mixed.standard_action_block_message(),
            Some("This store already mixes FIDO2 and normal keys. Remove one type before adding more.")
        );
        assert_eq!(
            StoreRecipientsSelectionMode::Mixed.fido2_action_block_message(),
            Some("This store already mixes FIDO2 and normal keys. Remove one type before adding more.")
        );
        assert_eq!(
            StoreRecipientsSelectionMode::Empty.standard_action_block_message(),
            None
        );
        assert_eq!(
            StoreRecipientsSelectionMode::Empty.fido2_action_block_message(),
            None
        );
    }
}
