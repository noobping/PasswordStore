use crate::preferences::Preferences;
use crate::store::labels::shortened_store_labels;
use crate::support::actions::register_window_action;
use adw::gtk::{Box as GtkBox, StringList, INVALID_LIST_POSITION};
use adw::prelude::*;
use adw::{
    ApplicationWindow, ComboRow, Dialog, EntryRow, HeaderBar, PreferencesGroup, PreferencesPage,
    WindowTitle,
};
use std::cell::RefCell;
use std::rc::Rc;

#[derive(Clone)]
pub struct NewPasswordPopoverState {
    pub dialog: Dialog,
    pub path_entry: EntryRow,
    pub store_dropdown: ComboRow,
    pub store_roots: Rc<RefCell<Vec<String>>>,
}

fn dialog_content_shell(title: &str, subtitle: &str, child: &impl IsA<adw::gtk::Widget>) -> GtkBox {
    let window_title = WindowTitle::builder().title(title).build();
    if !subtitle.trim().is_empty() {
        window_title.set_subtitle(subtitle);
    }

    let header = HeaderBar::new();
    header.set_title_widget(Some(&window_title));

    let shell = GtkBox::new(adw::gtk::Orientation::Vertical, 0);
    shell.append(&header);
    shell.append(child);
    shell
}

pub(crate) fn build_new_password_dialog() -> (Dialog, ComboRow, EntryRow) {
    let store_dropdown = ComboRow::new();
    store_dropdown.set_title("Store");
    store_dropdown.set_visible(false);

    let path_entry = EntryRow::new();
    path_entry.set_title("Path or name");
    path_entry.set_show_apply_button(true);

    let group = PreferencesGroup::new();
    group.add(&store_dropdown);
    group.add(&path_entry);

    let page = PreferencesPage::new();
    page.add(&group);

    let dialog = Dialog::builder()
        .title("New item")
        .content_width(460)
        .child(&dialog_content_shell(
            "New item",
            "Create a new pass file.",
            &page,
        ))
        .build();

    (dialog, store_dropdown, path_entry)
}

fn available_store_roots() -> Vec<String> {
    Preferences::new().store_roots()
}

fn resolve_selected_store(stores: &[String], selected: Option<&str>) -> Option<String> {
    selected
        .and_then(|selected| stores.iter().find(|store| store.as_str() == selected))
        .cloned()
        .or_else(|| stores.first().cloned())
}

fn selected_store_position(stores: &[String], selected: Option<&str>) -> u32 {
    resolve_selected_store(stores, selected)
        .and_then(|selected| stores.iter().position(|store| store == &selected))
        .and_then(|index| u32::try_from(index).ok())
        .unwrap_or(INVALID_LIST_POSITION)
}

pub fn sync_new_password_store_selector(state: &NewPasswordPopoverState) {
    let stores = available_store_roots();
    let labels = shortened_store_labels(&stores);
    let selected = selected_new_password_store(state);
    state.store_roots.borrow_mut().clone_from(&stores);
    state.store_dropdown.set_visible(stores.len() > 1);

    let label_refs = labels.iter().map(String::as_str).collect::<Vec<_>>();
    state
        .store_dropdown
        .set_model(Some(&StringList::new(&label_refs)));
    state
        .store_dropdown
        .set_selected(selected_store_position(&stores, selected.as_deref()));
}

pub fn selected_new_password_store(state: &NewPasswordPopoverState) -> Option<String> {
    let stores = state.store_roots.borrow();
    stores
        .get(state.store_dropdown.selected() as usize)
        .cloned()
        .or_else(|| stores.first().cloned())
}

pub fn register_open_new_password_action(
    window: &ApplicationWindow,
    state: &NewPasswordPopoverState,
) {
    let window_for_action = window.clone();
    let window_for_dialog = window.clone();
    let state = state.clone();
    register_window_action(&window_for_action, "open-new-password", move || {
        sync_new_password_store_selector(&state);
        state.path_entry.set_text("");
        state.dialog.present(Some(&window_for_dialog));
        state.path_entry.grab_focus();
    });
}

#[cfg(test)]
mod tests {
    use super::{resolve_selected_store, selected_store_position};
    use adw::gtk::INVALID_LIST_POSITION;

    #[test]
    fn selected_store_uses_current_dropdown_index() {
        let stores = vec![
            "/home/nick/.password-store".to_string(),
            "/home/nick/work/.password-store".to_string(),
        ];

        assert_eq!(
            resolve_selected_store(&stores, Some("/home/nick/work/.password-store")),
            Some("/home/nick/work/.password-store".to_string())
        );
    }

    #[test]
    fn selected_store_position_falls_back_to_the_first_store() {
        let stores = vec![
            "/home/nick/.password-store".to_string(),
            "/home/nick/work/.password-store".to_string(),
        ];

        assert_eq!(selected_store_position(&stores, None), 0);
        assert_eq!(selected_store_position(&stores, Some("/missing/store")), 0);
        assert_eq!(selected_store_position(&[], None), INVALID_LIST_POSITION);
    }
}
