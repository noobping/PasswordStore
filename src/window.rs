use adw::prelude::*;
use adw::{
    Application,
    ApplicationWindow,
    EntryRow,
    NavigationView,
    NavigationPage,
    PasswordEntryRow,
    ToastOverlay,
    WindowTitle,
    glib::clone,
};
use adw::gio::{Menu, SimpleAction, prelude::*};
use gtk4::{
    Box as GtkBox,
    Builder,
    Button,
    ListBox,
    Popover,
    SearchEntry,
    Spinner,
    TextView,
};

const UI_SRC: &str = include_str!("../data/window.ui");

pub struct Window {
    pub window: ApplicationWindow,

    pub back_button: Button,
    pub add_button: Button,
    pub add_button_popover: Popover,
    pub path_entry: EntryRow,
    pub git_button: Button,
    pub git_popover: Popover,
    pub git_url_entry: EntryRow,
    pub search_button: Button,
    pub window_title: WindowTitle,
    pub save_button: Button,

    pub toast_overlay: ToastOverlay,
    pub passphrase_popover: Popover,
    pub passphrase_entry: EntryRow,
    pub rename_popover: Popover,
    pub new_path_entry: EntryRow,

    pub navigation_view: NavigationView,
    pub list_page: NavigationPage,
    pub search_entry: SearchEntry,
    pub list: ListBox,
    pub spinner: Spinner,

    pub text_page: NavigationPage,
    pub password_entry: PasswordEntryRow,
    pub copy_password_button: Button,
    pub dynamic_box: GtkBox,
    pub text_view: TextView,
}

pub fn create_main_window(app: &Application) -> Window {
    // The resources are registered in main.rs
    let builder = Builder::from_string(UI_SRC);

    // Root window
    let window: ApplicationWindow = builder
        .object("main_window")
        .expect("Failed to get main_window from UI");
    window.set_application(Some(app));

    // Headerbar + top controls
    let back_button: Button = builder
        .object("back_button")
        .expect("Failed to get back_button");
    let add_button: Button = builder
        .object("add_button")
        .expect("Failed to get add_button");
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
    let git_url_entry: EntryRow = builder
        .object("git_url_entry")
        .expect("Failed to get git_url_entry");
    let search_button: Button = builder
        .object("search_button")
        .expect("Failed to get search_button");
    let window_title: WindowTitle = builder
        .object("window_title")
        .expect("Failed to get window_title");
    let save_button: Button = builder
        .object("save_button")
        .expect("Failed to get save_button");

    // Toast overlay + popovers
    let toast_overlay: ToastOverlay = builder
        .object("toast_overlay")
        .expect("Failed to get toast_overlay");
    let passphrase_popover: Popover = builder
        .object("passphrase_popover")
        .expect("Failed to get passphrase_popover");
    let passphrase_entry: EntryRow = builder
        .object("passphrase_entry")
        .expect("Failed to get passphrase_entry");
    let rename_popover: Popover = builder
        .object("rename_popover")
        .expect("Failed to get rename_popover");
    let new_path_entry: EntryRow = builder
        .object("new_path_entry")
        .expect("Failed to get new_path_entry");

    // Navigation + list page
    let navigation_view: NavigationView = builder
        .object("navigation_view")
        .expect("Failed to get navigation_view");
    let list_page: NavigationPage = builder
        .object("list_page")
        .expect("Failed to get list_page");
    let search_entry: SearchEntry = builder
        .object("search_entry")
        .expect("Failed to get search_entry");
    let list: ListBox = builder
        .object("list")
        .expect("Failed to get list");
    let spinner: Spinner = builder
        .object("spinner")
        .expect("Failed to get spinner");

    // Text editor page
    let text_page: NavigationPage = builder
        .object("text_page")
        .expect("Failed to get text_page");
    let password_entry: PasswordEntryRow = builder
        .object("password_entry")
        .expect("Failed to get password_entry");
    let copy_password_button: Button = builder
        .object("copy_password_button")
        .expect("Failed to get copy_password_button");
    let dynamic_box: GtkBox = builder
        .object("dynamic_box")
        .expect("Failed to get dynamic_box");
    let text_view: TextView = builder
        .object("text_view")
        .expect("Failed to get text_view");

    // Input
    {
        let nav = navigation_view.clone();
        let page = text_page.clone();
        let popover = add_button_popover.clone();
        let overlay = toast_overlay.clone();
        path_entry.connect_apply(move |row| {
            let path = row.text().to_string(); // Get the text from the entry

            // Do your “apply” logic here:
            if path.is_empty() {
                // example: warn the user
                let toast = adw::Toast::new("Path cannot be empty");
                overlay.add_toast(toast);
                return;
            }

            // TODO: create the password / entry at `path`
            nav.push(&page);

            popover.popdown(); // Close the popover once we've handled it
        });
    }

    // actions
    {
        let popover = add_button_popover.clone();
        let action = SimpleAction::new("add-password", None);
        action.connect_activate(move |_, _| {
            popover.popup();
        });
        window.add_action(&action);
    }
    
    {
        let popover = git_popover.clone();
        let action = SimpleAction::new("git-page", None);
        action.connect_activate(move |_, _| {
            popover.popup();
        });
        window.add_action(&action);
    }

    {
        let entry = git_url_entry.clone();
        let overlay = toast_overlay.clone();
        let action = SimpleAction::new("git-clone", None);
        action.connect_activate(move |_, _| {
            let url = entry.text().to_string();

            // TODO: run your clone logic here
            // e.g. spawn a task, then show a toast:
            if !url.is_empty() {
                let toast = adw::Toast::new(&format!("Cloning from {url}…"));
                overlay.add_toast(toast);
            }
        });
        window.add_action(&action);
    }

    {
        let search = search_entry.clone();
        let action = SimpleAction::new("toggle-search", None);
        action.connect_activate(move |_, _| {
            let visible = search.is_visible();
            search.set_visible(!visible);
            if !visible {
                search.grab_focus();
            }
        });
        window.add_action(&action);
    }

    {
        let nav = navigation_view.clone();
        let page = list_page.clone();
        let action = SimpleAction::new("home-page", None);
        action.connect_activate(move |_, _| {
            nav.push(&page);
        });
        window.add_action(&action);
    }

    {
        let nav = navigation_view.clone();
        let page = text_page.clone();
        let action = SimpleAction::new("text-page", None);
        action.connect_activate(move |_, _| {
            nav.push(&page);
        });
        window.add_action(&action);
    }

    // TODO: Actions
    // win.add-password
    // win.git-page
    // win.git-clone
    // win.save-password

    // keyboard shortcuts
    app.set_accels_for_action("win.home-page", &["Escape"]);
    app.set_accels_for_action("win.toggle-search", &["<primary>f"]);

    Window {
        window,
        back_button,
        add_button,
        add_button_popover,
        path_entry,
        git_button,
        git_popover,
        git_url_entry,
        search_button,
        window_title,
        save_button,
        toast_overlay,
        passphrase_popover,
        passphrase_entry,
        rename_popover,
        new_path_entry,
        navigation_view,
        list_page,
        search_entry,
        list,
        spinner,
        text_page,
        password_entry,
        copy_password_button,
        dynamic_box,
        text_view,
    }
}
