use super::StoreRecipientsPageState;
use crate::i18n::gettext;
use crate::logging::log_error;
use crate::preferences::Preferences;
use crate::private_key::sync::{sync_private_keys_with_host, PrivateKeySyncDirection};
use crate::support::actions::activate_widget_action;
use adw::Toast;

fn handle_private_key_sync_failure(state: &StoreRecipientsPageState, err: &str) {
    log_error(format!("Failed to sync private keys with the host: {err}"));
    if let Err(save_err) = Preferences::new().set_sync_private_keys_with_host(false) {
        log_error(format!(
            "Failed to turn off private-key sync after an error: {}",
            save_err.message
        ));
    }
    state.platform.overlay.add_toast(Toast::new(&gettext(
        "Couldn't keep private keys synced. Sync was turned off.",
    )));
}

fn sync_private_keys_if_enabled(
    state: &StoreRecipientsPageState,
    direction: PrivateKeySyncDirection,
) -> bool {
    if !Preferences::new().sync_private_keys_with_host() {
        return true;
    }

    match sync_private_keys_with_host(direction) {
        Ok(()) => {
            activate_widget_action(&state.window, "win.reload-password-list");
            true
        }
        Err(err) => {
            handle_private_key_sync_failure(state, &err);
            false
        }
    }
}

pub(super) fn sync_private_keys_from_host_if_enabled(state: &StoreRecipientsPageState) -> bool {
    sync_private_keys_if_enabled(state, PrivateKeySyncDirection::HostToApp)
}

pub(super) fn sync_private_keys_to_host_if_enabled(state: &StoreRecipientsPageState) -> bool {
    sync_private_keys_if_enabled(state, PrivateKeySyncDirection::AppToHost)
}
