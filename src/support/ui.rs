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

pub(crate) fn visible_navigation_page_is(nav: &NavigationView, page: &NavigationPage) -> bool {
    nav.visible_page().as_ref().is_some_and(|visible| visible == page)
}

pub(crate) fn push_navigation_page_if_needed(
    nav: &NavigationView,
    page: &NavigationPage,
) -> bool {
    if visible_navigation_page_is(nav, page) {
        return false;
    }

    nav.push(page);
    true
}

pub(crate) fn navigation_stack_is_root(nav: &NavigationView) -> bool {
    nav.navigation_stack().n_items() <= 1
}

pub(crate) fn pop_navigation_to_root(nav: &NavigationView) {
    while !navigation_stack_is_root(nav) {
        nav.pop();
    }
}
