#[cfg(feature = "setup")]
use adw::gio::Menu;
use adw::glib::{object::IsA, Object};
use adw::gtk::{Box as GtkBox, Builder, Button, ListBox, Popover, SearchEntry, TextView};
use adw::{
    ApplicationWindow, EntryRow, NavigationPage, NavigationView, PasswordEntryRow, StatusPage,
    ToastOverlay, WindowTitle,
};

pub(super) struct WindowWidgets {
    pub(super) window: ApplicationWindow,
    #[cfg(feature = "setup")]
    pub(super) primary_menu: Menu,
    pub(super) back_button: Button,
    pub(super) add_button: Button,
    pub(super) find_button: Button,
    pub(super) add_button_popover: Popover,
    pub(super) new_password_store_box: GtkBox,
    pub(super) new_password_store_list: GtkBox,
    pub(super) path_entry: EntryRow,
    pub(super) git_button: Button,
    pub(super) git_popover: Popover,
    pub(super) window_title: WindowTitle,
    pub(super) save_button: Button,
    pub(super) toast_overlay: ToastOverlay,
    pub(super) settings_page: NavigationPage,
    pub(super) store_recipients_page: NavigationPage,
    pub(super) store_recipients_list: ListBox,
    pub(super) log_page: NavigationPage,
    pub(super) new_pass_file_template_view: TextView,
    pub(super) password_stores: ListBox,
    pub(super) navigation_view: NavigationView,
    pub(super) search_entry: SearchEntry,
    pub(super) list: ListBox,
    pub(super) text_page: NavigationPage,
    pub(super) raw_text_page: NavigationPage,
    pub(super) password_status: StatusPage,
    pub(super) password_entry: PasswordEntryRow,
    pub(super) username_entry: EntryRow,
    pub(super) otp_entry: PasswordEntryRow,
    pub(super) copy_password_button: Button,
    pub(super) copy_username_button: Button,
    pub(super) copy_otp_button: Button,
    pub(super) text_view: TextView,
    pub(super) dynamic_fields_box: GtkBox,
    pub(super) open_raw_button: Button,
}

impl WindowWidgets {
    pub(super) fn load(builder: &Builder) -> Self {
        Self {
            window: required_object(builder, "main_window"),
            #[cfg(feature = "setup")]
            primary_menu: required_object(builder, "primary_menu"),
            back_button: required_object(builder, "back_button"),
            add_button: required_object(builder, "add_button"),
            find_button: required_object(builder, "find_button"),
            add_button_popover: required_object(builder, "add_button_popover"),
            new_password_store_box: required_object(builder, "new_password_store_box"),
            new_password_store_list: required_object(builder, "new_password_store_list"),
            path_entry: required_object(builder, "path_entry"),
            git_button: required_object(builder, "git_button"),
            git_popover: required_object(builder, "git_popover"),
            window_title: required_object(builder, "window_title"),
            save_button: required_object(builder, "save_button"),
            toast_overlay: required_object(builder, "toast_overlay"),
            settings_page: required_object(builder, "settings_page"),
            store_recipients_page: required_object(builder, "store_recipients_page"),
            store_recipients_list: required_object(builder, "store_recipients_list"),
            log_page: required_object(builder, "log_page"),
            new_pass_file_template_view: required_object(builder, "new_pass_file_template_view"),
            password_stores: required_object(builder, "password_stores"),
            navigation_view: required_object(builder, "navigation_view"),
            search_entry: required_object(builder, "search_entry"),
            list: required_object(builder, "list"),
            text_page: required_object(builder, "text_page"),
            raw_text_page: required_object(builder, "raw_text_page"),
            password_status: required_object(builder, "password_status"),
            password_entry: required_object(builder, "password_entry"),
            username_entry: required_object(builder, "username_entry"),
            otp_entry: required_object(builder, "otp_entry"),
            copy_password_button: required_object(builder, "copy_password_button"),
            copy_username_button: required_object(builder, "copy_username_button"),
            copy_otp_button: required_object(builder, "copy_otp_button"),
            text_view: required_object(builder, "text_view"),
            dynamic_fields_box: required_object(builder, "dynamic_fields_box"),
            open_raw_button: required_object(builder, "open_raw_button"),
        }
    }
}

fn required_object<T: IsA<Object> + Clone + 'static>(builder: &Builder, id: &str) -> T {
    builder
        .object(id)
        .unwrap_or_else(|| panic!("Failed to get {id}"))
}
