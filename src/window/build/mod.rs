mod actions;
mod state;
pub(super) mod widgets;

use crate::password::list::{
    connect_selected_pass_file_shortcuts, focus_first_password_list_row, load_passwords_async,
    setup_search_filter, PasswordListActions,
};
use crate::password::model::OpenPassFile;
use crate::password::new_item::register_open_new_password_action;
use crate::password::new_item::NewPasswordDialogState;
use crate::password::otp::PasswordOtpState;
use crate::password::page::PasswordPageState;
use crate::password::page::{open_password_entry_page, password_page_has_unsaved_changes};
use crate::preferences::Preferences;
use crate::private_key::sync::{sync_private_keys_with_host, PrivateKeySyncDirection};
use crate::store::management::register_open_store_picker_action;
use crate::store::management::{
    connect_store_recipients_controls, rebuild_store_actions_list,
    register_store_recipients_reload_action, register_store_recipients_save_action,
    StoreRecipientsPageState,
};
use crate::store::management::{
    initialize_store_import_page, StoreImportChrome, StoreImportControls, StoreImportPageState,
    StoreImportPageWidgets,
};
use adw::gtk::{gdk, Builder, DirectionType, EventControllerKey, ListBox, Widget};
use adw::{prelude::*, Application, ApplicationWindow};

use self::actions::{
    connect_new_password_submit, connect_password_copy_buttons, connect_password_list_activation,
    register_password_page_actions,
};
use self::state::{
    back_action_state, build_git_action_state, context_undo_action_state,
    list_visibility_action_state, new_password_dialog_state, password_page_state,
    preferences_action_state, store_git_page_state, store_recipients_page_state,
    window_navigation_state,
};
use self::widgets::WindowWidgets;
use super::controls::{
    apply_startup_query, configure_window_shortcuts, connect_search_visibility,
    register_back_action, register_context_reload_action, register_context_save_action,
    register_context_undo_action, register_go_home_action, register_list_visibility_action,
    register_reload_password_list_action, register_toggle_find_action, ListVisibilityState,
};
use super::docs::{register_open_docs_action, DocumentationPageState, DocumentationPageWidgets};
use super::git::GitActionState;
use super::git::{
    register_open_git_action, register_synchronize_action, set_git_action_availability,
};
use super::host_access::append_optional_host_access_group_row;
use super::logs::{register_open_log_action, start_log_poller};
use super::navigation::{set_save_button_for_password, WindowNavigationState};
use super::preferences::{
    connect_audit_history_recipient_row, connect_backend_row, connect_pass_command_row,
    connect_private_key_sync_row, initialize_backend_row,
};
use super::preferences::{
    connect_clear_empty_fields_before_save_autosave, connect_new_password_template_autosave,
    connect_password_generation_autosave, connect_password_list_sort_autosave,
    connect_username_fallback_autosave, register_open_preferences_action, PreferencesActionState,
};
use super::tools::{
    register_open_tools_action, sync_tools_action_availability, ToolAuditWidgets,
    ToolBrowserWidgets, ToolsPageState, ToolsPageWidgets,
};
use crate::logging::{log_error, log_info};
use crate::support::actions::activate_widget_action;
use crate::support::runtime::{
    has_host_permission, log_runtime_capabilities_once, supports_host_command_features,
    supports_logging_features,
};
use crate::support::ui::{
    configure_touch_friendly_search_entry, connect_horizontal_arrow_adjustment_for_spin_buttons,
    connect_ordered_list_arrow_navigation, connect_vertical_arrow_navigation_for_buttons,
    focus_first_keyboard_focusable_list_row, focus_first_matching_list_row_in_order,
    focus_first_visible_widget, focus_last_matching_list_row_in_order, focus_last_visible_widget,
    focused_row_is_last_matching_list_row, list_row_is_keyboard_focusable,
    navigation_stack_is_root, text_view_cursor_is_on_first_line, text_view_cursor_is_on_last_line,
    visible_navigation_page_is, widget_contains_focus,
};
use crate::window::session::initialize_window_session;
use adw::glib::{self, Propagation};
use std::rc::Rc;

const UI_SRC: &str = include_str!(concat!(env!("OUT_DIR"), "/window.ui"));

fn build_store_recipients_page_state(
    widgets: &WindowWidgets,
    store_git_page: &crate::store::git_page::StoreGitPageState,
) -> StoreRecipientsPageState {
    store_recipients_page_state(widgets, store_git_page)
}

fn register_platform_window_actions(
    widgets: &WindowWidgets,
    recipients_page: &StoreRecipientsPageState,
) {
    register_open_store_picker_action(
        &widgets.window,
        &widgets.password_stores,
        &widgets.toast_overlay,
        recipients_page,
    );
}

fn register_platform_git_actions(widgets: &WindowWidgets, git_action_state: &GitActionState) {
    let git_supported = supports_host_command_features();
    if git_supported {
        register_open_git_action(git_action_state);
        register_synchronize_action(git_action_state);
    }
    let git_available = git_supported && has_host_permission();
    set_git_action_availability(&widgets.window, git_available);
    log_info(format!(
        "Window Git actions: open-git, git-clone, and synchronize are {}.",
        if git_available { "enabled" } else { "disabled" }
    ));
}

fn register_platform_log_actions(
    widgets: &WindowWidgets,
    navigation_state: &WindowNavigationState,
) {
    if !supports_logging_features() {
        return;
    }

    register_open_log_action(&widgets.window, navigation_state);
    start_log_poller(&widgets.log_view);
}

fn initialize_platform_log_features(widgets: &WindowWidgets) {
    let logging_supported = supports_logging_features();
    widgets.log_page.set_visible(logging_supported);
    widgets
        .git_busy_show_logs_button
        .set_visible(logging_supported);
}

fn register_docs_actions(widgets: &WindowWidgets, docs_page_state: &DocumentationPageState) {
    register_open_docs_action(&widgets.window, docs_page_state);
}

fn initialize_backend_preferences(widgets: &WindowWidgets, preferences: &Preferences) {
    let host_features_supported = supports_host_command_features();
    widgets
        .backend_preferences
        .set_visible(host_features_supported);
    initialize_backend_row(
        &widgets.backend_row,
        &widgets.pass_command_row,
        &widgets.sync_private_keys_with_host_row,
        &widgets.sync_private_keys_with_host_check,
        &widgets.audit_use_commit_history_recipients_row,
        &widgets.audit_use_commit_history_recipients_check,
        preferences,
    );
    if !host_features_supported {
        widgets.host_access_preferences_group.set_visible(false);
        return;
    }
    widgets
        .backend_preferences
        .set_sensitive(has_host_permission());
    append_optional_host_access_group_row(
        &widgets.host_access_preferences_group,
        &widgets.toast_overlay,
    );
}

fn connect_backend_preferences(
    widgets: &WindowWidgets,
    preferences: &Preferences,
    preferences_action_state: &PreferencesActionState,
    tools_page_state: &ToolsPageState,
) {
    connect_pass_command_row(
        &widgets.pass_command_row,
        &widgets.toast_overlay,
        preferences,
    );
    connect_private_key_sync_row(preferences_action_state);
    connect_audit_history_recipient_row(preferences_action_state);
    connect_backend_row(
        &widgets.backend_row,
        &widgets.pass_command_row,
        &widgets.toast_overlay,
        preferences,
        {
            let preferences = preferences.clone();
            let preferences_action_state = preferences_action_state.clone();
            let tools_page_state = tools_page_state.clone();
            let window = widgets.window.clone();
            move || {
                tools_page_state.refresh_select_page();
                rebuild_store_actions_list(
                    &preferences_action_state.store_actions_list,
                    &preferences_action_state.stores_list,
                    &preferences,
                    &preferences_action_state.page_state.window,
                    &preferences_action_state.overlay,
                    &preferences_action_state.recipients_page,
                );
                activate_widget_action(&window, "win.reload-store-recipients-list");
            }
        },
    );
}

fn initialize_store_import_page_ui(
    widgets: &WindowWidgets,
    navigation_state: &WindowNavigationState,
) {
    let state = StoreImportPageState::new(StoreImportPageWidgets {
        chrome: StoreImportChrome {
            window: &widgets.window,
            navigation: navigation_state,
            overlay: &widgets.toast_overlay,
            page: &widgets.store_import_page,
            stack: &widgets.store_import_stack,
            form: &widgets.store_import_form,
            loading: &widgets.store_import_loading,
        },
        controls: StoreImportControls {
            store_dropdown: &widgets.store_import_store_dropdown,
            source_dropdown: &widgets.store_import_source_dropdown,
            source_path_row: &widgets.store_import_source_path_row,
            source_file_button: &widgets.store_import_source_file_button,
            source_folder_button: &widgets.store_import_source_folder_button,
            source_clear_button: &widgets.store_import_source_clear_button,
            source_password_row: &widgets.store_import_password_row,
            target_path_row: &widgets.store_import_target_path_row,
            import_button: &widgets.store_import_button,
        },
    });
    initialize_store_import_page(&state);
}

fn connect_window_behaviors(
    widgets: &WindowWidgets,
    preferences: &Preferences,
    password_list_state: &PasswordPageState,
    preferences_action_state: &PreferencesActionState,
    tools_page_state: &ToolsPageState,
    store_recipients_page_state: &StoreRecipientsPageState,
    new_password_dialog_state: &NewPasswordDialogState,
) {
    connect_password_list_activation(
        &widgets.list,
        &widgets.search_entry,
        &widgets.toast_overlay,
        password_list_state,
    );

    connect_new_password_template_autosave(
        &widgets.new_pass_file_template_view,
        &widgets.toast_overlay,
    );
    connect_clear_empty_fields_before_save_autosave(
        &preferences_action_state.clear_empty_fields_before_save_row,
        &preferences_action_state.clear_empty_fields_before_save_check,
        &widgets.toast_overlay,
    );
    connect_username_fallback_autosave(
        &widgets.preferences_username_folder_check,
        &widgets.preferences_username_filename_check,
        &widgets.toast_overlay,
    );
    connect_password_list_sort_autosave(
        &widgets.preferences_password_list_sort_filename_check,
        &widgets.preferences_password_list_sort_store_path_check,
        &widgets.toast_overlay,
        &widgets.window,
    );
    connect_password_generation_autosave(
        &password_list_state.generator_controls,
        std::slice::from_ref(&preferences_action_state.generator_controls),
        &widgets.toast_overlay,
    );
    connect_password_generation_autosave(
        &preferences_action_state.generator_controls,
        std::slice::from_ref(&password_list_state.generator_controls),
        &widgets.toast_overlay,
    );
    connect_backend_preferences(
        widgets,
        preferences,
        preferences_action_state,
        tools_page_state,
    );
    connect_store_recipients_controls(store_recipients_page_state);
    connect_password_copy_buttons(
        &widgets.toast_overlay,
        &widgets.password_entry,
        &widgets.copy_password_button,
        &widgets.username_entry,
        &widgets.copy_username_button,
        &widgets.otp_entry,
        &widgets.copy_otp_button,
    );
    connect_new_password_submit(password_list_state, new_password_dialog_state);

    let revealer = widgets.password_generator_settings_revealer.clone();
    widgets
        .password_generator_settings_button
        .connect_toggled(move |button| {
            revealer.set_reveal_child(button.is_active());
        });
}

fn initialize_password_list(widgets: &WindowWidgets) {
    let list_actions = PasswordListActions::new(
        &widgets.add_button,
        &widgets.git_button,
        &widgets.store_button,
        &widgets.find_button,
        &widgets.save_button,
    );
    load_passwords_async(
        &widgets.list,
        &list_actions,
        &widgets.toast_overlay,
        Rc::new({
            let navigation = widgets.navigation_view.clone();
            move || navigation_stack_is_root(&navigation)
        }),
        false,
        false,
    );
    sync_tools_action_availability(&widgets.window);
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

fn store_recipients_page_lists(widgets: &WindowWidgets) -> [ListBox; 8] {
    [
        widgets.store_recipients_host_gpg_warning_list.clone(),
        widgets.store_recipients_fido2_info_list.clone(),
        widgets.store_recipients_scope_list.clone(),
        widgets.store_recipients_list.clone(),
        widgets.store_recipients_create_list.clone(),
        widgets.store_recipients_add_list.clone(),
        widgets.store_recipients_options_list.clone(),
        widgets.store_recipients_git_list.clone(),
    ]
}

fn store_git_page_lists(widgets: &WindowWidgets) -> [ListBox; 3] {
    [
        widgets.store_git_remotes_list.clone(),
        widgets.store_git_actions_list.clone(),
        widgets.store_git_status_list.clone(),
    ]
}

fn tools_page_lists(widgets: &WindowWidgets) -> [ListBox; 2] {
    [widgets.tools_list.clone(), widgets.tools_logs_list.clone()]
}

fn preferences_page_detail_widgets(widgets: &WindowWidgets) -> Vec<adw::gtk::Widget> {
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
        &store_recipients_page_lists(widgets),
        Some(&primary_menu_button),
        list_row_is_keyboard_focusable,
    );

    connect_ordered_list_arrow_navigation(
        &store_git_page_lists(widgets),
        Some(&primary_menu_button),
        list_row_is_keyboard_focusable,
    );

    connect_ordered_list_arrow_navigation(
        &tools_page_lists(widgets),
        Some(&primary_menu_button),
        list_row_is_keyboard_focusable,
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
        if focus_first_password_list_row(&widgets.list) {
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
        return focus_first_matching_list_row_in_order(
            &store_recipients_page_lists(widgets),
            list_row_is_keyboard_focusable,
        );
    }
    if visible_navigation_page_is(&navigation.nav, &widgets.store_git_page) {
        return focus_first_matching_list_row_in_order(
            &store_git_page_lists(widgets),
            list_row_is_keyboard_focusable,
        );
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
        &widgets.docs_search_entry,
        &widgets.tools_field_values_search_entry,
        &widgets.tools_value_values_search_entry,
        &widgets.tools_weak_passwords_search_entry,
        &widgets.tools_audit_search_entry,
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

pub fn create_main_window(
    app: &Application,
    startup_query: Option<String>,
    initial_pass_file: Option<OpenPassFile>,
) -> Result<ApplicationWindow, String> {
    let builder = Builder::from_string(UI_SRC);
    let widgets = WindowWidgets::load(&builder)?;
    widgets.window.set_application(Some(app));
    configure_search_entries(&widgets);
    initialize_window_session(&widgets.window);
    log_runtime_capabilities_once();
    let preferences = Preferences::new();
    if preferences.sync_private_keys_with_host() {
        if let Err(err) = sync_private_keys_with_host(PrivateKeySyncDirection::HostToApp) {
            log_error(format!("Failed to sync private keys during startup: {err}"));
            let _ = preferences.set_sync_private_keys_with_host(false);
        }
    }
    initialize_platform_log_features(&widgets);
    restore_window_size(&widgets.window, &preferences);
    connect_window_size_persistence(&widgets.window);
    initialize_backend_preferences(&widgets, &preferences);
    set_save_button_for_password(&widgets.save_button);
    set_save_button_for_password(&widgets.editor_save_button);
    connect_page_keyboard_navigation(&widgets);
    connect_page_list_keyboard_navigation(&widgets);
    connect_preferences_page_detail_navigation(&widgets);
    let primary_menu_button = widgets.primary_menu_button.clone().upcast();

    setup_search_filter(
        &widgets.list,
        &widgets.search_entry,
        &primary_menu_button,
        &widgets.password_list_stack,
        &widgets.password_list_status,
        &widgets.password_list_spinner,
        &widgets.password_list_scrolled,
    );
    connect_selected_pass_file_shortcuts(&widgets.list, &widgets.toast_overlay);
    initialize_password_list(&widgets);
    let new_password_dialog_state = new_password_dialog_state(&widgets);
    let password_otp_state = PasswordOtpState::new(&widgets.otp_entry, &widgets.toast_overlay);
    let password_list_state = password_page_state(&widgets, &password_otp_state);
    let list_visibility = ListVisibilityState::new(false, false);
    let store_git_page_state = store_git_page_state(&widgets);
    let store_recipients_page_state =
        build_store_recipients_page_state(&widgets, &store_git_page_state);
    let window_navigation_state = window_navigation_state(&widgets);
    connect_headerbar_down_navigation(&widgets, &window_navigation_state);
    connect_page_autofocus(&widgets, &window_navigation_state);
    let docs_page_state = DocumentationPageState::new(DocumentationPageWidgets::new(
        &window_navigation_state,
        &widgets.docs_search_entry,
        &widgets.docs_list,
        &widgets.docs_detail_page,
        &widgets.docs_detail_scrolled,
        &widgets.docs_detail_box,
    ));
    let tools_page_state = ToolsPageState::new(ToolsPageWidgets {
        window: &widgets.window,
        navigation: &window_navigation_state,
        page: &widgets.tools_page,
        list: &widgets.tools_list,
        field_values_row: &widgets.tools_field_values_row,
        field_values_suffix_stack: &widgets.tools_field_values_suffix_stack,
        field_values_suffix_arrow: &widgets.tools_field_values_suffix_arrow,
        field_values_spinner: &widgets.tools_field_values_spinner,
        weak_passwords_row: &widgets.tools_weak_passwords_row,
        weak_passwords_suffix_stack: &widgets.tools_weak_passwords_suffix_stack,
        weak_passwords_suffix_arrow: &widgets.tools_weak_passwords_suffix_arrow,
        weak_passwords_spinner: &widgets.tools_weak_passwords_spinner,
        audit_row: &widgets.tools_audit_row,
        audit_suffix_stack: &widgets.tools_audit_suffix_stack,
        audit_suffix_arrow: &widgets.tools_audit_suffix_arrow,
        audit_spinner: &widgets.tools_audit_spinner,
        logs_list: &widgets.tools_logs_list,
        docs_row: &widgets.tools_docs_row,
        logs_row: &widgets.tools_logs_row,
        copy_logs_row: &widgets.tools_copy_logs_row,
        copy_logs_button: &widgets.tools_copy_logs_button,
        overlay: &widgets.toast_overlay,
        password_page: &password_list_state,
        field_values: ToolBrowserWidgets {
            page: &widgets.tools_field_values_page,
            search_entry: &widgets.tools_field_values_search_entry,
            list: &widgets.tools_field_values_list,
        },
        value_values: ToolBrowserWidgets {
            page: &widgets.tools_value_values_page,
            search_entry: &widgets.tools_value_values_search_entry,
            list: &widgets.tools_value_values_list,
        },
        weak_passwords: ToolBrowserWidgets {
            page: &widgets.tools_weak_passwords_page,
            search_entry: &widgets.tools_weak_passwords_search_entry,
            list: &widgets.tools_weak_passwords_list,
        },
        audit: ToolAuditWidgets {
            page: &widgets.tools_audit_page,
            search_entry: &widgets.tools_audit_search_entry,
            stack: &widgets.tools_audit_stack,
            status: &widgets.tools_audit_status,
            scrolled: &widgets.tools_audit_scrolled,
            content: &widgets.tools_audit_content,
            filter_button: &widgets.tools_audit_filter_button,
            filter_popover: &widgets.tools_audit_filter_popover,
            filter_store_box: &widgets.tools_audit_filter_store_box,
            filter_branch_box: &widgets.tools_audit_filter_branch_box,
        },
        root_list: &widgets.list,
        root_search_entry: &widgets.search_entry,
    });
    initialize_store_import_page_ui(&widgets, &window_navigation_state);
    let preferences_action_state = preferences_action_state(&widgets, &store_recipients_page_state);
    let git_action_state = build_git_action_state(
        &widgets,
        &window_navigation_state,
        &store_recipients_page_state,
        &store_git_page_state,
        &list_visibility,
    );
    let back_action_state = back_action_state(
        &password_list_state,
        &store_recipients_page_state,
        &store_git_page_state,
        &window_navigation_state,
        &list_visibility,
        &git_action_state,
    );
    let list_visibility_action_state =
        list_visibility_action_state(&widgets, &window_navigation_state, &list_visibility);
    let context_undo_state = context_undo_action_state(
        &password_list_state,
        &store_recipients_page_state,
        &store_git_page_state,
        &window_navigation_state,
        &list_visibility,
    );

    connect_window_behaviors(
        &widgets,
        &preferences,
        &password_list_state,
        &preferences_action_state,
        &tools_page_state,
        &store_recipients_page_state,
        &new_password_dialog_state,
    );
    register_password_page_actions(&widgets.window, &password_list_state);
    register_store_recipients_save_action(
        &widgets.window,
        &widgets.toast_overlay,
        &widgets.password_stores,
        &store_recipients_page_state,
    );
    register_store_recipients_reload_action(&widgets.window, &store_recipients_page_state);
    register_platform_git_actions(&widgets, &git_action_state);
    register_platform_log_actions(&widgets, &window_navigation_state);
    register_docs_actions(&widgets, &docs_page_state);
    register_platform_window_actions(&widgets, &store_recipients_page_state);
    register_open_preferences_action(&widgets.window, &preferences_action_state);
    register_open_tools_action(&widgets.window, &tools_page_state);

    register_open_new_password_action(&widgets.window, &new_password_dialog_state);
    register_context_save_action(
        &widgets.window,
        &window_navigation_state,
        &store_recipients_page_state,
    );
    register_context_reload_action(
        &widgets.window,
        &window_navigation_state,
        &store_recipients_page_state,
    );
    register_context_undo_action(&widgets.window, &context_undo_state);
    connect_search_visibility(&widgets.find_button, &widgets.search_entry, &widgets.list);
    register_toggle_find_action(
        &widgets.window,
        &window_navigation_state,
        &widgets.find_button,
        &widgets.search_entry,
        &widgets.list,
        &widgets.tools_audit_search_entry,
        Rc::new({
            let tools_page_state = tools_page_state.clone();
            move || tools_page_state.render_audit_page()
        }),
    );
    register_list_visibility_action(&widgets.window, &list_visibility_action_state);
    register_reload_password_list_action(&widgets.window, &list_visibility_action_state);
    register_go_home_action(&widgets.window, &back_action_state);
    register_back_action(&widgets.window, &back_action_state);
    crate::updater::register_window(
        app,
        &widgets.window,
        &widgets.toast_overlay,
        Rc::new({
            let password_page = password_list_state.clone();
            let recipients_page = store_recipients_page_state.clone();
            move || {
                password_page_has_unsaved_changes(&password_page)
                    || recipients_page.recipients_are_dirty()
            }
        }),
    );

    configure_window_shortcuts(app);
    apply_startup_query(startup_query, &widgets.search_entry, &widgets.list);
    if let Some(initial_pass_file) = initial_pass_file {
        open_password_entry_page(&password_list_state, initial_pass_file, true);
    } else {
        schedule_focus_first_visible_page_target(&widgets, &window_navigation_state);
    }

    Ok(widgets.window)
}
