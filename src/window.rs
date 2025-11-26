use crate::app::*;
use crate::item::{collect_all_password_items, PassEntry};
use crate::methods::non_null_to_string_option;
use crate::preferences::Preferences;
use adw::gio::{prelude::*, Menu, MenuItem, SimpleAction};
use adw::{
    glib, prelude::*, ActionRow, Application, ApplicationWindow, EntryRow, NavigationPage,
    NavigationView, PasswordEntryRow, StatusPage, Toast, ToastOverlay, WindowTitle,
};
use gtk4::{
    gdk::Display, Box as GtkBox, Builder, Button, ListBox, ListBoxRow, MenuButton, Orientation,
    Popover, SearchEntry, Spinner, TextView,
};
use std::cell::RefCell;
use std::io::Write;
use std::process::{Command, Stdio};
use std::rc::Rc;
use std::sync::mpsc;
use std::sync::mpsc::TryRecvError;
use std::thread;
use std::time::Duration;

const UI_SRC: &str = include_str!("../data/window.ui");

pub fn create_main_window(app: &Application, startup_query: Option<String>) -> ApplicationWindow {
    // The resources are registered in main.rs
    let builder = Builder::from_string(UI_SRC);
    let settings = Preferences::new();

    // Root window
    let window: ApplicationWindow = builder
        .object("main_window")
        .expect("Failed to get main_window from UI");
    window.set_application(Some(app));

    let primary_menu: Menu = builder
        .object("primary_menu")
        .expect("Failed to get primary menu");
    if can_install_locally() {
        let item = if is_installed_locally() {
            MenuItem::new(Some("Uninstall this App"), Some("win.install-locally"))
        } else {
            MenuItem::new(Some("Install this App"), Some("win.install-locally"))
        };
        primary_menu.append_item(&item);
    }

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

    // Settings
    let settings_page: NavigationPage = builder
        .object("settings_page")
        .expect("Failed to get settings page");
    let pass_row: adw::EntryRow = builder.object("pass_command_row").unwrap();
    pass_row.set_text(&settings.command());

    // Navigation + list page
    let navigation_view: NavigationView = builder
        .object("navigation_view")
        .expect("Failed to get navigation_view");
    let search_entry: SearchEntry = builder
        .object("search_entry")
        .expect("Failed to get search_entry");
    let list: ListBox = builder.object("list").expect("Failed to get list");

    load_passwords_async(&list, git_button.clone(), save_button.clone(), window.clone());

    // Text editor page
    let text_page: NavigationPage = builder
        .object("text_page")
        .expect("Failed to get text_page");
    let password_status: StatusPage = builder
        .object("password_status")
        .expect("Failed to get password_status");
    let password_entry: PasswordEntryRow = builder
        .object("password_entry")
        .expect("Failed to get password_entry");
    let copy_password_button: Button = builder
        .object("copy_password_button")
        .expect("Failed to get copy_password_button");
    let text_view: TextView = builder
        .object("text_view")
        .expect("Failed to get text_view");

    // Selecting an item from the list → decrypt with `pass`
    {
        let nav = navigation_view.clone();
        let page = text_page.clone();
        let back = back_button.clone();
        let add = add_button.clone();
        let git = git_button.clone();
        let save = save_button.clone();
        let entry = password_entry.clone();
        let status = password_status.clone();
        let text = text_view.clone();
        let overlay = toast_overlay.clone();
        let win = window_title.clone();

        list.connect_row_activated(move |_list, row| {
            // Retrieve the pass entry name (relative label) stored on the row
            let title = non_null_to_string_option(row, "name");
            let label = non_null_to_string_option(row, "label");
            let root = non_null_to_string_option(row, "root");

            let Some(title) = title else {
                let toast = Toast::new("Can not find password file name.");
                overlay.add_toast(toast);
                return;
            };

            let Some(label) = label else {
                let toast = Toast::new("Can not find password file.");
                overlay.add_toast(toast);
                return;
            };

            let Some(root) = root else {
                let toast = Toast::new("Can not open password file form a unknown password store.");
                overlay.add_toast(toast);
                return;
            };

            // Navigate to the text editor page and update header buttons
            add.set_visible(false);
            git.set_visible(false);
            back.set_visible(true);
            save.set_visible(true);
            win.set_title(&title);
            win.set_subtitle(&label);
            text.set_visible(false);
            entry.set_visible(false);
            status.set_visible(true);
            nav.push(&page);

            // Background worker: run `pass <label>`
            let (tx, rx) = mpsc::channel::<Result<String, String>>();
            let label_for_thread = label.clone();
            let store_for_thread = root.clone();
            thread::spawn(move || {
                let settings = Preferences::new();
                let output = Command::new(settings.command())
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
            let password_status = status.clone();
            let password_entry = entry.clone();
            let text_view = text.clone();
            let overlay = overlay.clone();

            glib::timeout_add_local(Duration::from_millis(50), move || {
                use std::sync::mpsc::TryRecvError;

                match rx.try_recv() {
                    Ok(Ok(output)) => {
                        password_status.set_visible(false);
                        password_entry.set_visible(true);
                        text_view.set_visible(true);

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
                        let toast = Toast::new(&msg);
                        overlay.add_toast(toast);
                        glib::ControlFlow::Break
                    }
                    Err(TryRecvError::Empty) => glib::ControlFlow::Continue,
                    Err(TryRecvError::Disconnected) => {
                        let toast = Toast::new("Failed to decrypt password entry");
                        overlay.add_toast(toast);
                        glib::ControlFlow::Break
                    }
                }
            });
        });
    }

    // Input
    {
        let overlay = toast_overlay.clone();
        let entry = password_entry.clone();
        let btn = copy_password_button.clone();
        btn.connect_clicked(move |_| {
            entry.grab_focus_without_selecting();
            let text = entry.text().to_string();
            if let Some(display) = Display::default() {
                let clipboard = display.clipboard();
                clipboard.set_text(&text);
            } else {
                let toast = Toast::new("No display / clipboard available");
                overlay.add_toast(toast);
            }
        });
    }

    {
        let back = back_button.clone();
        let git = git_button.clone();
        let add = add_button.clone();
        let save = save_button.clone();
        let nav = navigation_view.clone();
        let page = text_page.clone();
        let popover_add = add_button_popover.clone();
        let popover_git = git_popover.clone();
        let overlay = toast_overlay.clone();
        let entry = password_entry.clone();
        let text = text_view.clone();
        let status = password_status.clone();
        let win = window_title.clone();
        path_entry.connect_apply(move |row| {
            let path = row.text().to_string(); // Get the text from the entry
            if path.is_empty() {
                let toast = Toast::new("Path cannot be empty");
                overlay.add_toast(toast);
                return;
            }

            let settings = Preferences::new();
            let store_root = settings.store();

            // Show editor page
            status.set_visible(false);
            entry.set_visible(true);
            text.set_visible(true);
            add.set_visible(false);
            git.set_visible(false);
            back.set_visible(true);
            save.set_visible(true);
            nav.push(&page);

            popover_add.popdown();
            popover_git.popdown();

            // Remember where to save later
            unsafe {
                page.set_data("label", path.clone());
                page.set_data("root", store_root.clone());
            };

            win.set_title("New password");
            win.set_subtitle(&path);
            entry.set_text("");
            let buffer = text.buffer();
            buffer.set_text("");
        });
    }

    // actions
    {
        let page = text_page.clone();
        let entry = password_entry.clone();
        let text = text_view.clone();
        let overlay = toast_overlay.clone();
        let action = SimpleAction::new("save-password", None);
        action.connect_activate(move |_, _| {
            // Get label/root that we stored on `page`
            let Some(label) = non_null_to_string_option(&page, "label") else {
                let toast = Toast::new("No password entry selected");
                overlay.add_toast(toast);
                return;
            };
            let Some(root) = non_null_to_string_option(&page, "root") else {
                let toast = Toast::new("Unknown password store");
                overlay.add_toast(toast);
                return;
            };

            // Collect password + notes from the editor widgets
            let password = entry.text().to_string();

            let buffer = text.buffer();
            let (start, end) = buffer.bounds();
            let notes = buffer.text(&start, &end, false).to_string();

            if password.is_empty() {
                let toast = Toast::new("Password cannot be empty");
                overlay.add_toast(toast);
                return;
            }

            // Here we call the helper from step 2.
            // `true` => overwrite if it already exists (update).
            match write_pass_entry(&root, &label, &password, &notes, true) {
                Ok(()) => {
                    let toast = Toast::new("Password saved");
                    overlay.add_toast(toast);
                }
                Err(msg) => {
                    let toast = Toast::new(&msg);
                    overlay.add_toast(toast);
                }
            }
        });

        window.add_action(&action);
    }

    {
        let nav = navigation_view.clone();
        let page = settings_page.clone();
        let back = back_button.clone();
        let add = add_button.clone();
        let git = git_button.clone();
        let save = save_button.clone();
        let win = window_title.clone();
        let action = SimpleAction::new("open-preferences", None);
        action.connect_activate(move |_, _| {
            add.set_visible(false);
            git.set_visible(false);
            back.set_visible(true);
            save.set_visible(false);
            win.set_title("Preferences");
            win.set_subtitle("Password Store");
            nav.push(&page);
        });
        window.add_action(&action);
    }

    {
        let menu = primary_menu.clone();
        let overlay = toast_overlay.clone();
        let action = SimpleAction::new("install-locally", None);
        action.connect_activate(move |_, _| {
            if !can_install_locally() {
                let toast = Toast::new(&format!("Can not install this App locally"));
                overlay.add_toast(toast);
                return;
            }
            let items = menu.n_items();
            if items > 0 {
                menu.remove(items - 1);
            }
            let installed = is_installed_locally();
            let ok: bool = !installed && install_locally().is_ok();
            let uninstalled = installed && uninstall_locally().is_ok();
            let item = if ok || !uninstalled {
                MenuItem::new(Some("Uninstall this App"), Some("win.install-locally"))
            } else {
                MenuItem::new(Some("Install this App"), Some("win.install-locally"))
            };
            menu.append_item(&item);
        });
        window.add_action(&action);
    }

    {
        let popover = add_button_popover.clone();
        let action = SimpleAction::new("open-new-password", None);
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
        let action = SimpleAction::new("open-git", None);
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

            // TODO: clone logic
            // e.g. spawn a task, then show a toast:
            if !url.is_empty() {
                let toast = Toast::new(&format!("Cloning from {url}…"));
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
        let page = text_page.clone();
        let entry = password_entry.clone();
        let text = text_view.clone();
        let list_clone = list.clone();
        let win = window_title.clone();
        let window_clone = window.clone();
        let back = back_button.clone();
        let git = git_button.clone();
        let add = add_button.clone();
        let save = save_button.clone();
        let nav = navigation_view.clone();
        let action = SimpleAction::new("back", None);
        action.connect_activate(move |_, _| {
            nav.pop();
            let stack = nav.navigation_stack();
            if stack.n_items() > 1 {
                back.set_visible(true);
                add.set_visible(false);
                let is_text_page = nav
                    .visible_page()
                    .as_ref()
                    .map(|p| p == &page)
                    .unwrap_or(false);
                save.set_visible(is_text_page);
                if is_text_page {
                    win.set_title("Password Store");
                    let label = non_null_to_string_option(&page, "label")
                        .unwrap_or_else(|| "Unknown pass file".to_string());
                    win.set_subtitle(&label);
                }
            } else {
                back.set_visible(false);
                save.set_visible(false);
                add.set_visible(true);

                win.set_title("Password Store");
                win.set_subtitle("Manage your passwords");

                entry.set_text("");
                let buffer = text.buffer();
                buffer.set_text("");
            }
            load_passwords_async(&list_clone, git.clone(), save.clone(), window_clone.clone());
        });
        window.add_action(&action);
    }

    {
        let overlay_clone = toast_overlay.clone();
        let action = SimpleAction::new("synchronize", None);
        action.connect_activate(move |_, _| {
            let overlay = overlay_clone.clone();

            // Channel from worker → main thread
            let (tx, rx) = mpsc::channel::<String>();

            // Background worker
            thread::spawn(move || {
                let settings = Preferences::new();
                let roots = settings.stores();
                for root in roots {
                    // List of git operations we want to run for each store
                    let commands: [&[&str]; 3] = [
                        &["git", "fetch", "--all"],
                        &["git", "pull"],
                        &["git", "push"],
                    ];

                    for args in commands {
                        let output = Command::new(settings.command())
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
                                    let message = format!("{} Using: {}", fatal_line, root);
                                    eprintln!("{}", message);
                                    let _ = tx.send(message);

                                    // stop further commands for this store
                                    break;
                                }
                            }
                            Err(e) => {
                                let message = format!("Failed: {} with {e}", root);
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
                        let toast = Toast::new(&msg);
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

    // keyboard shortcuts
    app.set_accels_for_action("win.back", &["Escape"]);
    app.set_accels_for_action("win.toggle-search", &["<primary>f"]);
    app.set_accels_for_action("win.synchronize", &["<primary>s"]);
    app.set_accels_for_action("win.open-new-password", &["<primary>n"]);
    app.set_accels_for_action("win.open-git", &["<primary>i"]);
    app.set_accels_for_action("win.open-preferences", &["<primary>p"]);

    setup_search_filter(&list, &search_entry);

    if let Some(q) = startup_query {
        if !q.is_empty() {
            search_entry.set_visible(true); // make the field appear
            search_entry.set_text(&q); // fill text
            list.invalidate_filter(); // re-run filter for all rows
        }
    }

    window
}

fn load_passwords_async(list: &ListBox, git: Button, save: Button, win: ApplicationWindow) {
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

    // Spawn worker thread – ONLY data goes in here
    thread::spawn(move || {
        let all_items = match collect_all_password_items() {
            Ok(v) => v,
            Err(err) => {
                eprintln!("Error scanning pass stores: {err}");
                Vec::new()
            }
        };
        // Send everything back to main thread
        let _ = tx.send(all_items);
    });

    // Clone GTK widgets on the main thread (they stay on this thread)
    let list_clone = list.clone();
    let git_clone = git.clone();
    let save_clone = save.clone();

    // Poll the channel from the main thread using a GLib timeout
    //
    //   - timeout_add_local does NOT require Send
    //   - closure runs on the GTK main loop thread
    glib::timeout_add_local(Duration::from_millis(50), move || {
        match rx.try_recv() {
            Ok(items) => {
                let mut index = 0;
                let len = items.len();
                let empty = items.is_empty();
                while index < len {
                    let item = items[index].clone();

                    let row = ListBoxRow::new();
                    let action_row = ActionRow::builder()
                        .title(item.basename.clone()) // first column: basename
                        .subtitle(item.relative_path.clone()) // second line: relative_path
                        .activatable(true) // makes row respond to double-click/Enter
                        .build();

                    // 3) Per-row menu button on the right
                    let menu_button = MenuButton::builder()
                        .icon_name("view-more-symbolic")
                        .has_frame(false)
                        .css_classes(vec!["flat"])
                        .build();

                    // 4) Popover with actions
                    let popover = Popover::new();
                    let popover_box = GtkBox::new(Orientation::Vertical, 0);

                    let copy_btn = Button::with_label("Copy password");
                    copy_btn.add_css_class("flat");
                    copy_btn.add_css_class("linked");
                    let rename_btn = Button::with_label("Rename or move");
                    rename_btn.add_css_class("flat");
                    rename_btn.add_css_class("linked");
                    let delete_btn = Button::with_label("Delete");
                    delete_btn.add_css_class("flat");
                    delete_btn.add_css_class("linked");
                    delete_btn.add_css_class("destructive-action");

                    popover_box.append(&copy_btn);
                    popover_box.append(&rename_btn);
                    popover_box.append(&delete_btn);

                    popover.set_child(Some(&popover_box));
                    menu_button.set_popover(Some(&popover));

                    // Attach menu button as suffix (right side) of the row
                    action_row.add_suffix(&menu_button);

                    // Put the ActionRow inside the ListBoxRow
                    row.set_child(Some(&action_row));

                    // Store full path on row for later use
                    unsafe {
                        row.set_data("name", item.basename.clone());
                        row.set_data("root", item.store_path.clone());
                        row.set_data("label", item.label());
                    }
                    // Copy password
                    {
                        let entry = item.clone();
                        let popover = popover.clone();
                        copy_btn.connect_clicked(move |_| {
                            popover.popdown();
                            let item = entry.clone();
                            std::thread::spawn({
                                move || {
                                    let settings = Preferences::new();
                                    let _ = Command::new(settings.command())
                                        .env("PASSWORD_STORE_DIR", &item.store_path)
                                        .arg("-c")
                                        .arg(&item.label())
                                        .status();
                                }
                            });
                        });
                    }

                    // rename pass file
                    {
                        let entry = item.clone();
                        let win = win.clone();
                        rename_btn.connect_clicked(move |_| {
                            // Create dialog
                            let dialog = adw::MessageDialog::new(
                                Some(&win),
                                None,
                                Some("Rename or move"),
                            );

                            // Add an EntryRow as extra child
                            let entry_row = adw::EntryRow::new();
                            entry_row.set_title("New path");
                            entry_row.set_text(&entry.label()); // prefill with current label
                            dialog.set_extra_child(Some(&entry_row));

                            // Responses
                            dialog.add_response("cancel", "Cancel");
                            dialog.add_response("rename", "Rename");
                            dialog.set_default_response(Some("rename"));
                            dialog.set_close_response("cancel");

                            // Handle response
                            let entry_clone = entry.clone();
                            dialog.connect_response(None, move |dialog, response| {
                                if response != "rename" {
                                    dialog.close();
                                    return;
                                }

                                let new_label = entry_row.text().to_string();
                                if new_label.is_empty() {
                                    return;
                                }

                                let old_label = entry_clone.label();
                                let root = entry_clone.store_path.clone();

                                // Don’t move any GTK objects into the thread – only Strings
                                std::thread::spawn(move || {
                                    let settings = Preferences::new();
                                    let _ = Command::new(settings.command())
                                        .env("PASSWORD_STORE_DIR", root)
                                        .arg("mv")
                                        .arg(&old_label)
                                        .arg(&new_label)
                                        .status();
                                });

                                dialog.close();
                            });

                            dialog.present();
                        });
                    }

                    // delete pass file
                    {
                        let entry = item.clone();
                        let row_clone = row.clone();
                        let list = list_clone.clone();
                        delete_btn.connect_clicked(move |_| {
                            std::thread::spawn({
                                let root = entry.store_path.clone();
                                let label = entry.label();
                                move || {
                                    let settings = Preferences::new();
                                    let _ = Command::new(settings.command())
                                        .env("PASSWORD_STORE_DIR", root)
                                        .arg("rm")
                                        .arg("-rf")
                                        .arg(&label)
                                        .status();
                                }
                            });
                            list.remove(&row_clone);
                        });
                    }

                    list_clone.append(&row);
                    index += 1;
                }

                if empty {
                    save_clone.set_visible(false);
                }
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

                save_clone.set_visible(false);
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

fn write_pass_entry(
    store_root: &str, // e.g. entry.store_path or selected store
    label: &str,      // e.g. "mail/github.com/nick"
    password: &str,
    notes: &str,
    overwrite: bool,
) -> Result<(), String> {
    let settings = Preferences::new();

    let mut cmd = Command::new(settings.command());
    cmd.env("PASSWORD_STORE_DIR", store_root)
        .arg("insert")
        .arg("-m"); // read from stdin

    if overwrite {
        cmd.arg("-f"); // force overwrite if it exists
    }

    cmd.arg(label)
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::piped());

    let mut child = cmd
        .spawn()
        .map_err(|e| format!("Failed to run pass: {e}"))?;

    {
        let stdin = child
            .stdin
            .as_mut()
            .ok_or("Failed to open stdin for pass")?;

        // 1st line = password
        writeln!(stdin, "{password}").map_err(|e| format!("Failed to write password: {e}"))?;

        // remaining lines = notes (optional)
        if !notes.is_empty() {
            writeln!(stdin, "{notes}").map_err(|e| format!("Failed to write notes: {e}"))?;
        }
    }

    let status = child
        .wait()
        .map_err(|e| format!("Failed to wait for pass: {e}"))?;

    if status.success() {
        Ok(())
    } else {
        Err(format!("pass insert failed: {status}"))
    }
}
