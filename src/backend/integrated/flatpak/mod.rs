mod crypto;
mod entries;
mod paths;
mod recipients;
mod store;
#[cfg(test)]
mod tests;

pub(crate) use self::entries::{
    delete_password_entry, read_password_entry, read_password_line, rename_password_entry,
    save_password_entry,
};
pub(crate) use self::recipients::preferred_ripasso_private_key_fingerprint_for_entry;
pub(crate) use self::store::save_store_recipients;

pub use super::keys::{
    import_ripasso_private_key_bytes, is_ripasso_private_key_unlocked, list_ripasso_private_keys,
    ManagedRipassoPrivateKey, remove_ripasso_private_key, ripasso_private_key_requires_passphrase,
    ripasso_private_key_requires_session_unlock, ripasso_private_key_title,
    unlock_ripasso_private_key_for_session,
};
