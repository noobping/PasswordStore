use super::widgets::WindowWidgets;
use crate::backend::StoreRecipientsPrivateKeyRequirement;
use crate::password::file::{DynamicFieldRow, StructuredPassLine};
use crate::password::generation::PasswordGenerationControls;
use crate::password::new_item::NewPasswordPopoverState;
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
use crate::window::git::GitActionState;
use crate::window::navigation::{WindowNavigationState, WindowPageState};
use crate::window::preferences::PreferencesActionState;
use std::cell::{Cell, RefCell};
use std::rc::Rc;

pub(super) fn new_password_popover_state(_widgets: &WindowWidgets) -> NewPasswordPopoverState {
    let (dialog, store_dropdown, path_entry, error_label) =
        crate::password::new_item::build_new_password_dialog();
    NewPasswordPopoverState {
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
        page: widgets.text_page.clone(),
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
        username: widgets.username_entry.clone(),
        otp: otp.clone(),
        field_add_row: widgets.add_field_row.clone(),
        template_button: widgets.apply_template_button.clone(),
        clean_button: widgets.clean_pass_file_button.clone(),
        otp_add_button: widgets.add_otp_button.clone(),
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
    StoreGitPageState {
        window: widgets.window.clone(),
        nav: widgets.navigation_view.clone(),
        page: widgets.store_git_page.clone(),
        remotes_list: widgets.store_git_remotes_list.clone(),
        actions_list: widgets.store_git_actions_list.clone(),
        status_list: widgets.store_git_status_list.clone(),
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
    }
}

fn build_store_recipients_platform_state(
    widgets: &WindowWidgets,
    store_git_page: &StoreGitPageState,
) -> StoreRecipientsPlatformState {
    StoreRecipientsPlatformState {
        overlay: widgets.toast_overlay.clone(),
        host_gpg_warning_group: widgets.store_recipients_host_gpg_warning_group.clone(),
        host_gpg_warning_row: widgets.store_recipients_host_gpg_warning_row.clone(),
        add_group: widgets.store_recipients_add_group.clone(),
        create_group: widgets.store_recipients_create_group.clone(),
        options_group: widgets.store_recipients_options_group.clone(),
        git_group: widgets.store_recipients_git_group.clone(),
        git_list: widgets.store_recipients_git_list.clone(),
        add_hardware_key_row: widgets.store_recipients_add_hardware_key_row.clone(),
        store_git_page: store_git_page.clone(),
        import_hardware_key_row: widgets.store_recipients_import_hardware_key_row.clone(),
        import_clipboard_row: widgets.store_recipients_import_clipboard_row.clone(),
        import_file_row: widgets.store_recipients_import_file_row.clone(),
        generate_key_row: widgets.store_recipients_generate_key_row.clone(),
        require_all_row: widgets.store_recipients_require_all_row.clone(),
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
    }
}

fn build_store_recipients_page_state(
    widgets: &WindowWidgets,
    platform: StoreRecipientsPlatformState,
) -> StoreRecipientsPageState {
    let request = Rc::new(RefCell::new(None::<StoreRecipientsRequest>));
    let recipients = Rc::new(RefCell::new(Vec::<String>::new()));
    let saved_recipients = Rc::new(RefCell::new(Vec::<String>::new()));
    let private_key_requirement = Rc::new(Cell::new(
        StoreRecipientsPrivateKeyRequirement::AnyManagedKey,
    ));
    let saved_private_key_requirement = Rc::new(Cell::new(
        StoreRecipientsPrivateKeyRequirement::AnyManagedKey,
    ));
    let save_in_flight = Rc::new(Cell::new(false));
    let save_queued = Rc::new(Cell::new(false));

    StoreRecipientsPageState {
        window: widgets.window.clone(),
        nav: widgets.navigation_view.clone(),
        page: widgets.store_recipients_page.clone(),
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
        private_key_requirement,
        saved_private_key_requirement,
        save_in_flight,
        save_queued,
    }
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
        text_page: widgets.text_page.clone(),
        raw_text_page: widgets.raw_text_page.clone(),
        settings_page: widgets.settings_page.clone(),
        tools_page: widgets.tools_page.clone(),
        docs_page: widgets.docs_page.clone(),
        docs_detail_page: widgets.docs_detail_page.clone(),
        tools_field_values_page: widgets.tools_field_values_page.clone(),
        tools_value_values_page: widgets.tools_value_values_page.clone(),
        tools_weak_passwords_page: widgets.tools_weak_passwords_page.clone(),
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
    PreferencesActionState {
        page_state: window_page_state(widgets, &widgets.settings_page),
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
