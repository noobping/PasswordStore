use super::StoreRecipientsPageState;
use crate::backend::{
    armored_ripasso_private_key, armored_ripasso_public_key, is_ripasso_private_key_unlocked,
    ManagedRipassoPrivateKey, ManagedRipassoPrivateKeyProtection,
};
use crate::clipboard::set_clipboard_text;
use crate::i18n::gettext;
use crate::logging::log_error;
use crate::private_key::unlock::prompt_private_key_unlock_for_action;
use adw::gtk::Button;
use adw::{Toast, ToastOverlay};
use std::rc::Rc;

fn copy_key_material_to_clipboard(
    overlay: &ToastOverlay,
    key: &ManagedRipassoPrivateKey,
    button: Option<&Button>,
) -> Result<(), String> {
    let armored = if key.uses_hardware() {
        armored_ripasso_public_key(&key.fingerprint)?
    } else {
        armored_ripasso_private_key(&key.fingerprint)?
    };
    set_clipboard_text(&armored, overlay, button);
    Ok(())
}

pub(super) fn copy_managed_key_material(
    state: &StoreRecipientsPageState,
    key: &ManagedRipassoPrivateKey,
    button: Option<&Button>,
) {
    if matches!(
        key.protection,
        ManagedRipassoPrivateKeyProtection::Fido2HmacSecret
    ) && matches!(is_ripasso_private_key_unlocked(&key.fingerprint), Ok(false))
    {
        let state_for_unlock = state.clone();
        let key_for_unlock = key.clone();
        let button = button.cloned();
        let after_unlock: Rc<dyn Fn()> = Rc::new(move || {
            copy_managed_key_material(&state_for_unlock, &key_for_unlock, button.as_ref());
        });
        let on_finish: Rc<dyn Fn(bool)> = Rc::new(|_| {});
        prompt_private_key_unlock_for_action(
            &state.platform.overlay,
            key.fingerprint.clone(),
            after_unlock,
            on_finish,
        );
        return;
    }

    if let Err(err) = copy_key_material_to_clipboard(&state.platform.overlay, key, button) {
        log_error(format!(
            "Failed to copy key material '{}': {err}",
            key.fingerprint
        ));
        state
            .platform
            .overlay
            .add_toast(Toast::new(&gettext("Couldn't copy that key.")));
    }
}
