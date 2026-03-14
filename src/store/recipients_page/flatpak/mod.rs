mod export;
mod generate;
mod import;
mod list;

use super::StoreRecipientsPageState;

#[derive(Clone)]
pub(crate) struct StoreRecipientsPlatformState {
    pub(crate) overlay: adw::ToastOverlay,
}

pub(crate) fn connect_store_recipients_entry(_state: &StoreRecipientsPageState) {}

pub(crate) fn prepare_store_recipients_page(_state: &StoreRecipientsPageState) {}

pub(crate) fn rebuild_store_recipients_list(state: &StoreRecipientsPageState) {
    self::list::rebuild_store_recipients_list(state);
}
