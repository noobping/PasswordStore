#[cfg(feature = "setup")]
use crate::setup::*;
#[cfg(feature = "setup")]
use adw::gio::{Menu, MenuItem};

use crate::config::APP_ID;
use crate::item::{collect_all_password_items, OpenPassFile, PassEntry};
use crate::logging::{
    log_error, log_snapshot, run_command_output, run_command_status, run_command_with_input,
    CommandLogOptions,
};
use crate::methods::{
    clear_opened_pass_file, get_opened_pass_file, is_opened_pass_file,
    non_null_to_string_option, refresh_opened_pass_file_from_contents, set_opened_pass_file,
};
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

    #[cfg(any(feature = "setup", feature = "host"))]
    let backend_preferences: adw::PreferencesGroup = builder
        .object("backend_preferences")
        .expect("Failed to get backend_preferences");
    #[cfg(any(feature = "setup", feature = "host"))]
    backend_preferences.set_visible(true);

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
    #[cfg(any(feature = "setup", feature = "host"))]
    let settings = Preferences::new();
    let settings_page: NavigationPage = builder
        .object("settings_page")
        .expect("Failed to get settings page");
    let log_page: NavigationPage = builder
        .object("log_page")
        .expect("Failed to get log page");
    let git_busy_page: NavigationPage = builder
        .object("git_busy_page")
        .expect("Failed to get git busy page");
    let git_busy_status: StatusPage = builder
        .object("git_busy_status")
        .expect("Failed to get git busy status");
    #[cfg(any(feature = "setup", feature = "host"))]
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
        true,
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
    let log_view: TextView = builder
        .object("log_view")
        .expect("Failed to get log_view");

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
        let username = username_entry.clone();
        let otp = otp_entry.clone();
        let status = password_status.clone();
        let text = text_view.clone();
        let overlay = toast_overlay.clone();
        let win = window_title.clone();
        list.connect_row_activated(move |_list, row| {
            let label = non_null_to_string_option(row, "label");
            let root = non_null_to_string_option(row, "root");

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
            let opened_pass_file = OpenPassFile::from_label(root, &label);
            let pass_label = opened_pass_file.label();
            let store_for_thread = opened_pass_file.store_path().to_string();
            set_opened_pass_file(opened_pass_file.clone());

            // Navigate to the text editor page and update header buttons
            add.set_visible(false);
            find.set_visible(false);
            git.set_visible(false);
            back.set_visible(true);
            save.set_visible(true);
            win.set_title(opened_pass_file.title());
            win.set_subtitle(&pass_label);
            text.set_visible(false);
            entry.set_visible(false);
            sync_username_row(&username, Some(&opened_pass_file));
            otp.set_visible(false);
            status.set_visible(true);
            nav.push(&page);

            // Background worker: run `pass <label>`
            let (tx, rx) = mpsc::channel::<Result<String, String>>();
            let label_for_thread = pass_label.clone();
            thread::spawn(move || {
                let settings = Preferences::new();
                let mut cmd = settings.command();
                cmd.env("PASSWORD_STORE_DIR", &store_for_thread)
                    .arg(&label_for_thread);
                let output =
                    run_command_output(&mut cmd, "Read password entry", CommandLogOptions::SENSITIVE);
                let result = match output {
                    Ok(o) if o.status.success() => {
                        Ok(String::from_utf8_lossy(&o.stdout).to_string())
                    }
                    Ok(o) => {
                        let stderr = String::from_utf8_lossy(&o.stderr).trim().to_string();
                        if stderr.is_empty() {
                            Err(format!("pass failed: {}", o.status))
                        } else {
                            Err(stderr)
                        }
                    }
                    Err(e) => Err(format!("Failed to run pass: {e}")),
                };

                let _ = tx.send(result);
            });

            // UI updater: poll the channel from the main thread
            let password_status = status.clone();
            let password_entry = entry.clone();
            let username_entry = username.clone();
            let otp_entry = otp.clone();
            let text_view = text.clone();
            let overlay = overlay.clone();
            let opened_pass_file_for_result = opened_pass_file.clone();
            let label_for_otp = pass_label.clone();
            let store_for_otp = opened_pass_file.store_path().to_string();
            glib::timeout_add_local(Duration::from_millis(50), move || {
                use std::sync::mpsc::TryRecvError;

                if !is_opened_pass_file(&opened_pass_file_for_result) {
                    return glib::ControlFlow::Break;
                }

                match rx.try_recv() {
                    Ok(Ok(output)) => {
                        let updated_pass_file = refresh_opened_pass_file_from_contents(
                            &opened_pass_file_for_result,
                            &output,
                        );
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
                        sync_username_row(&username_entry, updated_pass_file.as_ref());

                        otp_entry.set_visible(otp);
                        if otp {
                            let settings = Preferences::new();
                            let mut cmd = settings.command();
                            cmd.env("PASSWORD_STORE_DIR", &store_for_otp)
                                .args(["otp", &label_for_otp]);
                            match run_command_output(
                                &mut cmd,
                                "Read OTP code",
                                CommandLogOptions::SENSITIVE,
                            ) {
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
                        } else {
                            otp_entry.set_text("");
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
    #[cfg(any(feature = "setup", feature = "host"))]
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
    // Copy username button on password page
    {
        let overlay = toast_overlay.clone();
        let entry = username_entry.clone();
        let btn = copy_username_button.clone();
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
        let username = username_entry.clone();
        let otp = otp_entry.clone();
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
            let opened_pass_file = OpenPassFile::from_label(store_root, &path);
            set_opened_pass_file(opened_pass_file);
            status.set_visible(false);
            entry.set_visible(true);
            sync_username_row(&username, get_opened_pass_file().as_ref());
            otp.set_visible(false);
            text.set_visible(true);
            add.set_visible(false);
            find.set_visible(false);
            git.set_visible(false);
            back.set_visible(true);
            save.set_visible(true);
            nav.push(&page);

            popover_add.popdown();
            popover_git.popdown();
            win.set_title("New password");
            win.set_subtitle(&path);
            entry.set_text("");
            otp.set_text("");
            let buffer = text.buffer();
            buffer.set_text("");
        });
    }

    // actions
    {
        let entry = password_entry.clone();
        let username = username_entry.clone();
        let text = text_view.clone();
        let overlay = toast_overlay.clone();
        let action = SimpleAction::new("save-password", None);
        action.connect_activate(move |_, _| {
            let Some(pass_file) = get_opened_pass_file() else {
                let toast = Toast::new("No password entry selected");
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
            let label = pass_file.label();
            match write_pass_entry(pass_file.store_path(), &label, &password, &notes, true) {
                Ok(()) => {
                    let contents = if notes.is_empty() {
                        password.clone()
                    } else {
                        format!("{password}\n{notes}")
                    };
                    let updated_pass_file =
                        refresh_opened_pass_file_from_contents(&pass_file, &contents);
                    sync_username_row(&username, updated_pass_file.as_ref());
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
        #[cfg(any(feature = "setup", feature = "host"))]
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
            #[cfg(any(feature = "setup", feature = "host"))]
            command.set_text(&settings.command_value());
            rebuild_store_list(&list, &settings);
        });
        window.add_action(&action);
    }

    {
        let nav = navigation_view.clone();
        let page = log_page.clone();
        let back = back_button.clone();
        let add = add_button.clone();
        let find = find_button.clone();
        let git = git_button.clone();
        let save = save_button.clone();
        let win = window_title.clone();
        let action = SimpleAction::new("open-log", None);
        action.connect_activate(move |_, _| {
            show_log_page(&nav, &page, &back, &add, &find, &git, &save, &win);
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
        let popover = git_popover.clone();
        let list_clone = list.clone();
        let nav = navigation_view.clone();
        let text_page = text_page.clone();
        let settings_page = settings_page.clone();
        let log_page = log_page.clone();
        let busy_page = git_busy_page.clone();
        let busy_status = git_busy_status.clone();
        let back = back_button.clone();
        let git = git_button.clone();
        let add = add_button.clone();
        let find = find_button.clone();
        let save = save_button.clone();
        let win = window_title.clone();
        let username = username_entry.clone();
        let action = SimpleAction::new("git-clone", None);
        action.connect_activate(move |_, _| {
            let url = entry.text().trim().to_string();
            if url.is_empty() {
                let toast = Toast::new("Git URL cannot be empty");
                overlay.add_toast(toast);
                return;
            }

            popover.popdown();
            show_git_busy_page(
                &nav,
                &busy_page,
                &busy_status,
                &back,
                &add,
                &find,
                &git,
                &save,
                &win,
                "Cloning password store",
                Some(&url),
            );

            let toast = Toast::new(&format!("Cloning from {url}..."));
            overlay.add_toast(toast);

            let (tx, rx) = mpsc::channel::<Result<(), String>>();
            let url_for_thread = url.clone();
            thread::spawn(move || {
                let settings = Preferences::new();
                let store_root = settings.store();
                let mut cmd = settings.command();
                cmd.env("PASSWORD_STORE_DIR", &store_root)
                    .args(["git", "clone", &url_for_thread]);
                let result =
                    match run_command_output(&mut cmd, "Clone password store", CommandLogOptions::DEFAULT)
                    {
                        Ok(output) if output.status.success() => Ok(()),
                        Ok(output) => {
                            let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
                            if stderr.is_empty() {
                                Err(format!("Failed to clone password store: {}", output.status))
                            } else {
                                Err(stderr)
                            }
                        }
                        Err(err) => Err(format!("Failed to run clone command: {err}")),
                    };
                let _ = tx.send(result);
            });

            let overlay = overlay.clone();
            let entry = entry.clone();
            let list = list_clone.clone();
            let nav = nav.clone();
            let text_page = text_page.clone();
            let settings_page = settings_page.clone();
            let log_page = log_page.clone();
            let busy_page = busy_page.clone();
            let back = back.clone();
            let git = git.clone();
            let add = add.clone();
            let find = find.clone();
            let save = save.clone();
            let win = win.clone();
            let username = username.clone();
            glib::timeout_add_local(Duration::from_millis(50), move || match rx.try_recv() {
                Ok(Ok(())) => {
                    entry.set_text("");
                    finish_git_busy_page(
                        &nav,
                        &busy_page,
                        &text_page,
                        &settings_page,
                        &log_page,
                        &back,
                        &add,
                        &find,
                        &git,
                        &save,
                        &win,
                        &username,
                    );
                    let toast = Toast::new("Password store cloned");
                    overlay.add_toast(toast);
                    let show_list_actions = nav.navigation_stack().n_items() <= 1;
                    load_passwords_async(
                        &list,
                        git.clone(),
                        find.clone(),
                        save.clone(),
                        overlay.clone(),
                        show_list_actions,
                    );
                    glib::ControlFlow::Break
                }
                Ok(Err(message)) => {
                    finish_git_busy_page(
                        &nav,
                        &busy_page,
                        &text_page,
                        &settings_page,
                        &log_page,
                        &back,
                        &add,
                        &find,
                        &git,
                        &save,
                        &win,
                        &username,
                    );
                    let toast = Toast::new(&message);
                    overlay.add_toast(toast);
                    glib::ControlFlow::Break
                }
                Err(TryRecvError::Empty) => glib::ControlFlow::Continue,
                Err(TryRecvError::Disconnected) => {
                    finish_git_busy_page(
                        &nav,
                        &busy_page,
                        &text_page,
                        &settings_page,
                        &log_page,
                        &back,
                        &add,
                        &find,
                        &git,
                        &save,
                        &win,
                        &username,
                    );
                    let toast = Toast::new("Clone command stopped unexpectedly");
                    overlay.add_toast(toast);
                    glib::ControlFlow::Break
                }
            });
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
        let settings = settings_page.clone();
        let log_page = log_page.clone();
        let busy_page = git_busy_page.clone();
        let entry = password_entry.clone();
        let username = username_entry.clone();
        let otp = otp_entry.clone();
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
            let busy_visible = nav
                .visible_page()
                .as_ref()
                .map(|visible| visible == &busy_page)
                .unwrap_or(false);
            if busy_visible {
                return;
            }

            nav.pop();
            let stack = nav.navigation_stack();
            if stack.n_items() > 1 {
                back.set_visible(true);
                add.set_visible(false);
                find.set_visible(false);
                let visible_page = nav.visible_page();
                let is_text_page = visible_page
                    .as_ref()
                    .map(|p| p == &page)
                    .unwrap_or(false);
                let is_settings_page = visible_page
                    .as_ref()
                    .map(|p| p == &settings)
                    .unwrap_or(false);
                let is_log_page = visible_page
                    .as_ref()
                    .map(|p| p == &log_page)
                    .unwrap_or(false);
                save.set_visible(is_text_page);
                if is_text_page {
                    if let Some(pass_file) = get_opened_pass_file() {
                        let label = pass_file.label();
                        win.set_title(pass_file.title());
                        win.set_subtitle(&label);
                        sync_username_row(&username, Some(&pass_file));
                    } else {
                        win.set_title("Password Store");
                        win.set_subtitle("Manage your passwords");
                        sync_username_row(&username, None);
                    }
                } else if is_settings_page {
                    win.set_title("Preferences");
                    win.set_subtitle("Password Store");
                } else if is_log_page {
                    win.set_title("Logs");
                    win.set_subtitle("Command output");
                }
            } else {
                clear_opened_pass_file();
                back.set_visible(false);
                save.set_visible(false);
                add.set_visible(true);
                find.set_visible(true);

                win.set_title("Password Store");
                win.set_subtitle("Manage your passwords");

                entry.set_text("");
                sync_username_row(&username, None);
                otp.set_visible(false);
                otp.set_text("");
                let buffer = text.buffer();
                buffer.set_text("");
            }
            load_passwords_async(
                &list_clone,
                git.clone(),
                find.clone(),
                save.clone(),
                overlay.clone(),
                stack.n_items() <= 1,
            );
        });
        window.add_action(&action);
    }

    {
        let overlay_clone = toast_overlay.clone();
        let nav = navigation_view.clone();
        let text_page = text_page.clone();
        let settings_page = settings_page.clone();
        let log_page = log_page.clone();
        let busy_page = git_busy_page.clone();
        let busy_status = git_busy_status.clone();
        let back = back_button.clone();
        let git = git_button.clone();
        let add = add_button.clone();
        let find = find_button.clone();
        let save = save_button.clone();
        let win = window_title.clone();
        let username = username_entry.clone();
        let list_clone = list.clone();
        let action = SimpleAction::new("synchronize", None);
        action.connect_activate(move |_, _| {
            let overlay = overlay_clone.clone();
            show_git_busy_page(
                &nav,
                &busy_page,
                &busy_status,
                &back,
                &add,
                &find,
                &git,
                &save,
                &win,
                "Synchronizing password stores",
                Some("Running git fetch, pull, and push."),
            );
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
                        let mut cmd = settings.command();
                        cmd.env("PASSWORD_STORE_DIR", &root).args(args);
                        let output = run_command_output(
                            &mut cmd,
                            &format!("Synchronize password store {root}"),
                            CommandLogOptions::DEFAULT,
                        );
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
                                    let _ = tx.send(message);

                                    // stop further commands for this store
                                    break;
                                }
                            }
                            Err(e) => {
                                let message = format!("Failed: {} with {e}", root);
                                let _ = tx.send(message);
                                break;
                            }
                        }
                    }
                }
            });

            // Main-thread: poll for messages
            let nav = nav.clone();
            let text_page = text_page.clone();
            let settings_page = settings_page.clone();
            let log_page = log_page.clone();
            let busy_page = busy_page.clone();
            let back = back.clone();
            let git = git.clone();
            let add = add.clone();
            let find = find.clone();
            let save = save.clone();
            let win = win.clone();
            let username = username.clone();
            let list = list_clone.clone();
            glib::timeout_add_local(Duration::from_millis(100), move || {
                match rx.try_recv() {
                    Ok(msg) => {
                        finish_git_busy_page(
                            &nav,
                            &busy_page,
                            &text_page,
                            &settings_page,
                            &log_page,
                            &back,
                            &add,
                            &find,
                            &git,
                            &save,
                            &win,
                            &username,
                        );
                        let toast = Toast::new(&msg);
                        overlay.add_toast(toast);
                        let show_list_actions = nav.navigation_stack().n_items() <= 1;
                        load_passwords_async(
                            &list,
                            git.clone(),
                            find.clone(),
                            save.clone(),
                            overlay.clone(),
                            show_list_actions,
                        );
                        glib::ControlFlow::Break
                    }
                    Err(TryRecvError::Empty) => glib::ControlFlow::Continue,
                    Err(TryRecvError::Disconnected) => {
                        finish_git_busy_page(
                            &nav,
                            &busy_page,
                            &text_page,
                            &settings_page,
                            &log_page,
                            &back,
                            &add,
                            &find,
                            &git,
                            &save,
                            &win,
                            &username,
                        );
                        let show_list_actions = nav.navigation_stack().n_items() <= 1;
                        load_passwords_async(
                            &list,
                            git.clone(),
                            find.clone(),
                            save.clone(),
                            overlay.clone(),
                            show_list_actions,
                        );
                        glib::ControlFlow::Break
                    }
                }
            });
        });
        window.add_action(&action);
    }

    {
        let nav = navigation_view.clone();
        let page = log_page.clone();
        let back = back_button.clone();
        let add = add_button.clone();
        let find = find_button.clone();
        let git = git_button.clone();
        let save = save_button.clone();
        let win = window_title.clone();
        let view = log_view.clone();
        let seen_revision = Rc::new(RefCell::new(0usize));
        let seen_error_revision = Rc::new(RefCell::new(0usize));
        glib::timeout_add_local(Duration::from_millis(200), move || {
            let (revision, error_revision, text) = log_snapshot();
            {
                let mut seen = seen_revision.borrow_mut();
                if revision != *seen {
                    view.buffer().set_text(&text);
                    *seen = revision;
                }
            }

            if cfg!(debug_assertions) {
                let mut seen_error = seen_error_revision.borrow_mut();
                if error_revision > *seen_error {
                    *seen_error = error_revision;
                    show_log_page(&nav, &page, &back, &add, &find, &git, &save, &win);
                }
            }

            glib::ControlFlow::Continue
        });
    }

    // keyboard shortcuts
    app.set_accels_for_action("win.back", &["Escape"]);
    app.set_accels_for_action("win.toggle-find", &["<primary>f"]);
    app.set_accels_for_action("win.synchronize", &["<primary>s"]);
    app.set_accels_for_action("win.open-new-password", &["<primary>n"]);
    app.set_accels_for_action("win.open-git", &["<primary>i"]);
    app.set_accels_for_action("win.open-log", &["F12"]);
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

fn sync_username_row(row: &EntryRow, pass_file: Option<&OpenPassFile>) {
    if let Some(username) = pass_file.and_then(OpenPassFile::username) {
        row.set_text(username);
        row.set_visible(true);
    } else {
        row.set_text("");
        row.set_visible(false);
    }
}

fn show_log_page(
    nav: &NavigationView,
    page: &NavigationPage,
    back: &Button,
    add: &Button,
    find: &Button,
    git: &Button,
    save: &Button,
    win: &WindowTitle,
) {
    add.set_visible(false);
    find.set_visible(false);
    git.set_visible(false);
    back.set_visible(true);
    save.set_visible(false);
    win.set_title("Logs");
    win.set_subtitle("Command output");

    let already_visible = nav
        .visible_page()
        .as_ref()
        .map(|visible| visible == page)
        .unwrap_or(false);
    if !already_visible {
        nav.push(page);
    }
}

fn show_git_busy_page(
    nav: &NavigationView,
    page: &NavigationPage,
    status: &StatusPage,
    back: &Button,
    add: &Button,
    find: &Button,
    git: &Button,
    save: &Button,
    win: &WindowTitle,
    title: &str,
    description: Option<&str>,
) {
    add.set_visible(false);
    find.set_visible(false);
    git.set_visible(false);
    back.set_visible(false);
    save.set_visible(false);
    win.set_title("Git");
    win.set_subtitle(title);
    status.set_title(title);
    status.set_description(description);

    let already_visible = nav
        .visible_page()
        .as_ref()
        .map(|visible| visible == page)
        .unwrap_or(false);
    if !already_visible {
        nav.push(page);
    }
}

fn finish_git_busy_page(
    nav: &NavigationView,
    busy_page: &NavigationPage,
    text_page: &NavigationPage,
    settings_page: &NavigationPage,
    log_page: &NavigationPage,
    back: &Button,
    add: &Button,
    find: &Button,
    git: &Button,
    save: &Button,
    win: &WindowTitle,
    username: &EntryRow,
) {
    let current_page = nav.visible_page();
    let busy_visible = current_page
        .as_ref()
        .map(|visible| visible == busy_page)
        .unwrap_or(false);
    let busy_in_stack = navigation_stack_contains_page(nav, busy_page);

    if busy_visible {
        nav.pop();
    } else if busy_in_stack {
        if let Some(current_page) = current_page.filter(|page| page != busy_page) {
            let _ = nav.pop_to_page(busy_page);
            let _ = nav.pop();
            nav.push(&current_page);
        }
    }

    let stack = nav.navigation_stack();
    if stack.n_items() <= 1 {
        back.set_visible(false);
        save.set_visible(false);
        add.set_visible(true);
        find.set_visible(true);
        git.set_visible(false);
        win.set_title("Password Store");
        win.set_subtitle("Manage your passwords");
        return;
    }

    back.set_visible(true);
    add.set_visible(false);
    find.set_visible(false);
    git.set_visible(false);

    let visible_page = nav.visible_page();
    let is_text_page = visible_page
        .as_ref()
        .map(|page| page == text_page)
        .unwrap_or(false);
    let is_settings_page = visible_page
        .as_ref()
        .map(|page| page == settings_page)
        .unwrap_or(false);
    let is_log_page = visible_page
        .as_ref()
        .map(|page| page == log_page)
        .unwrap_or(false);

    save.set_visible(is_text_page);
    if is_text_page {
        if let Some(pass_file) = get_opened_pass_file() {
            let label = pass_file.label();
            win.set_title(pass_file.title());
            win.set_subtitle(&label);
            sync_username_row(username, Some(&pass_file));
        } else {
            win.set_title("Password Store");
            win.set_subtitle("Manage your passwords");
            sync_username_row(username, None);
        }
    } else if is_settings_page {
        win.set_title("Preferences");
        win.set_subtitle("Password Store");
    } else if is_log_page {
        win.set_title("Logs");
        win.set_subtitle("Command output");
    }
}

fn navigation_stack_contains_page(nav: &NavigationView, page: &NavigationPage) -> bool {
    let stack = nav.navigation_stack();
    let mut index = 0;
    let len = stack.n_items();
    while index < len {
        if let Some(item) = stack.item(index) {
            if let Ok(stack_page) = item.downcast::<NavigationPage>() {
                if stack_page == *page {
                    return true;
                }
            }
        }
        index += 1;
    }
    false
}

fn load_passwords_async(
    list: &ListBox,
    git: Button,
    find: Button,
    save: Button,
    overlay: ToastOverlay,
    show_list_actions: bool,
) {
    while let Some(child) = list.first_child() {
        list.remove(&child);
    }

    git.set_visible(false);
    find.set_visible(show_list_actions);

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
                log_error(format!("Error scanning pass stores: {err}"));
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
                                    let mut cmd = settings.command();
                                    cmd.env("PASSWORD_STORE_DIR", &item.store_path)
                                        .arg("-c")
                                        .arg(item.label());
                                    let _ = run_command_status(
                                        &mut cmd,
                                        "Copy password to clipboard",
                                        CommandLogOptions::SENSITIVE,
                                    );
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
                            let mut cmd = settings.command();
                            cmd.env("PASSWORD_STORE_DIR", &root)
                                .arg("mv")
                                .arg(&old_label)
                                .arg(&new_label);
                            let status = run_command_status(
                                &mut cmd,
                                "Rename password entry",
                                CommandLogOptions::DEFAULT,
                            );
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
                                    let mut cmd = settings.command();
                                    cmd.env("PASSWORD_STORE_DIR", root)
                                        .arg("rm")
                                        .arg("-rf")
                                        .arg(&label);
                                    let _ = run_command_status(
                                        &mut cmd,
                                        "Delete password entry",
                                        CommandLogOptions::DEFAULT,
                                    );
                                }
                            });
                            list.remove(&row_clone);
                        });
                    }
                    list_clone.append(&row);
                    index += 1;
                }

                if show_list_actions {
                    if empty {
                        save_clone.set_visible(false);
                        find_clone.set_visible(false);
                    } else {
                        find_clone.set_visible(true);
                    }
                    git_clone.set_visible(empty);
                } else {
                    find_clone.set_visible(false);
                    git_clone.set_visible(false);
                }

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
                git_clone.set_visible(show_list_actions);
                find_clone.set_visible(false);

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
    let mut cmd = settings.command();
    cmd.env("PASSWORD_STORE_DIR", store_root)
        .arg("insert")
        .arg("-m"); // read from stdin
    if overwrite {
        cmd.arg("-f");
    }
    cmd.arg(label);

    let mut input = String::new();
    input.push_str(password);
    input.push('\n');
    if !notes.is_empty() {
        input.push_str(notes);
        input.push('\n');
    }

    let output = run_command_with_input(
        &mut cmd,
        "Save password entry",
        &input,
        CommandLogOptions::SENSITIVE,
    )?;
    if output.status.success() {
        Ok(())
    } else {
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
        if stderr.is_empty() {
            Err(format!("pass insert failed: {}", output.status))
        } else {
            Err(stderr)
        }
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
                log_error(format!("Failed to save stores: {err}"));
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
                log_error(format!("Failed to save stores: {err}"));
            } else {
                list.remove(&row_clone);
            }
        }
    });
}
