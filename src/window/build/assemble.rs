use super::actions::{
    connect_new_password_submit, connect_password_copy_buttons, connect_password_list_activation,
    register_password_page_actions,
};
use super::deferred::DeferredState;
use super::widgets::WindowWidgets;
use crate::logging::log_info;
use crate::password::list::{
    connect_selected_pass_file_shortcuts, load_passwords_async, setup_search_filter,
    PasswordListActions,
};
use crate::password::new_item::{register_open_new_password_action, NewPasswordDialogState};
use crate::password::page::PasswordPageState;
use crate::preferences::Preferences;
use crate::store::git_page::{connect_store_git_controls, StoreGitPageState};
use crate::store::management::{
    connect_store_recipients_controls, initialize_store_import_page, rebuild_store_actions_list,
    register_open_store_picker_action, register_open_store_recipients_shortcut_actions,
    register_store_recipients_reload_action, register_store_recipients_save_action,
    StoreImportChrome, StoreImportControls, StoreImportPageState, StoreImportPageWidgets,
    StoreRecipientsPageState,
};
use crate::support::actions::activate_widget_action;
use crate::support::runtime::{
    has_host_permission, supports_host_command_features, supports_logging_features,
};
use crate::window::controls::{
    connect_search_visibility, register_back_action, register_context_reload_action,
    register_context_save_action, register_context_undo_action, register_go_home_action,
    register_list_visibility_action, register_reload_password_list_action,
    register_toggle_find_action, BackActionState, ContextUndoActionState,
    ListVisibilityActionState,
};
use crate::window::docs::{register_open_docs_action, DocumentationPageState};
use crate::window::git::{
    register_open_git_action, register_synchronize_action, set_git_action_availability,
    GitActionState,
};
use crate::window::host_access::append_optional_host_access_group_row;
use crate::window::logs::{register_open_log_action, start_log_poller};
use crate::window::navigation::{set_save_button_for_password, WindowNavigationState};
use crate::window::preferences::{
    connect_audit_history_recipient_row, connect_backend_row,
    connect_clear_empty_fields_before_save_autosave, connect_new_password_template_autosave,
    connect_pass_command_row, connect_password_generation_autosave,
    connect_password_list_sort_autosave, connect_private_key_sync_row,
    connect_username_fallback_autosave, initialize_backend_row, register_open_preferences_action,
    PreferencesActionState,
};
use crate::window::tools::{
    register_open_tools_action, sync_tools_action_availability, ToolsPageState,
};
use adw::prelude::*;
use std::rc::Rc;

pub(super) fn assemble_password_list_page(widgets: &WindowWidgets) {
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
            move || crate::support::ui::navigation_stack_is_root(&navigation)
        }),
        false,
        false,
    );
    sync_tools_action_availability(&widgets.window);
}

pub(super) fn assemble_password_page(
    widgets: &WindowWidgets,
    password_page_state: &PasswordPageState,
    preferences_action_state: &PreferencesActionState,
    new_password_dialog_state: &NewPasswordDialogState,
) {
    set_save_button_for_password(&widgets.save_button);
    set_save_button_for_password(&widgets.editor_save_button);

    connect_password_list_activation(
        &widgets.list,
        &widgets.search_entry,
        &widgets.toast_overlay,
        password_page_state,
    );
    connect_password_copy_buttons(
        &widgets.toast_overlay,
        &widgets.password_entry,
        &widgets.copy_password_button,
        &widgets.username_entry,
        &widgets.copy_username_button,
        &widgets.otp_entry,
        &widgets.copy_otp_button,
    );
    connect_new_password_submit(password_page_state, new_password_dialog_state);
    connect_password_generation_autosave(
        &password_page_state.generator_controls,
        std::slice::from_ref(&preferences_action_state.generator_controls),
        &widgets.toast_overlay,
    );

    let revealer = widgets.password_generator_settings_revealer.clone();
    widgets
        .password_generator_settings_button
        .connect_toggled(move |button| {
            revealer.set_reveal_child(button.is_active());
        });

    register_password_page_actions(&widgets.window, password_page_state);
    register_open_new_password_action(&widgets.window, new_password_dialog_state);
}

pub(super) fn assemble_preferences_page(
    widgets: &WindowWidgets,
    preferences: &Preferences,
    password_page_state: &PasswordPageState,
    preferences_action_state: &PreferencesActionState,
    tools_page_state: &DeferredState<ToolsPageState>,
) {
    preferences_action_state.search.connect_handlers();
    initialize_backend_preferences(widgets, preferences);

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
        &preferences_action_state.generator_controls,
        std::slice::from_ref(&password_page_state.generator_controls),
        &widgets.toast_overlay,
    );
    connect_backend_preferences(
        widgets,
        preferences,
        preferences_action_state,
        tools_page_state,
    );

    register_open_preferences_action(&widgets.window, preferences_action_state);
}

pub(super) fn assemble_store_import_page(
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

pub(super) fn assemble_store_recipients_page(
    widgets: &WindowWidgets,
    store_recipients_page_state: &StoreRecipientsPageState,
) {
    store_recipients_page_state.search.connect_handlers();
    connect_store_recipients_controls(store_recipients_page_state);
    register_store_recipients_save_action(
        &widgets.window,
        &widgets.toast_overlay,
        &widgets.password_stores,
        store_recipients_page_state,
    );
    register_store_recipients_reload_action(&widgets.window, store_recipients_page_state);
    register_open_store_picker_action(
        &widgets.window,
        &widgets.password_stores,
        &widgets.toast_overlay,
        store_recipients_page_state,
    );
    register_open_store_recipients_shortcut_actions(&widgets.window, store_recipients_page_state);
}

pub(super) fn assemble_git_page(
    widgets: &WindowWidgets,
    store_git_page: &StoreGitPageState,
    git_action_state: &GitActionState,
) {
    store_git_page.search.connect_handlers();
    let git_supported = supports_host_command_features();
    connect_store_git_controls(store_git_page);
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

pub(super) fn assemble_log_page(widgets: &WindowWidgets, navigation_state: &WindowNavigationState) {
    let logging_supported = supports_logging_features();
    widgets.log_page.set_visible(logging_supported);
    widgets
        .git_busy_show_logs_button
        .set_visible(logging_supported);

    if !logging_supported {
        return;
    }

    register_open_log_action(&widgets.window, navigation_state);
    start_log_poller(&widgets.log_view);
}

pub(super) fn assemble_docs_page(
    widgets: &WindowWidgets,
    docs_page_state: &DeferredState<DocumentationPageState>,
) {
    let docs_page_state = docs_page_state.clone();
    register_open_docs_action(&widgets.window, move || {
        docs_page_state.with(DocumentationPageState::open);
    });
}

pub(super) fn assemble_tools_page(
    widgets: &WindowWidgets,
    tools_page_state: &DeferredState<ToolsPageState>,
) {
    let tools_page_state = tools_page_state.clone();
    register_open_tools_action(&widgets.window, move || {
        tools_page_state.with(ToolsPageState::open);
    });
}

pub(super) fn register_window_navigation_actions(
    widgets: &WindowWidgets,
    navigation_state: &WindowNavigationState,
    tools_page_state: &DeferredState<ToolsPageState>,
    store_recipients_page_state: &StoreRecipientsPageState,
    list_visibility_action_state: &ListVisibilityActionState,
    back_action_state: &BackActionState,
    context_undo_state: &ContextUndoActionState,
) {
    register_context_save_action(
        &widgets.window,
        navigation_state,
        store_recipients_page_state,
    );
    register_context_reload_action(
        &widgets.window,
        navigation_state,
        store_recipients_page_state,
    );
    register_context_undo_action(&widgets.window, context_undo_state);

    connect_search_visibility(&widgets.find_button, &widgets.search_entry, &widgets.list);
    register_toggle_find_action(
        &widgets.window,
        navigation_state,
        &widgets.find_button,
        &widgets.search_entry,
        &widgets.list,
        &widgets.settings_search_entry,
        &widgets.store_recipients_page,
        &widgets.store_recipients_search_entry,
        &widgets.store_git_page,
        &widgets.store_git_search_entry,
        &widgets.tools_search_entry,
        &widgets.docs_search_entry,
        &widgets.tools_field_values_search_entry,
        &widgets.tools_value_values_search_entry,
        &widgets.tools_weak_passwords_search_entry,
        &widgets.tools_audit_search_entry,
        Rc::new({
            let tools_page_state = tools_page_state.clone();
            move || {
                let _ = tools_page_state.with_initialized(|state| state.render_audit_page());
            }
        }),
    );
    register_list_visibility_action(&widgets.window, list_visibility_action_state);
    register_reload_password_list_action(&widgets.window, list_visibility_action_state);
    register_go_home_action(&widgets.window, back_action_state);
    register_back_action(&widgets.window, back_action_state);
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
    tools_page_state: &DeferredState<ToolsPageState>,
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
                let _ = tools_page_state.with_initialized(ToolsPageState::refresh_select_page);
                rebuild_store_actions_list(
                    &preferences_action_state.store_actions_list,
                    &preferences_action_state.stores_list,
                    &preferences,
                    &preferences_action_state.page_state.window,
                    &preferences_action_state.overlay,
                    &preferences_action_state.recipients_page,
                    None,
                );
                preferences_action_state.search.sync();
                activate_widget_action(&window, "win.reload-store-recipients-list");
            }
        },
    );
}
