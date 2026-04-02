#[cfg(any(target_os = "windows", test))]
use adw::gio;
#[cfg(any(target_os = "windows", test))]
use adw::prelude::FileExt;
#[cfg(not(target_os = "windows"))]
use gtk::UriLauncher;

#[cfg(target_os = "windows")]
use winsafe::{self as w, co};

pub fn launch_default_uri(uri: &str, on_result: impl FnOnce(Result<(), String>) + 'static) {
    #[cfg(target_os = "windows")]
    {
        on_result(
            w::HWND::GetDesktopWindow()
                .ShellExecute(
                    "open",
                    &windows_shell_target(uri),
                    None,
                    None,
                    co::SW::SHOWNORMAL,
                )
                .map_err(|err| format!("Windows ShellExecute failed: {err}")),
        );
        return;
    }

    #[cfg(not(target_os = "windows"))]
    {
        UriLauncher::new(uri).launch(
            None::<&gtk::Window>,
            None::<&adw::gio::Cancellable>,
            move |result| on_result(result.map_err(|err| err.to_string())),
        );
    }
}

#[cfg(any(target_os = "windows", test))]
fn windows_shell_target(uri: &str) -> String {
    if !uri.starts_with("file://") {
        return uri.to_string();
    }

    gio::File::for_uri(uri)
        .path()
        .map(|path| path.to_string_lossy().to_string())
        .filter(|path| !path.is_empty())
        .unwrap_or_else(|| uri.to_string())
}

#[cfg(test)]
mod tests {
    use super::windows_shell_target;

    #[test]
    fn windows_shell_target_keeps_web_urls() {
        assert_eq!(
            windows_shell_target("https://example.com/path"),
            "https://example.com/path".to_string()
        );
    }

    #[test]
    fn windows_shell_target_converts_file_uris_to_local_paths() {
        assert_eq!(
            windows_shell_target("file:///tmp/keycord"),
            "/tmp/keycord".to_string()
        );
    }
}
