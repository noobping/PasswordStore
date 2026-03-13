use super::build::widgets::WindowWidgets;
use adw::prelude::*;
use adw::EntryRow;

#[derive(Clone)]
pub(crate) struct StandardWindowState {
    pub(crate) store_recipients_entry: EntryRow,
}

pub(crate) fn configure_standard_window(_widgets: &WindowWidgets) -> StandardWindowState {
    let store_recipients_entry = EntryRow::new();
    store_recipients_entry.set_title("Add recipient");
    store_recipients_entry.set_show_apply_button(true);

    StandardWindowState {
        store_recipients_entry,
    }
}
