#[cfg(feature = "setup")]
use crate::setup::*;
#[cfg(feature = "setup")]
use adw::gio::{Menu, MenuItem};

use crate::config::APP_ID;
use crate::item::{collect_all_password_items, OpenPassFile, PassEntry};
use crate::logging::{
    log_error, log_info, log_snapshot, run_command_output, run_command_output_controlled,
    run_command_status, run_command_with_input, CommandControl, CommandLogOptions,
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
    Box as GtkBox, Widget, gdk::Display, Builder, Button, Dialog, Entry, Label, ListBox,
    ListBoxRow, MenuButton, Popover, SearchEntry, Spinner, TextView,
};
use adw::gtk::{FileChooserAction, FileChooserNative, ResponseType};
use std::cell::RefCell;
use std::fs;
use std::io;
use std::rc::Rc;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc;
use std::sync::mpsc::TryRecvError;
use std::thread;
use std::time::Duration;

const UI_SRC: &str = include_str!("../data/window.ui");

const USERNAME_FIELD_KEYS: [&str; 3] = ["login", "username", "user"];
const SENSITIVE_FIELD_HINTS: [&str; 8] = [
    "pass",
    "secret",
    "token",
    "pin",
    "key",
    "code",
    "phrase",
    "credential",
];

#[derive(Clone, Debug, PartialEq, Eq)]
struct DynamicFieldTemplate {
    raw_key: String,
    title: String,
    separator_spacing: String,
    sensitive: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
enum StructuredPassLine {
    Field(DynamicFieldTemplate),
    Preserved(String),
}

#[derive(Clone)]
enum DynamicFieldRow {
    Plain(EntryRow),
    Secret(PasswordEntryRow),
}

impl DynamicFieldRow {
    fn text(&self) -> String {
        match self {
            Self::Plain(row) => row.text().to_string(),
            Self::Secret(row) => row.text().to_string(),
        }
    }

    fn widget(&self) -> Widget {
        match self {
            Self::Plain(row) => row.clone().upcast(),
            Self::Secret(row) => row.clone().upcast(),
        }
    }
}

#[derive(Clone, Default)]
struct GitOperationControl {
    command: CommandControl,
    cancel_requested: Arc<AtomicBool>,
}

impl GitOperationControl {
    fn begin(&self) {
        self.cancel_requested.store(false, Ordering::Relaxed);
    }

    fn finish(&self) {
        self.cancel_requested.store(false, Ordering::Relaxed);
    }

    fn request_cancel(&self) -> io::Result<bool> {
        self.cancel_requested.store(true, Ordering::Relaxed);
        self.command.cancel()
    }

    fn is_cancel_requested(&self) -> bool {
        self.cancel_requested.load(Ordering::Relaxed)
    }
}

enum GitOperationResult {
    Success,
    Failed(String),
    Canceled,
}

fn with_logs_hint(message: &str) -> String {
    format!("{message} Check Logs for details.")
}

fn show_clipboard_unavailable_toast(overlay: &ToastOverlay) {
    let toast = Toast::new("Clipboard is not available right now.");
    overlay.add_toast(toast);
}

fn is_username_field_key(key: &str) -> bool {
    let key = key.trim().to_ascii_lowercase();
    USERNAME_FIELD_KEYS.contains(&key.as_str())
}

fn is_otpauth_line(key: &str, value: &str, raw_line: &str) -> bool {
    let key = key.trim().to_ascii_lowercase();
    key == "otpauth" || value.contains("otpauth://") || raw_line.contains("otpauth://")
}

fn is_sensitive_field(key: &str) -> bool {
    let key = key.trim().to_ascii_lowercase();
    SENSITIVE_FIELD_HINTS
        .iter()
        .any(|hint| key.contains(hint))
}

fn clear_box_children(box_widget: &GtkBox) {
    while let Some(child) = box_widget.first_child() {
        box_widget.remove(&child);
    }
}

fn add_copy_suffix<W: IsA<Widget>>(widget: &W, text: impl Fn() -> String + 'static, overlay: &ToastOverlay)
where
    W: Clone,
{
    let button = Button::from_icon_name("edit-copy-symbolic");
    button.set_tooltip_text(Some("Copy value"));
    button.add_css_class("flat");
    let overlay = overlay.clone();
    button.connect_clicked(move |_| {
        let text = text();
        if let Some(display) = Display::default() {
            let clipboard = display.clipboard();
            clipboard.set_text(&text);
        } else {
            show_clipboard_unavailable_toast(&overlay);
        }
    });

    if let Some(row) = widget.dynamic_cast_ref::<EntryRow>() {
        row.add_suffix(&button);
    } else if let Some(row) = widget.dynamic_cast_ref::<PasswordEntryRow>() {
        row.add_suffix(&button);
    }
}

fn apply_field_row_style<W: IsA<Widget>>(widget: &W) {
    widget.set_margin_start(15);
    widget.set_margin_end(15);
    widget.set_margin_bottom(6);
}

fn build_dynamic_field_row(
    template: &DynamicFieldTemplate,
    value: &str,
    overlay: &ToastOverlay,
) -> DynamicFieldRow {
    if template.sensitive {
        let row = PasswordEntryRow::new();
        row.set_title(&template.title);
        row.set_text(value);
        apply_field_row_style(&row);
        let row_clone = row.clone();
        add_copy_suffix(&row, move || row_clone.text().to_string(), overlay);
        DynamicFieldRow::Secret(row)
    } else {
        let row = EntryRow::new();
        row.set_title(&template.title);
        row.set_text(value);
        apply_field_row_style(&row);
        let row_clone = row.clone();
        add_copy_suffix(&row, move || row_clone.text().to_string(), overlay);
        DynamicFieldRow::Plain(row)
    }
}

fn parse_structured_pass_lines(contents: &str) -> (String, Vec<(StructuredPassLine, Option<String>)>) {
    let mut lines = contents.lines();
    let password = lines.next().unwrap_or_default().to_string();
    let structured = lines
        .map(|line| {
            let Some((raw_key, raw_value)) = line.split_once(':') else {
                return (StructuredPassLine::Preserved(line.to_string()), None);
            };

            let title = raw_key.trim().to_string();
            if title.is_empty() {
                return (StructuredPassLine::Preserved(line.to_string()), None);
            }

            if is_username_field_key(&title) || is_otpauth_line(&title, raw_value, line) {
                return (StructuredPassLine::Preserved(line.to_string()), None);
            }

            let separator_spacing = raw_value
                .chars()
                .take_while(|c| c.is_ascii_whitespace())
                .collect::<String>();
            let value = raw_value
                .trim_start_matches(|c: char| c.is_ascii_whitespace())
                .to_string();
            let template = DynamicFieldTemplate {
                raw_key: raw_key.to_string(),
                title,
                separator_spacing,
                sensitive: is_sensitive_field(raw_key),
            };

            (StructuredPassLine::Field(template), Some(value))
        })
        .collect();

    (password, structured)
}

fn rebuild_dynamic_fields(
    box_widget: &GtkBox,
    overlay: &ToastOverlay,
    templates_state: &Rc<RefCell<Vec<StructuredPassLine>>>,
    rows_state: &Rc<RefCell<Vec<DynamicFieldRow>>>,
    contents: &str,
) {
    clear_box_children(box_widget);
    templates_state.borrow_mut().clear();
    rows_state.borrow_mut().clear();

    let (_, structured_lines) = parse_structured_pass_lines(contents);
    let mut rows = Vec::new();
    let mut templates = Vec::new();

    for (line, value) in structured_lines {
        match line {
            StructuredPassLine::Field(template) => {
                let row = build_dynamic_field_row(&template, value.as_deref().unwrap_or_default(), overlay);
                box_widget.append(&row.widget());
                rows.push(row);
                templates.push(StructuredPassLine::Field(template));
            }
            StructuredPassLine::Preserved(line) => {
                templates.push(StructuredPassLine::Preserved(line));
            }
        }
    }

    box_widget.set_visible(!rows.is_empty());
    *rows_state.borrow_mut() = rows;
    *templates_state.borrow_mut() = templates;
}

fn structured_pass_contents(
    password: &str,
    templates: &[StructuredPassLine],
    rows: &[DynamicFieldRow],
) -> String {
    let values = rows.iter().map(DynamicFieldRow::text).collect::<Vec<_>>();
    structured_pass_contents_from_values(password, templates, &values)
}

fn structured_pass_contents_from_values(
    password: &str,
    templates: &[StructuredPassLine],
    values: &[String],
) -> String {
    let mut output = String::new();
    output.push_str(password);

    let mut row_index = 0usize;
    for line in templates {
        output.push('\n');
        match line {
            StructuredPassLine::Field(template) => {
                output.push_str(&template.raw_key);
                output.push(':');
                output.push_str(&template.separator_spacing);
                output.push_str(values[row_index].as_str());
                row_index += 1;
            }
            StructuredPassLine::Preserved(line) => output.push_str(line),
        }
    }

    output
}

fn set_window_action_enabled(window: &ApplicationWindow, name: &str, enabled: bool) {
    let Some(action) = window.lookup_action(name) else {
        return;
    };
    let Ok(action) = action.downcast::<SimpleAction>() else {
        return;
    };
    action.set_enabled(enabled);
}

fn set_git_busy_actions_enabled(window: &ApplicationWindow, enabled: bool) {
    for action in [
        "open-new-password",
        "toggle-find",
        "open-git",
        "open-raw-pass-file",
        "git-clone",
        "save-password",
        "synchronize",
        "open-preferences",
    ] {
        set_window_action_enabled(window, action, enabled);
    }
}

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

    #[cfg(not(feature = "flatpak"))]
    let backend_preferences: adw::PreferencesGroup = builder
        .object("backend_preferences")
        .expect("Failed to get backend_preferences");
    #[cfg(not(feature = "flatpak"))]
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
    let log_page: NavigationPage = builder
        .object("log_page")
        .expect("Failed to get log page");
    let git_busy_page: NavigationPage = builder
        .object("git_busy_page")
        .expect("Failed to get git busy page");
    let git_busy_status: StatusPage = builder
        .object("git_busy_status")
        .expect("Failed to get git busy status");
    #[cfg(not(feature = "flatpak"))]
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
    let log_view: TextView = builder
        .object("log_view")
        .expect("Failed to get log_view");
    let structured_templates = Rc::new(RefCell::new(Vec::<StructuredPassLine>::new()));
    let dynamic_field_rows = Rc::new(RefCell::new(Vec::<DynamicFieldRow>::new()));

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
        let dynamic_box = dynamic_fields_box.clone();
        let raw_button = open_raw_button.clone();
        let structured_templates = structured_templates.clone();
        let dynamic_rows = dynamic_field_rows.clone();
        let overlay = toast_overlay.clone();
        let win = window_title.clone();
        list.connect_row_activated(move |_list, row| {
            let label = non_null_to_string_option(row, "label");
            let root = non_null_to_string_option(row, "root");

            let Some(label) = label else {
                let toast = Toast::new("This password entry could not be opened.");
                overlay.add_toast(toast);
                return;
            };
            let Some(root) = root else {
                let toast = Toast::new("This password entry is missing its password store.");
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
            entry.set_visible(false);
            sync_username_row(&username, Some(&opened_pass_file));
            otp.set_visible(false);
            dynamic_box.set_visible(false);
            raw_button.set_visible(false);
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
            let dynamic_box = dynamic_box.clone();
            let raw_button = raw_button.clone();
            let structured_templates = structured_templates.clone();
            let dynamic_rows = dynamic_rows.clone();
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
                        raw_button.set_visible(true);

                        let (password, _) = parse_structured_pass_lines(&output);
                        password_entry.set_text(&password);
                        text_view.buffer().set_text(&output);
                        rebuild_dynamic_fields(
                            &dynamic_box,
                            &overlay,
                            &structured_templates,
                            &dynamic_rows,
                            &output,
                        );
                        sync_username_row(&username_entry, updated_pass_file.as_ref());

                        let otp = output.lines().skip(1).any(|line| line.contains("otpauth://"));
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
                                Ok(_) => {
                                    let toast = Toast::new(&with_logs_hint(
                                        "Couldn't load the one-time password.",
                                    ));
                                    overlay.add_toast(toast);
                                }
                                Err(e) => {
                                    log_error(format!("Failed to read OTP code: {e}"));
                                    let toast = Toast::new(&with_logs_hint(
                                        "Couldn't load the one-time password.",
                                    ));
                                    overlay.add_toast(toast);
                                }
                            }
                        } else {
                            otp_entry.set_text("");
                        }

                        glib::ControlFlow::Break
                    }
                    Ok(Err(msg)) => {
                        log_error(format!("Failed to open password entry: {msg}"));
                        let toast = Toast::new(&with_logs_hint("Couldn't open the password entry."));
                        overlay.add_toast(toast);
                        glib::ControlFlow::Break
                    }
                    Err(TryRecvError::Empty) => glib::ControlFlow::Continue,
                    Err(TryRecvError::Disconnected) => {
                        let toast = Toast::new(&with_logs_hint("Couldn't open the password entry."));
                        overlay.add_toast(toast);
                        glib::ControlFlow::Break
                    }
                }
            });
        });
    }

    // Pass command preference
    #[cfg(not(feature = "flatpak"))]
    {
        let overlay = toast_overlay.clone();
        let preferences = settings.clone();
        pass_row.connect_apply(move |row| {
            let text = row.text().to_string();
            let text = text.trim();
            if text.is_empty() {
                let toast = Toast::new("Enter a command for pass.");
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
                show_clipboard_unavailable_toast(&overlay);
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
                show_clipboard_unavailable_toast(&overlay);
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
                show_clipboard_unavailable_toast(&overlay);
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
        let dynamic_box = dynamic_fields_box.clone();
        let raw_button = open_raw_button.clone();
        let structured_templates = structured_templates.clone();
        let dynamic_rows = dynamic_field_rows.clone();
        let status = password_status.clone();
        let win = window_title.clone();
        path_entry.connect_apply(move |row| {
            let path = row.text().to_string();
            let settings = Preferences::new();
            let store_root = settings.store();
            if path.is_empty() {
                let toast = Toast::new("Enter a name or path for the new entry.");
                overlay.add_toast(toast);
                return;
            }
            let opened_pass_file = OpenPassFile::from_label(store_root, &path);
            set_opened_pass_file(opened_pass_file);
            status.set_visible(false);
            entry.set_visible(true);
            sync_username_row(&username, get_opened_pass_file().as_ref());
            otp.set_visible(false);
            dynamic_box.set_visible(false);
            raw_button.set_visible(true);
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
            clear_box_children(&dynamic_box);
            structured_templates.borrow_mut().clear();
            dynamic_rows.borrow_mut().clear();
            text.buffer().set_text("");
        });
    }

    // actions
    {
        let nav = navigation_view.clone();
        let raw_page = raw_text_page.clone();
        let entry = password_entry.clone();
        let username = username_entry.clone();
        let otp = otp_entry.clone();
        let text = text_view.clone();
        let dynamic_box = dynamic_fields_box.clone();
        let structured_templates = structured_templates.clone();
        let dynamic_rows = dynamic_field_rows.clone();
        let overlay = toast_overlay.clone();
        let action = SimpleAction::new("save-password", None);
        action.connect_activate(move |_, _| {
            let Some(pass_file) = get_opened_pass_file() else {
                let toast = Toast::new("Open a password entry before saving.");
                overlay.add_toast(toast);
                return;
            };

            let raw_visible = nav
                .visible_page()
                .as_ref()
                .map(|page| page == &raw_page)
                .unwrap_or(false);

            let contents = if raw_visible {
                let buffer = text.buffer();
                let (start, end) = buffer.bounds();
                buffer.text(&start, &end, false).to_string()
            } else {
                structured_pass_contents(
                    &entry.text(),
                    &structured_templates.borrow(),
                    &dynamic_rows.borrow(),
                )
            };

            let password = contents.lines().next().unwrap_or_default().to_string();
            if password.is_empty() {
                let toast = Toast::new("Enter a password before saving.");
                overlay.add_toast(toast);
                return;
            }
            let label = pass_file.label();
            match write_pass_entry(pass_file.store_path(), &label, &contents, true) {
                Ok(()) => {
                    let updated_pass_file =
                        refresh_opened_pass_file_from_contents(&pass_file, &contents);
                    text.buffer().set_text(&contents);
                    rebuild_dynamic_fields(
                        &dynamic_box,
                        &overlay,
                        &structured_templates,
                        &dynamic_rows,
                        &contents,
                    );
                    entry.set_text(&password);
                    sync_username_row(&username, updated_pass_file.as_ref());
                    let otp_visible = contents.lines().skip(1).any(|line| line.contains("otpauth://"));
                    otp.set_visible(otp_visible);
                    if otp_visible {
                        let settings = Preferences::new();
                        let mut cmd = settings.command();
                        cmd.env("PASSWORD_STORE_DIR", pass_file.store_path())
                            .args(["otp", &label]);
                        match run_command_output(
                            &mut cmd,
                            "Read OTP code",
                            CommandLogOptions::SENSITIVE,
                        ) {
                            Ok(output) if output.status.success() => {
                                let code = String::from_utf8_lossy(&output.stdout).trim().to_string();
                                otp.set_text(&code);
                            }
                            _ => otp.set_text(""),
                        }
                    } else {
                        otp.set_text("");
                    }
                    let toast = Toast::new("Changes saved.");
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
        #[cfg(not(feature = "flatpak"))]
        let command = pass_row.clone();
        let list = password_stores.clone();
        let parent = window.clone();
        let overlay = toast_overlay.clone();
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
            #[cfg(not(feature = "flatpak"))]
            command.set_text(&settings.command_value());
            rebuild_store_list(&list, &settings, &parent, &overlay);
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

    {
        let nav = navigation_view.clone();
        let page = raw_text_page.clone();
        let back = back_button.clone();
        let add = add_button.clone();
        let find = find_button.clone();
        let git = git_button.clone();
        let save = save_button.clone();
        let win = window_title.clone();
        let entry = password_entry.clone();
        let text = text_view.clone();
        let structured_templates = structured_templates.clone();
        let dynamic_rows = dynamic_field_rows.clone();
        let action = SimpleAction::new("open-raw-pass-file", None);
        action.connect_activate(move |_, _| {
            let contents = structured_pass_contents(
                &entry.text(),
                &structured_templates.borrow(),
                &dynamic_rows.borrow(),
            );
            text.buffer().set_text(&contents);

            add.set_visible(false);
            find.set_visible(false);
            git.set_visible(false);
            back.set_visible(true);
            save.set_visible(true);
            win.set_title("Raw Pass File");
            if let Some(pass_file) = get_opened_pass_file() {
                let label = pass_file.label();
                win.set_subtitle(&label);
            } else {
                win.set_subtitle("Password Store");
            }

            let already_visible = nav
                .visible_page()
                .as_ref()
                .map(|visible| visible == &page)
                .unwrap_or(false);
            if !already_visible {
                nav.push(&page);
            }
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
                let toast = Toast::new("This app cannot be installed here.");
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
        let entry = git_url_entry.clone();
        let action = SimpleAction::new("open-git", None);
        action.connect_activate(move |_, _| {
            if popover.is_visible() {
                popover.popdown()
            } else {
                popover.popup();
                entry.grab_focus();
            }
        });
        window.add_action(&action);
    }

    {
        let window = window.clone();
        git_url_entry.connect_apply(move |_| {
            let _ = adw::prelude::WidgetExt::activate_action(&window, "win.git-clone", None);
        });
    }

    {
        let entry = git_url_entry.clone();
        let overlay = toast_overlay.clone();
        let popover = git_popover.clone();
        let window_for_action = window.clone();
        let list_clone = list.clone();
        let nav = navigation_view.clone();
        let text_page = text_page.clone();
        let raw_text_page = raw_text_page.clone();
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
        let git_operation = git_operation.clone();
        let action = SimpleAction::new("git-clone", None);
        action.connect_activate(move |_, _| {
            let url = entry.text().trim().to_string();
            if url.is_empty() {
                let toast = Toast::new("Enter a repository URL.");
                overlay.add_toast(toast);
                return;
            }

            popover.popdown();
            git_operation.begin();
            set_git_busy_actions_enabled(&window_for_action, false);
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
                "Restoring password store",
                Some("Downloading the password store from the repository."),
            );

            let (tx, rx) = mpsc::channel::<GitOperationResult>();
            let url_for_thread = url.clone();
            let git_operation_for_thread = git_operation.clone();
            thread::spawn(move || {
                if git_operation_for_thread.is_cancel_requested() {
                    let _ = tx.send(GitOperationResult::Canceled);
                    return;
                }

                let settings = Preferences::new();
                let store_root = settings.store();
                if store_root.is_empty() {
                    let _ = tx.send(GitOperationResult::Failed(
                        "Add a password store folder in Preferences before restoring from Git."
                            .to_string(),
                    ));
                    return;
                }

                let mut cmd = settings.git_command();
                cmd.arg("clone").arg(&url_for_thread).arg(&store_root);
                let result = match run_command_output_controlled(
                    &mut cmd,
                    "Clone password store",
                    CommandLogOptions::DEFAULT,
                    &git_operation_for_thread.command,
                ) {
                    Ok(output) if output.status.success() => GitOperationResult::Success,
                    Ok(output) if git_operation_for_thread.is_cancel_requested() => {
                        GitOperationResult::Canceled
                    }
                    Ok(_) => GitOperationResult::Failed(with_logs_hint(
                        "Couldn't restore the password store.",
                    )),
                    Err(err) if git_operation_for_thread.is_cancel_requested() => {
                        GitOperationResult::Canceled
                    }
                    Err(err) => {
                        log_error(format!("Failed to start restore from Git: {err}"));
                        GitOperationResult::Failed(with_logs_hint(
                            "Couldn't restore the password store.",
                        ))
                    }
                };
                let _ = tx.send(result);
            });

            let overlay = overlay.clone();
            let entry = entry.clone();
            let window = window_for_action.clone();
            let list = list_clone.clone();
            let nav = nav.clone();
            let text_page = text_page.clone();
            let raw_text_page = raw_text_page.clone();
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
            let git_operation = git_operation.clone();
            glib::timeout_add_local(Duration::from_millis(50), move || match rx.try_recv() {
                Ok(GitOperationResult::Success) => {
                    entry.set_text("");
                    git_operation.finish();
                    finish_git_busy_page(
                        &window,
                        &nav,
                        &busy_page,
                        &text_page,
                        &raw_text_page,
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
                    let toast = Toast::new("Password store restored.");
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
                Ok(GitOperationResult::Failed(message)) => {
                    git_operation.finish();
                    finish_git_busy_page(
                        &window,
                        &nav,
                        &busy_page,
                        &text_page,
                        &raw_text_page,
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
                Ok(GitOperationResult::Canceled) => {
                    git_operation.finish();
                    finish_git_busy_page(
                        &window,
                        &nav,
                        &busy_page,
                        &text_page,
                        &raw_text_page,
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
                    let toast = Toast::new("Restore canceled.");
                    overlay.add_toast(toast);
                    glib::ControlFlow::Break
                }
                Err(TryRecvError::Empty) => glib::ControlFlow::Continue,
                Err(TryRecvError::Disconnected) => {
                    git_operation.finish();
                    finish_git_busy_page(
                        &window,
                        &nav,
                        &busy_page,
                        &text_page,
                        &raw_text_page,
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
                    let toast =
                        Toast::new(&with_logs_hint("The restore operation stopped unexpectedly."));
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
        let raw_page = raw_text_page.clone();
        let settings = settings_page.clone();
        let log_page = log_page.clone();
        let busy_page = git_busy_page.clone();
        let busy_status = git_busy_status.clone();
        let entry = password_entry.clone();
        let username = username_entry.clone();
        let otp = otp_entry.clone();
        let text = text_view.clone();
        let dynamic_box = dynamic_fields_box.clone();
        let raw_button = open_raw_button.clone();
        let structured_templates = structured_templates.clone();
        let dynamic_rows = dynamic_field_rows.clone();
        let list_clone = list.clone();
        let win = window_title.clone();
        let back = back_button.clone();
        let git = git_button.clone();
        let add = add_button.clone();
        let find = find_button.clone();
        let save = save_button.clone();
        let nav = navigation_view.clone();
        let git_operation = git_operation.clone();
        let action = SimpleAction::new("back", None);
        action.connect_activate(move |_, _| {
            let busy_visible = nav
                .visible_page()
                .as_ref()
                .map(|visible| visible == &busy_page)
                .unwrap_or(false);
            if busy_visible {
                if git_operation.is_cancel_requested() {
                    return;
                }
                match git_operation.request_cancel() {
                    Ok(true) => {
                        log_info("Git operation cancellation requested");
                        busy_status.set_title("Stopping Git operation");
                        busy_status
                            .set_description(Some("Waiting for the current git command to stop."));
                    }
                    Ok(false) => {}
                    Err(err) => {
                        log_error(format!("Failed to cancel Git operation: {err}"));
                        let toast = Toast::new("Couldn't stop the Git operation.");
                        overlay.add_toast(toast);
                    }
                }
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
                let is_raw_page = visible_page
                    .as_ref()
                    .map(|p| p == &raw_page)
                    .unwrap_or(false);
                let is_settings_page = visible_page
                    .as_ref()
                    .map(|p| p == &settings)
                    .unwrap_or(false);
                let is_log_page = visible_page
                    .as_ref()
                    .map(|p| p == &log_page)
                    .unwrap_or(false);
                save.set_visible(is_text_page || is_raw_page);
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
                } else if is_raw_page {
                    win.set_title("Raw Pass File");
                    if let Some(pass_file) = get_opened_pass_file() {
                        let label = pass_file.label();
                        win.set_subtitle(&label);
                    } else {
                        win.set_subtitle("Password Store");
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
                clear_box_children(&dynamic_box);
                dynamic_box.set_visible(false);
                raw_button.set_visible(false);
                structured_templates.borrow_mut().clear();
                dynamic_rows.borrow_mut().clear();
                text.buffer().set_text("");
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
        let window_for_action = window.clone();
        let nav = navigation_view.clone();
        let text_page = text_page.clone();
        let raw_text_page = raw_text_page.clone();
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
        let git_operation = git_operation.clone();
        let action = SimpleAction::new("synchronize", None);
        action.connect_activate(move |_, _| {
            let overlay = overlay_clone.clone();
            git_operation.begin();
            set_git_busy_actions_enabled(&window_for_action, false);
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
                "Syncing password stores",
                Some("Checking for changes and pushing updates."),
            );
            // Channel from worker to main thread
            let (tx, rx) = mpsc::channel::<GitOperationResult>();
            // Background worker
            let git_operation_for_thread = git_operation.clone();
            thread::spawn(move || {
                let settings = Preferences::new();
                let roots = settings.stores();
                for root in roots {
                    if git_operation_for_thread.is_cancel_requested() {
                        let _ = tx.send(GitOperationResult::Canceled);
                        return;
                    }
                    let commands: [&[&str]; 3] = [&["fetch", "--all"], &["pull"], &["push"]];
                    for args in commands {
                        if git_operation_for_thread.is_cancel_requested() {
                            let _ = tx.send(GitOperationResult::Canceled);
                            return;
                        }
                        let mut cmd = settings.git_command();
                        cmd.arg("-C").arg(&root).args(args);
                        let output = run_command_output_controlled(
                            &mut cmd,
                            &format!("Synchronize password store {root}"),
                            CommandLogOptions::DEFAULT,
                            &git_operation_for_thread.command,
                        );
                        match output {
                            Ok(out) => {
                                if !out.status.success() {
                                    if git_operation_for_thread.is_cancel_requested() {
                                        let _ = tx.send(GitOperationResult::Canceled);
                                        return;
                                    }
                                    let stderr = String::from_utf8_lossy(&out.stderr);
                                    let fatal_line = stderr
                                        .lines()
                                        .rev()
                                        .find(|line| line.contains("fatal:"))
                                        .unwrap_or(stderr.trim());
                                    log_error(format!(
                                        "Password store sync failed for {root}: {fatal_line}"
                                    ));
                                    let message = with_logs_hint(
                                        "Couldn't sync one of the password stores.",
                                    );
                                    let _ = tx.send(GitOperationResult::Failed(message));

                                    // stop further commands for this store
                                    return;
                                }
                            }
                            Err(e) => {
                                if git_operation_for_thread.is_cancel_requested() {
                                    let _ = tx.send(GitOperationResult::Canceled);
                                } else {
                                    log_error(format!(
                                        "Password store sync failed for {root}: {e}"
                                    ));
                                    let message = with_logs_hint(
                                        "Couldn't sync one of the password stores.",
                                    );
                                    let _ = tx.send(GitOperationResult::Failed(message));
                                }
                                return;
                            }
                        }
                    }
                }
                let _ = tx.send(GitOperationResult::Success);
            });

            // Main-thread: poll for messages
            let window = window_for_action.clone();
            let nav = nav.clone();
            let text_page = text_page.clone();
            let raw_text_page = raw_text_page.clone();
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
            let git_operation = git_operation.clone();
            glib::timeout_add_local(Duration::from_millis(100), move || {
                match rx.try_recv() {
                    Ok(GitOperationResult::Success) => {
                        git_operation.finish();
                        finish_git_busy_page(
                            &window,
                            &nav,
                            &busy_page,
                            &text_page,
                            &raw_text_page,
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
                    Ok(GitOperationResult::Failed(msg)) => {
                        git_operation.finish();
                        finish_git_busy_page(
                            &window,
                            &nav,
                            &busy_page,
                            &text_page,
                            &raw_text_page,
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
                    Ok(GitOperationResult::Canceled) => {
                        git_operation.finish();
                        finish_git_busy_page(
                            &window,
                            &nav,
                            &busy_page,
                            &text_page,
                            &raw_text_page,
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
                        let toast = Toast::new("Sync canceled.");
                        overlay.add_toast(toast);
                        glib::ControlFlow::Break
                    }
                    Err(TryRecvError::Empty) => glib::ControlFlow::Continue,
                    Err(TryRecvError::Disconnected) => {
                        git_operation.finish();
                        finish_git_busy_page(
                            &window,
                            &nav,
                            &busy_page,
                            &text_page,
                            &raw_text_page,
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
        glib::timeout_add_local(Duration::from_millis(50), move || {
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
    back.set_visible(true);
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
    window: &ApplicationWindow,
    nav: &NavigationView,
    busy_page: &NavigationPage,
    text_page: &NavigationPage,
    raw_text_page: &NavigationPage,
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
    set_git_busy_actions_enabled(window, true);

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
    let is_raw_page = visible_page
        .as_ref()
        .map(|page| page == raw_text_page)
        .unwrap_or(false);
    let is_settings_page = visible_page
        .as_ref()
        .map(|page| page == settings_page)
        .unwrap_or(false);
    let is_log_page = visible_page
        .as_ref()
        .map(|page| page == log_page)
        .unwrap_or(false);

    save.set_visible(is_text_page || is_raw_page);
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
    } else if is_raw_page {
        win.set_title("Raw Pass File");
        if let Some(pass_file) = get_opened_pass_file() {
            let label = pass_file.label();
            win.set_subtitle(&label);
        } else {
            win.set_subtitle("Password Store");
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

    let has_store_dirs = !Preferences::new().stores().is_empty();

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
    let has_store_dirs_for_placeholder = has_store_dirs;
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
                                let toast = adw::Toast::new("Enter a new name.");
                                overlay.add_toast(toast);
                                return;
                            }

                            let old_label = entry.label();
                            if new_label == old_label {
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
                                    let toast =
                                        adw::Toast::new("Couldn't rename the password entry.");
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
                    build_empty_password_list_placeholder(&symbolic, has_store_dirs_for_placeholder)
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
                let placeholder =
                    build_empty_password_list_placeholder(&symbolic, has_store_dirs_for_placeholder);
                list_clone.set_placeholder(Some(&placeholder));

                save_clone.set_visible(false);
                git_clone.set_visible(show_list_actions);
                find_clone.set_visible(false);

                glib::ControlFlow::Break
            }
        }
    });
}

fn build_empty_password_list_placeholder(symbolic: &str, has_store_dirs: bool) -> StatusPage {
    let builder = StatusPage::builder().icon_name(symbolic);
    if has_store_dirs {
        builder
            .title("Empty")
            .description("Create a new password to get started.")
            .build()
    } else {
        builder
            .title("No password store folders added")
            .description("Open Preferences and choose a password store folder to get started.")
            .build()
    }
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
    contents: &str,
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

    let output = run_command_with_input(
        &mut cmd,
        "Save password entry",
        contents,
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

fn rebuild_store_list(
    list: &ListBox,
    settings: &Preferences,
    window: &ApplicationWindow,
    overlay: &ToastOverlay,
) {
    while let Some(child) = list.first_child() {
        list.remove(&child);
    }

    for store in settings.stores() {
        append_store_row(list, settings, &store);
    }

    append_store_picker_row(list, settings, window, overlay);
    append_store_creator_row(list, settings, window, overlay);
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

fn append_store_picker_row(
    list: &ListBox,
    settings: &Preferences,
    window: &ApplicationWindow,
    overlay: &ToastOverlay,
) {
    let row = ActionRow::builder()
        .title("Add password store folder")
        .subtitle("Choose a folder with the system file chooser.")
        .build();
    row.set_activatable(true);

    let button = Button::from_icon_name("folder-open-symbolic");
    button.add_css_class("flat");
    row.add_suffix(&button);
    list.append(&row);

    {
        let settings = settings.clone();
        let list = list.clone();
        let window = window.clone();
        let overlay = overlay.clone();
        row.connect_activated(move |_| {
            open_store_picker(&window, &list, &settings, &overlay);
        });
    }

    {
        let settings = settings.clone();
        let list = list.clone();
        let window = window.clone();
        let overlay = overlay.clone();
        button.connect_clicked(move |_| {
            open_store_picker(&window, &list, &settings, &overlay);
        });
    }
}

fn open_store_picker(
    window: &ApplicationWindow,
    list: &ListBox,
    settings: &Preferences,
    overlay: &ToastOverlay,
) {
    let dialog = FileChooserNative::new(
        Some("Choose password store folder"),
        Some(window),
        FileChooserAction::SelectFolder,
        Some("Select"),
        Some("Cancel"),
    );
    let list = list.clone();
    let settings = settings.clone();
    let window = window.clone();
    let overlay = overlay.clone();

    dialog.connect_response(move |dialog, response| {
        if response != ResponseType::Accept {
            dialog.hide();
            return;
        }

        let Some(file) = dialog.file() else {
            dialog.hide();
            return;
        };

        let Some(path) = file.path() else {
            log_error(
                "The selected folder is not available as a local path. Choose a local folder."
                    .to_string(),
            );
            let toast = Toast::new("Choose a local password store folder.");
            overlay.add_toast(toast);
            dialog.hide();
            return;
        };

        let store = path.to_string_lossy().to_string();
        let mut stores = settings.stores();
        if !stores.contains(&store) {
            stores.push(store.clone());
            if let Err(err) = settings.set_stores(stores) {
                log_error(format!("Failed to save stores: {err}"));
                let toast = Toast::new("Couldn't add the password store folder.");
                overlay.add_toast(toast);
            } else {
                rebuild_store_list(&list, &settings, &window, &overlay);
            }
        }

        dialog.hide();
    });

    dialog.show();
}

fn append_store_creator_row(
    list: &ListBox,
    settings: &Preferences,
    window: &ApplicationWindow,
    overlay: &ToastOverlay,
) {
    let row = ActionRow::builder()
        .title("Create password store")
        .subtitle("Choose a folder and initialize it with GPG recipients.")
        .build();
    row.set_activatable(true);

    let button = Button::from_icon_name("folder-new-symbolic");
    button.add_css_class("flat");
    row.add_suffix(&button);
    list.append(&row);

    {
        let settings = settings.clone();
        let list = list.clone();
        let window = window.clone();
        let overlay = overlay.clone();
        row.connect_activated(move |_| {
            open_store_creator_picker(&window, &list, &settings, &overlay);
        });
    }

    {
        let settings = settings.clone();
        let list = list.clone();
        let window = window.clone();
        let overlay = overlay.clone();
        button.connect_clicked(move |_| {
            open_store_creator_picker(&window, &list, &settings, &overlay);
        });
    }
}

fn open_store_creator_picker(
    window: &ApplicationWindow,
    list: &ListBox,
    settings: &Preferences,
    overlay: &ToastOverlay,
) {
    let dialog = FileChooserNative::new(
        Some("Choose new password store folder"),
        Some(window),
        FileChooserAction::SelectFolder,
        Some("Select"),
        Some("Cancel"),
    );
    dialog.set_create_folders(true);

    let list = list.clone();
    let settings = settings.clone();
    let window = window.clone();
    let overlay = overlay.clone();
    dialog.connect_response(move |dialog, response| {
        if response != ResponseType::Accept {
            dialog.hide();
            return;
        }

        let Some(file) = dialog.file() else {
            dialog.hide();
            return;
        };

        let Some(path) = file.path() else {
            log_error(
                "The selected folder is not available as a local path. Choose a local folder."
                    .to_string(),
            );
            let toast = Toast::new("Choose a local password store folder.");
            overlay.add_toast(toast);
            dialog.hide();
            return;
        };

        let store = path.to_string_lossy().to_string();
        open_store_creator_dialog(&window, &list, &settings, &overlay, &store);
        dialog.hide();
    });

    dialog.show();
}

fn open_store_creator_dialog(
    window: &ApplicationWindow,
    list: &ListBox,
    settings: &Preferences,
    overlay: &ToastOverlay,
    store: &str,
) {
    let dialog = Dialog::builder()
        .title("Create password store")
        .transient_for(window)
        .modal(true)
        .use_header_bar(1)
        .default_width(320)
        .resizable(false)
        .build();
    dialog.add_button("Cancel", ResponseType::Cancel);
    dialog.add_button("Create", ResponseType::Accept);
    dialog.set_default_response(ResponseType::Accept);

    let content = GtkBox::new(adw::gtk::Orientation::Vertical, 8);
    content.set_margin_top(12);
    content.set_margin_bottom(12);
    content.set_margin_start(12);
    content.set_margin_end(12);

    let description = Label::new(Some(
        "Enter GPG recipients separated by commas or semicolons. The new store will become the default.",
    ));
    description.set_wrap(true);
    description.set_max_width_chars(28);
    description.set_xalign(0.0);
    content.append(&description);

    let recipients_label = Label::new(Some("GPG recipients"));
    recipients_label.set_xalign(0.0);
    content.append(&recipients_label);

    let recipients_entry = Entry::new();
    recipients_entry.set_hexpand(true);
    recipients_entry.set_activates_default(true);
    recipients_entry.set_placeholder_text(Some("alice@example.com,bob@example.com"));
    recipients_entry.set_text(&normalized_gpg_recipients(&suggested_gpg_recipients(settings)));
    content.append(&recipients_entry);
    dialog.content_area().append(&content);

    {
        let list = list.clone();
        let settings = settings.clone();
        let window = window.clone();
        let overlay = overlay.clone();
        let store = store.to_string();
        let recipients_entry = recipients_entry.clone();
        dialog.connect_response(move |dialog, response| {
            if response != ResponseType::Accept {
                dialog.close();
                return;
            }

            let recipients = parse_gpg_recipients(recipients_entry.text().as_str());
            if recipients.is_empty() {
                let toast = Toast::new("Enter at least one GPG recipient.");
                overlay.add_toast(toast);
                return;
            }

            dialog.close();

            let (tx, rx) = mpsc::channel::<Result<(), String>>();
            let store_for_thread = store.clone();
            thread::spawn(move || {
                let result = initialize_password_store(&store_for_thread, &recipients);
                let _ = tx.send(result);
            });

            let list = list.clone();
            let settings = settings.clone();
            let window = window.clone();
            let overlay = overlay.clone();
            let store = store.clone();
            glib::timeout_add_local(Duration::from_millis(50), move || match rx.try_recv() {
                Ok(Ok(())) => {
                    let stores = stores_with_preferred_first(&settings.stores(), &store);
                    if let Err(err) = settings.set_stores(stores) {
                        log_error(format!("Failed to save stores: {err}"));
                        let toast = Toast::new(
                            "Password store created, but it couldn't be added to Preferences.",
                        );
                        overlay.add_toast(toast);
                    } else {
                        rebuild_store_list(&list, &settings, &window, &overlay);
                        let toast = Toast::new("Password store created and set as default.");
                        overlay.add_toast(toast);
                    }
                    glib::ControlFlow::Break
                }
                Ok(Err(message)) => {
                    let toast = Toast::new(&message);
                    overlay.add_toast(toast);
                    glib::ControlFlow::Break
                }
                Err(TryRecvError::Empty) => glib::ControlFlow::Continue,
                Err(TryRecvError::Disconnected) => {
                    let toast = Toast::new(&with_logs_hint("Couldn't create the password store."));
                    overlay.add_toast(toast);
                    glib::ControlFlow::Break
                }
            });
        });
    }

    dialog.present();
}

fn suggested_gpg_recipients(settings: &Preferences) -> Vec<String> {
    for root in settings.paths() {
        let path = root.join(".gpg-id");
        let Ok(contents) = fs::read_to_string(path) else {
            continue;
        };

        let recipients = parse_gpg_recipients(&contents);
        if !recipients.is_empty() {
            return recipients;
        }
    }

    Vec::new()
}

fn parse_gpg_recipients(value: &str) -> Vec<String> {
    let mut recipients = Vec::new();
    for recipient in value.split(|c| c == ',' || c == ';' || c == '\n') {
        let recipient = recipient.trim();
        if recipient.is_empty() || recipients.iter().any(|existing| existing == recipient) {
            continue;
        }
        recipients.push(recipient.to_string());
    }
    recipients
}

fn normalized_gpg_recipients(recipients: &[String]) -> String {
    recipients.join(",")
}

fn stores_with_preferred_first(stores: &[String], preferred: &str) -> Vec<String> {
    let mut ordered = vec![preferred.to_string()];
    for store in stores {
        if store != preferred {
            ordered.push(store.clone());
        }
    }
    ordered
}

fn initialize_password_store(store_root: &str, recipients: &[String]) -> Result<(), String> {
    let settings = Preferences::new();
    let mut cmd = settings.command();
    cmd.env("PASSWORD_STORE_DIR", store_root)
        .arg("init")
        .args(recipients);

    match run_command_output(
        &mut cmd,
        "Initialize password store",
        CommandLogOptions::DEFAULT,
    ) {
        Ok(output) if output.status.success() => Ok(()),
        Ok(_) => Err(with_logs_hint("Couldn't create the password store.")),
        Err(err) => {
            log_error(format!("Failed to start password store initialization: {err}"));
            Err(with_logs_hint("Couldn't create the password store."))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{
        normalized_gpg_recipients, parse_gpg_recipients, parse_structured_pass_lines,
        stores_with_preferred_first,
        structured_pass_contents_from_values, StructuredPassLine,
    };

    #[test]
    fn gpg_recipients_are_trimmed_and_deduplicated() {
        assert_eq!(
            parse_gpg_recipients("alice@example.com; bob@example.com,\nalice@example.com"),
            vec![
                "alice@example.com".to_string(),
                "bob@example.com".to_string()
            ]
        );
    }

    #[test]
    fn gpg_recipients_are_normalized_without_separator_spaces() {
        assert_eq!(
            normalized_gpg_recipients(&parse_gpg_recipients(
                "alice@example.com, bob@example.com; carol@example.com"
            )),
            "alice@example.com,bob@example.com,carol@example.com"
        );
    }

    #[test]
    fn preferred_store_moves_to_the_front_once() {
        let stores = vec![
            "/tmp/one".to_string(),
            "/tmp/two".to_string(),
            "/tmp/three".to_string(),
        ];
        assert_eq!(
            stores_with_preferred_first(&stores, "/tmp/two"),
            vec![
                "/tmp/two".to_string(),
                "/tmp/one".to_string(),
                "/tmp/three".to_string()
            ]
        );
    }

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
                StructuredPassLine::Preserved(_) => None,
            })
            .collect::<Vec<_>>();

        assert_eq!(values, vec!["hello@example.com".to_string(), "hello".to_string()]);
        assert_eq!(
            structured_pass_contents_from_values(&password, &templates, &values),
            contents
        );
    }

    #[test]
    fn username_and_otpauth_lines_stay_out_of_dynamic_fields() {
        let contents = "secret\nusername:alice\notpauth://totp/example\nurl: https://example.com";
        let (_, parsed) = parse_structured_pass_lines(contents);

        assert!(matches!(
            parsed[0].0,
            StructuredPassLine::Preserved(ref line) if line == "username:alice"
        ));
        assert!(matches!(
            parsed[1].0,
            StructuredPassLine::Preserved(ref line) if line == "otpauth://totp/example"
        ));
        assert!(matches!(parsed[2].0, StructuredPassLine::Field(_)));
        assert_eq!(parsed[2].1.as_deref(), Some("https://example.com"));
    }
}
