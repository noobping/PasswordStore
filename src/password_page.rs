#[cfg(any(feature = "setup", feature = "flatpak"))]
use crate::backend::{read_otp_code, read_password_entry};
#[cfg(feature = "flatpak")]
use crate::backend::resolved_ripasso_own_fingerprint;
use crate::item::OpenPassFile;
use crate::logging::log_error;
#[cfg(all(not(feature = "setup"), not(feature = "flatpak")))]
use crate::logging::{run_command_output, CommandLogOptions};
use crate::methods::{
    clear_opened_pass_file, is_opened_pass_file, refresh_opened_pass_file_from_contents,
    set_opened_pass_file,
};
use crate::pass_file::{
    clear_box_children, parse_structured_pass_lines, rebuild_dynamic_fields_from_lines,
    sync_username_row, sync_username_row_from_parsed_lines, DynamicFieldRow, StructuredPassLine,
};
use crate::password_list::load_passwords_async;
#[cfg(all(not(feature = "setup"), not(feature = "flatpak")))]
use crate::preferences::Preferences;
#[cfg(feature = "flatpak")]
use crate::ripasso_unlock::{is_locked_private_key_error, prompt_private_key_unlock_for_action};
use crate::window_messages::with_logs_hint;
use crate::window_navigation::set_save_button_for_password;
use adw::prelude::*;
use adw::{
    glib, EntryRow, NavigationPage, PasswordEntryRow, StatusPage, Toast, ToastOverlay,
    WindowTitle,
};
use adw::gtk::{Box as GtkBox, Button, ListBox, TextView};
use std::cell::RefCell;
use std::rc::Rc;
use std::sync::mpsc;
use std::thread;
use std::time::Duration;

#[derive(Clone)]
pub(crate) struct PasswordPageState {
    pub(crate) nav: adw::NavigationView,
    pub(crate) page: NavigationPage,
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
    pub(crate) otp: PasswordEntryRow,
    pub(crate) dynamic_box: GtkBox,
    pub(crate) raw_button: Button,
    pub(crate) structured_templates: Rc<RefCell<Vec<StructuredPassLine>>>,
    pub(crate) dynamic_rows: Rc<RefCell<Vec<DynamicFieldRow>>>,
    pub(crate) text: TextView,
    pub(crate) overlay: ToastOverlay,
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

pub(crate) fn open_password_entry_page(
    state: &PasswordPageState,
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
                let updated_pass_file =
                    refresh_opened_pass_file_from_contents(&opened_pass_file_for_result, &output);
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
                            overlay.add_toast(Toast::new(&with_logs_hint(
                                "Couldn't load the one-time password.",
                            )));
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
                                overlay.add_toast(Toast::new(&with_logs_hint(
                                    "Couldn't load the one-time password.",
                                )));
                            }
                            Err(e) => {
                                log_error(format!("Failed to read OTP code: {e}"));
                                overlay.add_toast(Toast::new(&with_logs_hint(
                                    "Couldn't load the one-time password.",
                                )));
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
                overlay.add_toast(Toast::new(&with_logs_hint(
                    "Couldn't open the password entry.",
                )));
                glib::ControlFlow::Break
            }
        }
    });
}

pub(crate) fn show_password_list_page(state: &PasswordPageState) {
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
