use adw::prelude::*;
use adw::{ActionRow, NavigationPage, NavigationView};
use adw::gtk::{Button, ListBox};
use std::rc::Rc;

pub(crate) fn clear_list_box(list: &ListBox) {
    while let Some(child) = list.first_child() {
        list.remove(&child);
    }
}

pub(crate) fn connect_row_and_button_action(
    row: &ActionRow,
    button: &Button,
    action: impl Fn() + 'static,
) {
    let action = Rc::new(action);

    let row_action = action.clone();
    row.connect_activated(move |_| row_action());

    let button_action = action.clone();
    button.connect_clicked(move |_| button_action());
}

pub(crate) fn navigation_stack_contains_page(
    nav: &NavigationView,
    page: &NavigationPage,
) -> bool {
    let stack = nav.navigation_stack();
    let mut index = 0;
    let len = stack.n_items();
    while index < len {
        if let Some(item) = stack.item(index) {
            if let Ok(stack_page) = item.downcast::<NavigationPage>() {
                if stack_page == *page {
                    return true;
                }
            }
        }
        index += 1;
    }

    false
}
