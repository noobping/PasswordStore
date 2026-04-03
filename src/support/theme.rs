#[cfg(target_os = "linux")]
use adw::gio::{self, prelude::*};

#[cfg(target_os = "linux")]
const GNOME_INTERFACE_SCHEMA: &str = "org.gnome.desktop.interface";

#[cfg(target_os = "linux")]
pub fn install_color_scheme_tracking(display: &adw::gtk::gdk::Display) {
    let style_manager = adw::StyleManager::for_display(display);
    let gtk_settings = adw::gtk::Settings::for_display(display);
    let desktop_settings = gnome_interface_settings();

    sync_color_scheme(&style_manager, &gtk_settings, desktop_settings.as_ref());

    {
        let gtk_settings = gtk_settings.clone();
        let desktop_settings = desktop_settings.clone();
        style_manager.connect_system_supports_color_schemes_notify(move |style_manager| {
            sync_color_scheme(style_manager, &gtk_settings, desktop_settings.as_ref());
        });
    }

    for property_name in ["gtk-application-prefer-dark-theme", "gtk-theme-name"] {
        let style_manager = style_manager.clone();
        let desktop_settings = desktop_settings.clone();
        gtk_settings.connect_notify_local(Some(property_name), move |gtk_settings, _| {
            sync_color_scheme(&style_manager, gtk_settings, desktop_settings.as_ref());
        });
    }

    if let Some(desktop_settings) = desktop_settings {
        let schema = desktop_settings.settings_schema();
        for key in ["color-scheme", "gtk-theme"] {
            if schema.as_ref().is_none_or(|schema| !schema.has_key(key)) {
                continue;
            }

            let style_manager = style_manager.clone();
            let gtk_settings = gtk_settings.clone();
            desktop_settings.connect_changed(Some(key), move |desktop_settings, _| {
                sync_color_scheme(&style_manager, &gtk_settings, Some(desktop_settings));
            });
        }
    }
}

#[cfg(target_os = "linux")]
fn sync_color_scheme(
    style_manager: &adw::StyleManager,
    gtk_settings: &adw::gtk::Settings,
    desktop_settings: Option<&gio::Settings>,
) {
    let color_scheme = if style_manager.system_supports_color_schemes() {
        adw::ColorScheme::Default
    } else {
        match preferred_dark(gtk_settings, desktop_settings) {
            Some(true) => adw::ColorScheme::PreferDark,
            Some(false) => adw::ColorScheme::PreferLight,
            None => adw::ColorScheme::Default,
        }
    };

    if style_manager.color_scheme() != color_scheme {
        style_manager.set_color_scheme(color_scheme);
    }
}

#[cfg(target_os = "linux")]
fn gnome_interface_settings() -> Option<gio::Settings> {
    let source = gio::SettingsSchemaSource::default()?;
    let schema = source.lookup(GNOME_INTERFACE_SCHEMA, true)?;
    if !(schema.has_key("color-scheme") || schema.has_key("gtk-theme")) {
        return None;
    }

    Some(gio::Settings::new(GNOME_INTERFACE_SCHEMA))
}

#[cfg(target_os = "linux")]
fn preferred_dark(
    gtk_settings: &adw::gtk::Settings,
    desktop_settings: Option<&gio::Settings>,
) -> Option<bool> {
    gnome_preferred_dark(desktop_settings)
        .or_else(|| {
            gtk_settings
                .property::<bool>("gtk-application-prefer-dark-theme")
                .then_some(true)
        })
        .or_else(|| theme_name_preferred_dark(&gtk_settings.property::<String>("gtk-theme-name")))
}

#[cfg(target_os = "linux")]
fn gnome_preferred_dark(desktop_settings: Option<&gio::Settings>) -> Option<bool> {
    let desktop_settings = desktop_settings?;
    let schema = desktop_settings.settings_schema()?;

    if schema.has_key("color-scheme") {
        let preference = parse_color_scheme_preference(&desktop_settings.string("color-scheme"));
        if preference.is_some() {
            return preference;
        }
    }

    if schema.has_key("gtk-theme") {
        return theme_name_preferred_dark(&desktop_settings.string("gtk-theme"));
    }

    None
}

#[cfg(target_os = "linux")]
fn parse_color_scheme_preference(value: &str) -> Option<bool> {
    match value.trim().to_ascii_lowercase().as_str() {
        "prefer-dark" => Some(true),
        "default" | "prefer-light" => Some(false),
        _ => None,
    }
}

#[cfg(target_os = "linux")]
fn theme_name_preferred_dark(theme_name: &str) -> Option<bool> {
    let theme_name = theme_name.trim();
    if theme_name.is_empty() {
        return None;
    }

    Some(
        theme_name
            .to_ascii_lowercase()
            .split(|ch: char| !ch.is_ascii_alphanumeric())
            .any(|part| part == "dark"),
    )
}

#[cfg(all(test, target_os = "linux"))]
mod tests {
    use super::{parse_color_scheme_preference, theme_name_preferred_dark};

    #[test]
    fn color_scheme_preference_detects_dark_and_light() {
        assert_eq!(parse_color_scheme_preference("prefer-dark"), Some(true));
        assert_eq!(parse_color_scheme_preference("prefer-light"), Some(false));
        assert_eq!(parse_color_scheme_preference("default"), Some(false));
        assert_eq!(parse_color_scheme_preference("unsupported"), None);
    }

    #[test]
    fn dark_theme_names_are_detected() {
        assert_eq!(theme_name_preferred_dark("Adwaita-dark"), Some(true));
        assert_eq!(theme_name_preferred_dark("Yaru:dark"), Some(true));
        assert_eq!(
            theme_name_preferred_dark("Catppuccin Mocha Dark"),
            Some(true)
        );
    }

    #[test]
    fn light_theme_names_are_detected() {
        assert_eq!(theme_name_preferred_dark("Adwaita"), Some(false));
        assert_eq!(theme_name_preferred_dark("Yaru"), Some(false));
        assert_eq!(theme_name_preferred_dark(""), None);
    }
}
