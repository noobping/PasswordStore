use crate::logging::log_error;
use crate::support::secure_fs::{ensure_private_dir, write_private_file};
use adw::glib;
#[cfg(target_os = "linux")]
use adw::prelude::*;
#[cfg(target_os = "linux")]
use adw::AlertDialog;
use std::fmt::Display;
use std::path::{Path, PathBuf};

const STARTUP_LOG_FILE: &str = "startup-error.log";
const STARTUP_RECOVERY_LOG_FILE: &str = "startup-recovery.log";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StartupRecoveryChoice {
    Quit,
    ContinueAndRemove,
}

pub fn fatal_startup_error(app_name: &str, context: &str, error: impl Display) -> glib::ExitCode {
    let detail = format!("{context}\nerror: {error}");
    log_error(&detail);
    eprintln!("{app_name}: {detail}");

    let log_path = persist_startup_error_log(app_name, &detail);
    let dialog_body = fatal_startup_dialog_body(app_name, &detail, log_path.as_deref());
    show_startup_error_dialog(app_name, &dialog_body);

    1.into()
}

fn persist_startup_error_log(app_name: &str, detail: &str) -> Option<PathBuf> {
    persist_startup_log(app_name, STARTUP_LOG_FILE, detail)
}

pub fn prompt_startup_recovery_dialog(app_name: &str, detail: &str) -> StartupRecoveryChoice {
    let log_path = persist_startup_log(app_name, STARTUP_RECOVERY_LOG_FILE, detail);
    let body = startup_recovery_dialog_body(detail, log_path.as_deref());
    show_startup_recovery_dialog(app_name, &body)
}

fn persist_startup_log(app_name: &str, file_name: &str, detail: &str) -> Option<PathBuf> {
    let path = startup_log_path(app_name, file_name);
    if let Some(parent) = path.parent() {
        ensure_private_dir(parent).ok()?;
    }
    write_private_file(&path, detail.as_bytes()).ok()?;
    Some(path)
}

fn startup_log_path(app_name: &str, file_name: &str) -> PathBuf {
    let app_dir = app_name.trim().to_ascii_lowercase();
    let base_dir = dirs_next::data_local_dir()
        .or_else(dirs_next::data_dir)
        .or_else(dirs_next::cache_dir)
        .or_else(dirs_next::home_dir);

    base_dir.map_or_else(
        || PathBuf::from(file_name),
        |dir: PathBuf| dir.join(app_dir).join(file_name),
    )
}

fn fatal_startup_dialog_body(app_name: &str, detail: &str, log_path: Option<&Path>) -> String {
    let mut body = format!("{app_name} couldn't start.\n\n{detail}");
    if let Some(path) = log_path {
        body.push_str("\n\nA startup error log was written to:\n");
        body.push_str(&path.to_string_lossy());
    }
    body
}

fn startup_recovery_dialog_body(detail: &str, log_path: Option<&Path>) -> String {
    let mut body = String::from(
        "Keycord found incompatible private-key data while preparing the app-managed key storage.\n\nQuit keeps the data untouched.\nContinue and remove incompatible data permanently deletes only the incompatible private-key files or folders so startup can continue.",
    );
    body.push_str("\n\nBlocked items:\n");
    body.push_str(detail);
    if let Some(path) = log_path {
        body.push_str("\n\nA startup recovery log was written to:\n");
        body.push_str(&path.to_string_lossy());
    }
    body
}

#[cfg(target_os = "windows")]
pub fn show_startup_error_dialog(title: &str, body: &str) {
    use std::ffi::c_void;
    use std::os::windows::ffi::OsStrExt;

    const MB_ICONERROR: u32 = 0x0000_0010;
    const MB_OK: u32 = 0x0000_0000;

    unsafe extern "system" {
        fn MessageBoxW(hwnd: *mut c_void, text: *const u16, caption: *const u16, kind: u32) -> i32;
    }

    fn utf16_null_terminated(value: &str) -> Vec<u16> {
        std::ffi::OsStr::new(value)
            .encode_wide()
            .chain(std::iter::once(0))
            .collect()
    }

    let title = utf16_null_terminated(title);
    let body = utf16_null_terminated(body);

    unsafe {
        let _ = MessageBoxW(
            std::ptr::null_mut(),
            body.as_ptr(),
            title.as_ptr(),
            MB_OK | MB_ICONERROR,
        );
    }
}

#[cfg(target_os = "windows")]
fn show_startup_recovery_dialog(title: &str, body: &str) -> StartupRecoveryChoice {
    use std::ffi::c_void;
    use std::os::windows::ffi::OsStrExt;

    const IDYES: i32 = 6;
    const MB_DEFBUTTON2: u32 = 0x0000_0100;
    const MB_ICONWARNING: u32 = 0x0000_0030;
    const MB_YESNO: u32 = 0x0000_0004;

    unsafe extern "system" {
        fn MessageBoxW(hwnd: *mut c_void, text: *const u16, caption: *const u16, kind: u32) -> i32;
    }

    fn utf16_null_terminated(value: &str) -> Vec<u16> {
        std::ffi::OsStr::new(value)
            .encode_wide()
            .chain(std::iter::once(0))
            .collect()
    }

    let title = utf16_null_terminated(title);
    let body = utf16_null_terminated(&format!(
        "{body}\n\nChoose Yes to continue and remove incompatible data.\nChoose No to quit without changing anything."
    ));

    let response = unsafe {
        MessageBoxW(
            std::ptr::null_mut(),
            body.as_ptr(),
            title.as_ptr(),
            MB_YESNO | MB_ICONWARNING | MB_DEFBUTTON2,
        )
    };

    if response == IDYES {
        StartupRecoveryChoice::ContinueAndRemove
    } else {
        StartupRecoveryChoice::Quit
    }
}

#[cfg(target_os = "linux")]
pub fn show_startup_error_dialog(title: &str, body: &str) {
    if adw::init().is_err() {
        return;
    }

    let dialog = AlertDialog::new(Some(title), Some(body));
    dialog.add_response("close", "Close");
    dialog.set_close_response("close");
    dialog.set_default_response(Some("close"));

    let loop_ = glib::MainLoop::new(None, false);
    let loop_for_response = loop_.clone();
    dialog.connect_response(None, move |dialog, _| {
        dialog.close();
        loop_for_response.quit();
    });

    dialog.present(None::<&adw::gtk::Widget>);
    loop_.run();
}

#[cfg(target_os = "linux")]
fn show_startup_recovery_dialog(title: &str, body: &str) -> StartupRecoveryChoice {
    if adw::init().is_err() {
        return StartupRecoveryChoice::Quit;
    }

    let dialog = AlertDialog::new(Some(title), Some(body));
    dialog.add_response("quit", "Quit");
    dialog.add_response("continue", "Continue and remove incompatible data");
    dialog.set_close_response("quit");
    dialog.set_default_response(Some("quit"));

    let loop_ = glib::MainLoop::new(None, false);
    let loop_for_response = loop_.clone();
    let choice = std::rc::Rc::new(std::cell::Cell::new(StartupRecoveryChoice::Quit));
    let choice_for_response = choice.clone();
    dialog.connect_response(None, move |dialog, response| {
        let selected = if response == "continue" {
            StartupRecoveryChoice::ContinueAndRemove
        } else {
            StartupRecoveryChoice::Quit
        };
        choice_for_response.set(selected);
        dialog.close();
        loop_for_response.quit();
    });

    dialog.present(None::<&adw::gtk::Widget>);
    loop_.run();
    choice.get()
}

#[cfg(test)]
mod tests {
    use super::{
        fatal_startup_dialog_body, startup_recovery_dialog_body, StartupRecoveryChoice,
    };
    use std::path::Path;

    #[test]
    fn startup_dialog_body_includes_log_path_when_available() {
        let body = fatal_startup_dialog_body(
            "Keycord",
            "Failed to initialize libadwaita.",
            Some(Path::new("/tmp/keycord/startup-error.log")),
        );

        assert!(body.contains("Keycord couldn't start."));
        assert!(body.contains("Failed to initialize libadwaita."));
        assert!(body.contains("/tmp/keycord/startup-error.log"));
    }

    #[test]
    fn startup_dialog_body_omits_log_path_when_unavailable() {
        let body = fatal_startup_dialog_body("Keycord", "No display available.", None);

        assert!(body.contains("Keycord couldn't start."));
        assert!(body.contains("No display available."));
        assert!(!body.contains("startup error log was written"));
    }

    #[test]
    fn startup_recovery_dialog_body_includes_log_path_when_available() {
        let body = startup_recovery_dialog_body(
            "/tmp/keycord/keys/BROKEN: That private key is invalid.",
            Some(Path::new("/tmp/keycord/startup-recovery.log")),
        );

        assert!(body.contains("Continue and remove incompatible data"));
        assert!(body.contains("/tmp/keycord/keys/BROKEN"));
        assert!(body.contains("/tmp/keycord/startup-recovery.log"));
    }

    #[test]
    fn startup_recovery_choice_defaults_to_quit() {
        assert_eq!(StartupRecoveryChoice::Quit, StartupRecoveryChoice::Quit);
    }
}
