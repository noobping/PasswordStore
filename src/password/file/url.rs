use crate::i18n::gettext;
use crate::logging::log_error;
use crate::support::ui::flat_icon_button_with_tooltip;
use crate::support::uri::launch_default_uri;
use adw::prelude::*;
use adw::{EntryRow, Toast, ToastOverlay};
use url::Url;

pub fn uri_to_open(value: &str) -> Result<String, &'static str> {
    let value = value.trim();
    if value.is_empty() {
        return Err("Enter a URL.");
    }

    let uri = if value.contains("://") {
        value.to_string()
    } else {
        format!("https://{value}")
    };
    let parsed = Url::parse(&uri).map_err(|_| "Enter a URL.")?;
    if !matches!(parsed.scheme(), "http" | "https") || parsed.host_str().is_none() {
        return Err("Use an http:// or https:// URL.");
    }

    Ok(parsed.into())
}

pub(super) fn add_open_url_suffix(
    row: &EntryRow,
    text: impl Fn() -> String + 'static,
    overlay: &ToastOverlay,
) {
    let button = flat_icon_button_with_tooltip("external-link-symbolic", "Open URL");
    let overlay = overlay.clone();
    button.connect_clicked(move |_| {
        let uri = match uri_to_open(&text()) {
            Ok(uri) => uri,
            Err(message) => {
                overlay.add_toast(Toast::new(&gettext(message)));
                return;
            }
        };

        let overlay_for_result = overlay.clone();
        let uri_for_result = uri.clone();
        launch_default_uri(&uri, move |result| {
            if let Err(error) = result {
                log_error(format!(
                    "Failed to open URL in the default browser.\nURL: {uri_for_result}\nerror: {error}"
                ));
                overlay_for_result.add_toast(Toast::new(&gettext("Couldn't open the link.")));
            }
        });
    });
    row.add_suffix(&button);
}

#[cfg(test)]
mod tests {
    use super::uri_to_open;

    #[test]
    fn bare_urls_get_https_when_opened() {
        assert_eq!(
            uri_to_open("example.com/path"),
            Ok("https://example.com/path".to_string())
        );
    }

    #[test]
    fn explicit_http_urls_are_preserved() {
        assert_eq!(
            uri_to_open("https://example.com/path"),
            Ok("https://example.com/path".to_string())
        );
    }

    #[test]
    fn unsupported_schemes_are_rejected() {
        assert_eq!(
            uri_to_open("file:///tmp/keycord"),
            Err("Use an http:// or https:// URL.")
        );
    }
}
