mod actions;
mod state;
pub(super) mod widgets;

use crate::password::list::{load_passwords_async, setup_search_filter, PasswordListActions};
use crate::password::new_item::register_open_new_password_action;
use crate::password::new_item::NewPasswordPopoverState;
use crate::password::otp::PasswordOtpState;
use crate::password::page::PasswordPageState;
use crate::preferences::Preferences;
use crate::store::management::register_open_store_picker_action;
use crate::store::management::{
    connect_store_recipients_controls, rebuild_store_actions_list,
    register_store_recipients_reload_action, register_store_recipients_save_action,
    StoreRecipientsPageState,
};
use crate::store::management::{initialize_store_import_page, StoreImportPageState};
use adw::gtk::Builder;
use adw::{prelude::*, Application, ApplicationWindow};

use self::actions::{
    connect_new_password_submit, connect_password_copy_buttons, connect_password_list_activation,
    register_password_page_actions,
};
use self::state::{
    back_action_state, build_git_action_state, context_undo_action_state,
    list_visibility_action_state, new_password_popover_state, password_page_state,
    preferences_action_state, store_recipients_page_state, window_navigation_state,
};
use self::widgets::WindowWidgets;
use super::controls::{
    apply_startup_query, configure_window_shortcuts, connect_search_visibility,
    register_back_action, register_context_save_action, register_context_undo_action,
    register_go_home_action, register_list_visibility_action, register_reload_password_list_action,
    register_toggle_find_action, ListVisibilityState,
};
use super::git::GitActionState;
use super::git::{
    register_open_git_action, register_synchronize_action, set_git_action_availability,
};
use super::logs::{register_open_log_action, start_log_poller};
use super::navigation::{set_save_button_for_password, WindowNavigationState};
use super::preferences::{connect_backend_row, connect_pass_command_row, initialize_backend_row};
use super::preferences::{
    connect_new_password_template_autosave, connect_password_generation_autosave,
    connect_username_fallback_autosave, register_open_preferences_action, PreferencesActionState,
};
use super::tools::{register_open_tools_action, ToolsPageState};
use crate::logging::log_info;
use crate::support::runtime::{has_host_permission, log_runtime_capabilities_once};

const UI_SRC: &str = include_str!(concat!(env!("OUT_DIR"), "/window.ui"));

fn build_store_recipients_page_state(widgets: &WindowWidgets) -> StoreRecipientsPageState {
    store_recipients_page_state(widgets)
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
    register_open_git_action(git_action_state);
    register_synchronize_action(git_action_state);
    let git_available = has_host_permission();
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
    register_open_log_action(&widgets.window, navigation_state);
    start_log_poller(&widgets.log_view);
}

fn initialize_backend_preferences(widgets: &WindowWidgets, preferences: &Preferences) {
    widgets
        .backend_preferences
        .set_sensitive(has_host_permission());
    initialize_backend_row(&widgets.backend_row, &widgets.pass_command_row, preferences);
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
    connect_backend_row(
        &widgets.backend_row,
        &widgets.pass_command_row,
        &widgets.toast_overlay,
        preferences,
        {
            let preferences = preferences.clone();
            let preferences_action_state = preferences_action_state.clone();
            let tools_page_state = tools_page_state.clone();
            move || {
                tools_page_state.rebuild();
                rebuild_store_actions_list(
                    &preferences_action_state.store_actions_list,
                    &preferences_action_state.stores_list,
                    &preferences,
                    &preferences_action_state.page_state.window,
                    &preferences_action_state.overlay,
                    &preferences_action_state.recipients_page,
                );
            }
        },
    );
}

fn initialize_store_import_page_ui(
    widgets: &WindowWidgets,
    navigation_state: &WindowNavigationState,
) {
    let state = StoreImportPageState::new(
        &widgets.window,
        navigation_state,
        &widgets.toast_overlay,
        &widgets.store_import_page,
        &widgets.store_import_stack,
        &widgets.store_import_form,
        &widgets.store_import_loading,
        &widgets.store_import_store_dropdown,
        &widgets.store_import_source_dropdown,
        &widgets.store_import_source_path_row,
        &widgets.store_import_source_file_button,
        &widgets.store_import_source_folder_button,
        &widgets.store_import_source_clear_button,
        &widgets.store_import_target_path_row,
        &widgets.store_import_button,
    );
    initialize_store_import_page(&state);
}

fn connect_window_behaviors(
    widgets: &WindowWidgets,
    preferences: &Preferences,
    password_list_state: &PasswordPageState,
    preferences_action_state: &PreferencesActionState,
    tools_page_state: &ToolsPageState,
    store_recipients_page_state: &StoreRecipientsPageState,
    new_password_popover_state: &NewPasswordPopoverState,
) {
    connect_password_list_activation(&widgets.list, &widgets.toast_overlay, password_list_state);

    connect_new_password_template_autosave(
        &widgets.new_pass_file_template_view,
        &widgets.toast_overlay,
    );
    connect_username_fallback_autosave(
        &widgets.preferences_username_folder_check,
        &widgets.preferences_username_filename_check,
        &widgets.toast_overlay,
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
    connect_new_password_submit(
        &widgets.path_entry,
        password_list_state,
        new_password_popover_state,
        &widgets.add_button_popover,
    );

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
        true,
        false,
        false,
    );
}

pub fn create_main_window(app: &Application, startup_query: Option<String>) -> ApplicationWindow {
    let builder = Builder::from_string(UI_SRC);
    let widgets = WindowWidgets::load(&builder);
    widgets.window.set_application(Some(app));
    log_runtime_capabilities_once();
    let preferences = Preferences::new();
    initialize_backend_preferences(&widgets, &preferences);
    set_save_button_for_password(&widgets.save_button);

    setup_search_filter(&widgets.list, &widgets.search_entry);
    initialize_password_list(&widgets);
    let new_password_popover_state = new_password_popover_state(&widgets);
    let password_otp_state = PasswordOtpState::new(&widgets.otp_entry, &widgets.toast_overlay);
    let password_list_state = password_page_state(&widgets, &password_otp_state);
    let list_visibility = ListVisibilityState::new(false, false);
    let store_recipients_page_state = build_store_recipients_page_state(&widgets);
    let window_navigation_state = window_navigation_state(&widgets);
    let tools_page_state = ToolsPageState::new(
        &widgets.window,
        &window_navigation_state,
        &widgets.tools_page,
        &widgets.tools_list,
        &widgets.toast_overlay,
        &widgets.tools_field_values_page,
        &widgets.tools_field_values_search_entry,
        &widgets.tools_field_values_list,
        &widgets.tools_value_values_page,
        &widgets.tools_value_values_search_entry,
        &widgets.tools_value_values_list,
        &widgets.list,
        &widgets.search_entry,
    );
    initialize_store_import_page_ui(&widgets, &window_navigation_state);
    let preferences_action_state = preferences_action_state(&widgets, &store_recipients_page_state);
    let git_action_state = build_git_action_state(
        &widgets,
        &window_navigation_state,
        &store_recipients_page_state,
        &list_visibility,
    );
    let back_action_state = back_action_state(
        &password_list_state,
        &store_recipients_page_state,
        &window_navigation_state,
        &list_visibility,
        &git_action_state,
    );
    let list_visibility_action_state =
        list_visibility_action_state(&widgets, &window_navigation_state, &list_visibility);
    let context_undo_state = context_undo_action_state(
        &password_list_state,
        &store_recipients_page_state,
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
        &new_password_popover_state,
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
    register_platform_window_actions(&widgets, &store_recipients_page_state);
    register_open_preferences_action(&widgets.window, &preferences_action_state);
    register_open_tools_action(&widgets.window, &tools_page_state);

    register_open_new_password_action(&widgets.window, &new_password_popover_state);
    register_context_save_action(
        &widgets.window,
        &window_navigation_state,
        &store_recipients_page_state,
    );
    register_context_undo_action(&widgets.window, &context_undo_state);
    connect_search_visibility(&widgets.find_button, &widgets.search_entry, &widgets.list);
    register_toggle_find_action(
        &widgets.window,
        &widgets.find_button,
        &widgets.search_entry,
        &widgets.list,
    );
    register_list_visibility_action(&widgets.window, &list_visibility_action_state);
    register_reload_password_list_action(&widgets.window, &list_visibility_action_state);
    register_go_home_action(&widgets.window, &back_action_state);
    register_back_action(&widgets.window, &back_action_state);

    configure_window_shortcuts(app);
    apply_startup_query(startup_query, &widgets.search_entry, &widgets.list);

    widgets.window
}
