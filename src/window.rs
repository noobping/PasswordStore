use crate::item::{collect_all_password_items, PasswordItem};
use adw::gio::{prelude::*, SimpleAction};
use adw::glib::{clone, prelude::*, MainContext};
use adw::{
    glib, prelude::*, Application, ApplicationWindow, EntryRow, NavigationPage, NavigationView,
    PasswordEntryRow, StatusPage, ToastOverlay, WindowTitle,
};
use gtk4::{
    Box as GtkBox, Builder, Button, Label, ListBox, ListBoxRow, Orientation, Popover, SearchEntry,
    Spinner, TextView,
};
use std::path::PathBuf;
use std::sync::mpsc;
use std::sync::mpsc::TryRecvError;
use std::thread;
use std::time::Duration;

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
    let list: ListBox = builder.object("list").expect("Failed to get list");

    let home = std::env::var("HOME").unwrap_or(String::new());
    let mut roots: Vec<PathBuf> = Vec::new();
    roots.push(PathBuf::from(format!("{}/.password-store", home)));

    load_passwords_async(&list, roots.clone(), search_button.clone(), git_button.clone());

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
        let back = back_button.clone();
        let search = search_button.clone();
        let git = git_button.clone();
        let add = add_button.clone();
        let nav = navigation_view.clone();
        let page = text_page.clone();
        let popover_add = add_button_popover.clone();
        let popover_git = git_popover.clone();
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
            search.set_visible(false);
            add.set_visible(false);
            git.set_visible(false);
            back.set_visible(true);
            nav.push(&page);

            popover_add.popdown();
            popover_git.popdown();
        });
    }

    // actions
    {
        let popover = add_button_popover.clone();
        let action = SimpleAction::new("add-password", None);
        action.connect_activate(move |_, _| {
            if popover.is_visible() {
                popover.popdown()
            } else {
                popover.popup()
            }
        });
        window.add_action(&action);
    }

    {
        let popover = git_popover.clone();
        let action = SimpleAction::new("git-page", None);
        action.connect_activate(move |_, _| {
            if popover.is_visible() {
                popover.popdown()
            } else {
                popover.popup()
            }
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
        let list_clone = list.clone();
        let roots_clone = roots.clone();
        let back = back_button.clone();
        let search = search_button.clone();
        let git = git_button.clone();
        let add = add_button.clone();
        let nav = navigation_view.clone();
        let page = list_page.clone();
        let action = SimpleAction::new("back", None);
        action.connect_activate(move |_, _| {
            add.set_visible(true);
            back.set_visible(false);
            nav.pop();
            load_passwords_async(&list_clone, roots_clone.clone(), search.clone(), git.clone());
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

    // TODO: Action: win.save-password

    // keyboard shortcuts
    app.set_accels_for_action("win.back", &["Escape"]);
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
        text_page,
        password_entry,
        copy_password_button,
        dynamic_box,
        text_view,
    }
}

fn clear_list(list: &gtk4::ListBox) {
    while let Some(child) = list.first_child() {
        list.remove(&child);
    }
}

fn load_passwords_async(list: &ListBox, roots: Vec<PathBuf>, search: Button, git: Button) {
    while let Some(child) = list.first_child() {
        list.remove(&child);
    }

    git.set_visible(false);
    search.set_visible(false);
    let bussy = Spinner::new();
    bussy.start();
    list.set_placeholder(Some(&bussy));

    // Standard library channel: main thread will own `rx`, worker gets `tx`
    let (tx, rx) = mpsc::channel::<Vec<PasswordItem>>();

    // Spawn worker thread – ONLY data goes in here (roots + tx)
    thread::spawn(move || {
        let all_items = match collect_all_password_items(&roots) {
            Ok(v) => v,
            Err(err) => {
                eprintln!("Error scanning pass roots: {err}");
                Vec::new()
            }
        };
        // Send everything back to main thread
        let _ = tx.send(all_items);
    });

    // Clone GTK widgets on the main thread (they stay on this thread)
    let list_clone = list.clone();
    let search_clone = search.clone();
    let git_clone = git.clone();

    // Poll the channel from the main thread using a GLib timeout
    //
    //   - timeout_add_local does NOT require Send
    //   - closure runs on the GTK main loop thread
    glib::timeout_add_local(Duration::from_millis(50), move || {
        match rx.try_recv() {
            Ok(items) => {
                let mut index = 0;
                let len = items.len();
                while index < len {
                    let item = &items[index];

                    let row = ListBoxRow::new();
                    let hbox = gtk4::Box::new(Orientation::Horizontal, 6);

                    let label = Label::new(Some(&item.label));
                    label.set_xalign(0.0);

                    hbox.append(&label);
                    row.set_child(Some(&hbox));

                    // Store full path on row for later use
                    unsafe {
                        row.set_data("pass-path", item.path.to_string_lossy().to_string());
                    }

                    list_clone.append(&row);

                    index += 1;
                }

                let empty = items.is_empty();
                git_clone.set_visible(empty);
                search_clone.set_visible(!empty);
                let placeholder = if empty {
                    StatusPage::builder()
                        .icon_name("passadw")
                        .title("No passwords found")
                        .description("Create a new password to get started.")
                        .build()
                } else {
                    StatusPage::builder()
                        .icon_name("edit-find-symbolic")
                        .title("No passwords found")
                        .description("Try another search query.")
                        .build()
                };
                list_clone.set_placeholder(Some(&placeholder));

                // One-shot: stop calling this timeout
                glib::ControlFlow::Break
            }
            Err(TryRecvError::Empty) => {
                // Worker not done yet → check again later
                glib::ControlFlow::Continue
            }
            Err(TryRecvError::Disconnected) => {
                // Worker died

                // TODO: Add placeholder, remove spinner, show search btn and hide git btn

                git_clone.set_visible(true);
                search_clone.set_visible(false);

                glib::ControlFlow::Break
            }
        }
    });
}
