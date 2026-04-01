use super::StoreRecipientsPageState;
use crate::backend::{
    armored_ripasso_private_key, armored_ripasso_public_key, ManagedRipassoPrivateKey,
    ManagedRipassoPrivateKeyProtection,
};
use crate::clipboard::set_clipboard_text;
use crate::i18n::gettext;
use crate::logging::log_error;
use adw::gtk::Button;
use adw::{Toast, ToastOverlay};

fn copy_key_material_to_clipboard(
    overlay: &ToastOverlay,
    key: &ManagedRipassoPrivateKey,
    button: Option<&Button>,
) -> Result<(), String> {
    let armored = match key.protection {
        ManagedRipassoPrivateKeyProtection::Password => {
            armored_ripasso_private_key(&key.fingerprint)?
        }
        ManagedRipassoPrivateKeyProtection::HardwareOpenPgpCard => {
            armored_ripasso_public_key(&key.fingerprint)?
        }
        #[cfg(feature = "fidokey")]
        ManagedRipassoPrivateKeyProtection::Fido2HmacSecret => {
            armored_ripasso_private_key(&key.fingerprint)?
        }
    };
    set_clipboard_text(&armored, overlay, button);
    Ok(())
}

pub(super) fn copy_managed_key_material(
    state: &StoreRecipientsPageState,
    key: &ManagedRipassoPrivateKey,
    button: Option<&Button>,
) {
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
