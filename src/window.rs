#[cfg(feature = "setup")]
use crate::setup::*;
#[cfg(any(feature = "setup", feature = "flatpak"))]
use crate::backend::{
    delete_password_entry, read_otp_code, read_password_entry, rename_password_entry,
    save_password_entry,
};
use crate::clipboard::{connect_copy_button, copy_password_entry_to_clipboard};
#[cfg(feature = "flatpak")]
use crate::backend::resolved_ripasso_own_fingerprint;
#[cfg(any(feature = "setup", feature = "flatpak"))]
use adw::gio::Menu;
#[cfg(feature = "setup")]
use adw::gio::MenuItem;

use crate::config::APP_ID;
use crate::item::{collect_all_password_items, OpenPassFile, PassEntry};
use crate::logging::{log_error, CommandControl};
#[cfg(not(feature = "flatpak"))]
use crate::logging::{log_snapshot, CommandLogOptions};
#[cfg(all(not(feature = "setup"), not(feature = "flatpak")))]
use crate::logging::{run_command_output, run_command_status, run_command_with_input};
#[cfg(not(feature = "flatpak"))]
use crate::logging::run_command_output_controlled;
use crate::methods::{
    clear_opened_pass_file, get_opened_pass_file, is_opened_pass_file,
    non_null_to_string_option, refresh_opened_pass_file_from_contents, set_opened_pass_file,
};
use crate::pass_file::{
    clear_box_children, new_pass_file_contents_from_template, parse_structured_pass_lines,
    rebuild_dynamic_fields_from_lines, structured_pass_contents, sync_username_row,
    sync_username_row_from_parsed_lines, DynamicFieldRow, StructuredPassLine,
};
#[cfg(all(feature = "setup", not(feature = "flatpak")))]
use crate::preferences::BackendKind;
use crate::preferences::Preferences;
#[cfg(feature = "flatpak")]
use crate::ripasso_keys::{rebuild_ripasso_private_keys_list, RipassoPrivateKeysState};
#[cfg(feature = "flatpak")]
use crate::ripasso_unlock::{
    is_locked_private_key_error, prompt_private_key_unlock_for_action,
};
use crate::stores::{
    append_gpg_recipients, apply_password_store_recipients, read_store_gpg_recipients,
    store_gpg_recipients_subtitle, stores_with_preferred_first, suggested_gpg_recipients,
};
#[cfg(all(feature = "setup", not(feature = "flatpak")))]
use adw::ComboRow;
use adw::gio::{prelude::*, SimpleAction};
use adw::{
    glib, prelude::*, ActionRow, Application, ApplicationWindow, EntryRow, NavigationPage,
    NavigationView, PasswordEntryRow, StatusPage, Toast, ToastOverlay, WindowTitle,
};
#[cfg(all(feature = "setup", not(feature = "flatpak")))]
use adw::gtk::StringList;
use adw::gtk::{
    Box as GtkBox, Builder, Button, Image, ListBox, ListBoxRow, MenuButton, Popover,
    SearchEntry, Spinner, TextView,
};
use adw::gtk::{FileChooserAction, FileChooserNative, ResponseType};
use std::cell::{Cell, RefCell};
use std::io;
use std::rc::Rc;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc;
use std::sync::mpsc::TryRecvError;
use std::thread;
use std::time::Duration;

const UI_SRC: &str = include_str!("../data/window.ui");

#[derive(Clone, Default)]
struct GitOperationControl {
    command: CommandControl,
    cancel_requested: Arc<AtomicBool>,
}

impl GitOperationControl {
    #[cfg(not(feature = "flatpak"))]
    fn begin(&self) {
        self.cancel_requested.store(false, Ordering::Relaxed);
    }

    #[cfg(not(feature = "flatpak"))]
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

#[cfg(not(feature = "flatpak"))]
enum GitOperationResult {
    Success,
    Failed(String),
    Canceled,
}

#[derive(Clone)]
struct PasswordListPageState {
    nav: NavigationView,
    page: NavigationPage,
    list: ListBox,
    back: Button,
    add: Button,
    find: Button,
    git: Button,
    save: Button,
    win: WindowTitle,
    status: StatusPage,
    entry: PasswordEntryRow,
    username: EntryRow,
    otp: PasswordEntryRow,
    dynamic_box: GtkBox,
    raw_button: Button,
    structured_templates: Rc<RefCell<Vec<StructuredPassLine>>>,
    dynamic_rows: Rc<RefCell<Vec<DynamicFieldRow>>>,
    text: TextView,
    overlay: ToastOverlay,
}

#[derive(Clone, Debug, PartialEq, Eq)]
enum StoreRecipientsMode {
    Create,
    Edit,
}

impl StoreRecipientsMode {
    fn page_title(&self) -> &'static str {
        match self {
            Self::Create => "Create Password Store",
            Self::Edit => "GPG Recipients",
        }
    }

    fn empty_state_subtitle(&self) -> &'static str {
        match self {
            Self::Create => "Add at least one recipient to initialize the password store.",
            Self::Edit => "Add at least one recipient before saving your changes.",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct StoreRecipientsRequest {
    store: String,
    mode: StoreRecipientsMode,
}

#[derive(Clone)]
struct StoreRecipientsPageState {
    window: ApplicationWindow,
    nav: NavigationView,
    page: NavigationPage,
    list: ListBox,
    entry: EntryRow,
    back: Button,
    add: Button,
    find: Button,
    git: Button,
    save: Button,
    win: WindowTitle,
    request: Rc<RefCell<Option<StoreRecipientsRequest>>>,
    recipients: Rc<RefCell<Vec<String>>>,
    saved_recipients: Rc<RefCell<Vec<String>>>,
    save_in_flight: Rc<Cell<bool>>,
    save_queued: Rc<Cell<bool>>,
}

#[cfg(not(feature = "flatpak"))]
fn with_logs_hint(message: &str) -> String {
    format!("{message} Check Logs for details.")
}

#[cfg(feature = "flatpak")]
fn with_logs_hint(message: &str) -> String {
    message.to_string()
}

#[cfg(feature = "flatpak")]
fn friendly_password_entry_error_message(message: &str) -> Option<&'static str> {
    if message.contains("cannot decrypt password store entries") {
        Some("The selected private key cannot decrypt password entries.")
    } else {
        None
    }
}

#[cfg(not(feature = "flatpak"))]
fn friendly_password_entry_error_message(_message: &str) -> Option<&'static str> {
    None
}

#[cfg(all(feature = "setup", not(feature = "flatpak")))]
fn sync_backend_preferences_rows(backend_row: &ComboRow, pass_row: &EntryRow, preferences: &Preferences) {
    let backend = preferences.backend_kind();
    if backend_row.selected() != backend.combo_position() {
        backend_row.set_selected(backend.combo_position());
    }
    pass_row.set_visible(backend.uses_pass_command());
}

#[cfg(not(feature = "flatpak"))]
fn set_window_action_enabled(window: &ApplicationWindow, name: &str, enabled: bool) {
    let Some(action) = window.lookup_action(name) else {
        return;
    };
    let Ok(action) = action.downcast::<SimpleAction>() else {
        return;
    };
    action.set_enabled(enabled);
}

#[cfg(not(feature = "flatpak"))]
fn set_git_busy_actions_enabled(window: &ApplicationWindow, enabled: bool) {
    for action in [
        "open-new-password",
        "toggle-find",
        "open-git",
        "open-raw-pass-file",
        "git-clone",
        "save-password",
        "save-store-recipients",
        "synchronize",
        "open-preferences",
    ] {
        set_window_action_enabled(window, action, enabled);
    }
}

fn set_save_button_for_password(save: &Button) {
    save.set_action_name(Some("win.save-password"));
    save.set_tooltip_text(Some("Save password"));
}

fn prune_missing_store_dirs(settings: &Preferences) {
    if let Err(err) = settings.prune_missing_stores() {
        log_error(format!("Failed to remove missing password stores: {err}"));
    }
}

fn open_password_entry_page(
    state: &PasswordListPageState,
    opened_pass_file: OpenPassFile,
    push_page: bool,
) {
    let pass_label = opened_pass_file.label();
    let store_for_thread = opened_pass_file.store_path().to_string();
    set_opened_pass_file(opened_pass_file.clone());

    state.add.set_visible(false);
    state.find.set_visible(false);
    state.git.set_visible(false);
    state.back.set_visible(true);
    state.save.set_visible(true);
    set_save_button_for_password(&state.save);
    state.win.set_title(opened_pass_file.title());
    state.win.set_subtitle(&pass_label);
    state.entry.set_visible(false);
    state.username.set_text("");
    state.username.set_visible(false);
    state.otp.set_visible(false);
    state.dynamic_box.set_visible(false);
    state.raw_button.set_visible(false);
    state.status.set_visible(true);
    state.status.set_title("Decrypting Password Entry");
    state
        .status
        .set_description(Some("Please wait while the pass file is opened."));
    if push_page {
        state.nav.push(&state.page);
    }

    let (tx, rx) = mpsc::channel::<Result<String, String>>();
    let label_for_thread = pass_label.clone();
    thread::spawn(move || {
        #[cfg(any(feature = "setup", feature = "flatpak"))]
        let result = read_password_entry(&store_for_thread, &label_for_thread);
        #[cfg(all(not(feature = "setup"), not(feature = "flatpak")))]
        let result = {
            let settings = Preferences::new();
            let mut cmd = settings.command();
            cmd.env("PASSWORD_STORE_DIR", &store_for_thread)
                .arg(&label_for_thread);
            let output = run_command_output(
                &mut cmd,
                "Read password entry",
                CommandLogOptions::SENSITIVE,
            );
            match output {
                Ok(o) if o.status.success() => Ok(String::from_utf8_lossy(&o.stdout).to_string()),
                Ok(o) => {
                    let stderr = String::from_utf8_lossy(&o.stderr).trim().to_string();
                    if stderr.is_empty() {
                        Err(format!("pass failed: {}", o.status))
                    } else {
                        Err(stderr)
                    }
                }
                Err(e) => Err(format!("Failed to run pass: {e}")),
            }
        };
        let _ = tx.send(result);
    });

    let password_status = state.status.clone();
    let password_entry = state.entry.clone();
    let username_entry = state.username.clone();
    let otp_entry = state.otp.clone();
    let text_view = state.text.clone();
    let dynamic_box = state.dynamic_box.clone();
    let raw_button = state.raw_button.clone();
    let structured_templates = state.structured_templates.clone();
    let dynamic_rows = state.dynamic_rows.clone();
    let overlay = state.overlay.clone();
    let opened_pass_file_for_result = opened_pass_file.clone();
    let label_for_otp = pass_label.clone();
    let store_for_otp = opened_pass_file.store_path().to_string();
    #[cfg(feature = "flatpak")]
    let retry_state = state.clone();
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

                let (password, structured_lines) = parse_structured_pass_lines(&output);
                password_entry.set_text(&password);
                text_view.buffer().set_text(&output);
                rebuild_dynamic_fields_from_lines(
                    &dynamic_box,
                    &overlay,
                    &structured_templates,
                    &dynamic_rows,
                    &structured_lines,
                );
                sync_username_row_from_parsed_lines(
                    &username_entry,
                    updated_pass_file.as_ref(),
                    &structured_lines,
                );

                let otp = output.lines().skip(1).any(|line| line.contains("otpauth://"));
                otp_entry.set_visible(otp);
                if otp {
                    #[cfg(any(feature = "setup", feature = "flatpak"))]
                    match read_otp_code(&store_for_otp, &label_for_otp) {
                        Ok(code) => otp_entry.set_text(&code),
                        Err(err) => {
                            log_error(format!("Failed to read OTP code: {err}"));
                            otp_entry.set_text("");
                            let toast =
                                Toast::new(&with_logs_hint("Couldn't load the one-time password."));
                            overlay.add_toast(toast);
                        }
                    }
                    #[cfg(all(not(feature = "setup"), not(feature = "flatpak")))]
                    {
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
                                let toast =
                                    Toast::new(&with_logs_hint("Couldn't load the one-time password."));
                                overlay.add_toast(toast);
                            }
                            Err(e) => {
                                log_error(format!("Failed to read OTP code: {e}"));
                                let toast =
                                    Toast::new(&with_logs_hint("Couldn't load the one-time password."));
                                overlay.add_toast(toast);
                            }
                        }
                    }
                } else {
                    otp_entry.set_text("");
                }

                glib::ControlFlow::Break
            }
            Ok(Err(msg)) => {
                log_error(format!("Failed to open password entry: {msg}"));
                #[cfg(feature = "flatpak")]
                if is_locked_private_key_error(&msg) {
                    password_status.set_title("Private Key Locked");
                    password_status.set_description(Some(
                        "Unlock the selected private key to continue opening this pass file.",
                    ));
                    match resolved_ripasso_own_fingerprint() {
                        Ok(fingerprint) => {
                            let retry_pass_file = opened_pass_file_for_result.clone();
                            let retry_page_state = retry_state.clone();
                            prompt_private_key_unlock_for_action(
                                &overlay,
                                fingerprint,
                                Rc::new(move || {
                                    open_password_entry_page(
                                        &retry_page_state,
                                        retry_pass_file.clone(),
                                        false,
                                    );
                                }),
                            );
                            return glib::ControlFlow::Break;
                        }
                        Err(err) => {
                            log_error(format!(
                                "Failed to resolve the selected ripasso private key: {err}"
                            ));
                        }
                    }
                }

                let toast = if let Some(message) = friendly_password_entry_error_message(&msg) {
                    Toast::new(message)
                } else {
                    Toast::new(&with_logs_hint("Couldn't open the password entry."))
                };
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
}

fn show_password_list_page(state: &PasswordListPageState) {
    while state.nav.navigation_stack().n_items() > 1 {
        state.nav.pop();
    }

    clear_opened_pass_file();
    state.back.set_visible(false);
    state.save.set_visible(false);
    set_save_button_for_password(&state.save);
    state.add.set_visible(true);
    state.find.set_visible(true);
    state.git.set_visible(false);

    state.win.set_title("Password Store");
    state.win.set_subtitle("Manage your passwords");

    state.entry.set_text("");
    sync_username_row(&state.username, None);
    state.otp.set_visible(false);
    state.otp.set_text("");
    clear_box_children(&state.dynamic_box);
    state.dynamic_box.set_visible(false);
    state.raw_button.set_visible(false);
    state.structured_templates.borrow_mut().clear();
    state.dynamic_rows.borrow_mut().clear();
    state.text.buffer().set_text("");

    load_passwords_async(
        &state.list,
        state.git.clone(),
        state.find.clone(),
        state.save.clone(),
        state.overlay.clone(),
        true,
    );
}

fn current_store_recipients_request(
    state: &StoreRecipientsPageState,
) -> Option<StoreRecipientsRequest> {
    state.request.borrow().clone()
}

fn store_recipients_are_dirty(state: &StoreRecipientsPageState) -> bool {
    *state.recipients.borrow() != *state.saved_recipients.borrow()
}

fn queue_store_recipients_autosave(state: &StoreRecipientsPageState) {
    if current_store_recipients_request(state).is_none() {
        return;
    }
    if state.recipients.borrow().is_empty() {
        return;
    }
    if !store_recipients_are_dirty(state) {
        return;
    }
    if state.save_in_flight.get() {
        state.save_queued.set(true);
        return;
    }

    let _ = adw::prelude::WidgetExt::activate_action(&state.window, "win.save-store-recipients", None);
}

fn rebuild_store_recipients_list(state: &StoreRecipientsPageState) {
    while let Some(child) = state.list.first_child() {
        state.list.remove(&child);
    }

    state.list.append(&state.entry);

    let empty_subtitle = current_store_recipients_request(state)
        .map(|request| request.mode.empty_state_subtitle())
        .unwrap_or("Add at least one recipient before saving.");

    if state.recipients.borrow().is_empty() {
        let empty_row = ActionRow::builder()
            .title("No GPG recipients added")
            .subtitle(empty_subtitle)
            .build();
        empty_row.set_activatable(false);
        state.list.append(&empty_row);
        return;
    }

    for recipient in state.recipients.borrow().iter().cloned() {
        let row = ActionRow::builder().title(&recipient).build();
        row.set_activatable(false);
        let row_icon = Image::from_icon_name("dialog-password-symbolic");
        row_icon.add_css_class("dim-label");
        row.add_prefix(&row_icon);

        let delete_button = Button::from_icon_name("user-trash-symbolic");
        delete_button.add_css_class("flat");
        row.add_suffix(&delete_button);
        state.list.append(&row);

        let page_state = state.clone();
        delete_button.connect_clicked(move |_| {
            page_state
                .recipients
                .borrow_mut()
                .retain(|value| value != &recipient);
            rebuild_store_recipients_list(&page_state);
            queue_store_recipients_autosave(&page_state);
        });
    }
}

fn sync_store_recipients_page_header(state: &StoreRecipientsPageState) {
    let Some(request) = current_store_recipients_request(state) else {
        state.save.set_visible(false);
        set_save_button_for_password(&state.save);
        state.win.set_title("GPG Recipients");
        state.win.set_subtitle("Password Store");
        return;
    };

    state.add.set_visible(false);
    state.find.set_visible(false);
    state.git.set_visible(false);
    state.back.set_visible(true);
    state.save.set_visible(false);
    set_save_button_for_password(&state.save);
    state.page.set_title(request.mode.page_title());
    state.win.set_title(request.mode.page_title());
    state.win.set_subtitle(&request.store);
}

fn show_store_recipients_page(
    state: &StoreRecipientsPageState,
    request: StoreRecipientsRequest,
    initial_recipients: Vec<String>,
) {
    let saved_recipients = read_store_gpg_recipients(&request.store);
    *state.request.borrow_mut() = Some(request);
    *state.recipients.borrow_mut() = initial_recipients;
    *state.saved_recipients.borrow_mut() = saved_recipients;
    state.save_in_flight.set(false);
    state.save_queued.set(false);
    state.entry.set_text("");
    rebuild_store_recipients_list(state);
    sync_store_recipients_page_header(state);

    let already_visible = state
        .nav
        .visible_page()
        .as_ref()
        .map(|visible| visible == &state.page)
        .unwrap_or(false);
    if already_visible {
        return;
    }

    if navigation_stack_contains_page(&state.nav, &state.page) {
        let _ = state.nav.pop_to_page(&state.page);
    } else {
        state.nav.push(&state.page);
    }

    if current_store_recipients_request(state)
        .map(|request| request.mode == StoreRecipientsMode::Create)
        .unwrap_or(false)
    {
        queue_store_recipients_autosave(state);
    }
}

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
        menu.append(Some("_Find password file"), Some("win.toggle-find"));
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
    #[cfg(all(feature = "setup", not(feature = "flatpak")))]
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
    #[cfg(all(feature = "setup", not(feature = "flatpak")))]
    {
        backend_row.set_model(Some(&StringList::new(&["Ripasso", "Pass command"])));
        backend_row.set_visible(true);
        sync_backend_preferences_rows(&backend_row, &pass_row, &settings);
    }
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
    store_recipients_entry.set_title("Add recipients");
    store_recipients_entry.set_show_apply_button(true);
    let password_list_state = PasswordListPageState {
        nav: navigation_view.clone(),
        page: text_page.clone(),
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
        otp: otp_entry.clone(),
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

    // Selecting an item from the list
    {
        let overlay = toast_overlay.clone();
        let list_state = password_list_state.clone();
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
            open_password_entry_page(&list_state, opened_pass_file, true);
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
    #[cfg(all(feature = "setup", not(feature = "flatpak")))]
    {
        let overlay = toast_overlay.clone();
        let preferences = settings.clone();
        let pass_row = pass_row.clone();
        backend_row.connect_selected_notify(move |row| {
            let selected_backend = BackendKind::from_combo_position(row.selected());
            let current_backend = preferences.backend_kind();
            if selected_backend == current_backend {
                pass_row.set_visible(selected_backend.uses_pass_command());
                return;
            }

            if let Err(err) = preferences.set_backend_kind(selected_backend) {
                pass_row.set_visible(current_backend.uses_pass_command());
                row.set_selected(current_backend.combo_position());
                let toast = Toast::new(&err.message.to_string());
                overlay.add_toast(toast);
                return;
            }

            pass_row.set_visible(selected_backend.uses_pass_command());
        });
    }
    {
        let overlay = toast_overlay.clone();
        let preferences = Preferences::new();
        let buffer = new_pass_file_template_view.buffer();
        buffer.connect_changed(move |buffer| {
            let (start, end) = buffer.bounds();
            let template = buffer.text(&start, &end, false).to_string();
            if template == preferences.new_pass_file_template() {
                return;
            }
            if let Err(err) = preferences.set_new_pass_file_template(&template) {
                let message = err.message.to_string();
                let toast = Toast::new(&message);
                overlay.add_toast(toast);
            }
        });
    }
    {
        let page_state = store_recipients_page_state.clone();
        store_recipients_entry.connect_apply(move |entry| {
            if append_gpg_recipients(&page_state.recipients, entry.text().as_str()) {
                entry.set_text("");
                rebuild_store_recipients_list(&page_state);
                queue_store_recipients_autosave(&page_state);
            }
        });
    }
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
            let template_contents =
                new_pass_file_contents_from_template(&settings.new_pass_file_template());
            if path.is_empty() {
                let toast = Toast::new("Enter a name or path for the new entry.");
                overlay.add_toast(toast);
                return;
            }
            let opened_pass_file = OpenPassFile::from_label(store_root, &path);
            set_opened_pass_file(opened_pass_file.clone());
            let template_pass_file = refresh_opened_pass_file_from_contents(
                &opened_pass_file,
                &template_contents,
            )
            .or_else(get_opened_pass_file);
            let (_, structured_lines) = parse_structured_pass_lines(&template_contents);
            status.set_visible(false);
            entry.set_visible(true);
            sync_username_row_from_parsed_lines(&username, template_pass_file.as_ref(), &structured_lines);
            otp.set_visible(false);
            raw_button.set_visible(true);
            add.set_visible(false);
            find.set_visible(false);
            git.set_visible(false);
            back.set_visible(true);
            save.set_visible(true);
            set_save_button_for_password(&save);
            nav.push(&page);

            popover_add.popdown();
            popover_git.popdown();
            win.set_title("New password");
            win.set_subtitle(&path);
            entry.set_text("");
            otp.set_text("");
            text.buffer().set_text(&template_contents);
            rebuild_dynamic_fields_from_lines(
                &dynamic_box,
                &overlay,
                &structured_templates,
                &dynamic_rows,
                &structured_lines,
            );
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
                    &username.text(),
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
                    let (_, structured_lines) = parse_structured_pass_lines(&contents);
                    text.buffer().set_text(&contents);
                    rebuild_dynamic_fields_from_lines(
                        &dynamic_box,
                        &overlay,
                        &structured_templates,
                        &dynamic_rows,
                        &structured_lines,
                    );
                    entry.set_text(&password);
                    sync_username_row_from_parsed_lines(
                        &username,
                        updated_pass_file.as_ref(),
                        &structured_lines,
                    );
                    let otp_visible = contents.lines().skip(1).any(|line| line.contains("otpauth://"));
                    otp.set_visible(otp_visible);
                    if otp_visible {
                        #[cfg(any(feature = "setup", feature = "flatpak"))]
                        match read_otp_code(pass_file.store_path(), &label) {
                            Ok(code) => otp.set_text(&code),
                            Err(_) => otp.set_text(""),
                        }
                        #[cfg(all(not(feature = "setup"), not(feature = "flatpak")))]
                        {
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
                                    let code =
                                        String::from_utf8_lossy(&output.stdout).trim().to_string();
                                    otp.set_text(&code);
                                }
                                _ => otp.set_text(""),
                            }
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
    {
        let overlay = toast_overlay.clone();
        let stores_list = password_stores.clone();
        let list_state = password_list_state.clone();
        let recipients_page = store_recipients_page_state.clone();
        let action = SimpleAction::new("save-store-recipients", None);
        action.connect_activate(move |_, _| {
            let Some(request) = current_store_recipients_request(&recipients_page) else {
                return;
            };

            let recipients = recipients_page.recipients.borrow().clone();
            if recipients.is_empty() {
                return;
            }
            if !store_recipients_are_dirty(&recipients_page) {
                recipients_page.save_queued.set(false);
                return;
            }
            if recipients_page.save_in_flight.replace(true) {
                recipients_page.save_queued.set(true);
                return;
            }
            recipients_page.save_queued.set(false);

            let (tx, rx) = mpsc::channel::<Result<(), String>>();
            let store_for_thread = request.store.clone();
            let recipients_for_save = recipients.clone();
            thread::spawn(move || {
                let result = apply_password_store_recipients(&store_for_thread, &recipients_for_save);
                let _ = tx.send(result);
            });

            let overlay = overlay.clone();
            let stores_list = stores_list.clone();
            let list_state = list_state.clone();
            let recipients_page = recipients_page.clone();
            let request = request.clone();
            glib::timeout_add_local(Duration::from_millis(50), move || match rx.try_recv() {
                Ok(Ok(())) => {
                    let settings = Preferences::new();
                    *recipients_page.saved_recipients.borrow_mut() = recipients.clone();
                    match request.mode {
                        StoreRecipientsMode::Create => {
                            let stores = stores_with_preferred_first(&settings.stores(), &request.store);
                            if let Err(err) = settings.set_stores(stores) {
                                log_error(format!("Failed to save stores: {err}"));
                                let toast = Toast::new(
                                    "Password store created, but it couldn't be added to Preferences.",
                                );
                                overlay.add_toast(toast);
                            } else {
                                rebuild_store_list(
                                    &stores_list,
                                    &settings,
                                    &recipients_page.window,
                                    &overlay,
                                    &list_state,
                                    &recipients_page,
                                );
                                *recipients_page.request.borrow_mut() = Some(StoreRecipientsRequest {
                                    store: request.store.clone(),
                                    mode: StoreRecipientsMode::Edit,
                                });
                                sync_store_recipients_page_header(&recipients_page);
                            }
                        }
                        StoreRecipientsMode::Edit => {
                            rebuild_store_list(
                                &stores_list,
                                &settings,
                                &recipients_page.window,
                                &overlay,
                                &list_state,
                                &recipients_page,
                            );
                        }
                    }
                    recipients_page.save_in_flight.set(false);
                    if recipients_page.save_queued.get() || store_recipients_are_dirty(&recipients_page)
                    {
                        recipients_page.save_queued.set(false);
                        queue_store_recipients_autosave(&recipients_page);
                    }
                    glib::ControlFlow::Break
                }
                Ok(Err(message)) => {
                    let message = if request.mode == StoreRecipientsMode::Create {
                        with_logs_hint("Couldn't create the password store.")
                    } else {
                        message
                    };
                    recipients_page.save_in_flight.set(false);
                    if recipients_page.save_queued.get() {
                        recipients_page.save_queued.set(false);
                        queue_store_recipients_autosave(&recipients_page);
                    }
                    let toast = Toast::new(&message);
                    overlay.add_toast(toast);
                    glib::ControlFlow::Break
                }
                Err(TryRecvError::Empty) => glib::ControlFlow::Continue,
                Err(TryRecvError::Disconnected) => {
                    let message = if request.mode == StoreRecipientsMode::Create {
                        with_logs_hint("Couldn't create the password store.")
                    } else {
                        with_logs_hint("Couldn't save the password store recipients.")
                    };
                    recipients_page.save_in_flight.set(false);
                    if recipients_page.save_queued.get() {
                        recipients_page.save_queued.set(false);
                        queue_store_recipients_autosave(&recipients_page);
                    }
                    let toast = Toast::new(&message);
                    overlay.add_toast(toast);
                    glib::ControlFlow::Break
                }
            });
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
        #[cfg(all(feature = "setup", not(feature = "flatpak")))]
        let backend = backend_row.clone();
        #[cfg(feature = "flatpak")]
        let ripasso_keys = ripasso_private_keys_state.clone();
        let template_view = new_pass_file_template_view.clone();
        let list = password_stores.clone();
        let parent = window.clone();
        let overlay = toast_overlay.clone();
        let list_state = password_list_state.clone();
        let recipients_page = store_recipients_page_state.clone();
        let action = SimpleAction::new("open-preferences", None);
        action.connect_activate(move |_, _| {
            add.set_visible(false);
            find.set_visible(false);
            git.set_visible(false);
            back.set_visible(true);
            save.set_visible(false);
            set_save_button_for_password(&save);
            win.set_title("Preferences");
            win.set_subtitle("Password Store");
            nav.push(&page);

            let settings = Preferences::new();
            #[cfg(not(feature = "flatpak"))]
            command.set_text(&settings.command_value());
            #[cfg(all(feature = "setup", not(feature = "flatpak")))]
            sync_backend_preferences_rows(&backend, &command, &settings);
            template_view
                .buffer()
                .set_text(&settings.new_pass_file_template());
            #[cfg(feature = "flatpak")]
            rebuild_ripasso_private_keys_list(&ripasso_keys);
            rebuild_store_list(
                &list,
                &settings,
                &parent,
                &overlay,
                &list_state,
                &recipients_page,
            );
        });
        window.add_action(&action);
    }

    #[cfg(not(feature = "flatpak"))]
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
        let username = username_entry.clone();
        let text = text_view.clone();
        let structured_templates = structured_templates.clone();
        let dynamic_rows = dynamic_field_rows.clone();
        let action = SimpleAction::new("open-raw-pass-file", None);
        action.connect_activate(move |_, _| {
            let contents = structured_pass_contents(
                &entry.text(),
                &username.text(),
                &structured_templates.borrow(),
                &dynamic_rows.borrow(),
            );
            text.buffer().set_text(&contents);

            add.set_visible(false);
            find.set_visible(false);
            git.set_visible(false);
            back.set_visible(true);
            save.set_visible(true);
            set_save_button_for_password(&save);
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

    #[cfg(not(feature = "flatpak"))]
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

    #[cfg(not(feature = "flatpak"))]
    {
        let window = window.clone();
        git_url_entry.connect_apply(move |_| {
            let _ = adw::prelude::WidgetExt::activate_action(&window, "win.git-clone", None);
        });
    }

    #[cfg(not(feature = "flatpak"))]
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
        let recipients_page = store_recipients_page_state.clone();
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
            let recipients_page = recipients_page.clone();
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
                        &recipients_page,
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
                        &recipients_page,
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
                        &recipients_page,
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
                        &recipients_page,
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
        let username = username_entry.clone();
        let list_clone = list.clone();
        let win = window_title.clone();
        let back = back_button.clone();
        let git = git_button.clone();
        let add = add_button.clone();
        let find = find_button.clone();
        let save = save_button.clone();
        let nav = navigation_view.clone();
        let git_operation = git_operation.clone();
        let list_state = password_list_state.clone();
        let recipients_page = store_recipients_page_state.clone();
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
                        crate::logging::log_info("Git operation cancellation requested");
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
                let is_recipients_page = visible_page
                    .as_ref()
                    .map(|p| p == &recipients_page.page)
                    .unwrap_or(false);
                let is_log_page = visible_page
                    .as_ref()
                    .map(|p| p == &log_page)
                    .unwrap_or(false);
                save.set_visible(is_text_page || is_raw_page);
                if is_text_page {
                    set_save_button_for_password(&save);
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
                    set_save_button_for_password(&save);
                    win.set_title("Raw Pass File");
                    if let Some(pass_file) = get_opened_pass_file() {
                        let label = pass_file.label();
                        win.set_subtitle(&label);
                    } else {
                        win.set_subtitle("Password Store");
                    }
                } else if is_settings_page {
                    set_save_button_for_password(&save);
                    win.set_title("Preferences");
                    win.set_subtitle("Password Store");
                } else if is_recipients_page {
                    set_save_button_for_password(&save);
                    sync_store_recipients_page_header(&recipients_page);
                } else if is_log_page {
                    set_save_button_for_password(&save);
                    win.set_title("Logs");
                    win.set_subtitle("Command output");
                }
            } else {
                show_password_list_page(&list_state);
                return;
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

    #[cfg(not(feature = "flatpak"))]
    {
        let overlay_clone = toast_overlay.clone();
        let window_for_action = window.clone();
        let nav = navigation_view.clone();
        let text_page = text_page.clone();
        let raw_text_page = raw_text_page.clone();
        let settings_page = settings_page.clone();
        let recipients_page = store_recipients_page_state.clone();
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
            let recipients_page = recipients_page.clone();
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
                            &recipients_page,
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
                            &recipients_page,
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
                            &recipients_page,
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
                            &recipients_page,
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

    #[cfg(not(feature = "flatpak"))]
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
    app.set_accels_for_action("win.open-new-password", &["<primary>n"]);
    #[cfg(not(feature = "flatpak"))]
    app.set_accels_for_action("win.open-log", &["F12"]);
    app.set_accels_for_action("win.open-preferences", &["<primary>p"]);
    #[cfg(not(feature = "flatpak"))]
    app.set_accels_for_action("win.synchronize", &["<primary>s"]);
    #[cfg(not(feature = "flatpak"))]
    app.set_accels_for_action("win.open-git", &["<primary>i"]);

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
#[cfg(not(feature = "flatpak"))]
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

#[cfg(not(feature = "flatpak"))]
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

#[cfg(not(feature = "flatpak"))]
fn finish_git_busy_page(
    window: &ApplicationWindow,
    nav: &NavigationView,
    busy_page: &NavigationPage,
    text_page: &NavigationPage,
    raw_text_page: &NavigationPage,
    settings_page: &NavigationPage,
    recipients_page: &StoreRecipientsPageState,
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
        set_save_button_for_password(save);
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
    let is_recipients_page = visible_page
        .as_ref()
        .map(|page| page == &recipients_page.page)
        .unwrap_or(false);
    let is_log_page = visible_page
        .as_ref()
        .map(|page| page == log_page)
        .unwrap_or(false);

    save.set_visible(is_text_page || is_raw_page);
    if is_text_page {
        set_save_button_for_password(save);
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
        set_save_button_for_password(save);
        win.set_title("Raw Pass File");
        if let Some(pass_file) = get_opened_pass_file() {
            let label = pass_file.label();
            win.set_subtitle(&label);
        } else {
            win.set_subtitle("Password Store");
        }
    } else if is_settings_page {
        set_save_button_for_password(save);
        win.set_title("Preferences");
        win.set_subtitle("Password Store");
    } else if is_recipients_page {
        set_save_button_for_password(save);
        sync_store_recipients_page_header(recipients_page);
    } else if is_log_page {
        set_save_button_for_password(save);
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

    let settings = Preferences::new();
    prune_missing_store_dirs(&settings);
    let has_store_dirs = !settings.stores().is_empty();

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
                        let copy_overlay = toast_overlay.clone();
                        let copy_button = copy_btn.clone();
                        copy_btn.connect_clicked(move |_| {
                            popover.popdown();
                            copy_password_entry_to_clipboard(
                                entry.clone(),
                                copy_overlay.clone(),
                                Some(copy_button.clone()),
                            );
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
                            #[cfg(any(feature = "setup", feature = "flatpak"))]
                            let rename_result = rename_password_entry(&root, &old_label, &new_label);
                            #[cfg(all(not(feature = "setup"), not(feature = "flatpak")))]
                            let rename_result = {
                                let settings = Preferences::new();
                                let mut cmd = settings.command();
                                cmd.env("PASSWORD_STORE_DIR", &root)
                                    .arg("mv")
                                    .arg(&old_label)
                                    .arg(&new_label);
                                match run_command_status(
                                    &mut cmd,
                                    "Rename password entry",
                                    CommandLogOptions::DEFAULT,
                                ) {
                                    Ok(status) if status.success() => Ok(()),
                                    Ok(_) => Err(()),
                                    Err(_) => Err(()),
                                }
                            };
                            match rename_result {
                                Ok(()) => {
                                    let (parent, tail) = match new_label.rsplit_once('/') {
                                        Some((parent, tail)) => (parent, tail),
                                        None => ("", new_label.as_str()),
                                    };
                                    action_row.set_title(&tail);
                                    action_row.set_subtitle(&parent);
                                }
                                Err(_) => {
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
                        let _overlay = toast_overlay.clone();
                        delete_btn.connect_clicked(move |_| {
                            #[cfg(any(feature = "setup", feature = "flatpak"))]
                            {
                                let (tx, rx) = mpsc::channel::<Result<(), String>>();
                                let root = entry.store_path.clone();
                                let label = entry.label();
                                thread::spawn(move || {
                                    let result = delete_password_entry(&root, &label);
                                    let _ = tx.send(result);
                                });

                                let list = list.clone();
                                let row_clone = row_clone.clone();
                                let overlay = _overlay.clone();
                                glib::timeout_add_local(
                                    Duration::from_millis(50),
                                    move || match rx.try_recv() {
                                        Ok(Ok(())) => {
                                            list.remove(&row_clone);
                                            glib::ControlFlow::Break
                                        }
                                        Ok(Err(err)) => {
                                            log_error(format!(
                                                "Failed to delete password entry: {err}"
                                            ));
                                            let toast = Toast::new(
                                                "Couldn't delete the password entry.",
                                            );
                                            overlay.add_toast(toast);
                                            glib::ControlFlow::Break
                                        }
                                        Err(TryRecvError::Empty) => glib::ControlFlow::Continue,
                                        Err(TryRecvError::Disconnected) => {
                                            let toast =
                                                Toast::new("Couldn't delete the password entry.");
                                            overlay.add_toast(toast);
                                            glib::ControlFlow::Break
                                        }
                                    },
                                );
                            }
                            #[cfg(all(not(feature = "setup"), not(feature = "flatpak")))]
                            {
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
                            }
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
                    git_clone.set_visible(should_show_restore_button(
                        show_list_actions,
                        has_store_dirs_for_placeholder,
                        empty,
                    ));
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
                git_clone.set_visible(should_show_restore_button(
                    show_list_actions,
                    has_store_dirs_for_placeholder,
                    true,
                ));
                find_clone.set_visible(false);

                glib::ControlFlow::Break
            }
        }
    });
}

#[cfg(not(feature = "flatpak"))]
fn should_show_restore_button(show_list_actions: bool, has_store_dirs: bool, empty: bool) -> bool {
    show_list_actions && empty && !has_store_dirs
}

#[cfg(feature = "flatpak")]
fn should_show_restore_button(_show_list_actions: bool, _has_store_dirs: bool, _empty: bool) -> bool {
    false
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

#[cfg(all(not(feature = "setup"), not(feature = "flatpak")))]
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

#[cfg(any(feature = "setup", feature = "flatpak"))]
fn write_pass_entry(
    store_root: &str,
    label: &str,
    contents: &str,
    overwrite: bool,
) -> Result<(), String> {
    save_password_entry(store_root, label, contents, overwrite)
}

fn rebuild_store_list(
    list: &ListBox,
    settings: &Preferences,
    window: &ApplicationWindow,
    overlay: &ToastOverlay,
    list_state: &PasswordListPageState,
    recipients_page: &StoreRecipientsPageState,
) {
    while let Some(child) = list.first_child() {
        list.remove(&child);
    }

    prune_missing_store_dirs(settings);

    for store in settings.stores() {
        append_store_row(list, settings, &store, recipients_page);
    }

    append_store_picker_row(list, settings, window, overlay, list_state, recipients_page);
    append_store_creator_row(list, settings, window, overlay, recipients_page);
}

fn append_store_row(
    list: &ListBox,
    settings: &Preferences,
    store: &str,
    recipients_page: &StoreRecipientsPageState,
) {
    let row = ActionRow::builder()
        .title(store)
        .subtitle(store_gpg_recipients_subtitle(store))
        .build();
    row.set_activatable(true);

    let delete_btn = Button::from_icon_name("user-trash-symbolic");
    delete_btn.add_css_class("flat");
    row.add_suffix(&delete_btn);

    list.append(&row);

    let settings = settings.clone();
    let list = list.clone();
    let row_clone = row.clone();
    let store = store.to_string();
    let recipients_page = recipients_page.clone();
    let edit_store = store.clone();

    row.connect_activated(move |_| {
        show_store_recipients_page(
            &recipients_page,
            StoreRecipientsRequest {
                store: edit_store.clone(),
                mode: StoreRecipientsMode::Edit,
            },
            read_store_gpg_recipients(&edit_store),
        );
    });

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
    list_state: &PasswordListPageState,
    recipients_page: &StoreRecipientsPageState,
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
        let list_state = list_state.clone();
        let recipients_page = recipients_page.clone();
        row.connect_activated(move |_| {
            open_store_picker(
                &window,
                &list,
                &settings,
                &overlay,
                &list_state,
                &recipients_page,
            );
        });
    }

    {
        let settings = settings.clone();
        let list = list.clone();
        let window = window.clone();
        let overlay = overlay.clone();
        let list_state = list_state.clone();
        let recipients_page = recipients_page.clone();
        button.connect_clicked(move |_| {
            open_store_picker(
                &window,
                &list,
                &settings,
                &overlay,
                &list_state,
                &recipients_page,
            );
        });
    }
}

fn open_store_picker(
    window: &ApplicationWindow,
    list: &ListBox,
    settings: &Preferences,
    overlay: &ToastOverlay,
    list_state: &PasswordListPageState,
    recipients_page: &StoreRecipientsPageState,
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
    let list_state = list_state.clone();
    let recipients_page = recipients_page.clone();

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
                rebuild_store_list(
                    &list,
                    &settings,
                    &window,
                    &overlay,
                    &list_state,
                    &recipients_page,
                );
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
    recipients_page: &StoreRecipientsPageState,
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
        let window = window.clone();
        let overlay = overlay.clone();
        let recipients_page = recipients_page.clone();
        row.connect_activated(move |_| {
            open_store_creator_picker(&window, &settings, &overlay, &recipients_page);
        });
    }

    {
        let settings = settings.clone();
        let window = window.clone();
        let overlay = overlay.clone();
        let recipients_page = recipients_page.clone();
        button.connect_clicked(move |_| {
            open_store_creator_picker(&window, &settings, &overlay, &recipients_page);
        });
    }
}

fn open_store_creator_picker(
    window: &ApplicationWindow,
    settings: &Preferences,
    overlay: &ToastOverlay,
    recipients_page: &StoreRecipientsPageState,
) {
    let dialog = FileChooserNative::new(
        Some("Choose new password store folder"),
        Some(window),
        FileChooserAction::SelectFolder,
        Some("Select"),
        Some("Cancel"),
    );
    dialog.set_create_folders(true);

    let settings = settings.clone();
    let overlay = overlay.clone();
    let recipients_page = recipients_page.clone();
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
        let mut recipients = read_store_gpg_recipients(&store);
        if recipients.is_empty() {
            recipients = suggested_gpg_recipients(&settings);
        }
        show_store_recipients_page(
            &recipients_page,
            StoreRecipientsRequest {
                store,
                mode: StoreRecipientsMode::Create,
            },
            recipients,
        );
        dialog.hide();
    });

    dialog.show();
}

#[cfg(test)]
mod tests {
    use super::should_show_restore_button;
    use crate::pass_file::{
        new_pass_file_contents_from_template, parse_structured_pass_lines,
        structured_pass_contents_from_values, structured_username_value, uri_to_open,
        StructuredPassLine, UsernameFieldTemplate,
    };

    #[test]
    fn restore_button_is_hidden_for_an_empty_existing_store() {
        assert!(!should_show_restore_button(true, true, true));
    }

    #[test]
    fn restore_button_stays_hidden_off_the_list_page() {
        assert!(!should_show_restore_button(false, false, true));
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
                StructuredPassLine::Username(_) => None,
                StructuredPassLine::Preserved(_) => None,
            })
            .collect::<Vec<_>>();

        assert_eq!(values, vec!["hello@example.com".to_string(), "hello".to_string()]);
        assert_eq!(
            structured_pass_contents_from_values(&password, "", &templates, &values),
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
            StructuredPassLine::Preserved(ref line) if line == "otpauth://totp/example"
        ));
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
            structured_pass_contents_from_values("secret", "alice@example.com", &templates, &values),
            "secret\nusername:alice@example.com\nurl: https://example.com".to_string()
        );
    }
}
