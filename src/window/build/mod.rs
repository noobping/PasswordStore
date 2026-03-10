mod actions;
mod state;
mod widgets;

#[cfg(feature = "setup")]
use crate::setup::*;
use crate::password::list::{load_passwords_async, setup_search_filter};
use crate::password::new_item::register_open_new_password_action;
use crate::password::otp::PasswordOtpState;
use crate::store::management::{
    connect_store_recipients_entry, register_store_recipients_save_action,
};
#[cfg(feature = "setup")]
use adw::gio::MenuItem;
use adw::gtk::Builder;
use adw::{prelude::*, Application, ApplicationWindow};
use std::cell::Cell;
use std::rc::Rc;

use self::actions::{
    connect_new_password_submit, connect_password_copy_buttons,
    connect_password_list_activation, register_password_page_actions,
};
use self::state::{
    back_action_state, hidden_entries_action_state, new_password_popover_state,
    password_page_state, preferences_action_state, store_recipients_page_state,
    window_navigation_state,
};
use super::controls::{
    apply_startup_query, configure_window_shortcuts, register_back_action,
    register_toggle_find_action, register_toggle_hidden_action,
};
#[cfg(feature = "flatpak")]
use super::flatpak::configure_flatpak_window;
use super::navigation::set_save_button_for_password;
use super::preferences::{
    connect_new_password_template_autosave, register_open_preferences_action,
};
#[cfg(feature = "setup")]
use super::preferences::register_install_locally_action;
#[cfg(not(feature = "flatpak"))]
use super::standard::{
    create_git_action_state, load_standard_window_parts, register_standard_window_actions,
};
use self::widgets::WindowWidgets;

const UI_SRC: &str = include_str!("../../../data/window.ui");

pub(crate) fn create_main_window(
    app: &Application,
    startup_query: Option<String>,
) -> ApplicationWindow {
    let builder = Builder::from_string(UI_SRC);
    let WindowWidgets {
        window,
        #[cfg(feature = "setup")]
        primary_menu,
        back_button,
        add_button,
        find_button,
        add_button_popover,
        new_password_store_box,
        new_password_store_list,
        path_entry,
        git_button,
        git_popover,
        window_title,
        save_button,
        toast_overlay,
        settings_page,
        store_recipients_page,
        store_recipients_list,
        log_page,
        new_pass_file_template_view,
        password_stores,
        navigation_view,
        search_entry,
        list,
        text_page,
        raw_text_page,
        password_status,
        password_entry,
        username_entry,
        otp_entry,
        copy_password_button,
        copy_username_button,
        copy_otp_button,
        text_view,
        dynamic_fields_box,
        open_raw_button,
    } = WindowWidgets::load(&builder);
    window.set_application(Some(app));

    #[cfg(feature = "setup")]
    if can_install_locally() {
        let item = MenuItem::new(
            Some(local_menu_action_label(is_installed_locally())),
            Some("win.install-locally"),
        );
        primary_menu.append_item(&item);
    }
    #[cfg(feature = "flatpak")]
    configure_flatpak_window(&builder);
    #[cfg(feature = "flatpak")]
    git_button.set_visible(false);
    set_save_button_for_password(&save_button);

    #[cfg(not(feature = "flatpak"))]
    let standard_parts = load_standard_window_parts(&builder);

    load_passwords_async(
        &list,
        git_button.clone(),
        find_button.clone(),
        save_button.clone(),
        toast_overlay.clone(),
        true,
        false,
    );
    let new_password_popover_state = new_password_popover_state(
        &add_button_popover,
        &path_entry,
        &new_password_store_box,
        &new_password_store_list,
    );
    let password_otp_state = PasswordOtpState::new(&otp_entry, &toast_overlay);
    let password_list_state = password_page_state(
        &navigation_view,
        &text_page,
        &raw_text_page,
        &list,
        &back_button,
        &add_button,
        &find_button,
        &git_button,
        &save_button,
        &window_title,
        &password_status,
        &password_entry,
        &username_entry,
        &password_otp_state,
        &dynamic_fields_box,
        &open_raw_button,
        &text_view,
        &toast_overlay,
    );
    let show_hidden_files = Rc::new(Cell::new(false));
    let store_recipients_page_state = store_recipients_page_state(
        &window,
        &navigation_view,
        &store_recipients_page,
        &store_recipients_list,
        &back_button,
        &add_button,
        &find_button,
        &git_button,
        &save_button,
        &window_title,
        &toast_overlay,
        #[cfg(not(feature = "flatpak"))]
        &standard_parts,
    );
    let window_navigation_state = window_navigation_state(
        &navigation_view,
        &text_page,
        &raw_text_page,
        &settings_page,
        &log_page,
        &back_button,
        &add_button,
        &find_button,
        &git_button,
        &save_button,
        &window_title,
        &username_entry,
    );
    let preferences_action_state = preferences_action_state(
        &window,
        &navigation_view,
        &settings_page,
        &back_button,
        &add_button,
        &find_button,
        &git_button,
        &save_button,
        &window_title,
        &new_pass_file_template_view,
        &password_stores,
        &toast_overlay,
        &store_recipients_page_state,
        #[cfg(not(feature = "flatpak"))]
        &standard_parts,
    );
    #[cfg(not(feature = "flatpak"))]
    let git_action_state = create_git_action_state(
        &standard_parts,
        &window,
        &toast_overlay,
        &list,
        &window_navigation_state,
        &store_recipients_page_state,
        &show_hidden_files,
    );
    let back_action_state = back_action_state(
        &password_list_state,
        &store_recipients_page_state,
        &window_navigation_state,
        &show_hidden_files,
        #[cfg(not(feature = "flatpak"))]
        &git_action_state,
    );
    let hidden_entries_action_state = hidden_entries_action_state(
        &toast_overlay,
        &list,
        &window_navigation_state,
        &show_hidden_files,
    );

    connect_password_list_activation(&list, &toast_overlay, &password_list_state);

    connect_new_password_template_autosave(&new_pass_file_template_view, &toast_overlay);
    connect_store_recipients_entry(&store_recipients_page_state);
    connect_password_copy_buttons(
        &toast_overlay,
        &password_entry,
        &copy_password_button,
        &username_entry,
        &copy_username_button,
        &otp_entry,
        &copy_otp_button,
    );
    connect_new_password_submit(
        &path_entry,
        &password_list_state,
        &new_password_popover_state,
        &add_button_popover,
        &git_popover,
    );
    register_password_page_actions(&window, &password_list_state);
    register_store_recipients_save_action(
        &window,
        &toast_overlay,
        &password_stores,
        &store_recipients_page_state,
    );
    register_open_preferences_action(&window, &preferences_action_state);

    #[cfg(not(feature = "flatpak"))]
    register_standard_window_actions(
        &standard_parts,
        &window,
        &toast_overlay,
        &window_navigation_state,
        &git_action_state,
        &git_popover,
    );

    #[cfg(feature = "setup")]
    register_install_locally_action(&window, &primary_menu, &toast_overlay);

    register_open_new_password_action(&window, &new_password_popover_state);
    register_toggle_find_action(&window, &search_entry);
    register_toggle_hidden_action(&window, &hidden_entries_action_state);
    register_back_action(&window, &back_action_state);

    configure_window_shortcuts(app);
    setup_search_filter(&list, &search_entry);
    apply_startup_query(startup_query, &search_entry, &list);

    window
}
