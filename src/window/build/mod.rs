mod actions;
mod state;
pub(super) mod widgets;

use crate::password::list::{
    connect_selected_pass_file_shortcuts, load_passwords_async, setup_search_filter,
    PasswordListActions,
};
use crate::password::model::OpenPassFile;
use crate::password::new_item::register_open_new_password_action;
use crate::password::new_item::NewPasswordPopoverState;
use crate::password::otp::PasswordOtpState;
use crate::password::page::open_password_entry_page;
#[cfg(target_os = "windows")]
use crate::password::page::password_page_has_unsaved_changes;
use crate::password::page::PasswordPageState;
use crate::preferences::Preferences;
use crate::private_key::sync::{sync_private_keys_with_host, PrivateKeySyncDirection};
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
use super::docs::{register_open_docs_action, DocumentationPageState};
use super::git::GitActionState;
use super::git::{
    register_open_git_action, register_synchronize_action, set_git_action_availability,
};
use super::host_access::{
    append_optional_host_access_group_row, append_optional_smartcard_access_row,
};
use super::logs::{register_open_log_action, start_log_poller};
use super::navigation::{set_save_button_for_password, WindowNavigationState};
use super::preferences::{
    connect_backend_row, connect_pass_command_row, connect_private_key_sync_row,
    initialize_backend_row,
};
use super::preferences::{
    connect_clear_empty_fields_before_save_autosave, connect_new_password_template_autosave,
    connect_password_generation_autosave, connect_password_list_sort_autosave,
    connect_username_fallback_autosave, register_open_preferences_action, PreferencesActionState,
};
use super::tools::{register_open_tools_action, ToolsPageState};
use crate::logging::{log_error, log_info};
use crate::support::runtime::{
    has_host_permission, log_runtime_capabilities_once, supports_host_command_features,
    supports_logging_features, supports_smartcard_features,
};
use crate::window::session::initialize_window_session;
use adw::glib::Propagation;
#[cfg(target_os = "windows")]
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

fn initialize_store_recipients_permissions(widgets: &WindowWidgets) {
    let smartcard_features_supported = supports_smartcard_features();
    widgets
        .store_recipients_add_hardware_key_row
        .set_visible(smartcard_features_supported);
    widgets
        .store_recipients_import_hardware_key_row
        .set_visible(smartcard_features_supported);

    if !smartcard_features_supported {
        return;
    }

    append_optional_smartcard_access_row(
        &widgets.store_recipients_add_list,
        &widgets.toast_overlay,
        &[
            &widgets.store_recipients_add_hardware_key_row,
            &widgets.store_recipients_import_hardware_key_row,
        ],
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
    connect_new_password_submit(password_list_state, new_password_popover_state);

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

fn restore_window_size(window: &ApplicationWindow, preferences: &Preferences) {
    let (width, height) = preferences.window_size();
    window.set_default_size(width, height);
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

    setup_search_filter(
        &widgets.list,
        &widgets.search_entry,
        &widgets.password_list_stack,
        &widgets.password_list_status,
        &widgets.password_list_spinner,
        &widgets.password_list_scrolled,
    );
    connect_selected_pass_file_shortcuts(&widgets.list, &widgets.toast_overlay);
    initialize_password_list(&widgets);
    let new_password_popover_state = new_password_popover_state(&widgets);
    let password_otp_state = PasswordOtpState::new(&widgets.otp_entry, &widgets.toast_overlay);
    let password_list_state = password_page_state(&widgets, &password_otp_state);
    let list_visibility = ListVisibilityState::new(false, false);
    let store_git_page_state = store_git_page_state(&widgets);
    let store_recipients_page_state =
        build_store_recipients_page_state(&widgets, &store_git_page_state);
    initialize_store_recipients_permissions(&widgets);
    let window_navigation_state = window_navigation_state(&widgets);
    let docs_page_state = DocumentationPageState::new(
        &window_navigation_state,
        &widgets.docs_page,
        &widgets.docs_search_entry,
        &widgets.docs_list,
        &widgets.docs_detail_page,
        &widgets.docs_detail_scrolled,
        &widgets.docs_detail_box,
    );
    let tools_page_state = ToolsPageState::new(
        &widgets.window,
        &window_navigation_state,
        &widgets.tools_page,
        &widgets.tools_list,
        &widgets.tools_logs_list,
        &widgets.toast_overlay,
        &password_list_state,
        &widgets.tools_field_values_page,
        &widgets.tools_field_values_search_entry,
        &widgets.tools_field_values_list,
        &widgets.tools_value_values_page,
        &widgets.tools_value_values_search_entry,
        &widgets.tools_value_values_list,
        &widgets.tools_weak_passwords_page,
        &widgets.tools_weak_passwords_search_entry,
        &widgets.tools_weak_passwords_list,
        &widgets.list,
        &widgets.search_entry,
    );
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
    register_docs_actions(&widgets, &docs_page_state);
    register_platform_window_actions(&widgets, &store_recipients_page_state);
    register_open_preferences_action(&widgets.window, &preferences_action_state);
    register_open_tools_action(&widgets.window, &tools_page_state);

    register_open_new_password_action(&widgets.window, &new_password_popover_state);
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
        &widgets.find_button,
        &widgets.search_entry,
        &widgets.list,
    );
    register_list_visibility_action(&widgets.window, &list_visibility_action_state);
    register_reload_password_list_action(&widgets.window, &list_visibility_action_state);
    register_go_home_action(&widgets.window, &back_action_state);
    register_back_action(&widgets.window, &back_action_state);
    #[cfg(target_os = "windows")]
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
    }

    Ok(widgets.window)
}
