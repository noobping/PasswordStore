use adw::gio::{self, prelude::*};
use adw::gtk::{self, CssProvider, STYLE_PROVIDER_PRIORITY_APPLICATION};

const GNOME_INTERFACE_SCHEMA: &str = "org.gnome.desktop.interface";
const ACCENT_FOREGROUND: &str = "#ffffff";

#[derive(Clone, Copy)]
struct AccentPalette {
    background: &'static str,
    standalone_light: &'static str,
    standalone_dark: &'static str,
}

pub fn install_color_scheme_tracking(display: &adw::gtk::gdk::Display) {
    let style_manager = adw::StyleManager::for_display(display);
    let gtk_settings = adw::gtk::Settings::for_display(display);
    let desktop_settings = gnome_interface_settings();
    let accent_provider = CssProvider::new();
    gtk::style_context_add_provider_for_display(
        display,
        &accent_provider,
        STYLE_PROVIDER_PRIORITY_APPLICATION,
    );

    sync_appearance(
        &style_manager,
        &gtk_settings,
        desktop_settings.as_ref(),
        &accent_provider,
    );

    {
        let gtk_settings = gtk_settings.clone();
        let desktop_settings = desktop_settings.clone();
        let accent_provider = accent_provider.clone();
        style_manager.connect_system_supports_color_schemes_notify(move |style_manager| {
            sync_appearance(
                style_manager,
                &gtk_settings,
                desktop_settings.as_ref(),
                &accent_provider,
            );
        });
    }

    {
        let gtk_settings = gtk_settings.clone();
        let desktop_settings = desktop_settings.clone();
        let accent_provider = accent_provider.clone();
        style_manager.connect_dark_notify(move |style_manager| {
            sync_appearance(
                style_manager,
                &gtk_settings,
                desktop_settings.as_ref(),
                &accent_provider,
            );
        });
    }

    for property_name in ["gtk-theme-name"] {
        let style_manager = style_manager.clone();
        let desktop_settings = desktop_settings.clone();
        let accent_provider = accent_provider.clone();
        gtk_settings.connect_notify_local(Some(property_name), move |gtk_settings, _| {
            sync_appearance(
                &style_manager,
                gtk_settings,
                desktop_settings.as_ref(),
                &accent_provider,
            );
        });
    }

    if let Some(desktop_settings) = desktop_settings {
        let schema = desktop_settings.settings_schema();
        for key in ["color-scheme", "gtk-theme", "accent-color"] {
            if schema.as_ref().is_none_or(|schema| !schema.has_key(key)) {
                continue;
            }

            let style_manager = style_manager.clone();
            let gtk_settings = gtk_settings.clone();
            let accent_provider = accent_provider.clone();
            desktop_settings.connect_changed(Some(key), move |desktop_settings, _| {
                sync_appearance(
                    &style_manager,
                    &gtk_settings,
                    Some(desktop_settings),
                    &accent_provider,
                );
            });
        }
    }
}

fn sync_appearance(
    style_manager: &adw::StyleManager,
    gtk_settings: &adw::gtk::Settings,
    desktop_settings: Option<&gio::Settings>,
    accent_provider: &CssProvider,
) {
    let preferred_dark = preferred_dark(gtk_settings, desktop_settings);
    let color_scheme = if style_manager.system_supports_color_schemes() {
        adw::ColorScheme::Default
    } else {
        match preferred_dark {
            Some(true) => adw::ColorScheme::PreferDark,
            Some(false) => adw::ColorScheme::PreferLight,
            None => adw::ColorScheme::Default,
        }
    };

    if style_manager.color_scheme() != color_scheme {
        style_manager.set_color_scheme(color_scheme);
    }

    let dark = preferred_dark.unwrap_or_else(|| style_manager.is_dark());
    sync_accent_provider(accent_provider, dark, desktop_settings);
}

fn sync_accent_provider(
    accent_provider: &CssProvider,
    dark: bool,
    desktop_settings: Option<&gio::Settings>,
) {
    let css = preferred_accent_palette(desktop_settings)
        .map(|palette| accent_css(palette, dark))
        .unwrap_or_default();

    accent_provider.load_from_data(&css);
}

fn gnome_interface_settings() -> Option<gio::Settings> {
    let source = gio::SettingsSchemaSource::default()?;
    let schema = source.lookup(GNOME_INTERFACE_SCHEMA, true)?;
    if !(schema.has_key("color-scheme")
        || schema.has_key("gtk-theme")
        || schema.has_key("accent-color"))
    {
        return None;
    }

    Some(gio::Settings::new(GNOME_INTERFACE_SCHEMA))
}

fn preferred_dark(
    gtk_settings: &adw::gtk::Settings,
    desktop_settings: Option<&gio::Settings>,
) -> Option<bool> {
    gnome_preferred_dark(desktop_settings)
        .or_else(|| theme_name_preferred_dark(&gtk_settings.property::<String>("gtk-theme-name")))
}

fn preferred_accent_palette(desktop_settings: Option<&gio::Settings>) -> Option<AccentPalette> {
    let desktop_settings = desktop_settings?;
    let schema = desktop_settings.settings_schema()?;
    if !schema.has_key("accent-color") {
        return None;
    }

    parse_accent_palette(&desktop_settings.string("accent-color"))
}

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

fn parse_color_scheme_preference(value: &str) -> Option<bool> {
    match value.trim().to_ascii_lowercase().as_str() {
        "prefer-dark" => Some(true),
        "default" | "prefer-light" => Some(false),
        _ => None,
    }
}

fn parse_accent_palette(value: &str) -> Option<AccentPalette> {
    match value.trim().to_ascii_lowercase().as_str() {
        "blue" => Some(AccentPalette {
            background: "#3584e4",
            standalone_light: "#0461be",
            standalone_dark: "#81d0ff",
        }),
        "teal" => Some(AccentPalette {
            background: "#2190a4",
            standalone_light: "#007184",
            standalone_dark: "#7bdff4",
        }),
        "green" => Some(AccentPalette {
            background: "#3a944a",
            standalone_light: "#15772e",
            standalone_dark: "#8de698",
        }),
        "yellow" => Some(AccentPalette {
            background: "#c88800",
            standalone_light: "#905300",
            standalone_dark: "#ffc057",
        }),
        "orange" => Some(AccentPalette {
            background: "#ed5b00",
            standalone_light: "#b62200",
            standalone_dark: "#ff9c5b",
        }),
        "red" => Some(AccentPalette {
            background: "#e62d42",
            standalone_light: "#c00023",
            standalone_dark: "#ff888c",
        }),
        "pink" => Some(AccentPalette {
            background: "#d56199",
            standalone_light: "#a2326c",
            standalone_dark: "#ffa0d8",
        }),
        "purple" => Some(AccentPalette {
            background: "#9141ac",
            standalone_light: "#8939a4",
            standalone_dark: "#fba7ff",
        }),
        "slate" => Some(AccentPalette {
            background: "#6f8396",
            standalone_light: "#526678",
            standalone_dark: "#bbd1e5",
        }),
        _ => None,
    }
}

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

fn accent_css(palette: AccentPalette, dark: bool) -> String {
    let accent = if dark {
        palette.standalone_dark
    } else {
        palette.standalone_light
    };

    format!(
        "@define-color accent_bg_color {background};\n@define-color accent_fg_color {foreground};\n@define-color accent_color {accent};\n:root {{\n  --accent-bg-color: {background};\n  --accent-fg-color: {foreground};\n  --accent-color: {accent};\n}}\n",
        background = palette.background,
        foreground = ACCENT_FOREGROUND,
        accent = accent,
    )
}

#[cfg(test)]
mod tests {
    use super::{
        accent_css, parse_accent_palette, parse_color_scheme_preference, theme_name_preferred_dark,
    };

    #[test]
    fn color_scheme_preference_detects_dark_and_light() {
        assert_eq!(parse_color_scheme_preference("prefer-dark"), Some(true));
        assert_eq!(parse_color_scheme_preference("prefer-light"), Some(false));
        assert_eq!(parse_color_scheme_preference("default"), Some(false));
        assert_eq!(parse_color_scheme_preference("unsupported"), None);
    }

    #[test]
    fn accent_palette_detects_known_values() {
        assert!(parse_accent_palette("blue").is_some());
        assert!(parse_accent_palette("teal").is_some());
        assert!(parse_accent_palette("purple").is_some());
        assert!(parse_accent_palette("unsupported").is_none());
    }

    #[test]
    fn accent_css_uses_dark_variant_when_requested() {
        let palette = parse_accent_palette("blue").expect("blue palette");
        let css = accent_css(palette, true);

        assert!(css.contains("#81d0ff"));
        assert!(css.contains("accent_bg_color #3584e4"));
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
