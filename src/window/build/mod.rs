mod actions;
mod state;
pub(super) mod widgets;

use crate::password::list::{load_passwords_async, setup_search_filter, PasswordListActions};
use crate::password::new_item::NewPasswordPopoverState;
use crate::password::new_item::register_open_new_password_action;
use crate::password::otp::PasswordOtpState;
use crate::password::page::PasswordPageState;
#[cfg(feature = "setup")]
use crate::setup::*;
#[cfg(feature = "flatpak")]
use crate::store::management::register_open_store_picker_action;
use crate::store::management::{
    connect_store_recipients_entry, register_store_recipients_save_action,
    StoreRecipientsPageState,
};
#[cfg(feature = "setup")]
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
    preferences_action_state,
    store_recipients_page_state, window_navigation_state,
};
use self::widgets::WindowWidgets;
use super::controls::{
    apply_startup_query, configure_window_shortcuts, register_back_action,
    register_context_save_action, register_context_undo_action, register_list_visibility_action,
    register_toggle_find_action, ListVisibilityState,
};
#[cfg(feature = "flatpak")]
use super::flatpak::configure_flatpak_window;
use super::git::{register_open_git_action, register_synchronize_action};
use super::logs::{register_open_log_action, start_log_poller};
use super::navigation::set_save_button_for_password;
#[cfg(feature = "setup")]
use super::preferences::register_install_locally_action;
use super::preferences::{
    connect_new_password_template_autosave, connect_password_generation_autosave,
    connect_username_fallback_autosave, register_open_preferences_action, PreferencesActionState,
};
#[cfg(not(feature = "flatpak"))]
use super::standard::{configure_standard_window, register_standard_window_actions};

const UI_SRC: &str = include_str!("../../../data/window.ui");

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

    #[cfg(feature = "setup")]
    if can_install_locally() {
        let item = MenuItem::new(
            Some(local_menu_action_label(is_installed_locally())),
            Some("win.install-locally"),
        );
        widgets.primary_menu.append_item(&item);
    }
    #[cfg(feature = "flatpak")]
    configure_flatpak_window(&widgets);
    set_save_button_for_password(&widgets.save_button);

    #[cfg(not(feature = "flatpak"))]
    let standard_window = configure_standard_window(&widgets);

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
    let store_recipients_page_state = store_recipients_page_state(
        &widgets,
        #[cfg(not(feature = "flatpak"))]
        &standard_window.store_recipients_entry,
    );
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
    register_open_git_action(&git_action_state);
    register_synchronize_action(&git_action_state);
    register_open_log_action(&widgets.window, &window_navigation_state);
    start_log_poller(&widgets.log_view, &window_navigation_state);
    #[cfg(feature = "flatpak")]
    register_open_store_picker_action(
        &widgets.window,
        &widgets.password_stores,
        &widgets.toast_overlay,
        &store_recipients_page_state,
    );
    register_open_preferences_action(&widgets.window, &preferences_action_state);

    #[cfg(not(feature = "flatpak"))]
    register_standard_window_actions(&standard_window, &widgets, &widgets.toast_overlay);

    #[cfg(feature = "setup")]
    register_install_locally_action(
        &widgets.window,
        &widgets.primary_menu,
        &widgets.toast_overlay,
    );

    register_open_new_password_action(&widgets.window, &new_password_popover_state);
    register_context_save_action(
        &widgets.window,
        &window_navigation_state,
        &store_recipients_page_state,
    );
    register_context_undo_action(&widgets.window, &context_undo_state);
    register_toggle_find_action(&widgets.window, &widgets.search_entry);
    register_list_visibility_action(&widgets.window, &list_visibility_action_state);
    register_back_action(&widgets.window, &back_action_state);

    configure_window_shortcuts(app);
    setup_search_filter(&widgets.list, &widgets.search_entry);
    apply_startup_query(startup_query, &widgets.search_entry, &widgets.list);

    widgets.window
}
