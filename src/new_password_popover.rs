use crate::preferences::Preferences;
use adw::gio::SimpleAction;
use adw::prelude::*;
use adw::{ApplicationWindow, ComboRow, EntryRow};
use adw::gtk::{Popover, StringList};

#[derive(Clone)]
pub(crate) struct NewPasswordPopoverState {
    pub(crate) popover: Popover,
    pub(crate) path_entry: EntryRow,
    pub(crate) store_row: ComboRow,
}

fn available_store_roots() -> Vec<String> {
    Preferences::new().stores()
}

pub(crate) fn sync_new_password_store_row(row: &ComboRow) {
    let stores = available_store_roots();
    let labels = stores.iter().map(String::as_str).collect::<Vec<_>>();
    row.set_model(Some(&StringList::new(&labels)));
    row.set_visible(stores.len() > 1);
    if !stores.is_empty() {
        row.set_selected(row.selected().min(stores.len() as u32 - 1));
    }
}

pub(crate) fn selected_new_password_store(row: &ComboRow) -> Option<String> {
    let stores = available_store_roots();
    if stores.len() <= 1 {
        return stores.into_iter().next();
    }

    stores
        .get(row.selected() as usize)
        .cloned()
        .or_else(|| stores.into_iter().next())
}

pub(crate) fn register_open_new_password_action(
    window: &ApplicationWindow,
    state: &NewPasswordPopoverState,
) {
    let state = state.clone();
    let action = SimpleAction::new("open-new-password", None);
    action.connect_activate(move |_, _| {
        if state.popover.is_visible() {
            state.popover.popdown();
        } else {
            sync_new_password_store_row(&state.store_row);
            state.popover.popup();
            state.path_entry.grab_focus();
        }
    });
    window.add_action(&action);
}
