#[cfg(feature = "setup")]
use crate::setup::*;
use crate::clipboard::connect_copy_button;
use crate::password::file::{DynamicFieldRow, StructuredPassLine};
use crate::password::list::{load_passwords_async, setup_search_filter};
use crate::password::model::OpenPassFile;
use crate::password::new_item::{
    register_open_new_password_action, selected_new_password_store, NewPasswordPopoverState,
};
use crate::password::otp::PasswordOtpState;
use crate::password::page::{
    begin_new_password_entry, open_password_entry_page, save_current_password_entry,
    show_raw_pass_file_page, PasswordPageState,
};
use crate::store::management::{
    connect_store_recipients_entry, register_store_recipients_save_action,
    StoreRecipientsPageState, StoreRecipientsPlatformState, StoreRecipientsRequest,
};
use crate::support::object_data::non_null_to_string_option;
#[cfg(feature = "setup")]
use adw::gio::{Menu, MenuItem};
use adw::gio::{prelude::*, SimpleAction};
use adw::gtk::{Box as GtkBox, Builder, Button, ListBox, Popover, SearchEntry, TextView};
use adw::{
    prelude::*, Application, ApplicationWindow, EntryRow, NavigationPage, NavigationView,
    PasswordEntryRow, StatusPage, Toast, ToastOverlay, WindowTitle,
};
use std::cell::{Cell, RefCell};
use std::rc::Rc;

use super::controls::{
    apply_startup_query, configure_window_shortcuts, register_back_action,
    register_toggle_find_action, register_toggle_hidden_action, BackActionState,
    HiddenEntriesActionState, StandardBackActionState,
};
#[cfg(feature = "flatpak")]
use super::flatpak::configure_flatpak_window;
use super::navigation::{set_save_button_for_password, WindowNavigationState};
use super::preferences::{
    connect_new_password_template_autosave, register_open_preferences_action,
    PreferencesActionState,
};
#[cfg(feature = "setup")]
use super::preferences::register_install_locally_action;
#[cfg(not(feature = "flatpak"))]
use super::standard::{
    create_git_action_state, load_standard_window_parts, register_standard_window_actions,
};

const UI_SRC: &str = include_str!("../../data/window.ui");

pub(crate) fn create_main_window(
    app: &Application,
    startup_query: Option<String>,
) -> ApplicationWindow {
    let builder = Builder::from_string(UI_SRC);
    let window: ApplicationWindow = builder
        .object("main_window")
        .expect("Failed to get main_window from UI");
    window.set_application(Some(app));

    #[cfg(feature = "setup")]
    let primary_menu: Menu = builder
        .object("primary_menu")
        .expect("Failed to get primary menu");
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

    let back_button: Button = builder
        .object("back_button")
        .expect("Failed to get back_button");
    let add_button: Button = builder
        .object("add_button")
        .expect("Failed to get add_button");
    let find_button: Button = builder
        .object("find_button")
        .expect("Failed to get find_button");
    let add_button_popover: Popover = builder
        .object("add_button_popover")
        .expect("Failed to get add_button_popover");
    let new_password_store_box: GtkBox = builder
        .object("new_password_store_box")
        .expect("Failed to get new_password_store_box");
    let new_password_store_list: GtkBox = builder
        .object("new_password_store_list")
        .expect("Failed to get new_password_store_list");
    let path_entry: EntryRow = builder
        .object("path_entry")
        .expect("Failed to get path_entry");
    let git_button: Button = builder
        .object("git_button")
        .expect("Failed to get git_button");
    let git_popover: Popover = builder
        .object("git_popover")
        .expect("Failed to get git_popover");
    #[cfg(feature = "flatpak")]
    git_button.set_visible(false);
    let window_title: WindowTitle = builder
        .object("window_title")
        .expect("Failed to get window_title");
    let save_button: Button = builder
        .object("save_button")
        .expect("Failed to get save_button");
    set_save_button_for_password(&save_button);

    let toast_overlay: ToastOverlay = builder
        .object("toast_overlay")
        .expect("Failed to get toast_overlay");

    #[cfg(not(feature = "flatpak"))]
    let standard_parts = load_standard_window_parts(&builder);

    let settings_page: NavigationPage = builder
        .object("settings_page")
        .expect("Failed to get settings page");
    let store_recipients_page: NavigationPage = builder
        .object("store_recipients_page")
        .expect("Failed to get store recipients page");
    let store_recipients_list: ListBox = builder
        .object("store_recipients_list")
        .expect("Failed to get store recipients list");
    let log_page: NavigationPage = builder
        .object("log_page")
        .expect("Failed to get log page");
    let new_pass_file_template_view: TextView = builder
        .object("new_pass_file_template_view")
        .expect("Failed to get new_pass_file_template_view");
    let password_stores: ListBox = builder
        .object("password_stores")
        .expect("Failed to get the password store list");

    let navigation_view: NavigationView = builder
        .object("navigation_view")
        .expect("Failed to get navigation_view");
    let search_entry: SearchEntry = builder
        .object("search_entry")
        .expect("Failed to get search_entry");
    let list: ListBox = builder.object("list").expect("Failed to get list");

    load_passwords_async(
        &list,
        git_button.clone(),
        find_button.clone(),
        save_button.clone(),
        toast_overlay.clone(),
        true,
        false,
    );

    let text_page: NavigationPage = builder
        .object("text_page")
        .expect("Failed to get text_page");
    let raw_text_page: NavigationPage = builder
        .object("raw_text_page")
        .expect("Failed to get raw_text_page");
    let password_status: StatusPage = builder
        .object("password_status")
        .expect("Failed to get password_status");
    let password_entry: PasswordEntryRow = builder
        .object("password_entry")
        .expect("Failed to get password_entry");
    let username_entry: EntryRow = builder
        .object("username_entry")
        .expect("Failed to get username_entry");
    let otp_entry: PasswordEntryRow = builder
        .object("otp_entry")
        .expect("Failed to get otp_entry");
    let copy_password_button: Button = builder
        .object("copy_password_button")
        .expect("Failed to get copy_password_button");
    let copy_username_button: Button = builder
        .object("copy_username_button")
        .expect("Failed to get copy_username_button");
    let copy_otp_button: Button = builder
        .object("copy_otp_button")
        .expect("Failed to get copy_otp_button");
    let text_view: TextView = builder
        .object("text_view")
        .expect("Failed to get text_view");
    let dynamic_fields_box: GtkBox = builder
        .object("dynamic_fields_box")
        .expect("Failed to get dynamic_fields_box");
    let open_raw_button: Button = builder
        .object("open_raw_button")
        .expect("Failed to get open_raw_button");
    let structured_templates = Rc::new(RefCell::new(Vec::<StructuredPassLine>::new()));
    let dynamic_field_rows = Rc::new(RefCell::new(Vec::<DynamicFieldRow>::new()));
    let new_password_popover_state = NewPasswordPopoverState {
        popover: add_button_popover.clone(),
        path_entry: path_entry.clone(),
        store_box: new_password_store_box.clone(),
        store_list: new_password_store_list.clone(),
        store_roots: Rc::new(RefCell::new(Vec::new())),
        selected_store: Rc::new(RefCell::new(None)),
    };
    let password_otp_state = PasswordOtpState::new(&otp_entry, &toast_overlay);
    let password_list_state = PasswordPageState {
        nav: navigation_view.clone(),
        page: text_page.clone(),
        raw_page: raw_text_page.clone(),
        list: list.clone(),
        back: back_button.clone(),
        add: add_button.clone(),
        find: find_button.clone(),
        git: git_button.clone(),
        save: save_button.clone(),
        win: window_title.clone(),
        status: password_status.clone(),
        entry: password_entry.clone(),
        username: username_entry.clone(),
        otp: password_otp_state.clone(),
        dynamic_box: dynamic_fields_box.clone(),
        raw_button: open_raw_button.clone(),
        structured_templates: structured_templates.clone(),
        dynamic_rows: dynamic_field_rows.clone(),
        text: text_view.clone(),
        overlay: toast_overlay.clone(),
    };
    let store_recipients_request = Rc::new(RefCell::new(None::<StoreRecipientsRequest>));
    let store_recipients_values = Rc::new(RefCell::new(Vec::<String>::new()));
    let store_recipients_saved = Rc::new(RefCell::new(Vec::<String>::new()));
    let store_recipients_save_in_flight = Rc::new(Cell::new(false));
    let store_recipients_save_queued = Rc::new(Cell::new(false));
    let show_hidden_files = Rc::new(Cell::new(false));
    let store_recipients_page_state = StoreRecipientsPageState {
        window: window.clone(),
        nav: navigation_view.clone(),
        page: store_recipients_page.clone(),
        list: store_recipients_list.clone(),
        platform: {
            #[cfg(feature = "flatpak")]
            {
                StoreRecipientsPlatformState {
                    overlay: toast_overlay.clone(),
                }
            }
            #[cfg(not(feature = "flatpak"))]
            {
                StoreRecipientsPlatformState {
                    entry: standard_parts.store_recipients_entry.clone(),
                }
            }
        },
        back: back_button.clone(),
        add: add_button.clone(),
        find: find_button.clone(),
        git: git_button.clone(),
        save: save_button.clone(),
        win: window_title.clone(),
        request: store_recipients_request.clone(),
        recipients: store_recipients_values.clone(),
        saved_recipients: store_recipients_saved.clone(),
        save_in_flight: store_recipients_save_in_flight.clone(),
        save_queued: store_recipients_save_queued.clone(),
    };
    let window_navigation_state = WindowNavigationState {
        nav: navigation_view.clone(),
        text_page: text_page.clone(),
        raw_text_page: raw_text_page.clone(),
        settings_page: settings_page.clone(),
        log_page: log_page.clone(),
        back: back_button.clone(),
        add: add_button.clone(),
        find: find_button.clone(),
        git: git_button.clone(),
        save: save_button.clone(),
        win: window_title.clone(),
        username: username_entry.clone(),
    };
    let preferences_action_state = PreferencesActionState {
        window: window.clone(),
        nav: navigation_view.clone(),
        page: settings_page.clone(),
        back: back_button.clone(),
        add: add_button.clone(),
        find: find_button.clone(),
        git: git_button.clone(),
        save: save_button.clone(),
        win: window_title.clone(),
        template_view: new_pass_file_template_view.clone(),
        stores_list: password_stores.clone(),
        overlay: toast_overlay.clone(),
        recipients_page: store_recipients_page_state.clone(),
        #[cfg(not(feature = "flatpak"))]
        pass_row: standard_parts.pass_row.clone(),
        #[cfg(not(feature = "flatpak"))]
        backend_row: standard_parts.backend_row.clone(),
    };
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
    let back_action_state = BackActionState {
        password_page: password_list_state.clone(),
        recipients_page: store_recipients_page_state.clone(),
        navigation: window_navigation_state.clone(),
        show_hidden: show_hidden_files.clone(),
        #[cfg(not(feature = "flatpak"))]
        platform: StandardBackActionState {
            git_actions: git_action_state.clone(),
        },
        #[cfg(feature = "flatpak")]
        platform: StandardBackActionState,
    };
    let hidden_entries_action_state = HiddenEntriesActionState {
        overlay: toast_overlay.clone(),
        list: list.clone(),
        navigation: window_navigation_state.clone(),
        show_hidden: show_hidden_files.clone(),
    };

    {
        let overlay = toast_overlay.clone();
        let list_state = password_list_state.clone();
        list.connect_row_activated(move |_list, row| {
            let label = non_null_to_string_option(row, "label");
            let root = non_null_to_string_option(row, "root");

            let Some(label) = label else {
                overlay.add_toast(Toast::new("Couldn't open that item."));
                return;
            };
            let Some(root) = root else {
                overlay.add_toast(Toast::new("That item is missing its store."));
                return;
            };
            let opened_pass_file = OpenPassFile::from_label(root, &label);
            open_password_entry_page(&list_state, opened_pass_file, true);
        });
    }

    connect_new_password_template_autosave(&new_pass_file_template_view, &toast_overlay);
    connect_store_recipients_entry(&store_recipients_page_state);

    {
        let entry = password_entry.clone();
        let btn = copy_password_button.clone();
        connect_copy_button(&btn, &toast_overlay, move || entry.text().to_string());
    }
    {
        let entry = username_entry.clone();
        let btn = copy_username_button.clone();
        connect_copy_button(&btn, &toast_overlay, move || entry.text().to_string());
    }
    {
        let entry = otp_entry.clone();
        let btn = copy_otp_button.clone();
        connect_copy_button(&btn, &toast_overlay, move || entry.text().to_string());
    }
    {
        let popover_add = add_button_popover.clone();
        let popover_git = git_popover.clone();
        let page_state = password_list_state.clone();
        let popover_state = new_password_popover_state.clone();
        path_entry.connect_apply(move |row| {
            begin_new_password_entry(
                &page_state,
                &row.text(),
                selected_new_password_store(&popover_state),
                &popover_add,
                &popover_git,
            );
        });
    }

    {
        let page_state = password_list_state.clone();
        let action = SimpleAction::new("save-password", None);
        action.connect_activate(move |_, _| {
            save_current_password_entry(&page_state);
        });
        window.add_action(&action);
    }
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

    {
        let page_state = password_list_state.clone();
        let action = SimpleAction::new("open-raw-pass-file", None);
        action.connect_activate(move |_, _| {
            show_raw_pass_file_page(&page_state);
        });
        window.add_action(&action);
    }

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
