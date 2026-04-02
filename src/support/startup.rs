use crate::logging::log_error;
use crate::support::secure_fs::write_private_file;
use adw::glib;
#[cfg(target_os = "linux")]
use adw::prelude::*;
#[cfg(target_os = "linux")]
use adw::MessageDialog;
use std::fmt::Display;
use std::path::{Path, PathBuf};

const STARTUP_LOG_FILE: &str = "startup-error.log";

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
    let path = startup_log_path(app_name);
    write_private_file(&path, detail.as_bytes()).ok()?;
    Some(path)
}

fn startup_log_path(app_name: &str) -> PathBuf {
    let app_dir = app_name.trim().to_ascii_lowercase();
    let base_dir = dirs_next::data_local_dir()
        .or_else(dirs_next::data_dir)
        .or_else(dirs_next::cache_dir)
        .or_else(dirs_next::home_dir);

    base_dir.map_or_else(
        || PathBuf::from(STARTUP_LOG_FILE),
        |dir: PathBuf| dir.join(app_dir).join(STARTUP_LOG_FILE),
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

#[cfg(target_os = "linux")]
pub fn show_startup_error_dialog(title: &str, body: &str) {
    if adw::init().is_err() {
        return;
    }

    let dialog = MessageDialog::new(None::<&adw::gtk::Window>, Some(title), Some(body));
    dialog.add_response("close", "Close");
    dialog.set_close_response("close");
    dialog.set_default_response(Some("close"));

    let loop_ = glib::MainLoop::new(None, false);
    let loop_for_response = loop_.clone();
    dialog.connect_response(None, move |dialog, _| {
        dialog.close();
        loop_for_response.quit();
    });

    dialog.present();
    loop_.run();
}

#[cfg(test)]
mod tests {
    use super::fatal_startup_dialog_body;
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
}
