#[cfg(feature = "setup")]
use crate::setup::*;
use crate::clipboard::connect_copy_button;
#[cfg(any(feature = "setup", feature = "flatpak"))]
use adw::gio::Menu;
#[cfg(feature = "setup")]
use adw::gio::MenuItem;
#[cfg(not(feature = "flatpak"))]
use adw::ComboRow;

use crate::item::OpenPassFile;
use crate::methods::non_null_to_string_option;
use crate::pass_file::{DynamicFieldRow, StructuredPassLine};
use crate::password_list::{load_passwords_async, setup_search_filter};
use crate::password_otp::PasswordOtpState;
use crate::password_page::{
    begin_new_password_entry, open_password_entry_page, save_current_password_entry,
    show_raw_pass_file_page, PasswordPageState,
};
#[cfg(not(feature = "flatpak"))]
use crate::preferences::Preferences;
#[cfg(feature = "flatpak")]
use crate::ripasso_keys::RipassoPrivateKeysState;
use crate::store_management::{
    connect_store_recipients_entry, register_store_recipients_save_action,
    StoreRecipientsPageState, StoreRecipientsRequest,
};
use crate::window_controls::{
    apply_startup_query, configure_window_shortcuts, register_back_action,
    register_open_new_password_action, register_toggle_find_action,
    register_toggle_hidden_action, BackActionState, HiddenEntriesActionState,
};
#[cfg(not(feature = "flatpak"))]
use crate::window_logs::{register_open_log_action, start_log_poller};
use crate::window_preferences::{
    connect_new_password_template_autosave, register_open_preferences_action,
    PreferencesActionState,
};
#[cfg(not(feature = "flatpak"))]
use crate::window_preferences::connect_pass_command_row;
#[cfg(not(feature = "flatpak"))]
use crate::window_preferences::{connect_backend_row, initialize_backend_row};
#[cfg(feature = "setup")]
use crate::window_preferences::register_install_locally_action;
#[cfg(not(feature = "flatpak"))]
use crate::window_git::{
    connect_git_clone_apply, register_git_clone_action, register_open_git_action,
    register_synchronize_action, GitActionState, GitOperationControl,
};
use crate::window_navigation::{set_save_button_for_password, WindowNavigationState};
use adw::gio::{prelude::*, SimpleAction};
use adw::{
    prelude::*, Application, ApplicationWindow, EntryRow, NavigationPage, NavigationView,
    PasswordEntryRow, StatusPage, Toast, ToastOverlay, WindowTitle,
};
#[cfg(feature = "flatpak")]
use adw::gtk::MenuButton;
use adw::gtk::{
    Box as GtkBox, Builder, Button, ListBox, Popover, SearchEntry, TextView,
};
use std::cell::{Cell, RefCell};
use std::rc::Rc;

const UI_SRC: &str = include_str!("../data/window.ui");

pub fn create_main_window(app: &Application, startup_query: Option<String>) -> ApplicationWindow {
    let builder = Builder::from_string(UI_SRC);
    let window: ApplicationWindow = builder
        .object("main_window")
        .expect("Failed to get main_window from UI");
    window.set_application(Some(app));

    #[cfg(feature = "flatpak")]
    let primary_menu_button: MenuButton = builder
        .object("primary_menu_button")
        .expect("Failed to get primary menu button");
    #[cfg(feature = "setup")]
    let primary_menu: Menu = builder
        .object("primary_menu")
        .expect("Failed to get primary menu");
    #[cfg(feature = "setup")]
    if can_install_locally() {
        let item = if is_installed_locally() {
            MenuItem::new(Some("Uninstall this App"), Some("win.install-locally"))
        } else {
            MenuItem::new(Some("Install this App"), Some("win.install-locally"))
        };
        primary_menu.append_item(&item);
    }
    #[cfg(feature = "flatpak")]
    {
        let menu = Menu::new();
        menu.append(Some("_Find item"), Some("win.toggle-find"));
        #[cfg(not(feature = "flatpak"))]
        menu.append(Some("_Logs"), Some("win.open-log"));
        menu.append(Some("_Preferences"), Some("win.open-preferences"));
        menu.append(Some("_About PasswordStore"), Some("app.about"));
        primary_menu_button.set_menu_model(Some(&menu));
    }

    #[cfg(not(feature = "flatpak"))]
    let backend_preferences: adw::PreferencesGroup = builder
        .object("backend_preferences")
        .expect("Failed to get backend_preferences");
    #[cfg(feature = "flatpak")]
    let ripasso_private_keys_preferences: adw::PreferencesGroup = builder
        .object("ripasso_private_keys_preferences")
        .expect("Failed to get ripasso_private_keys_preferences");
    #[cfg(feature = "flatpak")]
    let ripasso_private_keys_list: ListBox = builder
        .object("ripasso_private_keys_list")
        .expect("Failed to get ripasso_private_keys_list");
    #[cfg(not(feature = "flatpak"))]
    let backend_row: ComboRow = builder
        .object("backend_row")
        .expect("Failed to get backend_row");
    #[cfg(not(feature = "flatpak"))]
    backend_preferences.set_visible(true);
    #[cfg(feature = "flatpak")]
    ripasso_private_keys_preferences.set_visible(true);

    // Headerbar + top controls
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
    let path_entry: EntryRow = builder
        .object("path_entry")
        .expect("Failed to get path_entry");
    let git_button: Button = builder
        .object("git_button")
        .expect("Failed to get git_button");
    let git_popover: Popover = builder
        .object("git_popover")
        .expect("Failed to get git_popover");
    #[cfg(not(feature = "flatpak"))]
    let git_url_entry: EntryRow = builder
        .object("git_url_entry")
        .expect("Failed to get git_url_entry");
    #[cfg(feature = "flatpak")]
    git_button.set_visible(false);
    let window_title: WindowTitle = builder
        .object("window_title")
        .expect("Failed to get window_title");
    let save_button: Button = builder
        .object("save_button")
        .expect("Failed to get save_button");
    set_save_button_for_password(&save_button);
    #[cfg(not(feature = "flatpak"))]
    let git_operation = GitOperationControl::default();

    // Toast overlay
    let toast_overlay: ToastOverlay = builder
        .object("toast_overlay")
        .expect("Failed to get toast_overlay");

    // Settings
    #[cfg(not(feature = "flatpak"))]
    let settings = Preferences::new();
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
    #[cfg(not(feature = "flatpak"))]
    let git_busy_page: NavigationPage = builder
        .object("git_busy_page")
        .expect("Failed to get git busy page");
    #[cfg(not(feature = "flatpak"))]
    let git_busy_status: StatusPage = builder
        .object("git_busy_status")
        .expect("Failed to get git busy status");
    #[cfg(not(feature = "flatpak"))]
    let pass_row: EntryRow = builder
        .object("pass_command_row")
        .expect("Failed to get pass row");
    #[cfg(not(feature = "flatpak"))]
    initialize_backend_row(&backend_row, &pass_row, &settings);
    let new_pass_file_template_view: TextView = builder
        .object("new_pass_file_template_view")
        .expect("Failed to get new_pass_file_template_view");
    let password_stores: ListBox = builder
        .object("password_stores")
        .expect("Failed to get the password store list");

    // Navigation
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

    // Text editor page
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
    #[cfg(not(feature = "flatpak"))]
    let log_view: TextView = builder
        .object("log_view")
        .expect("Failed to get log_view");
    let structured_templates = Rc::new(RefCell::new(Vec::<StructuredPassLine>::new()));
    let dynamic_field_rows = Rc::new(RefCell::new(Vec::<DynamicFieldRow>::new()));
    let store_recipients_entry = EntryRow::new();
    store_recipients_entry.set_title("Add recipient");
    store_recipients_entry.set_show_apply_button(true);
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
        entry: store_recipients_entry.clone(),
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
    #[cfg(feature = "flatpak")]
    let ripasso_private_keys_state = RipassoPrivateKeysState {
        window: window.clone(),
        list: ripasso_private_keys_list.clone(),
        overlay: toast_overlay.clone(),
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
        pass_row: pass_row.clone(),
        #[cfg(not(feature = "flatpak"))]
        backend_row: backend_row.clone(),
        #[cfg(feature = "flatpak")]
        ripasso_keys_state: ripasso_private_keys_state.clone(),
    };
    #[cfg(not(feature = "flatpak"))]
    let git_action_state = GitActionState {
        window: window.clone(),
        overlay: toast_overlay.clone(),
        list: list.clone(),
        navigation: window_navigation_state.clone(),
        recipients_page: store_recipients_page_state.clone(),
        busy_page: git_busy_page.clone(),
        busy_status: git_busy_status.clone(),
        show_hidden: show_hidden_files.clone(),
    };
    let back_action_state = BackActionState {
        password_page: password_list_state.clone(),
        recipients_page: store_recipients_page_state.clone(),
        navigation: window_navigation_state.clone(),
        show_hidden: show_hidden_files.clone(),
        #[cfg(not(feature = "flatpak"))]
        git_actions: git_action_state.clone(),
    };
    let hidden_entries_action_state = HiddenEntriesActionState {
        overlay: toast_overlay.clone(),
        list: list.clone(),
        navigation: window_navigation_state.clone(),
        show_hidden: show_hidden_files.clone(),
    };

    // Selecting an item from the list
    {
        let overlay = toast_overlay.clone();
        let list_state = password_list_state.clone();
        list.connect_row_activated(move |_list, row| {
            let label = non_null_to_string_option(row, "label");
            let root = non_null_to_string_option(row, "root");

            let Some(label) = label else {
                let toast = Toast::new("Couldn't open that item.");
                overlay.add_toast(toast);
                return;
            };
            let Some(root) = root else {
                let toast = Toast::new("That item is missing its store.");
                overlay.add_toast(toast);
                return;
            };
            let opened_pass_file = OpenPassFile::from_label(root, &label);
            open_password_entry_page(&list_state, opened_pass_file, true);
        });
    }

    // Pass command preference
    #[cfg(not(feature = "flatpak"))]
    {
        connect_pass_command_row(&pass_row, &toast_overlay, &settings);
    }
    #[cfg(not(feature = "flatpak"))]
    {
        connect_backend_row(&backend_row, &pass_row, &toast_overlay, &settings);
    }
    connect_new_password_template_autosave(&new_pass_file_template_view, &toast_overlay);
    connect_store_recipients_entry(&store_recipients_page_state);
    // Copy password button on password page
    {
        let entry = password_entry.clone();
        let btn = copy_password_button.clone();
        connect_copy_button(&btn, &toast_overlay, move || {
            entry.grab_focus_without_selecting();
            entry.text().to_string()
        });
    }
    // Copy username button on password page
    {
        let entry = username_entry.clone();
        let btn = copy_username_button.clone();
        connect_copy_button(&btn, &toast_overlay, move || {
            entry.grab_focus_without_selecting();
            entry.text().to_string()
        });
    }
    // Copy OTP button on password page
    {
        let entry = otp_entry.clone();
        let btn = copy_otp_button.clone();
        connect_copy_button(&btn, &toast_overlay, move || {
            entry.grab_focus_without_selecting();
            entry.text().to_string()
        });
    }
    // new password
    {
        let popover_add = add_button_popover.clone();
        let popover_git = git_popover.clone();
        let page_state = password_list_state.clone();
        path_entry.connect_apply(move |row| {
            begin_new_password_entry(&page_state, &row.text(), &popover_add, &popover_git);
        });
    }

    // actions
    {
        let page_state = password_list_state.clone();
        let action = SimpleAction::new("save-password", None);
        action.connect_activate(move |_, _| {
            save_current_password_entry(&page_state);
        });

        window.add_action(&action);
    }
    {
        register_store_recipients_save_action(
            &window,
            &toast_overlay,
            &password_stores,
            &store_recipients_page_state,
        );
    }
    // open preferences
    {
        register_open_preferences_action(&window, &preferences_action_state);
    }

    #[cfg(not(feature = "flatpak"))]
    {
        register_open_log_action(&window, &window_navigation_state);
    }

    {
        let page_state = password_list_state.clone();
        let action = SimpleAction::new("open-raw-pass-file", None);
        action.connect_activate(move |_, _| {
            show_raw_pass_file_page(&page_state);
        });
        window.add_action(&action);
    }

    #[cfg(feature = "setup")]
    {
        register_install_locally_action(&window, &primary_menu, &toast_overlay);
    }

    {
        register_open_new_password_action(&window, &add_button_popover);
    }

    #[cfg(not(feature = "flatpak"))]
    {
        register_open_git_action(&window, &git_popover, &git_url_entry);
    }

    #[cfg(not(feature = "flatpak"))]
    {
        connect_git_clone_apply(&window, &git_url_entry);
    }

    #[cfg(not(feature = "flatpak"))]
    {
        register_git_clone_action(
            &git_action_state,
            &git_popover,
            &git_url_entry,
            &git_operation,
        );
    }

    {
        register_toggle_find_action(&window, &search_entry);
    }

    {
        register_toggle_hidden_action(&window, &hidden_entries_action_state);
    }

    {
        register_back_action(&window, &back_action_state);
    }

    #[cfg(not(feature = "flatpak"))]
    {
        register_synchronize_action(&git_action_state, &git_operation);
    }

    #[cfg(not(feature = "flatpak"))]
    {
        start_log_poller(&log_view, &window_navigation_state);
    }

    // keyboard shortcuts
    configure_window_shortcuts(app);

    setup_search_filter(&list, &search_entry);

    apply_startup_query(startup_query, &search_entry, &list);

    window
}

#[cfg(test)]
mod tests {
    use crate::pass_file::{
        new_pass_file_contents_from_template, parse_structured_pass_lines,
        structured_otp_line, structured_pass_contents_from_values, structured_username_value,
        uri_to_open, OtpFieldTemplate, StructuredPassLine, UsernameFieldTemplate,
    };

    #[test]
    fn structured_fields_strip_display_spacing_but_preserve_it_on_save() {
        let contents = "secret\nemail: hello@example.com\nname:hello";
        let (password, parsed) = parse_structured_pass_lines(contents);
        assert_eq!(password, "secret");

        let templates = parsed
            .iter()
            .map(|(line, _)| line.clone())
            .collect::<Vec<_>>();
        let values = parsed
            .iter()
            .filter_map(|(line, value)| match line {
                StructuredPassLine::Field(_) => value.clone(),
                StructuredPassLine::Username(_) => None,
                StructuredPassLine::Otp(_) => None,
                StructuredPassLine::Preserved(_) => None,
            })
            .collect::<Vec<_>>();

        assert_eq!(values, vec!["hello@example.com".to_string(), "hello".to_string()]);
        assert_eq!(
            structured_pass_contents_from_values(&password, "", None, &templates, &values),
            contents
        );
    }

    #[test]
    fn username_and_otpauth_lines_stay_out_of_dynamic_fields() {
        let contents = "secret\nusername:alice\notpauth://totp/example\nurl: https://example.com";
        let (_, parsed) = parse_structured_pass_lines(contents);

        assert!(matches!(
            parsed[0].0,
            StructuredPassLine::Username(_)
        ));
        assert_eq!(parsed[0].1.as_deref(), Some("alice"));
        assert!(matches!(
            parsed[1].0,
            StructuredPassLine::Otp(OtpFieldTemplate::BareUrl)
        ));
        assert_eq!(structured_otp_line(&parsed).map(|(_, url)| url), Some("otpauth://totp/example".to_string()));
        assert!(matches!(parsed[2].0, StructuredPassLine::Field(_)));
        assert_eq!(parsed[2].1.as_deref(), Some("https://example.com"));
    }

    #[test]
    fn new_password_template_becomes_body_after_password_line() {
        assert_eq!(
            new_pass_file_contents_from_template("username:alice\nurl:https://example.com"),
            "\nusername:alice\nurl:https://example.com".to_string()
        );
    }

    #[test]
    fn new_password_template_trims_only_edge_newlines() {
        assert_eq!(
            new_pass_file_contents_from_template("\nusername:alice\n\nurl:https://example.com\n"),
            "\nusername:alice\n\nurl:https://example.com".to_string()
        );
    }

    #[test]
    fn bare_urls_get_https_when_opened() {
        assert_eq!(
            uri_to_open("example.com/path"),
            Some("https://example.com/path".to_string())
        );
    }

    #[test]
    fn explicit_url_schemes_are_preserved() {
        assert_eq!(
            uri_to_open("https://example.com/path"),
            Some("https://example.com/path".to_string())
        );
    }

    #[test]
    fn blank_username_line_is_detected() {
        let (_, parsed) = parse_structured_pass_lines("secret\nusername:\nurl:https://example.com");
        assert_eq!(structured_username_value(&parsed), Some(String::new()));
    }

    #[test]
    fn structured_save_preserves_username_field_template() {
        let templates = vec![
            StructuredPassLine::Username(UsernameFieldTemplate {
                raw_key: "username".to_string(),
                separator_spacing: String::new(),
            }),
            StructuredPassLine::Preserved("url: https://example.com".to_string()),
        ];
        let values = Vec::<String>::new();

        assert_eq!(
            structured_pass_contents_from_values(
                "secret",
                "alice@example.com",
                None,
                &templates,
                &values,
            ),
            "secret\nusername:alice@example.com\nurl: https://example.com".to_string()
        );
    }
}
