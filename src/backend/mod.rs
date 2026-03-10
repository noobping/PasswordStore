mod integrated;
#[cfg(not(feature = "flatpak"))]
mod host;

#[cfg(feature = "flatpak")]
pub(crate) use integrated::{
    import_ripasso_private_key_bytes, is_ripasso_private_key_unlocked, list_ripasso_private_keys,
    preferred_ripasso_private_key_fingerprint_for_entry, remove_ripasso_private_key,
    ripasso_private_key_requires_passphrase, ripasso_private_key_requires_session_unlock,
    ripasso_private_key_title, unlock_ripasso_private_key_for_session,
    ManagedRipassoPrivateKey,
};

#[cfg(not(feature = "flatpak"))]
use crate::preferences::Preferences;

#[cfg(not(feature = "flatpak"))]
fn dispatch_backend<T>(
    integrated: impl FnOnce() -> Result<T, String>,
    host: impl FnOnce() -> Result<T, String>,
) -> Result<T, String> {
    if Preferences::new().uses_integrated_backend() {
        integrated()
    } else {
        host()
    }
}

macro_rules! dispatch_backend_call {
    ($(fn $name:ident($($arg:ident: $arg_ty:ty),* $(,)?) -> $ret:ty;)+) => {
        $(
            pub fn $name($($arg: $arg_ty),*) -> $ret {
                #[cfg(feature = "flatpak")]
                {
                    return integrated::$name($($arg),*);
                }

                #[cfg(not(feature = "flatpak"))]
                {
                    dispatch_backend(
                        || integrated::$name($($arg),*),
                        || host::$name($($arg),*),
                    )
                }
            }
        )+
    };
}

dispatch_backend_call! {
    fn read_password_entry(store_root: &str, label: &str) -> Result<String, String>;
    fn read_password_line(store_root: &str, label: &str) -> Result<String, String>;
    fn save_password_entry(
        store_root: &str,
        label: &str,
        contents: &str,
        overwrite: bool,
    ) -> Result<(), String>;
    fn rename_password_entry(
        store_root: &str,
        old_label: &str,
        new_label: &str,
    ) -> Result<(), String>;
    fn delete_password_entry(store_root: &str, label: &str) -> Result<(), String>;
    fn save_store_recipients(store_root: &str, recipients: &[String]) -> Result<(), String>;
}
