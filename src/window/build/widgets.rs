#[cfg(feature = "setup")]
use adw::gio::Menu;
use adw::glib::{object::IsA, Object};
#[cfg(feature = "flatpak")]
use adw::gtk::MenuButton;
use adw::gtk::{
    Box as GtkBox, Builder, Button, CheckButton, DropDown, ListBox, Popover, Revealer, SearchEntry,
    SpinButton, TextView, ToggleButton,
};
#[cfg(not(feature = "flatpak"))]
use adw::PreferencesGroup;
use adw::{
    ApplicationWindow, EntryRow, NavigationPage, NavigationView, PasswordEntryRow, StatusPage,
    ToastOverlay, WindowTitle,
};

pub(in crate::window) struct WindowWidgets {
    pub(in crate::window) window: ApplicationWindow,
    #[cfg(feature = "setup")]
    pub(in crate::window) primary_menu: Menu,
    #[cfg(feature = "flatpak")]
    pub(in crate::window) primary_menu_button: MenuButton,
    pub(in crate::window) back_button: Button,
    pub(in crate::window) add_button: Button,
    pub(in crate::window) find_button: Button,
    pub(in crate::window) add_button_popover: Popover,
    pub(in crate::window) new_password_store_dropdown: DropDown,
    pub(in crate::window) path_entry: EntryRow,
    pub(in crate::window) git_button: Button,
    pub(in crate::window) store_button: Button,
    pub(in crate::window) window_title: WindowTitle,
    pub(in crate::window) save_button: Button,
    pub(in crate::window) toast_overlay: ToastOverlay,
    pub(in crate::window) settings_page: NavigationPage,
    pub(in crate::window) store_recipients_page: NavigationPage,
    pub(in crate::window) store_recipients_list: ListBox,
    pub(in crate::window) log_page: NavigationPage,
    pub(in crate::window) new_pass_file_template_view: TextView,
    pub(in crate::window) preferences_username_folder_check: CheckButton,
    pub(in crate::window) preferences_username_filename_check: CheckButton,
    pub(in crate::window) password_stores: ListBox,
    pub(in crate::window) navigation_view: NavigationView,
    pub(in crate::window) search_entry: SearchEntry,
    pub(in crate::window) list: ListBox,
    pub(in crate::window) text_page: NavigationPage,
    pub(in crate::window) raw_text_page: NavigationPage,
    pub(in crate::window) password_status: StatusPage,
    pub(in crate::window) password_entry: PasswordEntryRow,
    pub(in crate::window) password_generator_settings_button: ToggleButton,
    pub(in crate::window) password_generator_settings_revealer: Revealer,
    pub(in crate::window) password_generator_length_spin: SpinButton,
    pub(in crate::window) password_generator_min_lowercase_spin: SpinButton,
    pub(in crate::window) password_generator_min_uppercase_spin: SpinButton,
    pub(in crate::window) password_generator_min_numbers_spin: SpinButton,
    pub(in crate::window) password_generator_min_symbols_spin: SpinButton,
    pub(in crate::window) username_entry: EntryRow,
    pub(in crate::window) otp_entry: PasswordEntryRow,
    pub(in crate::window) add_otp_button: Button,
    pub(in crate::window) copy_password_button: Button,
    pub(in crate::window) copy_username_button: Button,
    pub(in crate::window) copy_otp_button: Button,
    pub(in crate::window) text_view: TextView,
    pub(in crate::window) dynamic_fields_box: GtkBox,
    pub(in crate::window) open_raw_button: Button,
    pub(in crate::window) preferences_password_generator_length_spin: SpinButton,
    pub(in crate::window) preferences_password_generator_min_lowercase_spin: SpinButton,
    pub(in crate::window) preferences_password_generator_min_uppercase_spin: SpinButton,
    pub(in crate::window) preferences_password_generator_min_numbers_spin: SpinButton,
    pub(in crate::window) preferences_password_generator_min_symbols_spin: SpinButton,
    #[cfg(not(feature = "flatpak"))]
    pub(in crate::window) backend_preferences: PreferencesGroup,
    #[cfg(not(feature = "flatpak"))]
    pub(in crate::window) backend_row: adw::ComboRow,
    #[cfg(not(feature = "flatpak"))]
    pub(in crate::window) pass_command_row: EntryRow,
    #[cfg(not(feature = "flatpak"))]
    pub(in crate::window) git_busy_page: NavigationPage,
    #[cfg(not(feature = "flatpak"))]
    pub(in crate::window) git_busy_status: StatusPage,
    #[cfg(not(feature = "flatpak"))]
    pub(in crate::window) log_view: TextView,
}

impl WindowWidgets {
    pub(in crate::window) fn load(builder: &Builder) -> Self {
        Self {
            window: required_object(builder, "main_window"),
            #[cfg(feature = "setup")]
            primary_menu: required_object(builder, "primary_menu"),
            #[cfg(feature = "flatpak")]
            primary_menu_button: required_object(builder, "primary_menu_button"),
            back_button: required_object(builder, "back_button"),
            add_button: required_object(builder, "add_button"),
            find_button: required_object(builder, "find_button"),
            add_button_popover: required_object(builder, "add_button_popover"),
            new_password_store_dropdown: required_object(builder, "new_password_store_dropdown"),
            path_entry: required_object(builder, "path_entry"),
            git_button: required_object(builder, "git_button"),
            store_button: required_object(builder, "store_button"),
            window_title: required_object(builder, "window_title"),
            save_button: required_object(builder, "save_button"),
            toast_overlay: required_object(builder, "toast_overlay"),
            settings_page: required_object(builder, "settings_page"),
            store_recipients_page: required_object(builder, "store_recipients_page"),
            store_recipients_list: required_object(builder, "store_recipients_list"),
            log_page: required_object(builder, "log_page"),
            new_pass_file_template_view: required_object(builder, "new_pass_file_template_view"),
            preferences_username_folder_check: required_object(
                builder,
                "preferences_username_folder_check",
            ),
            preferences_username_filename_check: required_object(
                builder,
                "preferences_username_filename_check",
            ),
            password_stores: required_object(builder, "password_stores"),
            navigation_view: required_object(builder, "navigation_view"),
            search_entry: required_object(builder, "search_entry"),
            list: required_object(builder, "list"),
            text_page: required_object(builder, "text_page"),
            raw_text_page: required_object(builder, "raw_text_page"),
            password_status: required_object(builder, "password_status"),
            password_entry: required_object(builder, "password_entry"),
            password_generator_settings_button: required_object(
                builder,
                "password_generator_settings_button",
            ),
            password_generator_settings_revealer: required_object(
                builder,
                "password_generator_settings_revealer",
            ),
            password_generator_length_spin: required_object(
                builder,
                "password_generator_length_spin",
            ),
            password_generator_min_lowercase_spin: required_object(
                builder,
                "password_generator_min_lowercase_spin",
            ),
            password_generator_min_uppercase_spin: required_object(
                builder,
                "password_generator_min_uppercase_spin",
            ),
            password_generator_min_numbers_spin: required_object(
                builder,
                "password_generator_min_numbers_spin",
            ),
            password_generator_min_symbols_spin: required_object(
                builder,
                "password_generator_min_symbols_spin",
            ),
            username_entry: required_object(builder, "username_entry"),
            otp_entry: required_object(builder, "otp_entry"),
            add_otp_button: required_object(builder, "add_otp_button"),
            copy_password_button: required_object(builder, "copy_password_button"),
            copy_username_button: required_object(builder, "copy_username_button"),
            copy_otp_button: required_object(builder, "copy_otp_button"),
            text_view: required_object(builder, "text_view"),
            dynamic_fields_box: required_object(builder, "dynamic_fields_box"),
            open_raw_button: required_object(builder, "open_raw_button"),
            preferences_password_generator_length_spin: required_object(
                builder,
                "preferences_password_generator_length_spin",
            ),
            preferences_password_generator_min_lowercase_spin: required_object(
                builder,
                "preferences_password_generator_min_lowercase_spin",
            ),
            preferences_password_generator_min_uppercase_spin: required_object(
                builder,
                "preferences_password_generator_min_uppercase_spin",
            ),
            preferences_password_generator_min_numbers_spin: required_object(
                builder,
                "preferences_password_generator_min_numbers_spin",
            ),
            preferences_password_generator_min_symbols_spin: required_object(
                builder,
                "preferences_password_generator_min_symbols_spin",
            ),
            #[cfg(not(feature = "flatpak"))]
            backend_preferences: required_object(builder, "backend_preferences"),
            #[cfg(not(feature = "flatpak"))]
            backend_row: required_object(builder, "backend_row"),
            #[cfg(not(feature = "flatpak"))]
            pass_command_row: required_object(builder, "pass_command_row"),
            #[cfg(not(feature = "flatpak"))]
            git_busy_page: required_object(builder, "git_busy_page"),
            #[cfg(not(feature = "flatpak"))]
            git_busy_status: required_object(builder, "git_busy_status"),
            #[cfg(not(feature = "flatpak"))]
            log_view: required_object(builder, "log_view"),
        }
    }
}

pub(super) fn required_object<T: IsA<Object> + Clone + 'static>(builder: &Builder, id: &str) -> T {
    builder
        .object(id)
        .unwrap_or_else(|| panic!("Failed to get {id}"))
}
