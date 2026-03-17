use crate::logging::log_error;
use crate::support::ui::flat_icon_button_with_tooltip;
use adw::prelude::*;
use adw::{EntryRow, Toast, ToastOverlay};

pub fn uri_to_open(value: &str) -> Option<String> {
    let value = value.trim();
    if value.is_empty() {
        return None;
    }

    if value.contains("://") {
        Some(value.to_string())
    } else {
        Some(format!("https://{value}"))
    }
}

pub(super) fn add_open_url_suffix(
    row: &EntryRow,
    text: impl Fn() -> String + 'static,
    overlay: &ToastOverlay,
) {
    use adw::gtk::gdk::Display;

    let button = flat_icon_button_with_tooltip("external-link-symbolic", "Open URL");
    let overlay = overlay.clone();
    button.connect_clicked(move |_| {
        let Some(uri) = uri_to_open(&text()) else {
            overlay.add_toast(Toast::new("Enter a URL."));
            return;
        };

        let launch_result = Display::default().map_or_else(
            || adw::gio::AppInfo::launch_default_for_uri(&uri, None::<&adw::gio::AppLaunchContext>),
            |display| {
                let context = display.app_launch_context();
                adw::gio::AppInfo::launch_default_for_uri(&uri, Some(&context))
            },
        );

        if let Err(error) = launch_result {
            log_error(format!(
                "Failed to open URL in the default browser.\nURL: {uri}\nerror: {error}"
            ));
            overlay.add_toast(Toast::new("Couldn't open the link."));
        }
    });
    row.add_suffix(&button);
}
