use adw::glib::object::IsA;
use adw::gtk::{
    Align, Box as GtkBox, Button, Image, ListBox, ListBoxRow, Orientation, PolicyType,
    ScrolledWindow, Spinner,
};
use adw::prelude::*;
use adw::{ActionRow, Clamp, HeaderBar, NavigationPage, NavigationView, WindowTitle};
use std::rc::Rc;

pub fn clear_list_box(list: &ListBox) {
    while let Some(child) = list.first_child() {
        list.remove(&child);
    }
}

pub fn connect_row_action(row: &ActionRow, action: impl Fn() + 'static) {
    let action = Rc::new(action);
    row.connect_activated(move |_| action());
}

pub fn append_info_row(list: &ListBox, title: &str, subtitle: &str) {
    let row = ActionRow::builder().title(title).subtitle(subtitle).build();
    row.set_activatable(false);
    list.append(&row);
}

pub fn append_spinner_row(list: &ListBox) {
    let spinner = Spinner::builder().spinning(true).build();
    let container = GtkBox::builder()
        .orientation(Orientation::Vertical)
        .halign(Align::Center)
        .margin_top(12)
        .margin_bottom(12)
        .margin_start(12)
        .margin_end(12)
        .build();
    container.append(&spinner);

    let row = ListBoxRow::new();
    row.set_activatable(false);
    row.set_selectable(false);
    row.set_child(Some(&container));
    list.append(&row);
}

pub fn wrapped_dialog_body(child: &impl IsA<adw::gtk::Widget>) -> ScrolledWindow {
    let clamp = Clamp::new();
    clamp.set_maximum_size(560);
    clamp.set_tightening_threshold(400);
    clamp.set_size_request(420, -1);
    clamp.set_child(Some(child));

    ScrolledWindow::builder()
        .hscrollbar_policy(PolicyType::Never)
        .vscrollbar_policy(PolicyType::Automatic)
        .propagate_natural_width(true)
        .propagate_natural_height(true)
        .child(&clamp)
        .build()
}

pub fn dialog_content_shell(
    title: &str,
    subtitle: Option<&str>,
    child: &impl IsA<adw::gtk::Widget>,
) -> GtkBox {
    let window_title = WindowTitle::builder().title(title).build();
    if let Some(subtitle) = subtitle.filter(|subtitle| !subtitle.trim().is_empty()) {
        window_title.set_subtitle(subtitle);
    }

    let header = HeaderBar::new();
    header.set_title_widget(Some(&window_title));

    let shell = GtkBox::new(Orientation::Vertical, 0);
    shell.append(&header);
    shell.append(&wrapped_dialog_body(child));
    shell
}

pub fn flat_icon_button(icon_name: &str) -> Button {
    let button = Button::from_icon_name(icon_name);
    button.add_css_class("flat");
    button
}

pub fn flat_icon_button_with_tooltip(icon_name: &str, tooltip: &str) -> Button {
    let button = flat_icon_button(icon_name);
    button.set_tooltip_text(Some(tooltip));
    button
}

pub fn dim_label_icon(icon_name: &str) -> Image {
    let icon = Image::from_icon_name(icon_name);
    icon.add_css_class("dim-label");
    icon
}

pub fn append_action_row_with_button(
    list: &ListBox,
    title: &str,
    subtitle: &str,
    icon_name: &str,
    action: impl Fn() + 'static,
) -> ActionRow {
    let row = ActionRow::builder().title(title).subtitle(subtitle).build();
    row.set_activatable(true);

    let icon = Image::from_icon_name(icon_name);
    row.add_suffix(&icon);
    list.append(&row);

    let action = Rc::new(action);
    let row_action = action.clone();
    row.connect_activated(move |_| row_action());

    row
}

pub fn navigation_stack_contains_page(nav: &NavigationView, page: &NavigationPage) -> bool {
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

pub fn visible_navigation_page_is(nav: &NavigationView, page: &NavigationPage) -> bool {
    nav.visible_page()
        .as_ref()
        .is_some_and(|visible| visible == page)
}

pub fn push_navigation_page_if_needed(nav: &NavigationView, page: &NavigationPage) -> bool {
    if visible_navigation_page_is(nav, page) {
        return false;
    }

    nav.push(page);
    true
}

pub fn reveal_navigation_page(nav: &NavigationView, page: &NavigationPage) -> bool {
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

pub fn navigation_stack_is_root(nav: &NavigationView) -> bool {
    nav.navigation_stack().n_items() <= 1
}

pub fn pop_navigation_to_root(nav: &NavigationView) {
    while !navigation_stack_is_root(nav) {
        nav.pop();
    }
}
