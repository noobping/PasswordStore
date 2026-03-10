use super::{queue_store_recipients_autosave, StoreRecipientsPageState, StoreRecipientsRequest};
use crate::store::recipients::append_gpg_recipients;
use crate::support::ui::{append_info_row, clear_list_box, dim_label_icon, flat_icon_button};
use adw::prelude::*;
use adw::{ActionRow, EntryRow};

#[derive(Clone)]
pub(crate) struct StoreRecipientsPlatformState {
    pub(crate) entry: EntryRow,
}

fn empty_recipients_subtitle(request: Option<&StoreRecipientsRequest>) -> &'static str {
    request.map_or("Add at least one recipient before saving.", |request| {
        request.mode.empty_state_subtitle()
    })
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
        append_info_row(
            &state.list,
            "No recipients yet",
            empty_recipients_subtitle(state.current_request().as_ref()),
        );
        return;
    }

    for recipient in state.recipients.borrow().iter().cloned() {
        let row = ActionRow::builder().title(&recipient).build();
        row.set_activatable(false);
        let row_icon = dim_label_icon("dialog-password-symbolic");
        row.add_prefix(&row_icon);

        let delete_button = flat_icon_button("user-trash-symbolic");
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

#[cfg(test)]
mod tests {
    use super::empty_recipients_subtitle;
    use crate::store::recipients_page::StoreRecipientsRequest;

    #[test]
    fn empty_recipients_subtitle_uses_the_generic_message_without_a_request() {
        assert_eq!(
            empty_recipients_subtitle(None),
            "Add at least one recipient before saving."
        );
    }

    #[test]
    fn empty_recipients_subtitle_matches_the_store_mode() {
        assert_eq!(
            empty_recipients_subtitle(Some(&StoreRecipientsRequest::create("/tmp/store"))),
            "Add at least one recipient to create this store."
        );
        assert_eq!(
            empty_recipients_subtitle(Some(&StoreRecipientsRequest::edit("/tmp/store"))),
            "Add at least one recipient to keep saving changes."
        );
    }
}
