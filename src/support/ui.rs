use crate::i18n::gettext;
use crate::logging::log_error;
use crate::preferences::Preferences;
use adw::glib::{object::IsA, Propagation};
use adw::gtk::{
    gdk, Align, Box as GtkBox, Button, CheckButton, DirectionType, EventControllerKey, Image,
    ListBox, ListBoxRow, Orientation, PolicyType, PropagationPhase, ScrolledWindow, SearchEntry,
    SpinButton, SpinType, Spinner, TextView, Widget,
};
use adw::prelude::*;
use adw::{
    ActionRow, Clamp, EntryRow, HeaderBar, NavigationPage, NavigationView, PasswordEntryRow,
    PreferencesGroup, WindowTitle,
};
use std::cell::RefCell;
use std::rc::Rc;

const TOUCH_FRIENDLY_SEARCH_ENTRY_HEIGHT: i32 = 44;

fn apply_button_visible_for_text(text: &str) -> bool {
    !text.trim().is_empty()
}

pub fn sync_entry_row_apply_button(row: &EntryRow) {
    row.set_show_apply_button(apply_button_visible_for_text(&row.text()));
}

pub fn connect_entry_row_apply_button_to_nonempty_text(row: &EntryRow) {
    sync_entry_row_apply_button(row);
    row.connect_changed(sync_entry_row_apply_button);
}

pub fn sync_password_entry_row_apply_button(row: &PasswordEntryRow) {
    row.set_show_apply_button(apply_button_visible_for_text(&row.text()));
}

pub fn connect_password_entry_row_apply_button_to_nonempty_text(row: &PasswordEntryRow) {
    sync_password_entry_row_apply_button(row);
    row.connect_changed(sync_password_entry_row_apply_button);
}

pub fn clear_list_box(list: &ListBox) {
    while let Some(child) = list.first_child() {
        list.remove(&child);
    }
}

pub fn add_tracked_preferences_group_child(
    group: &PreferencesGroup,
    tracked: &RefCell<Vec<Widget>>,
    child: &(impl IsA<Widget> + Clone),
) {
    group.add(child);
    tracked.borrow_mut().push(child.clone().upcast());
}

pub fn clear_tracked_preferences_group(group: &PreferencesGroup, tracked: &RefCell<Vec<Widget>>) {
    for child in tracked.borrow_mut().drain(..) {
        group.remove(&child);
    }
}

pub fn connect_row_action(row: &ActionRow, action: impl Fn() + 'static) {
    let action = Rc::new(action);
    row.connect_activated(move |_| action());
}

pub fn connect_vertical_arrow_navigation_for_buttons(scope: &(impl IsA<Widget> + Clone + 'static)) {
    let scope = scope.clone().upcast::<Widget>();
    let scope_for_keys = scope.clone();
    let controller = EventControllerKey::new();
    controller.set_propagation_phase(PropagationPhase::Capture);
    controller.connect_key_pressed(move |_, key, _, _| {
        let direction = match key {
            gdk::Key::Up | gdk::Key::KP_Up => DirectionType::Up,
            gdk::Key::Down | gdk::Key::KP_Down => DirectionType::Down,
            _ => return Propagation::Proceed,
        };

        let Some(root) = scope_for_keys.root() else {
            return Propagation::Proceed;
        };
        let Some(focus) = adw::gtk::prelude::RootExt::focus(&root) else {
            return Propagation::Proceed;
        };
        if (!focus.is::<Button>() && !focus.is::<CheckButton>())
            || !focus.is_ancestor(&scope_for_keys)
        {
            return Propagation::Proceed;
        }
        if focus.ancestor(ListBoxRow::static_type()).is_some() {
            return Propagation::Proceed;
        }

        if scope_for_keys.child_focus(direction) {
            Propagation::Stop
        } else {
            Propagation::Proceed
        }
    });
    scope.add_controller(controller);
}

pub fn connect_horizontal_arrow_adjustment_for_spin_buttons(
    scope: &(impl IsA<Widget> + Clone + 'static),
) {
    let scope = scope.clone().upcast::<Widget>();
    let scope_for_keys = scope.clone();
    let controller = EventControllerKey::new();
    controller.set_propagation_phase(PropagationPhase::Capture);
    controller.connect_key_pressed(move |_, key, _, _| {
        let direction = match key {
            gdk::Key::Left | gdk::Key::KP_Left => SpinType::StepBackward,
            gdk::Key::Right | gdk::Key::KP_Right => SpinType::StepForward,
            _ => return Propagation::Proceed,
        };

        let Some(spin) = focused_spin_button_in_scope(&scope_for_keys) else {
            return Propagation::Proceed;
        };

        spin.spin(direction, 1.0);
        Propagation::Stop
    });
    scope.add_controller(controller);
}

pub fn configure_touch_friendly_search_entry(entry: &SearchEntry) {
    entry.set_hexpand(true);
    entry.set_height_request(TOUCH_FRIENDLY_SEARCH_ENTRY_HEIGHT);
}

pub fn connect_search_list_arrow_navigation(
    list: &ListBox,
    search_entry: &SearchEntry,
    row_is_focusable: impl Fn(&ListBoxRow) -> bool + 'static,
) {
    let row_is_focusable = Rc::new(row_is_focusable);

    let search_controller = EventControllerKey::new();
    search_controller.set_propagation_phase(PropagationPhase::Capture);
    let list_for_search = list.clone();
    let row_is_focusable_for_search = row_is_focusable.clone();
    search_controller.connect_key_pressed(move |_, key, _, _| {
        if matches!(key, gdk::Key::Down | gdk::Key::KP_Down)
            && focus_first_matching_list_row(&list_for_search, |row| {
                row_is_focusable_for_search(row)
            })
        {
            return Propagation::Stop;
        }

        Propagation::Proceed
    });
    search_entry.add_controller(search_controller);

    let list_controller = EventControllerKey::new();
    list_controller.set_propagation_phase(PropagationPhase::Capture);
    let list_for_keys = list.clone();
    let search_entry_for_list = search_entry.clone();
    let row_is_focusable_for_list = row_is_focusable.clone();
    list_controller.connect_key_pressed(move |_, key, _, _| {
        if !search_entry_for_list.is_visible() {
            return Propagation::Proceed;
        }

        if matches!(key, gdk::Key::Up | gdk::Key::KP_Up)
            && focused_row_is_first_matching_list_row(&list_for_keys, &*row_is_focusable_for_list)
        {
            search_entry_for_list.grab_focus();
            return Propagation::Stop;
        }

        Propagation::Proceed
    });
    list.add_controller(list_controller);
}

pub fn connect_keyboard_focusable_search_list_arrow_navigation(
    list: &ListBox,
    search_entry: &SearchEntry,
) {
    connect_search_list_arrow_navigation(list, search_entry, list_row_is_keyboard_focusable);
}

pub fn connect_ordered_search_list_arrow_navigation(
    lists: &[ListBox],
    search_entry: &SearchEntry,
    row_is_focusable: impl Fn(&ListBoxRow) -> bool + 'static,
) {
    let lists = Rc::new(lists.to_vec());
    let row_is_focusable = Rc::new(row_is_focusable);

    let search_controller = EventControllerKey::new();
    search_controller.set_propagation_phase(PropagationPhase::Capture);
    let lists_for_search = lists.clone();
    let row_is_focusable_for_search = row_is_focusable.clone();
    search_controller.connect_key_pressed(move |_, key, _, _| {
        if matches!(key, gdk::Key::Down | gdk::Key::KP_Down)
            && focus_first_matching_list_row_in_lists(
                &lists_for_search,
                &*row_is_focusable_for_search,
            )
        {
            return Propagation::Stop;
        }

        Propagation::Proceed
    });
    search_entry.add_controller(search_controller);

    for (index, list) in lists.iter().enumerate() {
        let controller = EventControllerKey::new();
        controller.set_propagation_phase(PropagationPhase::Capture);
        let lists_for_keys = lists.clone();
        let search_entry_for_keys = search_entry.clone();
        let row_is_focusable_for_keys = row_is_focusable.clone();
        controller.connect_key_pressed(move |_, key, _, _| {
            if !search_entry_for_keys.is_visible() || !matches!(key, gdk::Key::Up | gdk::Key::KP_Up)
            {
                return Propagation::Proceed;
            }

            if focused_row_is_first_matching_list_row(
                &lists_for_keys[index],
                &*row_is_focusable_for_keys,
            ) && !lists_have_matching_row(&lists_for_keys[..index], &*row_is_focusable_for_keys)
            {
                search_entry_for_keys.grab_focus();
                return Propagation::Stop;
            }

            Propagation::Proceed
        });
        list.add_controller(controller);
    }
}

pub fn connect_ordered_keyboard_focusable_search_list_arrow_navigation(
    lists: &[ListBox],
    search_entry: &SearchEntry,
) {
    connect_ordered_search_list_arrow_navigation(
        lists,
        search_entry,
        list_row_is_keyboard_focusable,
    );
}

pub fn connect_ordered_list_arrow_navigation(
    lists: &[ListBox],
    header_focus_target: Option<&Widget>,
    row_is_focusable: impl Fn(&ListBoxRow) -> bool + 'static,
) {
    let lists = Rc::new(lists.to_vec());
    let header_focus_target = header_focus_target.cloned();
    let row_is_focusable = Rc::new(row_is_focusable);

    for (index, list) in lists.iter().enumerate() {
        let controller = EventControllerKey::new();
        controller.set_propagation_phase(PropagationPhase::Capture);
        let lists_for_keys = lists.clone();
        let header_focus_target_for_keys = header_focus_target.clone();
        let row_is_focusable_for_keys = row_is_focusable.clone();
        controller.connect_key_pressed(move |_, key, _, _| {
            let direction = match key {
                gdk::Key::Up | gdk::Key::KP_Up => DirectionType::Up,
                gdk::Key::Down | gdk::Key::KP_Down => DirectionType::Down,
                _ => return Propagation::Proceed,
            };

            let Some(current_row) = focused_list_row(&lists_for_keys[index]) else {
                return Propagation::Proceed;
            };

            let moved = match direction {
                DirectionType::Up => {
                    focus_previous_matching_list_row(
                        &lists_for_keys[index],
                        &current_row,
                        &*row_is_focusable_for_keys,
                    ) || focus_last_matching_list_row_in_lists(
                        &lists_for_keys[..index],
                        &*row_is_focusable_for_keys,
                    ) || header_focus_target_for_keys
                        .as_ref()
                        .is_some_and(Widget::grab_focus)
                }
                DirectionType::Down => {
                    focus_next_matching_list_row(
                        &lists_for_keys[index],
                        &current_row,
                        &*row_is_focusable_for_keys,
                    ) || focus_first_matching_list_row_in_lists(
                        &lists_for_keys[index + 1..],
                        &*row_is_focusable_for_keys,
                    )
                }
                _ => false,
            };

            if moved {
                Propagation::Stop
            } else {
                Propagation::Proceed
            }
        });
        list.add_controller(controller);
    }
}

pub fn focus_first_keyboard_focusable_list_row(list: &ListBox) -> bool {
    focus_first_matching_list_row(list, list_row_is_keyboard_focusable)
}

pub fn list_row_is_keyboard_focusable(row: &ListBoxRow) -> bool {
    row.is_child_visible()
        && row.is_sensitive()
        && (row.is_activatable() || widget_or_descendant_is_focusable(row.upcast_ref()))
}

pub fn focus_first_matching_list_row_in_order(
    lists: &[ListBox],
    row_is_focusable: impl Fn(&ListBoxRow) -> bool,
) -> bool {
    focus_first_matching_list_row_in_lists(lists, &row_is_focusable)
}

pub fn focus_last_matching_list_row_in_order(
    lists: &[ListBox],
    row_is_focusable: impl Fn(&ListBoxRow) -> bool,
) -> bool {
    focus_last_matching_list_row_in_lists(lists, &row_is_focusable)
}

pub fn focus_first_visible_widget(widgets: &[Widget]) -> bool {
    widgets.iter().any(focus_widget)
}

pub fn focus_last_visible_widget(widgets: &[Widget]) -> bool {
    widgets.iter().rev().any(focus_widget)
}

pub fn focus_first_matching_list_row(
    list: &ListBox,
    row_is_focusable: impl Fn(&ListBoxRow) -> bool,
) -> bool {
    focus_matching_list_row(list, &row_is_focusable, false)
}

pub fn focus_last_matching_list_row(
    list: &ListBox,
    row_is_focusable: impl Fn(&ListBoxRow) -> bool,
) -> bool {
    focus_matching_list_row(list, &row_is_focusable, true)
}

fn focused_row_is_first_matching_list_row(
    list: &ListBox,
    row_is_focusable: &dyn Fn(&ListBoxRow) -> bool,
) -> bool {
    let Some(focused_row) = focused_list_row(list) else {
        return false;
    };
    let Some(first_row) = first_matching_list_row(list, row_is_focusable) else {
        return false;
    };

    focused_row.index() == first_row.index()
}

pub fn focused_row_is_last_matching_list_row(
    list: &ListBox,
    row_is_focusable: impl Fn(&ListBoxRow) -> bool,
) -> bool {
    list_focus_is_last_matching_list_row(list, &row_is_focusable)
}

fn list_focus_is_last_matching_list_row(
    list: &ListBox,
    row_is_focusable: &dyn Fn(&ListBoxRow) -> bool,
) -> bool {
    let Some(focused_row) = focused_list_row(list) else {
        return false;
    };
    let Some(last_row) = last_matching_list_row(list, row_is_focusable) else {
        return false;
    };

    focused_row.index() == last_row.index()
}

fn focused_list_row(list: &ListBox) -> Option<ListBoxRow> {
    let root = list.root()?;
    let focus = adw::gtk::prelude::RootExt::focus(&root)?;
    let row = focus
        .ancestor(ListBoxRow::static_type())
        .and_then(|widget| widget.downcast::<ListBoxRow>().ok())?;
    if row.is_ancestor(list) {
        return Some(row);
    }

    list.selected_row().filter(|row| row.is_ancestor(list))
}

fn focused_spin_button_in_scope(scope: &Widget) -> Option<SpinButton> {
    let root = scope.root()?;
    let focus = adw::gtk::prelude::RootExt::focus(&root)?;
    let spin = if let Ok(spin) = focus.clone().downcast::<SpinButton>() {
        spin
    } else {
        focus
            .ancestor(SpinButton::static_type())
            .and_then(|widget| widget.downcast::<SpinButton>().ok())?
    };
    spin.is_ancestor(scope).then_some(spin)
}

fn focus_matching_list_row(
    list: &ListBox,
    row_is_focusable: &dyn Fn(&ListBoxRow) -> bool,
    reverse: bool,
) -> bool {
    let mut rows = Vec::new();
    let mut index = 0;
    loop {
        let Some(row) = list.row_at_index(index) else {
            break;
        };
        if row_is_focusable(&row) {
            rows.push(row);
        }
        index += 1;
    }

    if reverse {
        rows.reverse();
    }

    rows.into_iter().any(|row| focus_list_row(list, &row))
}

fn focus_list_row(list: &ListBox, row: &ListBoxRow) -> bool {
    list.select_row(Some(row));
    let list_focused = list.grab_focus();
    if focus_widget(row.upcast_ref()) {
        return true;
    }

    list_focused
        && list
            .selected_row()
            .as_ref()
            .is_some_and(|selected_row| selected_row.index() == row.index())
}

fn focus_first_matching_list_row_in_lists(
    lists: &[ListBox],
    row_is_focusable: &dyn Fn(&ListBoxRow) -> bool,
) -> bool {
    lists
        .iter()
        .any(|list| focus_first_matching_list_row(list, |row| row_is_focusable(row)))
}

fn lists_have_matching_row(
    lists: &[ListBox],
    row_is_focusable: &dyn Fn(&ListBoxRow) -> bool,
) -> bool {
    lists
        .iter()
        .any(|list| first_matching_list_row(list, row_is_focusable).is_some())
}

fn focus_last_matching_list_row_in_lists(
    lists: &[ListBox],
    row_is_focusable: &dyn Fn(&ListBoxRow) -> bool,
) -> bool {
    lists
        .iter()
        .rev()
        .any(|list| focus_last_matching_list_row(list, |row| row_is_focusable(row)))
}

fn focus_previous_matching_list_row(
    list: &ListBox,
    current_row: &ListBoxRow,
    row_is_focusable: &dyn Fn(&ListBoxRow) -> bool,
) -> bool {
    focus_adjacent_matching_list_row(list, current_row, row_is_focusable, DirectionType::Up)
}

fn focus_next_matching_list_row(
    list: &ListBox,
    current_row: &ListBoxRow,
    row_is_focusable: &dyn Fn(&ListBoxRow) -> bool,
) -> bool {
    focus_adjacent_matching_list_row(list, current_row, row_is_focusable, DirectionType::Down)
}

fn focus_adjacent_matching_list_row(
    list: &ListBox,
    current_row: &ListBoxRow,
    row_is_focusable: &dyn Fn(&ListBoxRow) -> bool,
    direction: DirectionType,
) -> bool {
    let mut index = current_row.index();
    loop {
        index = match direction {
            DirectionType::Up => index - 1,
            DirectionType::Down => index + 1,
            _ => return false,
        };

        let Some(row) = list.row_at_index(index) else {
            return false;
        };
        if row_is_focusable(&row) {
            return focus_list_row(list, &row);
        }
    }
}

pub fn widget_contains_focus(widget: &Widget) -> bool {
    let Some(root) = widget.root() else {
        return false;
    };
    let Some(focus) = adw::gtk::prelude::RootExt::focus(&root) else {
        return false;
    };
    focus == *widget || focus.is_ancestor(widget)
}

fn focus_widget(widget: &Widget) -> bool {
    if !widget.is_visible() || !widget.is_sensitive() {
        return false;
    }
    if let Ok(row) = widget.clone().downcast::<ListBoxRow>() {
        if row.is_activatable() {
            row.set_focusable(true);
        }
    }
    if widget.grab_focus() {
        return true;
    }
    if widget.child_focus(DirectionType::TabForward) || widget.child_focus(DirectionType::Down) {
        return true;
    }

    let mut child = widget.first_child();
    while let Some(current_child) = child {
        if focus_widget(&current_child) {
            return true;
        }
        child = current_child.next_sibling();
    }

    false
}

fn widget_or_descendant_is_focusable(widget: &Widget) -> bool {
    if !widget.is_visible() || !widget.is_sensitive() {
        return false;
    }
    if widget.is_focusable() || widget.can_focus() {
        return true;
    }

    let mut child = widget.first_child();
    while let Some(current_child) = child {
        if widget_or_descendant_is_focusable(&current_child) {
            return true;
        }
        child = current_child.next_sibling();
    }

    false
}

pub fn text_view_cursor_is_on_first_line(text_view: &TextView) -> bool {
    let buffer = text_view.buffer();
    let mut iter = buffer.iter_at_offset(buffer.cursor_position());
    !iter.backward_line()
}

pub fn text_view_cursor_is_on_last_line(text_view: &TextView) -> bool {
    let buffer = text_view.buffer();
    let mut iter = buffer.iter_at_offset(buffer.cursor_position());
    !iter.forward_line()
}

fn first_matching_list_row(
    list: &ListBox,
    row_is_focusable: &dyn Fn(&ListBoxRow) -> bool,
) -> Option<ListBoxRow> {
    let mut index = 0;
    loop {
        let row = list.row_at_index(index)?;
        if row_is_focusable(&row) {
            return Some(row);
        }
        index += 1;
    }
}

fn last_matching_list_row(
    list: &ListBox,
    row_is_focusable: &dyn Fn(&ListBoxRow) -> bool,
) -> Option<ListBoxRow> {
    let mut index = 0;
    let mut last_row = None;
    loop {
        let Some(row) = list.row_at_index(index) else {
            return last_row;
        };
        if row_is_focusable(&row) {
            last_row = Some(row);
        }
        index += 1;
    }
}

pub fn append_info_row(list: &ListBox, title: &str, subtitle: &str) {
    let title = gettext(title);
    let subtitle = gettext(subtitle);
    let row = ActionRow::builder()
        .title(&title)
        .subtitle(&subtitle)
        .build();
    row.set_activatable(false);
    list.append(&row);
}

pub fn append_info_group_row(group: &PreferencesGroup, title: &str, subtitle: &str) -> ActionRow {
    let title = gettext(title);
    let subtitle = gettext(subtitle);
    let row = ActionRow::builder()
        .title(&title)
        .subtitle(&subtitle)
        .build();
    row.set_activatable(false);
    group.add(&row);
    row
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
    clamp.set_maximum_size(800);
    clamp.set_tightening_threshold(500);
    clamp.set_size_request(360, -1);
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
    let translated_title = gettext(title);
    let window_title = WindowTitle::builder().title(&translated_title).build();
    if let Some(subtitle) = subtitle.filter(|subtitle| !subtitle.trim().is_empty()) {
        window_title.set_subtitle(&gettext(subtitle));
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

#[cfg(target_os = "windows")]
pub fn flat_resource_icon_button(resource_path: &str) -> Button {
    let button = Button::new();
    button.add_css_class("flat");
    button.add_css_class("image-button");
    button.set_child(Some(&Image::from_resource(resource_path)));
    button
}

pub fn flat_icon_button_with_tooltip(icon_name: &str, tooltip: &str) -> Button {
    let button = flat_icon_button(icon_name);
    let tooltip = gettext(tooltip);
    button.set_tooltip_text(Some(&tooltip));
    button
}

#[cfg(target_os = "windows")]
pub fn flat_resource_icon_button_with_tooltip(resource_path: &str, tooltip: &str) -> Button {
    let button = flat_resource_icon_button(resource_path);
    let tooltip = gettext(tooltip);
    button.set_tooltip_text(Some(&tooltip));
    button
}

pub fn add_persistent_hide_button(row: &ActionRow, notice_id: &str, on_hide: impl Fn() + 'static) {
    let button = flat_icon_button_with_tooltip("window-close-symbolic", "Hide permanently");
    row.add_suffix(&button);

    let row = row.clone();
    let notice_id = notice_id.to_string();
    let on_hide = Rc::new(on_hide);
    button.connect_clicked(move |_| {
        if let Err(err) = Preferences::new().hide_notice(&notice_id) {
            log_error(format!("Failed to hide notice '{notice_id}': {err}"));
        }
        row.set_visible(false);
        on_hide();
    });
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
    let title = gettext(title);
    let subtitle = gettext(subtitle);
    let row = ActionRow::builder()
        .title(&title)
        .subtitle(&subtitle)
        .build();
    row.set_activatable(true);

    let icon = Image::from_icon_name(icon_name);
    row.add_suffix(&icon);
    list.append(&row);

    let action = Rc::new(action);
    let row_action = action.clone();
    row.connect_activated(move |_| row_action());

    row
}

pub fn append_action_group_row_with_button(
    group: &PreferencesGroup,
    title: &str,
    subtitle: &str,
    icon_name: &str,
    action: impl Fn() + 'static,
) -> ActionRow {
    let title = gettext(title);
    let subtitle = gettext(subtitle);
    let row = ActionRow::builder()
        .title(&title)
        .subtitle(&subtitle)
        .build();
    row.set_activatable(true);

    let icon = Image::from_icon_name(icon_name);
    row.add_suffix(&icon);
    group.add(&row);

    let action = Rc::new(action);
    let row_action = action.clone();
    row.connect_activated(move |_| row_action());

    row
}

pub fn focus_first_preferences_group_child_in_order(groups: &[PreferencesGroup]) -> bool {
    groups.iter().any(focus_first_preferences_group_child)
}

fn focus_first_preferences_group_child(group: &PreferencesGroup) -> bool {
    let mut child = group.first_child();
    while let Some(widget) = child {
        if focus_widget(&widget) {
            return true;
        }
        child = widget.next_sibling();
    }
    false
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

#[cfg(test)]
mod tests {
    use super::apply_button_visible_for_text;

    #[test]
    fn apply_button_visibility_requires_nonempty_trimmed_text() {
        assert!(!apply_button_visible_for_text(""));
        assert!(!apply_button_visible_for_text("   "));
        assert!(apply_button_visible_for_text("value"));
        assert!(apply_button_visible_for_text("  value  "));
    }
}
