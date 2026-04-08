use super::widgets::WindowWidgets;
use crate::preferences::Preferences;
use crate::support::ui::{
    configure_touch_friendly_search_entry, connect_horizontal_arrow_adjustment_for_spin_buttons,
    connect_ordered_keyboard_focusable_search_list_arrow_navigation,
    connect_ordered_list_arrow_navigation, connect_vertical_arrow_navigation_for_buttons,
    focus_first_keyboard_focusable_list_row, focus_first_matching_list_row_in_order,
    focus_first_visible_widget, focus_last_matching_list_row_in_order, focus_last_visible_widget,
    focused_row_is_last_matching_list_row, list_row_is_keyboard_focusable,
    navigation_stack_is_root, text_view_cursor_is_on_first_line, text_view_cursor_is_on_last_line,
    visible_navigation_page_is, widget_contains_focus,
};
use crate::window::navigation::WindowNavigationState;
use adw::glib::{self, Propagation};
use adw::gtk::{gdk, DirectionType, EventControllerKey, ListBox, Widget};
use adw::prelude::*;
use adw::ApplicationWindow;

pub(super) fn initialize_window_chrome(widgets: &WindowWidgets, preferences: &Preferences) {
    configure_search_entries(widgets);
    restore_window_size(&widgets.window, preferences);
    connect_window_size_persistence(&widgets.window);
}

pub(super) fn connect_window_keyboard_navigation(
    widgets: &WindowWidgets,
    navigation: &WindowNavigationState,
) {
    connect_page_keyboard_navigation(widgets);
    connect_page_list_keyboard_navigation(widgets);
    connect_preferences_page_detail_navigation(widgets);
    connect_headerbar_down_navigation(widgets, navigation);
    connect_page_autofocus(widgets, navigation);
}

pub(super) fn schedule_initial_focus(widgets: &WindowWidgets, navigation: &WindowNavigationState) {
    schedule_focus_first_visible_page_target(widgets, navigation);
}

fn connect_page_keyboard_navigation(widgets: &WindowWidgets) {
    for page in [
        widgets.settings_page.clone(),
        widgets.tools_page.clone(),
        widgets.tools_audit_page.clone(),
        widgets.store_import_page.clone(),
        widgets.store_recipients_page.clone(),
        widgets.store_git_page.clone(),
        widgets.private_key_generation_page.clone(),
        widgets.hardware_key_generation_page.clone(),
    ] {
        connect_vertical_arrow_navigation_for_buttons(&page);
    }

    for page in [widgets.password_page.clone(), widgets.settings_page.clone()] {
        connect_horizontal_arrow_adjustment_for_spin_buttons(&page);
    }
}

fn preferences_page_lists(widgets: &WindowWidgets) -> [ListBox; 2] {
    [
        widgets.password_stores.clone(),
        widgets.password_store_actions.clone(),
    ]
}

fn tools_page_lists(widgets: &WindowWidgets) -> [ListBox; 2] {
    [widgets.tools_list.clone(), widgets.tools_logs_list.clone()]
}

fn preferences_page_detail_widgets(widgets: &WindowWidgets) -> Vec<Widget> {
    vec![
        widgets.backend_row.clone().upcast(),
        widgets.pass_command_row.clone().upcast(),
        widgets.sync_private_keys_with_host_check.clone().upcast(),
        widgets
            .audit_use_commit_history_recipients_check
            .clone()
            .upcast(),
        widgets.preferences_username_filename_check.clone().upcast(),
        widgets.preferences_username_folder_check.clone().upcast(),
        widgets
            .preferences_password_list_sort_filename_check
            .clone()
            .upcast(),
        widgets
            .preferences_password_list_sort_store_path_check
            .clone()
            .upcast(),
        widgets.new_pass_file_template_view.clone().upcast(),
        widgets
            .clear_empty_fields_before_save_check
            .clone()
            .upcast(),
        widgets
            .preferences_password_generator_length_spin
            .clone()
            .upcast(),
        widgets
            .preferences_password_generator_min_lowercase_spin
            .clone()
            .upcast(),
        widgets
            .preferences_password_generator_min_uppercase_spin
            .clone()
            .upcast(),
        widgets
            .preferences_password_generator_min_numbers_spin
            .clone()
            .upcast(),
        widgets
            .preferences_password_generator_min_symbols_spin
            .clone()
            .upcast(),
    ]
}

fn focus_first_preferences_page_detail_target(widgets: &WindowWidgets) -> bool {
    focus_first_visible_widget(&preferences_page_detail_widgets(widgets))
}

fn connect_page_list_keyboard_navigation(widgets: &WindowWidgets) {
    let primary_menu_button = widgets.primary_menu_button.clone().upcast();

    connect_ordered_list_arrow_navigation(
        &preferences_page_lists(widgets),
        Some(&primary_menu_button),
        list_row_is_keyboard_focusable,
    );

    connect_ordered_list_arrow_navigation(
        &tools_page_lists(widgets),
        Some(&primary_menu_button),
        list_row_is_keyboard_focusable,
    );
    connect_ordered_keyboard_focusable_search_list_arrow_navigation(
        &tools_page_lists(widgets),
        &widgets.tools_search_entry,
    );
}

fn connect_preferences_page_detail_navigation(widgets: &WindowWidgets) {
    let actions_list = widgets.password_store_actions.clone();
    let widgets_for_down = widgets.clone();
    let down_controller = EventControllerKey::new();
    down_controller.set_propagation_phase(adw::gtk::PropagationPhase::Capture);
    down_controller.connect_key_pressed(move |_, key, _, _| {
        if !matches!(key, gdk::Key::Down | gdk::Key::KP_Down) {
            return Propagation::Proceed;
        }
        if !focused_row_is_last_matching_list_row(&actions_list, list_row_is_keyboard_focusable) {
            return Propagation::Proceed;
        }

        if focus_first_preferences_page_detail_target(&widgets_for_down) {
            Propagation::Stop
        } else {
            Propagation::Proceed
        }
    });
    widgets
        .password_store_actions
        .add_controller(down_controller);

    let widgets_for_details = widgets.clone();
    let details_controller = EventControllerKey::new();
    details_controller.set_propagation_phase(adw::gtk::PropagationPhase::Capture);
    details_controller.connect_key_pressed(move |_, key, _, _| {
        let direction = match key {
            gdk::Key::Up | gdk::Key::KP_Up => DirectionType::Up,
            gdk::Key::Down | gdk::Key::KP_Down => DirectionType::Down,
            _ => return Propagation::Proceed,
        };

        let detail_widgets = preferences_page_detail_widgets(&widgets_for_details);
        let Some(current_index) = detail_widgets.iter().position(widget_contains_focus) else {
            return Propagation::Proceed;
        };

        if widget_contains_focus(
            &widgets_for_details
                .new_pass_file_template_view
                .clone()
                .upcast(),
        ) && ((matches!(direction, DirectionType::Up)
            && !text_view_cursor_is_on_first_line(
                &widgets_for_details.new_pass_file_template_view,
            ))
            || (matches!(direction, DirectionType::Down)
                && !text_view_cursor_is_on_last_line(
                    &widgets_for_details.new_pass_file_template_view,
                )))
        {
            return Propagation::Proceed;
        }

        let moved = match direction {
            DirectionType::Up => {
                if current_index == 0 {
                    focus_last_matching_list_row_in_order(
                        &preferences_page_lists(&widgets_for_details),
                        list_row_is_keyboard_focusable,
                    )
                } else {
                    focus_last_visible_widget(&detail_widgets[..current_index])
                }
            }
            DirectionType::Down => focus_first_visible_widget(&detail_widgets[current_index + 1..]),
            _ => false,
        };

        if moved {
            Propagation::Stop
        } else {
            Propagation::Proceed
        }
    });
    widgets.settings_page.add_controller(details_controller);
}

fn focus_first_visible_page_target(
    widgets: &WindowWidgets,
    navigation: &WindowNavigationState,
) -> bool {
    if navigation_stack_is_root(&navigation.nav) {
        if crate::password::list::focus_first_password_list_row(&widgets.list) {
            return true;
        }
        if widgets.search_entry.is_visible() {
            return widgets.search_entry.grab_focus();
        }
        return false;
    }

    if visible_navigation_page_is(&navigation.nav, &widgets.password_page) {
        if widgets.password_entry.is_visible() {
            return widgets.password_entry.grab_focus();
        }
        return widgets.password_page.child_focus(DirectionType::Down);
    }
    if visible_navigation_page_is(&navigation.nav, &widgets.raw_text_page) {
        return widgets.text_view.grab_focus();
    }
    if visible_navigation_page_is(&navigation.nav, &widgets.settings_page) {
        if widgets.settings_search_entry.is_visible() {
            return widgets.settings_search_entry.grab_focus();
        }
        if focus_first_matching_list_row_in_order(
            &preferences_page_lists(widgets),
            list_row_is_keyboard_focusable,
        ) {
            return true;
        }
        return focus_first_preferences_page_detail_target(widgets)
            || widgets.settings_page.child_focus(DirectionType::Down);
    }
    if visible_navigation_page_is(&navigation.nav, &widgets.tools_page) {
        if widgets.tools_search_entry.is_visible() {
            return widgets.tools_search_entry.grab_focus();
        }
        if focus_first_keyboard_focusable_list_row(&widgets.tools_list) {
            return true;
        }
        return focus_first_keyboard_focusable_list_row(&widgets.tools_logs_list);
    }
    if visible_navigation_page_is(&navigation.nav, &widgets.docs_page) {
        if focus_first_keyboard_focusable_list_row(&widgets.docs_list) {
            return true;
        }
        return widgets.docs_search_entry.grab_focus();
    }
    if visible_navigation_page_is(&navigation.nav, &widgets.docs_detail_page) {
        return widgets.docs_detail_box.child_focus(DirectionType::Down);
    }
    if visible_navigation_page_is(&navigation.nav, &widgets.tools_field_values_page) {
        if focus_first_keyboard_focusable_list_row(&widgets.tools_field_values_list) {
            return true;
        }
        return widgets.tools_field_values_search_entry.grab_focus();
    }
    if visible_navigation_page_is(&navigation.nav, &widgets.tools_value_values_page) {
        if focus_first_keyboard_focusable_list_row(&widgets.tools_value_values_list) {
            return true;
        }
        return widgets.tools_value_values_search_entry.grab_focus();
    }
    if visible_navigation_page_is(&navigation.nav, &widgets.tools_weak_passwords_page) {
        if focus_first_keyboard_focusable_list_row(&widgets.tools_weak_passwords_list) {
            return true;
        }
        return widgets.tools_weak_passwords_search_entry.grab_focus();
    }
    if visible_navigation_page_is(&navigation.nav, &widgets.tools_audit_page) {
        return widgets.tools_audit_page.child_focus(DirectionType::Down);
    }
    if visible_navigation_page_is(&navigation.nav, &widgets.store_import_page) {
        return widgets.store_import_store_dropdown.grab_focus();
    }
    if visible_navigation_page_is(&navigation.nav, &widgets.store_recipients_page) {
        if widgets.store_recipients_search_entry.is_visible() {
            return widgets.store_recipients_search_entry.grab_focus();
        }
        return widgets
            .store_recipients_page
            .child_focus(DirectionType::Down);
    }
    if visible_navigation_page_is(&navigation.nav, &widgets.store_git_page) {
        if widgets.store_git_search_entry.is_visible() {
            return widgets.store_git_search_entry.grab_focus();
        }
        return widgets.store_git_page.child_focus(DirectionType::Down);
    }
    if visible_navigation_page_is(&navigation.nav, &widgets.private_key_generation_page) {
        return widgets.private_key_generation_name_row.grab_focus();
    }
    if visible_navigation_page_is(&navigation.nav, &widgets.hardware_key_generation_page) {
        return widgets.hardware_key_generation_name_row.grab_focus();
    }
    if visible_navigation_page_is(&navigation.nav, &widgets.log_page) {
        return widgets.log_view.grab_focus();
    }

    false
}

fn visible_page_contains_focus(
    widgets: &WindowWidgets,
    navigation: &WindowNavigationState,
) -> bool {
    if navigation_stack_is_root(&navigation.nav) {
        return widget_contains_focus(&widgets.list.clone().upcast())
            || widget_contains_focus(&widgets.search_entry.clone().upcast());
    }

    let visible_page_widget = if visible_navigation_page_is(&navigation.nav, &widgets.password_page)
    {
        Some(widgets.password_page.clone().upcast::<Widget>())
    } else if visible_navigation_page_is(&navigation.nav, &widgets.raw_text_page) {
        Some(widgets.raw_text_page.clone().upcast::<Widget>())
    } else if visible_navigation_page_is(&navigation.nav, &widgets.settings_page) {
        Some(widgets.settings_page.clone().upcast::<Widget>())
    } else if visible_navigation_page_is(&navigation.nav, &widgets.tools_page) {
        Some(widgets.tools_page.clone().upcast::<Widget>())
    } else if visible_navigation_page_is(&navigation.nav, &widgets.docs_page) {
        Some(widgets.docs_page.clone().upcast::<Widget>())
    } else if visible_navigation_page_is(&navigation.nav, &widgets.docs_detail_page) {
        Some(widgets.docs_detail_page.clone().upcast::<Widget>())
    } else if visible_navigation_page_is(&navigation.nav, &widgets.tools_field_values_page) {
        Some(widgets.tools_field_values_page.clone().upcast::<Widget>())
    } else if visible_navigation_page_is(&navigation.nav, &widgets.tools_value_values_page) {
        Some(widgets.tools_value_values_page.clone().upcast::<Widget>())
    } else if visible_navigation_page_is(&navigation.nav, &widgets.tools_weak_passwords_page) {
        Some(widgets.tools_weak_passwords_page.clone().upcast::<Widget>())
    } else if visible_navigation_page_is(&navigation.nav, &widgets.tools_audit_page) {
        Some(widgets.tools_audit_page.clone().upcast::<Widget>())
    } else if visible_navigation_page_is(&navigation.nav, &widgets.store_import_page) {
        Some(widgets.store_import_page.clone().upcast::<Widget>())
    } else if visible_navigation_page_is(&navigation.nav, &widgets.store_recipients_page) {
        Some(widgets.store_recipients_page.clone().upcast::<Widget>())
    } else if visible_navigation_page_is(&navigation.nav, &widgets.store_git_page) {
        Some(widgets.store_git_page.clone().upcast::<Widget>())
    } else if visible_navigation_page_is(&navigation.nav, &widgets.private_key_generation_page) {
        Some(
            widgets
                .private_key_generation_page
                .clone()
                .upcast::<Widget>(),
        )
    } else if visible_navigation_page_is(&navigation.nav, &widgets.hardware_key_generation_page) {
        Some(
            widgets
                .hardware_key_generation_page
                .clone()
                .upcast::<Widget>(),
        )
    } else if visible_navigation_page_is(&navigation.nav, &widgets.log_page) {
        Some(widgets.log_page.clone().upcast::<Widget>())
    } else {
        None
    };

    visible_page_widget
        .as_ref()
        .is_some_and(widget_contains_focus)
}

fn schedule_focus_first_visible_page_target(
    widgets: &WindowWidgets,
    navigation: &WindowNavigationState,
) {
    let widgets = widgets.clone();
    let navigation = navigation.clone();
    glib::idle_add_local_once(move || {
        if visible_page_contains_focus(&widgets, &navigation) {
            return;
        }
        let _ = focus_first_visible_page_target(&widgets, &navigation);
    });
}

fn connect_headerbar_down_navigation(widgets: &WindowWidgets, navigation: &WindowNavigationState) {
    let window = widgets.window.clone();
    let widgets = widgets.clone();
    let navigation = navigation.clone();
    let controller = EventControllerKey::new();
    controller.set_propagation_phase(adw::gtk::PropagationPhase::Capture);
    controller.connect_key_pressed(move |_, key, _, _| {
        if !matches!(key, gdk::Key::Down | gdk::Key::KP_Down) {
            return Propagation::Proceed;
        }

        let Some(focus) = adw::gtk::prelude::RootExt::focus(&widgets.window) else {
            return Propagation::Proceed;
        };
        if focus.ancestor(adw::HeaderBar::static_type()).is_none() {
            return Propagation::Proceed;
        }

        if focus_first_visible_page_target(&widgets, &navigation) {
            Propagation::Stop
        } else {
            Propagation::Proceed
        }
    });
    window.add_controller(controller);
}

fn connect_page_autofocus(widgets: &WindowWidgets, navigation: &WindowNavigationState) {
    let widgets = widgets.clone();
    let navigation = navigation.clone();
    let nav = navigation.nav.clone();
    nav.connect_notify_local(Some("visible-page"), move |_, _| {
        schedule_focus_first_visible_page_target(&widgets, &navigation);
    });
}

fn restore_window_size(window: &ApplicationWindow, preferences: &Preferences) {
    let (width, height) = preferences.window_size();
    window.set_default_size(width, height);
}

fn configure_search_entries(widgets: &WindowWidgets) {
    for search_entry in [
        &widgets.search_entry,
        &widgets.settings_search_entry,
        &widgets.tools_search_entry,
        &widgets.docs_search_entry,
        &widgets.tools_field_values_search_entry,
        &widgets.tools_value_values_search_entry,
        &widgets.tools_weak_passwords_search_entry,
        &widgets.tools_audit_search_entry,
        &widgets.store_recipients_search_entry,
        &widgets.store_git_search_entry,
    ] {
        configure_touch_friendly_search_entry(search_entry);
    }
}

fn connect_window_size_persistence(window: &ApplicationWindow) {
    let preferences = Preferences::new();
    window.connect_close_request(move |window| {
        let width = window.width();
        let height = window.height();
        if width > 0 && height > 0 {
            let _ = preferences.set_window_size(width, height);
        }
        Propagation::Proceed
    });
}
