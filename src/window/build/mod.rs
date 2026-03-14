mod actions;
mod state;
pub(super) mod widgets;

use crate::password::list::{load_passwords_async, setup_search_filter, PasswordListActions};
use crate::password::new_item::register_open_new_password_action;
use crate::password::new_item::NewPasswordPopoverState;
use crate::password::otp::PasswordOtpState;
use crate::password::page::PasswordPageState;
use crate::preferences::Preferences;
#[cfg(keycord_setup)]
use crate::setup::*;
#[cfg(keycord_restricted)]
use crate::store::management::register_open_store_picker_action;
use crate::store::management::{
    connect_store_recipients_entry, register_store_recipients_save_action, StoreRecipientsPageState,
};
#[cfg(keycord_setup)]
use adw::gio::MenuItem;
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
    apply_startup_query, configure_window_shortcuts, register_back_action,
    register_context_save_action, register_context_undo_action, register_list_visibility_action,
    register_reload_password_list_action, register_toggle_find_action, ListVisibilityState,
};
#[cfg(keycord_flatpak)]
use super::flatpak::configure_flatpak_window;
use super::git::GitActionState;
#[cfg(keycord_linux)]
use super::git::{
    register_open_git_action, register_synchronize_action, set_git_action_availability,
};
#[cfg(keycord_standard_linux)]
use super::logs::append_debug_log_menu_item;
#[cfg(keycord_linux)]
use super::logs::{register_open_log_action, start_log_poller};
use super::navigation::{set_save_button_for_password, WindowNavigationState};
#[cfg(not(keycord_linux))]
use super::non_linux::configure_non_linux_window;
#[cfg(keycord_setup)]
use super::preferences::register_install_locally_action;
#[cfg(keycord_linux)]
use super::preferences::{connect_backend_row, connect_pass_command_row, initialize_backend_row};
use super::preferences::{
    connect_new_password_template_autosave, connect_password_generation_autosave,
    connect_username_fallback_autosave, register_open_preferences_action, PreferencesActionState,
};
#[cfg(keycord_standard_linux)]
use super::standard::configure_standard_window;
use crate::logging::log_info;
use crate::support::runtime::log_runtime_capabilities_once;
#[cfg(keycord_flatpak)]
use crate::support::runtime::{git_network_operations_available, host_command_execution_available};

const UI_SRC: &str = include_str!(concat!(env!("OUT_DIR"), "/window.ui"));

#[cfg(keycord_setup)]
fn append_setup_menu_item_if_available(widgets: &WindowWidgets) {
    if can_install_locally() {
        let item = MenuItem::new(
            Some(local_menu_action_label(is_installed_locally())),
            Some("win.install-locally"),
        );
        widgets.primary_menu.append_item(&item);
    }
}

#[cfg(not(keycord_setup))]
fn append_setup_menu_item_if_available(_widgets: &WindowWidgets) {}

#[cfg(keycord_flatpak)]
fn configure_platform_window(widgets: &WindowWidgets) {
    configure_flatpak_window(widgets);
}

#[cfg(not(keycord_linux))]
fn configure_platform_window(widgets: &WindowWidgets) {
    configure_non_linux_window(widgets);
}

#[cfg(keycord_standard_linux)]
fn configure_platform_window(widgets: &WindowWidgets) {
    append_debug_log_menu_item(&widgets.primary_menu);
}

#[cfg(keycord_restricted)]
fn build_store_recipients_page_state(widgets: &WindowWidgets) -> StoreRecipientsPageState {
    store_recipients_page_state(widgets)
}

#[cfg(keycord_standard_linux)]
fn build_store_recipients_page_state(widgets: &WindowWidgets) -> StoreRecipientsPageState {
    let standard_window = configure_standard_window(widgets);
    store_recipients_page_state(widgets, &standard_window.store_recipients_entry)
}

#[cfg(keycord_restricted)]
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

#[cfg(keycord_standard_linux)]
fn register_platform_window_actions(
    _widgets: &WindowWidgets,
    _recipients_page: &StoreRecipientsPageState,
) {
}

#[cfg(keycord_setup)]
fn register_setup_action_if_available(widgets: &WindowWidgets) {
    register_install_locally_action(
        &widgets.window,
        &widgets.primary_menu,
        &widgets.toast_overlay,
    );
}

#[cfg(not(keycord_setup))]
fn register_setup_action_if_available(_widgets: &WindowWidgets) {}

#[cfg(keycord_flatpak)]
fn platform_git_actions_available() -> bool {
    git_network_operations_available()
}

#[cfg(keycord_standard_linux)]
fn platform_git_actions_available() -> bool {
    true
}

#[cfg(not(keycord_linux))]
fn register_platform_git_actions(_widgets: &WindowWidgets, _git_action_state: &GitActionState) {
    log_info(
        "Window Git actions: open-git, git-clone, and synchronize are disabled on non-Linux builds."
            .to_string(),
    );
}

#[cfg(keycord_linux)]
fn register_platform_git_actions(widgets: &WindowWidgets, git_action_state: &GitActionState) {
    register_open_git_action(git_action_state);
    register_synchronize_action(git_action_state);
    let git_available = platform_git_actions_available();
    set_git_action_availability(&widgets.window, git_available);
    log_info(format!(
        "Window Git actions: open-git, git-clone, and synchronize are {}.",
        if git_available { "enabled" } else { "disabled" }
    ));
}

#[cfg(not(keycord_linux))]
fn register_platform_log_actions(
    _widgets: &WindowWidgets,
    _navigation_state: &WindowNavigationState,
) {
}

#[cfg(keycord_linux)]
fn register_platform_log_actions(
    widgets: &WindowWidgets,
    navigation_state: &WindowNavigationState,
) {
    register_open_log_action(&widgets.window, navigation_state);
    start_log_poller(&widgets.log_view);
}

#[cfg(keycord_flatpak)]
fn initialize_backend_preferences(widgets: &WindowWidgets, preferences: &Preferences) {
    widgets
        .backend_preferences
        .set_visible(host_command_execution_available());
    initialize_backend_row(&widgets.backend_row, &widgets.pass_command_row, preferences);
}

#[cfg(keycord_standard_linux)]
fn initialize_backend_preferences(widgets: &WindowWidgets, preferences: &Preferences) {
    widgets.backend_preferences.set_visible(true);
    initialize_backend_row(&widgets.backend_row, &widgets.pass_command_row, preferences);
}

#[cfg(not(keycord_linux))]
fn initialize_backend_preferences(_widgets: &WindowWidgets, _preferences: &Preferences) {}

#[cfg(keycord_linux)]
fn connect_backend_preferences(widgets: &WindowWidgets, preferences: &Preferences) {
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
    );
}

#[cfg(not(keycord_linux))]
fn connect_backend_preferences(_widgets: &WindowWidgets, _preferences: &Preferences) {}

fn connect_window_behaviors(
    widgets: &WindowWidgets,
    password_list_state: &PasswordPageState,
    preferences_action_state: &PreferencesActionState,
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
    let backend_preferences = Preferences::new();
    connect_backend_preferences(widgets, &backend_preferences);
    connect_store_recipients_entry(store_recipients_page_state);
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

pub(crate) fn create_main_window(
    app: &Application,
    startup_query: Option<String>,
) -> ApplicationWindow {
    let builder = Builder::from_string(UI_SRC);
    let widgets = WindowWidgets::load(&builder);
    widgets.window.set_application(Some(app));
    log_runtime_capabilities_once();
    configure_platform_window(&widgets);
    append_setup_menu_item_if_available(&widgets);
    let backend_preferences = Preferences::new();
    initialize_backend_preferences(&widgets, &backend_preferences);
    set_save_button_for_password(&widgets.save_button);

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
    let new_password_popover_state = new_password_popover_state(&widgets);
    let password_otp_state = PasswordOtpState::new(&widgets.otp_entry, &widgets.toast_overlay);
    let password_list_state = password_page_state(&widgets, &password_otp_state);
    let list_visibility = ListVisibilityState::new(false, false);
    let store_recipients_page_state = build_store_recipients_page_state(&widgets);
    let window_navigation_state = window_navigation_state(&widgets);
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
        &password_list_state,
        &preferences_action_state,
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
    register_platform_git_actions(&widgets, &git_action_state);
    register_platform_log_actions(&widgets, &window_navigation_state);
    register_platform_window_actions(&widgets, &store_recipients_page_state);
    register_open_preferences_action(&widgets.window, &preferences_action_state);
    register_setup_action_if_available(&widgets);

    register_open_new_password_action(&widgets.window, &new_password_popover_state);
    register_context_save_action(
        &widgets.window,
        &window_navigation_state,
        &store_recipients_page_state,
    );
    register_context_undo_action(&widgets.window, &context_undo_state);
    register_toggle_find_action(&widgets.window, &widgets.search_entry);
    register_list_visibility_action(&widgets.window, &list_visibility_action_state);
    register_reload_password_list_action(&widgets.window, &list_visibility_action_state);
    register_back_action(&widgets.window, &back_action_state);

    configure_window_shortcuts(app);
    setup_search_filter(&widgets.list, &widgets.search_entry);
    apply_startup_query(startup_query, &widgets.search_entry, &widgets.list);

    widgets.window
}
