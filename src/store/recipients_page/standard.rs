use super::{queue_store_recipients_autosave, StoreRecipientsMode, StoreRecipientsPageState};
use crate::support::ui::clear_list_box;
use crate::store::recipients::append_gpg_recipients;
use adw::gtk::{Button, Image};
use adw::prelude::*;
use adw::{ActionRow, EntryRow};

#[derive(Clone)]
pub(crate) struct StoreRecipientsPlatformState {
    pub(crate) entry: EntryRow,
}

pub(crate) fn connect_store_recipients_entry(state: &StoreRecipientsPageState) {
    let page_state = state.clone();
    state.platform.entry.connect_apply(move |entry| {
        if append_gpg_recipients(&page_state.recipients, entry.text().as_str()) {
            entry.set_text("");
            rebuild_store_recipients_list(&page_state);
            queue_store_recipients_autosave(&page_state);
        }
    });
}

pub(crate) fn rebuild_store_recipients_list(state: &StoreRecipientsPageState) {
    clear_list_box(&state.list);
    state.list.append(&state.platform.entry);

    if state.recipients.borrow().is_empty() {
        let empty_row = ActionRow::builder()
            .title("No recipients yet")
            .subtitle(empty_state_subtitle(state.current_request().as_ref()))
            .build();
        empty_row.set_activatable(false);
        state.list.append(&empty_row);
        return;
    }

    for recipient in state.recipients.borrow().iter().cloned() {
        let row = ActionRow::builder().title(&recipient).build();
        row.set_activatable(false);
        let row_icon = Image::from_icon_name("dialog-password-symbolic");
        row_icon.add_css_class("dim-label");
        row.add_prefix(&row_icon);

        let delete_button = Button::from_icon_name("user-trash-symbolic");
        delete_button.add_css_class("flat");
        row.add_suffix(&delete_button);
        state.list.append(&row);

        let page_state = state.clone();
        delete_button.connect_clicked(move |_| {
            page_state
                .recipients
                .borrow_mut()
                .retain(|value| value != &recipient);
            rebuild_store_recipients_list(&page_state);
            queue_store_recipients_autosave(&page_state);
        });
    }
}

pub(crate) fn prepare_store_recipients_page(state: &StoreRecipientsPageState) {
    state.platform.entry.set_text("");
}

fn empty_state_subtitle(request: Option<&super::StoreRecipientsRequest>) -> &'static str {
    match request.map(|request| &request.mode) {
        Some(StoreRecipientsMode::Create) => "Add at least one recipient to create this store.",
        Some(StoreRecipientsMode::Edit) => "Add at least one recipient to keep saving changes.",
        None => "Add at least one recipient before saving.",
    }
}
