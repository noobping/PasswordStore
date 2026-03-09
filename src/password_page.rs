use crate::backend::{read_password_entry, save_password_entry};
use crate::background::spawn_result_task;
#[cfg(feature = "flatpak")]
use crate::backend::resolved_ripasso_own_fingerprint;
use crate::item::OpenPassFile;
use crate::logging::log_error;
use crate::methods::{
    clear_opened_pass_file, get_opened_pass_file, is_opened_pass_file,
    refresh_opened_pass_file_from_contents, set_opened_pass_file,
};
use crate::pass_file::{
    apply_sensitive_field_visibility, clear_box_children, new_pass_file_contents_from_template,
    parse_structured_pass_lines, rebuild_dynamic_fields_from_lines, structured_pass_contents,
    sync_username_row, sync_username_row_from_parsed_lines, DynamicFieldRow, StructuredPassLine,
};
use crate::password_otp::PasswordOtpState;
use crate::password_list::load_passwords_async;
use crate::preferences::Preferences;
#[cfg(feature = "flatpak")]
use crate::ripasso_unlock::{is_locked_private_key_error, prompt_private_key_unlock_for_action};
use crate::window_messages::with_logs_hint;
use crate::window_navigation::set_save_button_for_password;
use adw::prelude::*;
use adw::{EntryRow, NavigationPage, PasswordEntryRow, StatusPage, Toast, ToastOverlay, WindowTitle};
use adw::gtk::{Box as GtkBox, Button, ListBox, Popover, TextView};
use std::cell::{Cell, RefCell};
use std::rc::Rc;

#[derive(Clone)]
pub(crate) struct PasswordPageState {
    pub(crate) nav: adw::NavigationView,
    pub(crate) page: NavigationPage,
    pub(crate) raw_page: NavigationPage,
    pub(crate) list: ListBox,
    pub(crate) back: Button,
    pub(crate) add: Button,
    pub(crate) find: Button,
    pub(crate) git: Button,
    pub(crate) save: Button,
    pub(crate) win: WindowTitle,
    pub(crate) status: StatusPage,
    pub(crate) entry: PasswordEntryRow,
    pub(crate) username: EntryRow,
    pub(crate) otp: PasswordOtpState,
    pub(crate) dynamic_box: GtkBox,
    pub(crate) raw_button: Button,
    pub(crate) structured_templates: Rc<RefCell<Vec<StructuredPassLine>>>,
    pub(crate) dynamic_rows: Rc<RefCell<Vec<DynamicFieldRow>>>,
    pub(crate) text: TextView,
    pub(crate) overlay: ToastOverlay,
    pub(crate) show_hidden_fields: Rc<Cell<bool>>,
}

#[cfg(feature = "flatpak")]
fn friendly_password_entry_error_message(message: &str) -> Option<&'static str> {
    if message.contains("cannot decrypt password store entries") {
        Some("This key can't open your items.")
    } else if message.contains("Import a private key in Preferences") {
        Some("Add a private key in Preferences.")
    } else {
        None
    }
}

#[cfg(not(feature = "flatpak"))]
fn friendly_password_entry_error_message(_message: &str) -> Option<&'static str> {
    None
}

fn show_password_editor_chrome(state: &PasswordPageState, title: &str, subtitle: &str) {
    state.add.set_visible(false);
    state.find.set_visible(false);
    state.git.set_visible(false);
    state.back.set_visible(true);
    state.save.set_visible(true);
    set_save_button_for_password(&state.save);
    state.win.set_title(title);
    state.win.set_subtitle(subtitle);
}

fn show_password_loading_state(state: &PasswordPageState, title: &str, subtitle: &str) {
    show_password_editor_chrome(state, title, subtitle);
    set_password_page_hidden_fields_visible(state, false);
    state.entry.set_visible(false);
    state.username.set_text("");
    state.username.set_visible(false);
    state.otp.clear();
    state.dynamic_box.set_visible(false);
    state.raw_button.set_visible(false);
    state.status.set_visible(true);
    state.status.set_title("Opening item");
    state.status.set_description(Some("Please wait."));
}

fn show_password_editor_fields(state: &PasswordPageState) {
    state.status.set_visible(false);
    state.entry.set_visible(true);
    state.raw_button.set_visible(true);
    apply_password_page_hidden_fields(state);
}

fn show_password_open_error(state: &PasswordPageState) {
    state.entry.set_visible(false);
    state.username.set_visible(false);
    state.otp.clear();
    state.dynamic_box.set_visible(false);
    state.raw_button.set_visible(false);
    state.status.set_visible(true);
    state.status.set_title("Item unavailable");
    state.status.set_description(Some("Try again."));
}

fn save_error_toast(message: &str) -> &'static str {
    if message.contains("already exists") {
        "An item with that name already exists."
    } else {
        "Couldn't save changes."
    }
}

fn structured_editor_contents(state: &PasswordPageState) -> String {
    structured_pass_contents(
        &state.entry.text(),
        &state.username.text(),
        state.otp.current_url().as_deref(),
        &state.structured_templates.borrow(),
        &state.dynamic_rows.borrow(),
    )
}

fn current_editor_contents(state: &PasswordPageState) -> String {
    let raw_visible = state
        .nav
        .visible_page()
        .as_ref()
        .map(|page| page == &state.raw_page)
        .unwrap_or(false);
    if raw_visible {
        let buffer = state.text.buffer();
        let (start, end) = buffer.bounds();
        buffer.text(&start, &end, false).to_string()
    } else {
        structured_editor_contents(state)
    }
}

fn sync_editor_contents(
    state: &PasswordPageState,
    contents: &str,
    pass_file: Option<&OpenPassFile>,
) {
    let (password, structured_lines) = parse_structured_pass_lines(contents);
    state.entry.set_text(&password);
    state.text.buffer().set_text(contents);
    rebuild_dynamic_fields_from_lines(
        &state.dynamic_box,
        &state.overlay,
        &state.structured_templates,
        &state.dynamic_rows,
        &structured_lines,
    );
    sync_username_row_from_parsed_lines(&state.username, pass_file, &structured_lines);
    state.otp.sync_from_parsed_lines(&structured_lines, true);
    apply_password_page_hidden_fields(state);
}

fn apply_password_page_hidden_fields(state: &PasswordPageState) {
    apply_sensitive_field_visibility(
        &state.entry,
        &state.otp.row,
        &state.dynamic_rows.borrow(),
        state.show_hidden_fields.get(),
    );
}

pub(crate) fn set_password_page_hidden_fields_visible(
    state: &PasswordPageState,
    visible: bool,
) {
    state.show_hidden_fields.set(visible);
    apply_password_page_hidden_fields(state);
}

fn read_password_entry_contents(store_root: &str, label: &str) -> Result<String, String> {
    read_password_entry(store_root, label)
}

pub(crate) fn open_password_entry_page(
    state: &PasswordPageState,
    opened_pass_file: OpenPassFile,
    push_page: bool,
) {
    let pass_label = opened_pass_file.label();
    let store_for_thread = opened_pass_file.store_path().to_string();
    set_opened_pass_file(opened_pass_file.clone());

    show_password_loading_state(state, opened_pass_file.title(), &pass_label);
    if push_page {
        state.nav.push(&state.page);
    }

    let label_for_thread = pass_label.clone();
    let state_for_result = state.clone();
    let opened_pass_file_for_result = opened_pass_file.clone();
    let state_for_disconnect = state.clone();
    let opened_pass_file_for_disconnect = opened_pass_file.clone();
    #[cfg(feature = "flatpak")]
    let retry_state = state.clone();
    spawn_result_task(
        move || read_password_entry_contents(&store_for_thread, &label_for_thread),
        move |result| {
            if !is_opened_pass_file(&opened_pass_file_for_result) {
                return;
            }

            match result {
                Ok(output) => {
                    let updated_pass_file = refresh_opened_pass_file_from_contents(
                        &opened_pass_file_for_result,
                        &output,
                    );
                    show_password_editor_fields(&state_for_result);
                    sync_editor_contents(&state_for_result, &output, updated_pass_file.as_ref());
                }
                Err(msg) => {
                    log_error(format!("Failed to open password entry: {msg}"));
                    #[cfg(feature = "flatpak")]
                    if is_locked_private_key_error(&msg) {
                        state_for_result.status.set_title("Unlock key");
                        state_for_result
                            .status
                            .set_description(Some("Enter your key password to continue."));
                        match resolved_ripasso_own_fingerprint() {
                            Ok(fingerprint) => {
                                let retry_pass_file = opened_pass_file_for_result.clone();
                                let retry_page_state = retry_state.clone();
                                prompt_private_key_unlock_for_action(
                                    &state_for_result.overlay,
                                    fingerprint,
                                    Rc::new(move || {
                                        open_password_entry_page(
                                            &retry_page_state,
                                            retry_pass_file.clone(),
                                            false,
                                        );
                                    }),
                                );
                                return;
                            }
                            Err(err) => {
                                log_error(format!(
                                    "Failed to resolve the selected ripasso private key: {err}"
                                ));
                            }
                        }
                    }
                    #[cfg(feature = "flatpak")]
                    if msg.contains("Import a private key in Preferences") {
                        let _ = adw::prelude::WidgetExt::activate_action(
                            &state_for_result.nav,
                            "win.open-preferences",
                            None,
                        );
                    }

                    show_password_open_error(&state_for_result);
                    let toast = if let Some(message) = friendly_password_entry_error_message(&msg) {
                        Toast::new(message)
                    } else {
                        Toast::new(&with_logs_hint("Couldn't open the item."))
                    };
                    state_for_result.overlay.add_toast(toast);
                }
            }
        },
        move || {
            if !is_opened_pass_file(&opened_pass_file_for_disconnect) {
                return;
            }

            show_password_open_error(&state_for_disconnect);
            state_for_disconnect.overlay.add_toast(Toast::new(&with_logs_hint(
                "Couldn't open the item.",
            )));
        },
    );
}

pub(crate) fn begin_new_password_entry(
    state: &PasswordPageState,
    path: &str,
    store_root: Option<String>,
    add_popover: &Popover,
    git_popover: &Popover,
) {
    let path = path.trim();
    if path.is_empty() {
        state.overlay.add_toast(Toast::new("Enter a name."));
        return;
    }

    let settings = Preferences::new();
    let store_root = store_root.unwrap_or_else(|| settings.store());
    if store_root.trim().is_empty() {
        state
            .overlay
            .add_toast(Toast::new("Add a store folder first."));
        add_popover.popdown();
        return;
    }
    let template_contents =
        new_pass_file_contents_from_template(&settings.new_pass_file_template());
    let opened_pass_file = OpenPassFile::from_label(store_root, path);
    set_opened_pass_file(opened_pass_file.clone());
    let template_pass_file = refresh_opened_pass_file_from_contents(
        &opened_pass_file,
        &template_contents,
    )
    .or_else(get_opened_pass_file);

    show_password_editor_chrome(state, "New item", path);
    show_password_editor_fields(state);
    state.otp.clear();
    let already_visible = state
        .nav
        .visible_page()
        .as_ref()
        .map(|visible| visible == &state.page)
        .unwrap_or(false);
    if !already_visible {
        state.nav.push(&state.page);
    }

    add_popover.popdown();
    git_popover.popdown();
    sync_editor_contents(state, &template_contents, template_pass_file.as_ref());
}

pub(crate) fn show_raw_pass_file_page(state: &PasswordPageState) {
    let contents = structured_editor_contents(state);
    state.text.buffer().set_text(&contents);

    let subtitle = get_opened_pass_file()
        .map(|pass_file| pass_file.label())
        .unwrap_or_else(|| "Password Store".to_string());
    show_password_editor_chrome(state, "Raw Pass File", &subtitle);

    let already_visible = state
        .nav
        .visible_page()
        .as_ref()
        .map(|visible| visible == &state.raw_page)
        .unwrap_or(false);
    if !already_visible {
        state.nav.push(&state.raw_page);
    }
}

pub(crate) fn save_current_password_entry(state: &PasswordPageState) {
    let Some(pass_file) = get_opened_pass_file() else {
        state.overlay.add_toast(Toast::new("Open an item first."));
        return;
    };

    let contents = current_editor_contents(state);
    let password = contents.lines().next().unwrap_or_default().to_string();
    if password.is_empty() {
        state.overlay.add_toast(Toast::new("Enter a password."));
        return;
    }

    let otp_url = match state.otp.current_url_for_save() {
        Ok(otp_url) => otp_url,
        Err(message) => {
            state.overlay.add_toast(Toast::new(message));
            return;
        }
    };
    let contents = if state
        .nav
        .visible_page()
        .as_ref()
        .map(|page| page == &state.raw_page)
        .unwrap_or(false)
    {
        contents
    } else {
        structured_pass_contents(
            &state.entry.text(),
            &state.username.text(),
            otp_url.as_deref(),
            &state.structured_templates.borrow(),
            &state.dynamic_rows.borrow(),
        )
    };
    let label = pass_file.label();
    match write_pass_entry(pass_file.store_path(), &label, &contents, true) {
        Ok(()) => {
            let updated_pass_file =
                refresh_opened_pass_file_from_contents(&pass_file, &contents);
            show_password_editor_fields(state);
            sync_editor_contents(state, &contents, updated_pass_file.as_ref());
            state.overlay.add_toast(Toast::new("Saved."));
        }
        Err(message) => {
            log_error(format!("Failed to save password entry: {message}"));
            state
                .overlay
                .add_toast(Toast::new(save_error_toast(&message)));
        }
    }
}

pub(crate) fn show_password_list_page(state: &PasswordPageState, show_hidden: bool) {
    while state.nav.navigation_stack().n_items() > 1 {
        state.nav.pop();
    }

    clear_opened_pass_file();
    set_password_page_hidden_fields_visible(state, false);
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
    state.otp.clear();
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
        show_hidden,
    );
}

fn write_pass_entry(
    store_root: &str,
    label: &str,
    contents: &str,
    overwrite: bool,
) -> Result<(), String> {
    save_password_entry(store_root, label, contents, overwrite)
}

pub(crate) fn retry_open_password_entry_if_needed(state: &PasswordPageState) -> bool {
    let visible_text_page = state
        .nav
        .visible_page()
        .as_ref()
        .map(|page| page == &state.page)
        .unwrap_or(false);
    if !visible_text_page || !state.status.is_visible() || state.entry.is_visible() {
        return false;
    }

    let Some(pass_file) = get_opened_pass_file() else {
        return false;
    };
    open_password_entry_page(state, pass_file, false);
    true
}
