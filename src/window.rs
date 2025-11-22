use crate::item::{collect_all_password_items, PassEntry};
use adw::gio::{prelude::*, SimpleAction};
use adw::{
    glib, prelude::*, Application, ApplicationWindow, EntryRow, NavigationPage, NavigationView,
    PasswordEntryRow, StatusPage, ToastOverlay, WindowTitle,
};
use gtk4::{
    Box as GtkBox, Builder, Button, Label, ListBox, ListBoxRow, Orientation, Popover, SearchEntry,
    Spinner, TextView,
};
use std::cell::RefCell;
use std::path::PathBuf;
use std::process::Command;
use std::rc::Rc;
use std::sync::mpsc;
use std::sync::mpsc::TryRecvError;
use std::thread;
use std::time::Duration;

const UI_SRC: &str = include_str!("../data/window.ui");

pub struct Window {
    pub window: ApplicationWindow,

    pub toast_overlay: ToastOverlay,
    pub navigation_view: NavigationView,
    pub list_page: NavigationPage,
    pub text_page: NavigationPage,
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
    let window_title: WindowTitle = builder
        .object("window_title")
        .expect("Failed to get window_title");
    let save_button: Button = builder
        .object("save_button")
        .expect("Failed to get save_button");

    // Toast overlay
    let toast_overlay: ToastOverlay = builder
        .object("toast_overlay")
        .expect("Failed to get toast_overlay");

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

    load_passwords_async(&list, roots.clone(), git_button.clone());

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

    // Selecting an item from the list → decrypt with `pass`
    {
        let nav = navigation_view.clone();
        let text_page = text_page.clone();
        let back_button = back_button.clone();
        let add_button = add_button.clone();
        let git_button = git_button.clone();
        let password_entry = password_entry.clone();
        let text_view = text_view.clone();
        let overlay = toast_overlay.clone();
        let window_title = window_title.clone();

        list.connect_row_activated(move |_list, row| {
            // Retrieve the pass entry name (relative label) stored on the row
            let label = non_null_to_string_option(row, "label");
            let root = non_null_to_string_option(row, "root");

            let Some(label) = label else {
                let toast = adw::Toast::new("Can not find password file.");
                overlay.add_toast(toast);
                return;
            };

            let Some(root) = root else {
                let toast =
                    adw::Toast::new("Can not open password file form a unknown password store.");
                overlay.add_toast(toast);
                return;
            };

            // Navigate to the text editor page and update header buttons
            add_button.set_visible(false);
            git_button.set_visible(false);
            back_button.set_visible(true);
            window_title.set_subtitle(&label);
            nav.push(&text_page);

            // Background worker: run `pass <label>`
            let (tx, rx) = mpsc::channel::<Result<String, String>>();
            let label_for_thread = label.clone();
            let store_for_thread = root.clone();
            thread::spawn(move || {
                let output = Command::new("pass")
                    .env("PASSWORD_STORE_DIR", store_for_thread)
                    .arg(&label_for_thread)
                    .output();
                let result = match output {
                    Ok(o) if o.status.success() => {
                        Ok(String::from_utf8_lossy(&o.stdout).to_string())
                    }
                    Ok(o) => Err(format!("pass failed: {}", o.status)),
                    Err(e) => Err(format!("Failed to run pass: {e}")),
                };

                let _ = tx.send(result);
            });

            // UI updater: poll the channel from the main thread
            let password_entry = password_entry.clone();
            let text_view = text_view.clone();
            let overlay = overlay.clone();

            glib::timeout_add_local(Duration::from_millis(50), move || {
                use std::sync::mpsc::TryRecvError;

                match rx.try_recv() {
                    Ok(Ok(output)) => {
                        // Split into first line (password) and rest (notes)
                        let mut lines = output.lines();
                        if let Some(first) = lines.next() {
                            password_entry.set_text(first);
                        } else {
                            password_entry.set_text("");
                        }

                        let rest = lines.collect::<Vec<_>>().join("\n");
                        let buffer = text_view.buffer();
                        buffer.set_text(&rest);

                        glib::ControlFlow::Break
                    }
                    Ok(Err(msg)) => {
                        let toast = adw::Toast::new(&msg);
                        overlay.add_toast(toast);
                        glib::ControlFlow::Break
                    }
                    Err(TryRecvError::Empty) => glib::ControlFlow::Continue,
                    Err(TryRecvError::Disconnected) => {
                        let toast = adw::Toast::new("Failed to decrypt password entry");
                        overlay.add_toast(toast);
                        glib::ControlFlow::Break
                    }
                }
            });
        });
    }

    // Input
    {
        let back = back_button.clone();
        let git = git_button.clone();
        let add = add_button.clone();
        let nav = navigation_view.clone();
        let page = text_page.clone();
        let popover_add = add_button_popover.clone();
        let popover_git = git_popover.clone();
        let overlay = toast_overlay.clone();
        path_entry.connect_apply(move |row| {
            let path = row.text().to_string(); // Get the text from the entry
            if path.is_empty() {
                let toast = adw::Toast::new("Path cannot be empty");
                overlay.add_toast(toast);
                return;
            }
            add.set_visible(false);
            git.set_visible(false);
            back.set_visible(true);
            nav.push(&page);

            popover_add.popdown();
            popover_git.popdown();

            // TODO: create the password / entry at `path`
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
        let git = git_button.clone();
        let add = add_button.clone();
        let nav = navigation_view.clone();
        let action = SimpleAction::new("back", None);
        action.connect_activate(move |_, _| {
            add.set_visible(true);
            back.set_visible(false);
            nav.pop();

            // TODO: Clear password and text fields

            load_passwords_async(&list_clone, roots_clone.clone(), git.clone());
        });
        window.add_action(&action);
    }

    {
        let overlay_clone = toast_overlay.clone();
        let roots_clone = roots.clone();
        let action = SimpleAction::new("synchronize", None);
        action.connect_activate(move |_, _| {
            let roots = roots_clone.clone();
            let overlay = overlay_clone.clone();

            // Channel from worker → main thread
            let (tx, rx) = mpsc::channel::<String>();

            // Background worker
            thread::spawn(move || {
                for root in roots {
                    // List of git operations we want to run for each store
                    let commands: [&[&str]; 3] = [
                        &["git", "fetch", "--all"],
                        &["git", "pull"],
                        &["git", "push"],
                    ];

                    for args in commands {
                        let output = Command::new("pass")
                            .env("PASSWORD_STORE_DIR", &root)
                            .args(args)
                            .output();

                        match output {
                            Ok(out) => {
                                if !out.status.success() {
                                    // git wrote its messages to stderr
                                    let stderr = String::from_utf8_lossy(&out.stderr);

                                    // try to find the last line containing "fatal:"
                                    let fatal_line = stderr
                                        .lines()
                                        .rev() // start from the bottom
                                        .find(|line| line.contains("fatal:"))
                                        // fallback: whole stderr if no "fatal:" found
                                        .unwrap_or(stderr.trim());
                                    let message = format!("{} Using: {}", fatal_line, root.display());
                                    eprintln!("{}", message);
                                    let _ = tx.send(message);

                                    // stop further commands for this store
                                    break;
                                }
                            }
                            Err(e) => {
                                let message = format!("Failed: {} with {e}", root.display());
                                eprintln!("{}", message);
                                let _ = tx.send(message);
                                break;
                            }
                        }
                    }
                }
            });

            // Main-thread: poll for messages and show toasts
            glib::timeout_add_local(Duration::from_millis(100), move || {
                match rx.try_recv() {
                    Ok(msg) => {
                        let toast = adw::Toast::new(&msg);
                        overlay.add_toast(toast);
                        glib::ControlFlow::Continue
                    }
                    Err(TryRecvError::Empty) => {
                        // No message yet, keep polling
                        glib::ControlFlow::Continue
                    }
                    Err(TryRecvError::Disconnected) => {
                        // Worker is done and channel closed
                        glib::ControlFlow::Break
                    }
                }
            });
        });
        window.add_action(&action);
    }

    // TODO: Action: win.save-password

    // keyboard shortcuts
    app.set_accels_for_action("win.back", &["Escape"]);
    app.set_accels_for_action("win.toggle-search", &["<primary>f"]);

    setup_search_filter(&list, &search_entry);

    Window {
        window,
        toast_overlay,
        navigation_view,
        list_page,
        text_page,
    }
}

fn load_passwords_async(list: &ListBox, roots: Vec<PathBuf>, git: Button) {
    while let Some(child) = list.first_child() {
        list.remove(&child);
    }

    git.set_visible(false);

    let bussy = Spinner::new();
    bussy.start();
    let project = env!("CARGO_PKG_NAME");
    let symbolic = format!("{project}-symbolic");
    let placeholder = StatusPage::builder()
        .icon_name(symbolic)
        .child(&bussy)
        .build();
    list.set_placeholder(Some(&placeholder));

    // Standard library channel: main thread will own `rx`, worker gets `tx`
    let (tx, rx) = mpsc::channel::<Vec<PassEntry>>();

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
                    let item_name = item.basename.clone();
                    let item_dir = item.relative_path.clone();
                    let item_root = item.store_path.clone();

                    let row = ListBoxRow::new();
                    let hbox = gtk4::Box::new(Orientation::Horizontal, 6);

                    let label = Label::new(Some(&item.label()));
                    label.set_xalign(0.0);

                    hbox.append(&label);
                    row.set_child(Some(&hbox));

                    // Store full path on row for later use
                    unsafe {
                        row.set_data("name", item_name);
                        row.set_data("dir", item_dir);
                        row.set_data("root", item_root);
                        row.set_data("label", item.label());
                    }

                    list_clone.append(&row);

                    index += 1;
                }

                let empty = items.is_empty();
                git_clone.set_visible(empty);
                let project = env!("CARGO_PKG_NAME");
                let symbolic = format!("{project}-symbolic");
                let placeholder = if empty {
                    StatusPage::builder()
                        .icon_name(symbolic)
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
                let project = env!("CARGO_PKG_NAME");
                let symbolic = format!("{project}-symbolic");
                let placeholder = StatusPage::builder().icon_name(symbolic).build();
                list_clone.set_placeholder(Some(&placeholder));

                git_clone.set_visible(true);

                glib::ControlFlow::Break
            }
        }
    });
}

fn setup_search_filter(list: &ListBox, search_entry: &SearchEntry) {
    // shared state for the current query
    let query = Rc::new(RefCell::new(String::new()));

    // 1) Filter function for the ListBox
    let query_for_filter = query.clone();
    list.set_filter_func(move |row: &ListBoxRow| {
        let q_ref = query_for_filter.borrow();
        let q = q_ref.as_str();

        // empty query → show everything
        if q.is_empty() {
            return true;
        }

        if let Some(label) = non_null_to_string_option(row, "label") {
            let query_lower = q.to_lowercase();
            return label.to_lowercase().contains(&query_lower);
        }

        true
    });

    // 2) Update query when the user types, then invalidate the filter
    let query_for_entry = query.clone();
    let list_for_entry = list.clone();

    search_entry.connect_search_changed(move |entry| {
        let text = entry.text().to_string();

        {
            let mut q_mut = query_for_entry.borrow_mut();
            *q_mut = text;
        }

        // trigger re-evaluation of filter_func for all rows
        list_for_entry.invalidate_filter();
    });
}

fn non_null_to_string_option(row: &ListBoxRow, key: &str) -> Option<String> {
    non_null_to_string_result(unsafe { row.data::<String>(key) }).ok()
}

fn non_null_to_string_result(label_opt: Option<std::ptr::NonNull<String>>) -> Result<String, ()> {
    if let Some(ptr) = label_opt {
        // SAFETY: caller must guarantee the pointer is valid and points to a valid String
        let s: &String = unsafe { ptr.as_ref() };
        Ok(s.clone())
    } else {
        Err(())
    }
}
