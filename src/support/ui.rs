use adw::gtk::{Button, Image, ListBox, Popover};
use adw::prelude::*;
use adw::{ActionRow, NavigationPage, NavigationView};
use std::rc::Rc;

pub(crate) fn clear_list_box(list: &ListBox) {
    while let Some(child) = list.first_child() {
        list.remove(&child);
    }
}

pub(crate) fn toggle_popover(popover: &Popover) {
    if popover.is_visible() {
        popover.popdown();
    } else {
        popover.popup();
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

pub(crate) fn append_info_row(list: &ListBox, title: &str, subtitle: &str) {
    let row = ActionRow::builder().title(title).subtitle(subtitle).build();
    row.set_activatable(false);
    list.append(&row);
}

pub(crate) fn flat_icon_button(icon_name: &str) -> Button {
    let button = Button::from_icon_name(icon_name);
    button.add_css_class("flat");
    button
}

pub(crate) fn flat_icon_button_with_tooltip(icon_name: &str, tooltip: &str) -> Button {
    let button = flat_icon_button(icon_name);
    button.set_tooltip_text(Some(tooltip));
    button
}

pub(crate) fn dim_label_icon(icon_name: &str) -> Image {
    let icon = Image::from_icon_name(icon_name);
    icon.add_css_class("dim-label");
    icon
}

pub(crate) fn append_action_row_with_button(
    list: &ListBox,
    title: &str,
    subtitle: &str,
    button_icon_name: &str,
    action: impl Fn() + 'static,
) {
    let row = ActionRow::builder().title(title).subtitle(subtitle).build();
    row.set_activatable(true);

    let button = flat_icon_button(button_icon_name);
    row.add_suffix(&button);
    list.append(&row);
    connect_row_and_button_action(&row, &button, action);
}

pub(crate) fn navigation_stack_contains_page(nav: &NavigationView, page: &NavigationPage) -> bool {
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
    nav.visible_page()
        .as_ref()
        .is_some_and(|visible| visible == page)
}

pub(crate) fn push_navigation_page_if_needed(nav: &NavigationView, page: &NavigationPage) -> bool {
    if visible_navigation_page_is(nav, page) {
        return false;
    }

    nav.push(page);
    true
}

pub(crate) fn reveal_navigation_page(nav: &NavigationView, page: &NavigationPage) -> bool {
    if visible_navigation_page_is(nav, page) {
        return false;
    }

    if navigation_stack_contains_page(nav, page) {
        let _ = nav.pop_to_page(page);
    } else {
        nav.push(page);
    }

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
