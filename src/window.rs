#[cfg(feature = "setup")]
use crate::setup::*;
#[cfg(feature = "setup")]
use adw::gio::{Menu, MenuItem};

use crate::config::APP_ID;
use crate::item::{collect_all_password_items, PassEntry};
use crate::methods::non_null_to_string_option;
use crate::preferences::Preferences;
use adw::gio::{prelude::*, SimpleAction};
use adw::{
    glib, prelude::*, ActionRow, Application, ApplicationWindow, EntryRow, NavigationPage,
    NavigationView, PasswordEntryRow, StatusPage, Toast, ToastOverlay, WindowTitle,
};
use adw::gtk::{
    gdk::Display, Builder, Button, ListBox, ListBoxRow, MenuButton, Popover, SearchEntry, Spinner,
    TextView,
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
    let settings = Preferences::new();
    let settings_page: NavigationPage = builder
        .object("settings_page")
        .expect("Failed to get settings page");
    let pass_row: EntryRow = builder
        .object("pass_command_row")
        .expect("Failed to get pass row");
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
    );

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
    let otp_entry: PasswordEntryRow = builder
        .object("otp_entry")
        .expect("Failed to get otp_entry");
    let copy_password_button: Button = builder
        .object("copy_password_button")
        .expect("Failed to get copy_password_button");
    let copy_otp_button: Button = builder
        .object("copy_otp_button")
        .expect("Failed to get copy_otp_button");
    let text_view: TextView = builder
        .object("text_view")
        .expect("Failed to get text_view");

    // Selecting an item from the list
    {
        let nav = navigation_view.clone();
        let page = text_page.clone();
        let back = back_button.clone();
        let add = add_button.clone();
        let find = find_button.clone();
        let git = git_button.clone();
        let save = save_button.clone();
        let entry = password_entry.clone();
        let otp = otp_entry.clone();
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
                let toast = Toast::new("Unknown password store.");
                overlay.add_toast(toast);
                return;
            };
            // Navigate to the text editor page and update header buttons
            add.set_visible(false);
            find.set_visible(false);
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
                let output = settings
                    .command()
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
            let otp_entry = otp.clone();
            let text_view = text.clone();
            let overlay = overlay.clone();
            let label_for_otp = label.clone();
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
                        let otp = rest.contains("otpauth://");
                        let buffer = text_view.buffer();
                        buffer.set_text(&rest);

                        otp_entry.set_visible(otp);
                        if otp {
                            let settings = Preferences::new();
                            match settings
                                .command()
                                .env("PASSWORD_STORE_DIR", &settings.store())
                                .args(["otp", &label_for_otp])
                                .output()
                            {
                                Ok(o) if o.status.success() => {
                                    let code =
                                        String::from_utf8_lossy(&o.stdout).trim().to_string();
                                    otp_entry.set_text(&code);
                                }
                                Ok(o) => {
                                    let error = String::from_utf8_lossy(&o.stderr).trim().to_string();
                                    if !error.is_empty() {
                                        let toast = Toast::new(&error);
                                        overlay.add_toast(toast);
                                    }
                                }
                                Err(e) => {
                                    let toast = Toast::new(&format!("OTP Failed: {e}"));
                                    overlay.add_toast(toast);
                                }
                            }
                        }

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

    // Pass command preference
    {
        let overlay = toast_overlay.clone();
        let preferences = settings.clone();
        pass_row.connect_apply(move |row| {
            let text = row.text().to_string();
            let text = text.trim();
            if text.is_empty() {
                let toast = Toast::new("Pass command cannot be empty");
                overlay.add_toast(toast);
                return;
            }
            if let Err(err) = preferences.set_command(text) {
                let message = err.message.to_string();
                let toast = Toast::new(&message);
                overlay.add_toast(toast);
            }
        });
    }
    // Copy password button on password page
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
                let toast = Toast::new("No display or clipboard available");
                overlay.add_toast(toast);
            }
        });
    }
    // Copy OTP button on password page
    {
        let overlay = toast_overlay.clone();
        let entry = otp_entry.clone();
        let btn = copy_otp_button.clone();
        btn.connect_clicked(move |_| {
            entry.grab_focus_without_selecting();
            let text = entry.text().to_string();
            if let Some(display) = Display::default() {
                let clipboard = display.clipboard();
                clipboard.set_text(&text);
            } else {
                let toast = Toast::new("No display or clipboard available");
                overlay.add_toast(toast);
            }
        });
    }
    // new password
    {
        let back = back_button.clone();
        let git = git_button.clone();
        let add = add_button.clone();
        let find = find_button.clone();
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
            let path = row.text().to_string();
            let settings = Preferences::new();
            let store_root = settings.store();
            if path.is_empty() {
                let toast = Toast::new("Path cannot be empty");
                overlay.add_toast(toast);
                return;
            }
            status.set_visible(false);
            entry.set_visible(true);
            text.set_visible(true);
            add.set_visible(false);
            find.set_visible(false);
            git.set_visible(false);
            back.set_visible(true);
            save.set_visible(true);
            nav.push(&page);

            popover_add.popdown();
            popover_git.popdown();
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
            let buffer = text.buffer();
            let (start, end) = buffer.bounds();
            let notes = buffer.text(&start, &end, false).to_string();
            let password = entry.text().to_string();
            if password.is_empty() {
                let toast = Toast::new("Password cannot be empty");
                overlay.add_toast(toast);
                return;
            }
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
    // open preferences
    {
        let nav = navigation_view.clone();
        let page = settings_page.clone();
        let back = back_button.clone();
        let add = add_button.clone();
        let find = find_button.clone();
        let git = git_button.clone();
        let save = save_button.clone();
        let win = window_title.clone();
        let command = pass_row.clone();
        let list = password_stores.clone();
        let action = SimpleAction::new("open-preferences", None);
        action.connect_activate(move |_, _| {
            add.set_visible(false);
            find.set_visible(false);
            git.set_visible(false);
            back.set_visible(true);
            save.set_visible(false);
            win.set_title("Preferences");
            win.set_subtitle("Password Store");
            nav.push(&page);

            let settings = Preferences::new();
            command.set_text(&settings.command_value());
            rebuild_store_list(&list, &settings);
        });
        window.add_action(&action);
    }

    #[cfg(feature = "setup")]
    {
        let menu = primary_menu.clone();
        let overlay = toast_overlay.clone();
        let action = SimpleAction::new("install-locally", None);
        action.connect_activate(move |_, _| {
            if !can_install_locally() {
                let toast = Toast::new(&format!("Can not install this App"));
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

            if !url.is_empty() {
                let toast = Toast::new(&format!("Cloning from {url}…"));
                overlay.add_toast(toast);
            }
        });
        window.add_action(&action);
    }

    {
        let find = search_entry.clone();
        let action = SimpleAction::new("toggle-find", None);
        action.connect_activate(move |_, _| {
            let visible = find.is_visible();
            find.set_visible(!visible);
            if !visible {
                find.grab_focus();
            }
        });
        window.add_action(&action);
    }

    {
        let overlay = toast_overlay.clone();
        let page = text_page.clone();
        let entry = password_entry.clone();
        let text = text_view.clone();
        let list_clone = list.clone();
        let win = window_title.clone();
        let back = back_button.clone();
        let git = git_button.clone();
        let add = add_button.clone();
        let find = find_button.clone();
        let save = save_button.clone();
        let nav = navigation_view.clone();
        let action = SimpleAction::new("back", None);
        action.connect_activate(move |_, _| {
            nav.pop();
            let stack = nav.navigation_stack();
            if stack.n_items() > 1 {
                back.set_visible(true);
                add.set_visible(false);
                find.set_visible(false);
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
                find.set_visible(true);

                win.set_title("Password Store");
                win.set_subtitle("Manage your passwords");

                entry.set_text("");
                let buffer = text.buffer();
                buffer.set_text("");
            }
            load_passwords_async(
                &list_clone,
                git.clone(),
                find.clone(),
                save.clone(),
                overlay.clone(),
            );
        });
        window.add_action(&action);
    }

    {
        let overlay_clone = toast_overlay.clone();
        let action = SimpleAction::new("synchronize", None);
        action.connect_activate(move |_, _| {
            let overlay = overlay_clone.clone();
            // Channel from worker to main thread
            let (tx, rx) = mpsc::channel::<String>();
            // Background worker
            thread::spawn(move || {
                let settings = Preferences::new();
                let roots = settings.stores();
                for root in roots {
                    let commands: [&[&str]; 3] = [
                        &["git", "fetch", "--all"],
                        &["git", "pull"],
                        &["git", "push"],
                    ];
                    for args in commands {
                        let output = settings
                            .command()
                            .env("PASSWORD_STORE_DIR", &root)
                            .args(args)
                            .output();
                        match output {
                            Ok(out) => {
                                if !out.status.success() {
                                    let stderr = String::from_utf8_lossy(&out.stderr);
                                    let fatal_line = stderr
                                        .lines()
                                        .rev()
                                        .find(|line| line.contains("fatal:"))
                                        .unwrap_or(stderr.trim());
                                    let message = format!("{} Using: {}", fatal_line, root);
                                    eprintln!("{}", &message);
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

            // Main-thread: poll for messages
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
    app.set_accels_for_action("win.toggle-find", &["<primary>f"]);
    app.set_accels_for_action("win.synchronize", &["<primary>s"]);
    app.set_accels_for_action("win.open-new-password", &["<primary>n"]);
    app.set_accels_for_action("win.open-git", &["<primary>i"]);
    app.set_accels_for_action("win.open-preferences", &["<primary>p"]);

    setup_search_filter(&list, &search_entry);

    if let Some(q) = startup_query {
        if !q.is_empty() {
            search_entry.set_visible(true);
            search_entry.set_text(&q);
            list.invalidate_filter();
        }
    }

    window
}

fn load_passwords_async(
    list: &ListBox,
    git: Button,
    find: Button,
    save: Button,
    overlay: ToastOverlay,
) {
    while let Some(child) = list.first_child() {
        list.remove(&child);
    }

    git.set_visible(false);
    find.set_visible(true);

    let bussy = Spinner::new();
    bussy.start();

    let symbolic = format!("{APP_ID}-symbolic");
    let placeholder = StatusPage::builder()
        .icon_name(symbolic)
        .child(&bussy)
        .build();
    list.set_placeholder(Some(&placeholder));

    // Standard library channel: main thread will own `rx`, worker gets `tx`
    let (tx, rx) = mpsc::channel::<Vec<PassEntry>>();
    // Spawn worker thread
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
    // Clone GTK widgets on the main thread
    let list_clone = list.clone();
    let git_clone = git.clone();
    let find_clone = find.clone();
    let save_clone = save.clone();
    let toast_overlay = overlay.clone();
    // Poll the channel from the main thread using a GLib timeout
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
                    let menu_button = MenuButton::builder()
                        .icon_name("view-more-symbolic")
                        .has_frame(false)
                        .css_classes(vec!["flat"])
                        .build();
                    let popover = Popover::new();
                    let rename_row = EntryRow::new();
                    rename_row.set_title("Rename or move");
                    rename_row.set_show_apply_button(true);
                    rename_row.set_text(&item.label());
                    let copy_btn = Button::from_icon_name("edit-copy-symbolic");
                    copy_btn.add_css_class("flat");
                    action_row.add_suffix(&copy_btn);
                    let delete_btn = Button::from_icon_name("user-trash-symbolic");
                    delete_btn.add_css_class("flat");
                    delete_btn.add_css_class("destructive-action");
                    rename_row.add_suffix(&delete_btn);

                    popover.set_child(Some(&rename_row));
                    menu_button.set_popover(Some(&popover));
                    action_row.add_suffix(&menu_button);
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
                                    let _ = settings
                                        .command()
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
                        let overlay = toast_overlay.clone();
                        rename_row.connect_apply(move |row| {
                            let new_label = row.text().to_string();

                            if new_label.is_empty() {
                                let toast = adw::Toast::new("New name cannot be empty");
                                overlay.add_toast(toast);
                                return;
                            }

                            let old_label = entry.label();
                            if new_label == old_label {
                                let toast = adw::Toast::new("Name unchanged");
                                overlay.add_toast(toast);
                                return;
                            }

                            let root = entry.store_path.clone();
                            let settings = Preferences::new();
                            let status = settings
                                .command()
                                .env("PASSWORD_STORE_DIR", &root)
                                .arg("mv")
                                .arg(&old_label)
                                .arg(&new_label)
                                .status();
                            match status {
                                Ok(s) if s.success() => {
                                    let (parent, tail) = match new_label.rsplit_once('/') {
                                        Some((parent, tail)) => (parent, tail),
                                        None => ("", new_label.as_str()),
                                    };
                                    action_row.set_title(&tail);
                                    action_row.set_subtitle(&parent);
                                }
                                Ok(_) | Err(_) => {
                                    let toast = adw::Toast::new("Failed to rename entry");
                                    overlay.add_toast(toast);
                                }
                            }
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
                                    let _ = settings
                                        .command()
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
                    find_clone.set_visible(false);
                }
                git_clone.set_visible(empty);

                let symbolic = format!("{APP_ID}-symbolic");
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
                        .description("Try another query.")
                        .build()
                };
                list_clone.set_placeholder(Some(&placeholder));

                // One-shot: stop calling this timeout
                glib::ControlFlow::Break
            }
            Err(TryRecvError::Empty) => {
                // Worker not done yet
                glib::ControlFlow::Continue
            }
            Err(TryRecvError::Disconnected) => {
                // Worker died

                let symbolic = format!("{APP_ID}-symbolic");
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

        // empty query, show everything
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
    store_root: &str,
    label: &str,
    password: &str,
    notes: &str,
    overwrite: bool,
) -> Result<(), String> {
    let settings = Preferences::new();
    let mut cmd: Command = settings.command();
    cmd.env("PASSWORD_STORE_DIR", store_root)
        .arg("insert")
        .arg("-m"); // read from stdin
    if overwrite {
        cmd.arg("-f");
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

        // remaining lines = optional notes
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

fn rebuild_store_list(list: &ListBox, settings: &Preferences) {
    while let Some(child) = list.first_child() {
        list.remove(&child);
    }

    for store in settings.stores() {
        append_store_row(list, settings, &store);
    }

    let add_row = EntryRow::new();
    add_row.set_title("Add password store (absolute path)");
    add_row.set_show_apply_button(true);
    list.append(&add_row);

    {
        let settings = settings.clone();
        let list = list.clone();
        add_row.connect_apply(move |row| {
            let text = row.text().trim().to_string();
            if text.is_empty() {
                return;
            }
            let mut stores = settings.stores();
            if stores.contains(&text) {
                return;
            }
            stores.push(text.clone());
            if let Err(err) = settings.set_stores(stores) {
                eprintln!("Failed to save stores: {err}");
                return;
            } else {
                append_store_row(&list, &settings, &text);
                row.set_text(""); // clear field
            }
        });
    }
}

fn append_store_row(list: &ListBox, settings: &Preferences, store: &str) {
    let row = ActionRow::builder().title(store).build();
    row.set_activatable(false);

    let delete_btn = Button::from_icon_name("user-trash-symbolic");
    delete_btn.add_css_class("flat");
    row.add_suffix(&delete_btn);

    list.append(&row);

    let settings = settings.clone();
    let list = list.clone();
    let row_clone = row.clone();
    let store = store.to_string();

    delete_btn.connect_clicked(move |_| {
        let mut stores = settings.stores();
        if let Some(pos) = stores.iter().position(|s| s == &store) {
            stores.remove(pos);
            if let Err(err) = settings.set_stores(stores) {
                eprintln!("Failed to save stores: {err}");
            } else {
                list.remove(&row_clone);
            }
        }
    });
}
