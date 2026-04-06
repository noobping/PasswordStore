use crate::i18n::gettext;
use crate::logging::log_error;
#[cfg(not(target_os = "windows"))]
use crate::support::ui::flat_icon_button_with_tooltip;
#[cfg(target_os = "windows")]
use crate::support::ui::flat_resource_icon_button_with_tooltip;
use crate::support::uri::launch_default_uri;
use adw::prelude::*;
use adw::{EntryRow, Toast, ToastOverlay};
use url::Url;

#[cfg(target_os = "windows")]
const EXTERNAL_LINK_ICON_RESOURCE: &str = concat!(
    env!("RESOURCE_ID"),
    "/symbolic/apps/external-link-symbolic.svg"
);

pub fn uri_to_open(value: &str) -> Option<String> {
    let value = value.trim();
    if value.is_empty() {
        return None;
    }

    let uri = if value.contains("://") {
        value.to_string()
    } else {
        format!("https://{value}")
    };
    let parsed = Url::parse(&uri).ok()?;
    if matches!(parsed.scheme(), "http" | "https") {
        Some(parsed.into())
    } else {
        None
    }
}

pub(super) fn add_open_url_suffix(
    row: &EntryRow,
    text: impl Fn() -> String + 'static,
    overlay: &ToastOverlay,
) {
    #[cfg(target_os = "windows")]
    let button = flat_resource_icon_button_with_tooltip(EXTERNAL_LINK_ICON_RESOURCE, "Open URL");
    #[cfg(not(target_os = "windows"))]
    let button = flat_icon_button_with_tooltip("external-link-symbolic", "Open URL");
    let overlay = overlay.clone();
    button.connect_clicked(move |_| {
        let Some(uri) = uri_to_open(&text()) else {
            overlay.add_toast(Toast::new(&gettext("Enter an HTTP or HTTPS URL.")));
            return;
        };

        let overlay = overlay.clone();
        let uri_for_log = uri.clone();
        launch_default_uri(&uri, move |result| {
            if let Err(error) = result {
                log_error(format!(
                    "Failed to open URL in the default browser.\nURL: {uri_for_log}\nerror: {error}"
                ));
                overlay.add_toast(Toast::new(&gettext("Couldn't open the link.")));
            }
        });
    });
    row.add_suffix(&button);
}
