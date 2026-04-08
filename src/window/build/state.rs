use super::widgets::WindowWidgets;
use crate::backend::StoreRecipientsPrivateKeyRequirement;
use crate::password::file::{DynamicFieldRow, StructuredPassLine};
use crate::password::generation::PasswordGenerationControls;
use crate::password::new_item::NewPasswordDialogState;
use crate::password::otp::PasswordOtpState;
use crate::password::page::PasswordPageState;
use crate::store::git_page::StoreGitPageState;
use crate::store::management::{
    StoreRecipientsPageState, StoreRecipientsPlatformState, StoreRecipientsRequest,
};
use crate::window::controls::{
    BackActionState, ContextUndoActionState, ListVisibilityActionState, ListVisibilityState,
    PlatformBackActionState,
};
use crate::window::docs::{DocumentationPageState, DocumentationPageWidgets};
use crate::window::git::GitActionState;
use crate::window::navigation::{WindowNavigationState, WindowPageState};
use crate::window::preferences::PreferencesActionState;
use crate::window::preferences_search::{PreferencesPageSearchState, SearchablePreferencesGroup};
use crate::window::tools::{
    ToolAuditWidgets, ToolBrowserWidgets, ToolsPageState, ToolsPageWidgets,
};
use adw::prelude::*;
use std::cell::{Cell, RefCell};
use std::rc::Rc;

pub(super) fn new_password_dialog_state(_widgets: &WindowWidgets) -> NewPasswordDialogState {
    let (dialog, store_dropdown, path_entry, error_label) =
        crate::password::new_item::build_new_password_dialog();
    NewPasswordDialogState {
        dialog,
        path_entry,
        store_dropdown,
        error_label,
        store_roots: Rc::new(RefCell::new(Vec::new())),
    }
}

pub(super) fn password_page_state(
    widgets: &WindowWidgets,
    otp: &PasswordOtpState,
) -> PasswordPageState {
    PasswordPageState {
        nav: widgets.navigation_view.clone(),
        page: widgets.password_page.clone(),
        raw_page: widgets.raw_text_page.clone(),
        list: widgets.list.clone(),
        back: widgets.back_button.clone(),
        add: widgets.add_button.clone(),
        find: widgets.find_button.clone(),
        git: widgets.git_button.clone(),
        store: widgets.store_button.clone(),
        save: widgets.save_button.clone(),
        raw: widgets.open_raw_button.clone(),
        win: widgets.window_title.clone(),
        status: widgets.password_status.clone(),
        entry: widgets.password_entry.clone(),
        password_analysis_label: widgets.password_analysis_label.clone(),
        username: widgets.username_entry.clone(),
        otp: otp.clone(),
        field_add_row: widgets.add_field_row.clone(),
        template_button: widgets.apply_template_button.clone(),
        clean_button: widgets.clean_pass_file_button.clone(),
        otp_add_button: widgets.add_otp_button.clone(),
        import_private_key_button: widgets.import_private_key_button.clone(),
        editor_save_button: widgets.editor_save_button.clone(),
        generator_settings_button: widgets.password_generator_settings_button.clone(),
        generator_settings_revealer: widgets.password_generator_settings_revealer.clone(),
        generator_controls: PasswordGenerationControls::new(
            &widgets.password_generator_length_spin,
            &widgets.password_generator_min_lowercase_spin,
            &widgets.password_generator_min_uppercase_spin,
            &widgets.password_generator_min_numbers_spin,
            &widgets.password_generator_min_symbols_spin,
        ),
        dynamic_box: widgets.dynamic_fields_box.clone(),
        structured_templates: Rc::new(RefCell::new(Vec::<StructuredPassLine>::new())),
        dynamic_rows: Rc::new(RefCell::new(Vec::<DynamicFieldRow>::new())),
        text: widgets.text_view.clone(),
        overlay: widgets.toast_overlay.clone(),
        saved_contents: Rc::new(RefCell::new(String::new())),
        saved_entry_exists: Rc::new(Cell::new(false)),
    }
}

pub(super) fn store_git_page_state(widgets: &WindowWidgets) -> StoreGitPageState {
    let remote_rows = Rc::new(RefCell::new(Vec::new()));
    let action_rows = Rc::new(RefCell::new(Vec::new()));
    let status_rows = Rc::new(RefCell::new(Vec::new()));
    let search = PreferencesPageSearchState::new(
        &widgets.store_git_preferences_page,
        &widgets.store_git_search_entry,
        Some(&widgets.store_git_search_empty_group),
        vec![
            SearchablePreferencesGroup::with_tracked_widgets(
                &widgets.store_git_remotes_list,
                remote_rows.clone(),
            ),
            SearchablePreferencesGroup::with_tracked_widgets(
                &widgets.store_git_actions_list,
                action_rows.clone(),
            ),
            SearchablePreferencesGroup::with_tracked_widgets(
                &widgets.store_git_status_list,
                status_rows.clone(),
            ),
            SearchablePreferencesGroup::with_widgets(&widgets.store_git_access_list, Vec::new()),
        ],
    );
    StoreGitPageState {
        window: widgets.window.clone(),
        nav: widgets.navigation_view.clone(),
        page: widgets.store_git_page.clone(),
        back_row: widgets.store_git_back_row.clone(),
        search,
        remotes_list: widgets.store_git_remotes_list.clone(),
        actions_list: widgets.store_git_actions_list.clone(),
        status_list: widgets.store_git_status_list.clone(),
        access_list: widgets.store_git_access_list.clone(),
        overlay: widgets.toast_overlay.clone(),
        back: widgets.back_button.clone(),
        add: widgets.add_button.clone(),
        find: widgets.find_button.clone(),
        git: widgets.git_button.clone(),
        store: widgets.store_button.clone(),
        save: widgets.save_button.clone(),
        raw: widgets.open_raw_button.clone(),
        win: widgets.window_title.clone(),
        busy_page: widgets.git_busy_page.clone(),
        busy_status: widgets.git_busy_status.clone(),
        current_store: Rc::new(RefCell::new(None)),
        recipients_page: Rc::new(RefCell::new(None)),
        reopen_after_busy: Rc::new(Cell::new(false)),
        remote_rows,
        action_rows,
        status_rows,
    }
}

fn build_store_recipients_platform_state(
    widgets: &WindowWidgets,
    store_git_page: &StoreGitPageState,
) -> StoreRecipientsPlatformState {
    StoreRecipientsPlatformState {
        overlay: widgets.toast_overlay.clone(),
        host_gpg_warning_group: widgets.store_recipients_host_gpg_warning_group.clone(),
        host_gpg_warning_list: widgets.store_recipients_host_gpg_warning_list.clone(),
        host_gpg_warning_row: widgets.store_recipients_host_gpg_warning_row.clone(),
        fido2_info_group: widgets.store_recipients_fido2_info_group.clone(),
        fido2_info_list: widgets.store_recipients_fido2_info_list.clone(),
        scope_group: widgets.store_recipients_scope_group.clone(),
        saving_group: widgets.store_recipients_saving_group.clone(),
        keys_group: widgets.store_recipients_keys_group.clone(),
        scope_list: widgets.store_recipients_scope_list.clone(),
        add_group: widgets.store_recipients_add_group.clone(),
        add_list: widgets.store_recipients_add_list.clone(),
        create_group: widgets.store_recipients_create_group.clone(),
        create_list: widgets.store_recipients_create_list.clone(),
        options_group: widgets.store_recipients_options_group.clone(),
        options_list: widgets.store_recipients_options_list.clone(),
        scope_row: widgets.store_recipients_scope_row.clone(),
        git_group: widgets.store_recipients_git_group.clone(),
        git_list: widgets.store_recipients_git_list.clone(),
        setup_hardware_key_row: widgets.store_recipients_setup_hardware_key_row.clone(),
        add_hardware_key_row: widgets.store_recipients_add_hardware_key_row.clone(),
        add_fido2_key_row: widgets.store_recipients_add_fido2_key_row.clone(),
        store_git_page: store_git_page.clone(),
        import_hardware_key_row: widgets.store_recipients_import_hardware_key_row.clone(),
        import_clipboard_row: widgets.store_recipients_import_clipboard_row.clone(),
        import_file_row: widgets.store_recipients_import_file_row.clone(),
        generate_key_row: widgets.store_recipients_generate_key_row.clone(),
        generate_fido2_key_row: widgets.store_recipients_generate_fido2_key_row.clone(),
        require_all_row: widgets.store_recipients_require_all_row.clone(),
        all_fido2_keys_required_row: widgets.store_recipients_all_fido2_keys_required_row.clone(),
        require_all_check: widgets.store_recipients_require_all_check.clone(),
        private_key_generation_page: widgets.private_key_generation_page.clone(),
        private_key_generation_stack: widgets.private_key_generation_stack.clone(),
        private_key_generation_form: widgets.private_key_generation_form.clone(),
        private_key_generation_loading: widgets.private_key_generation_loading.clone(),
        private_key_generation_name_row: widgets.private_key_generation_name_row.clone(),
        private_key_generation_email_row: widgets.private_key_generation_email_row.clone(),
        private_key_generation_password_row: widgets.private_key_generation_password_row.clone(),
        private_key_generation_confirm_row: widgets.private_key_generation_confirm_row.clone(),
        private_key_generation_in_flight: Rc::new(Cell::new(false)),
        hardware_key_generation_page: widgets.hardware_key_generation_page.clone(),
        hardware_key_generation_stack: widgets.hardware_key_generation_stack.clone(),
        hardware_key_generation_form: widgets.hardware_key_generation_form.clone(),
        hardware_key_generation_loading: widgets.hardware_key_generation_loading.clone(),
        hardware_key_generation_name_row: widgets.hardware_key_generation_name_row.clone(),
        hardware_key_generation_email_row: widgets.hardware_key_generation_email_row.clone(),
        hardware_key_generation_admin_pin_row: widgets
            .hardware_key_generation_admin_pin_row
            .clone(),
        hardware_key_generation_user_pin_row: widgets.hardware_key_generation_user_pin_row.clone(),
        hardware_key_generation_token: Rc::new(RefCell::new(None)),
        hardware_key_generation_in_flight: Rc::new(Cell::new(false)),
    }
}

fn build_store_recipients_page_state(
    widgets: &WindowWidgets,
    platform: StoreRecipientsPlatformState,
) -> StoreRecipientsPageState {
    let request = Rc::new(RefCell::new(None::<StoreRecipientsRequest>));
    let recipients = Rc::new(RefCell::new(Vec::<String>::new()));
    let saved_recipients = Rc::new(RefCell::new(Vec::<String>::new()));
    let recipient_scope_dirs = Rc::new(RefCell::new(Vec::<String>::new()));
    let selected_recipient_scope = Rc::new(RefCell::new(".".to_string()));
    let private_key_requirement = Rc::new(Cell::new(
        StoreRecipientsPrivateKeyRequirement::AnyManagedKey,
    ));
    let saved_private_key_requirement = Rc::new(Cell::new(
        StoreRecipientsPrivateKeyRequirement::AnyManagedKey,
    ));
    let save_in_flight = Rc::new(Cell::new(false));
    let save_queued = Rc::new(Cell::new(false));
    let additional_fido2_save_guide_dialog = Rc::new(RefCell::new(None));
    let fido2_save_progress_dialog = Rc::new(RefCell::new(None));
    let key_rows = Rc::new(RefCell::new(Vec::new()));
    let git_rows = Rc::new(RefCell::new(Vec::new()));

    let state = StoreRecipientsPageState {
        window: widgets.window.clone(),
        nav: widgets.navigation_view.clone(),
        page: widgets.store_recipients_page.clone(),
        back_row: widgets.store_recipients_back_row.clone(),
        search: PreferencesPageSearchState::new(
            &widgets.store_recipients_preferences_page,
            &widgets.store_recipients_search_entry,
            Some(&widgets.store_recipients_search_empty_group),
            vec![
                SearchablePreferencesGroup::with_widgets(
                    &widgets.store_recipients_host_gpg_warning_group,
                    vec![widgets
                        .store_recipients_host_gpg_warning_row
                        .clone()
                        .upcast()],
                ),
                SearchablePreferencesGroup::with_widgets(
                    &widgets.store_recipients_fido2_info_group,
                    vec![widgets
                        .store_recipients_all_fido2_keys_required_row
                        .clone()
                        .upcast()],
                ),
                SearchablePreferencesGroup::with_widgets(
                    &widgets.store_recipients_scope_group,
                    vec![widgets.store_recipients_scope_row.clone().upcast()],
                ),
                SearchablePreferencesGroup::with_tracked_widgets(
                    &widgets.store_recipients_keys_group,
                    key_rows.clone(),
                ),
                SearchablePreferencesGroup::with_widgets(
                    &widgets.store_recipients_create_group,
                    vec![
                        widgets.store_recipients_generate_key_row.clone().upcast(),
                        widgets.store_recipients_add_fido2_key_row.clone().upcast(),
                    ],
                ),
                SearchablePreferencesGroup::with_widgets(
                    &widgets.store_recipients_add_group,
                    vec![
                        widgets
                            .store_recipients_generate_fido2_key_row
                            .clone()
                            .upcast(),
                        widgets
                            .store_recipients_setup_hardware_key_row
                            .clone()
                            .upcast(),
                        widgets
                            .store_recipients_add_hardware_key_row
                            .clone()
                            .upcast(),
                        widgets
                            .store_recipients_import_hardware_key_row
                            .clone()
                            .upcast(),
                        widgets
                            .store_recipients_import_clipboard_row
                            .clone()
                            .upcast(),
                        widgets.store_recipients_import_file_row.clone().upcast(),
                    ],
                ),
                SearchablePreferencesGroup::with_widgets(
                    &widgets.store_recipients_options_group,
                    vec![widgets.store_recipients_require_all_row.clone().upcast()],
                ),
                SearchablePreferencesGroup::with_tracked_widgets(
                    &widgets.store_recipients_git_group,
                    git_rows.clone(),
                ),
            ],
        ),
        list: widgets.store_recipients_list.clone(),
        platform,
        back: widgets.back_button.clone(),
        add: widgets.add_button.clone(),
        find: widgets.find_button.clone(),
        git: widgets.git_button.clone(),
        store: widgets.store_button.clone(),
        save: widgets.save_button.clone(),
        raw: widgets.open_raw_button.clone(),
        win: widgets.window_title.clone(),
        request,
        recipients,
        saved_recipients,
        recipient_scope_dirs,
        selected_recipient_scope,
        private_key_requirement,
        saved_private_key_requirement,
        save_in_flight,
        save_queued,
        additional_fido2_save_guide_dialog,
        fido2_save_progress_dialog,
        reopen_after_subpage: Rc::new(Cell::new(false)),
        key_rows,
        git_rows,
    };
    *state.platform.store_git_page.recipients_page.borrow_mut() = Some(state.clone());
    state
}

pub(super) fn store_recipients_page_state(
    widgets: &WindowWidgets,
    store_git_page: &StoreGitPageState,
) -> StoreRecipientsPageState {
    build_store_recipients_page_state(
        widgets,
        build_store_recipients_platform_state(widgets, store_git_page),
    )
}

pub(super) fn window_navigation_state(widgets: &WindowWidgets) -> WindowNavigationState {
    WindowNavigationState {
        nav: widgets.navigation_view.clone(),
        password_page: widgets.password_page.clone(),
        raw_text_page: widgets.raw_text_page.clone(),
        settings_page: widgets.settings_page.clone(),
        tools_page: widgets.tools_page.clone(),
        docs_page: widgets.docs_page.clone(),
        docs_detail_page: widgets.docs_detail_page.clone(),
        tools_field_values_page: widgets.tools_field_values_page.clone(),
        tools_value_values_page: widgets.tools_value_values_page.clone(),
        tools_weak_passwords_page: widgets.tools_weak_passwords_page.clone(),
        tools_audit_page: widgets.tools_audit_page.clone(),
        store_import_page: widgets.store_import_page.clone(),
        log_page: widgets.log_page.clone(),
        back: widgets.back_button.clone(),
        add: widgets.add_button.clone(),
        find: widgets.find_button.clone(),
        git: widgets.git_button.clone(),
        store: widgets.store_button.clone(),
        save: widgets.save_button.clone(),
        raw: widgets.open_raw_button.clone(),
        win: widgets.window_title.clone(),
        username: widgets.username_entry.clone(),
    }
}

pub(super) fn docs_page_state(
    widgets: &WindowWidgets,
    navigation: &WindowNavigationState,
) -> DocumentationPageState {
    DocumentationPageState::new(DocumentationPageWidgets::new(
        navigation,
        &widgets.docs_search_entry,
        &widgets.docs_list,
        &widgets.docs_detail_page,
        &widgets.docs_detail_scrolled,
        &widgets.docs_detail_box,
    ))
}

pub(super) fn tools_page_state(
    widgets: &WindowWidgets,
    navigation: &WindowNavigationState,
    password_page: &PasswordPageState,
) -> ToolsPageState {
    ToolsPageState::new(ToolsPageWidgets {
        window: &widgets.window,
        navigation,
        page: &widgets.tools_page,
        search_entry: &widgets.tools_search_entry,
        list: &widgets.tools_list,
        primary_group: &widgets.tools_primary_group,
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
        information_group: &widgets.tools_information_group,
        search_empty_group: &widgets.tools_search_empty_group,
        logs_list: &widgets.tools_logs_list,
        docs_row: &widgets.tools_docs_row,
        logs_row: &widgets.tools_logs_row,
        copy_logs_row: &widgets.tools_copy_logs_row,
        copy_logs_button: &widgets.tools_copy_logs_button,
        overlay: &widgets.toast_overlay,
        password_page,
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
    })
}

fn window_page_state(widgets: &WindowWidgets, page: &adw::NavigationPage) -> WindowPageState {
    WindowPageState {
        window: widgets.window.clone(),
        nav: widgets.navigation_view.clone(),
        page: page.clone(),
        back: widgets.back_button.clone(),
        add: widgets.add_button.clone(),
        find: widgets.find_button.clone(),
        git: widgets.git_button.clone(),
        store: widgets.store_button.clone(),
        save: widgets.save_button.clone(),
        raw: widgets.open_raw_button.clone(),
        win: widgets.window_title.clone(),
    }
}

pub(super) fn preferences_action_state(
    widgets: &WindowWidgets,
    recipients_page: &StoreRecipientsPageState,
) -> PreferencesActionState {
    let search = PreferencesPageSearchState::new(
        &widgets.settings_preferences_page,
        &widgets.settings_search_entry,
        Some(&widgets.settings_search_empty_group),
        vec![
            SearchablePreferencesGroup::with_list_box(
                &widgets.settings_store_list_group,
                &widgets.password_stores,
            ),
            SearchablePreferencesGroup::with_list_box(
                &widgets.settings_store_actions_group,
                &widgets.password_store_actions,
            ),
            SearchablePreferencesGroup::with_widgets(
                &widgets.backend_preferences,
                vec![
                    widgets.backend_row.clone().upcast(),
                    widgets.pass_command_row.clone().upcast(),
                    widgets.sync_private_keys_with_host_row.clone().upcast(),
                    widgets
                        .audit_use_commit_history_recipients_row
                        .clone()
                        .upcast(),
                ],
            ),
            SearchablePreferencesGroup::with_widgets(
                &widgets.host_access_preferences_group,
                Vec::new(),
            ),
            SearchablePreferencesGroup::with_widgets(
                &widgets.settings_username_group,
                vec![
                    widgets.preferences_username_filename_row.clone().upcast(),
                    widgets.preferences_username_folder_row.clone().upcast(),
                ],
            ),
            SearchablePreferencesGroup::with_widgets(
                &widgets.settings_password_list_group,
                vec![
                    widgets
                        .preferences_password_list_sort_filename_row
                        .clone()
                        .upcast(),
                    widgets
                        .preferences_password_list_sort_store_path_row
                        .clone()
                        .upcast(),
                ],
            ),
            SearchablePreferencesGroup::with_widgets(&widgets.settings_template_group, Vec::new()),
            SearchablePreferencesGroup::with_widgets(
                &widgets.settings_clear_empty_fields_group,
                vec![widgets.clear_empty_fields_before_save_row.clone().upcast()],
            ),
            SearchablePreferencesGroup::with_widgets(
                &widgets.settings_generator_group,
                vec![
                    widgets
                        .preferences_password_generator_length_row
                        .clone()
                        .upcast(),
                    widgets
                        .preferences_password_generator_min_lowercase_row
                        .clone()
                        .upcast(),
                    widgets
                        .preferences_password_generator_min_uppercase_row
                        .clone()
                        .upcast(),
                    widgets
                        .preferences_password_generator_min_numbers_row
                        .clone()
                        .upcast(),
                    widgets
                        .preferences_password_generator_min_symbols_row
                        .clone()
                        .upcast(),
                ],
            ),
        ],
    );
    PreferencesActionState {
        page_state: window_page_state(widgets, &widgets.settings_page),
        search,
        template_view: widgets.new_pass_file_template_view.clone(),
        clear_empty_fields_before_save_row: widgets.clear_empty_fields_before_save_row.clone(),
        clear_empty_fields_before_save_check: widgets.clear_empty_fields_before_save_check.clone(),
        username_folder_check: widgets.preferences_username_folder_check.clone(),
        username_filename_check: widgets.preferences_username_filename_check.clone(),
        password_list_sort_filename_check: widgets
            .preferences_password_list_sort_filename_check
            .clone(),
        password_list_sort_store_path_check: widgets
            .preferences_password_list_sort_store_path_check
            .clone(),
        generator_controls: PasswordGenerationControls::new(
            &widgets.preferences_password_generator_length_spin,
            &widgets.preferences_password_generator_min_lowercase_spin,
            &widgets.preferences_password_generator_min_uppercase_spin,
            &widgets.preferences_password_generator_min_numbers_spin,
            &widgets.preferences_password_generator_min_symbols_spin,
        ),
        stores_list: widgets.password_stores.clone(),
        store_actions_list: widgets.password_store_actions.clone(),
        overlay: widgets.toast_overlay.clone(),
        recipients_page: recipients_page.clone(),
        pass_row: widgets.pass_command_row.clone(),
        backend_row: widgets.backend_row.clone(),
        sync_private_keys_row: widgets.sync_private_keys_with_host_row.clone(),
        sync_private_keys_check: widgets.sync_private_keys_with_host_check.clone(),
        audit_use_commit_history_recipients_row: widgets
            .audit_use_commit_history_recipients_row
            .clone(),
        audit_use_commit_history_recipients_check: widgets
            .audit_use_commit_history_recipients_check
            .clone(),
    }
}

pub(super) fn build_git_action_state(
    widgets: &WindowWidgets,
    navigation: &WindowNavigationState,
    recipients_page: &StoreRecipientsPageState,
    store_git_page: &StoreGitPageState,
    visibility: &ListVisibilityState,
) -> GitActionState {
    GitActionState::new(
        widgets,
        navigation,
        recipients_page,
        store_git_page,
        visibility,
    )
}

fn build_back_action_platform_state(git_action_state: &GitActionState) -> PlatformBackActionState {
    PlatformBackActionState {
        git_actions: git_action_state.clone(),
    }
}

pub(super) fn back_action_state(
    password_page: &PasswordPageState,
    recipients_page: &StoreRecipientsPageState,
    store_git_page: &StoreGitPageState,
    navigation: &WindowNavigationState,
    visibility: &ListVisibilityState,
    git_action_state: &GitActionState,
) -> BackActionState {
    let platform = build_back_action_platform_state(git_action_state);

    BackActionState {
        password_page: password_page.clone(),
        recipients_page: recipients_page.clone(),
        store_git_page: store_git_page.clone(),
        navigation: navigation.clone(),
        visibility: visibility.clone(),
        platform,
    }
}

pub(super) fn list_visibility_action_state(
    widgets: &WindowWidgets,
    navigation: &WindowNavigationState,
    visibility: &ListVisibilityState,
) -> ListVisibilityActionState {
    ListVisibilityActionState {
        overlay: widgets.toast_overlay.clone(),
        list: widgets.list.clone(),
        navigation: navigation.clone(),
        visibility: visibility.clone(),
    }
}

pub(super) fn context_undo_action_state(
    password_page: &PasswordPageState,
    recipients_page: &StoreRecipientsPageState,
    store_git_page: &StoreGitPageState,
    navigation: &WindowNavigationState,
    visibility: &ListVisibilityState,
) -> ContextUndoActionState {
    ContextUndoActionState {
        password_page: password_page.clone(),
        recipients_page: recipients_page.clone(),
        store_git_page: store_git_page.clone(),
        navigation: navigation.clone(),
        visibility: visibility.clone(),
    }
}
