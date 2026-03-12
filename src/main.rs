#[cfg(target_os = "linux")]
#[cfg(feature = "setup")]
mod setup;

mod backend;
mod clipboard;
mod logging;
mod password;
mod preferences;
#[cfg(feature = "flatpak")]
mod private_key;
mod store;
mod support;
mod window;

#[cfg(not(feature = "flatpak"))]
use crate::logging::{run_command_output, CommandLogOptions};
#[cfg(not(feature = "flatpak"))]
use crate::preferences::Preferences;
use crate::support::object_data::{non_null_to_string_option, set_string_data};

use adw::gio::SimpleAction;
use adw::gtk::{
    gdk::Display,
    gio::{resources_register_include, ApplicationFlags},
    glib::ExitCode,
    Builder, IconTheme, License, ShortcutsWindow,
};
use adw::prelude::*;
use adw::Application;
use std::ffi::OsString;
use std::result::Result::Ok;

const APP_ID: &str = env!("APP_ID");
const RESOURCE_ID: &str = env!("RESOURCE_ID");
const ISSUE_URL: &str = concat!(env!("CARGO_PKG_REPOSITORY"), "/issues");
#[cfg(not(feature = "flatpak"))]
const RIPASSO_VERSION: &str = env!("RIPASSO_VERSION");
#[cfg(not(feature = "flatpak"))]
const SEQUOIA_OPENPGP_VERSION: &str = env!("SEQUOIA_OPENPGP_VERSION");
#[cfg(not(feature = "flatpak"))]
const SHORTCUTS_UI: &str = include_str!("../data/shortcuts-standard.ui");
#[cfg(feature = "flatpak")]
const SHORTCUTS_UI: &str = include_str!("../data/shortcuts-flatpak.ui");

fn main() -> ExitCode {
    resources_register_include!("compiled.gresource").expect("Failed to register resources");

    // Initialize libadwaita
    adw::init().expect("Failed to initialize libadwaita");

    let display = Display::default().expect("No display available");
    let theme = IconTheme::for_display(&display);
    theme.add_resource_path(RESOURCE_ID);
    #[cfg(debug_assertions)]
    {
        println!("RESOURCE_ID = {RESOURCE_ID}");
        println!("has git-symbolic? {}", theme.has_icon("git-symbolic"));
        println!("has left-symbolic? {}", theme.has_icon("left-symbolic"));
        println!(
            "has io.github.noobping.keycord? {}",
            theme.has_icon("io.github.noobping.keycord")
        );
    }

    // Create the application
    let app = Application::builder()
        .application_id(APP_ID)
        .flags(ApplicationFlags::HANDLES_OPEN | ApplicationFlags::HANDLES_COMMAND_LINE)
        .build();

    // keyboard shortcuts
    app.set_accels_for_action("app.about", &["F1"]);
    register_app_actions(&app);

    // When the desktop asks us to "open" something, just activate the app
    {
        app.connect_open(|app, _files, _hint| {
            app.activate();
        });
    }

    // Handle command-line arguments
    {
        app.connect_command_line(|app, cmd| {
            let args = cmd.arguments();
            if args.len() > 1 {
                // Everything after the program name becomes the query
                let query = args[1..].join(&OsString::from(" ")).into_string();
                if let Ok(query) = query {
                    // Stash it on the Application so we can read it in activate
                    set_string_data(app, "query", query);
                }
            }
            app.activate(); // continue normal startup path

            0.into()
        });
    }

    // When the app is activated, create and show the main window
    app.connect_activate(|app| {
        let query = non_null_to_string_option(app, "query");
        let win = window::create_main_window(app, query);
        win.present();
    });

    app.run()
}

fn register_app_actions(app: &Application) {
    let about_action = SimpleAction::new("about", None);
    let app_for_about = app.clone();
    about_action.connect_activate(move |_, _| {
        let about = build_about_dialog();
        let active_window = app_for_about.active_window();
        about.present(active_window.as_ref());
    });
    app.add_action(&about_action);

    let shortcuts_action = SimpleAction::new("shortcuts", None);
    let app_for_shortcuts = app.clone();
    shortcuts_action.connect_activate(move |_, _| {
        let shortcuts = build_shortcuts_window();
        if let Some(active_window) = app_for_shortcuts.active_window() {
            shortcuts.set_transient_for(Some(&active_window));
        }
        shortcuts.present();
    });
    app.add_action(&shortcuts_action);
}

fn build_shortcuts_window() -> ShortcutsWindow {
    let builder = Builder::from_string(SHORTCUTS_UI);
    builder
        .object("shortcuts_window")
        .expect("Failed to build shortcuts window")
}

fn build_about_dialog() -> adw::AboutDialog {
    let project = env!("CARGO_PKG_NAME");
    let authors: Vec<_> = env!("CARGO_PKG_AUTHORS").split(':').collect();
    let about = adw::AboutDialog::builder()
        .application_name(project)
        .application_icon(APP_ID)
        .version(env!("CARGO_PKG_VERSION"))
        .developer_name(authors.first().copied().unwrap_or(project))
        .developers(&authors[..])
        .comments(about_comments(project))
        .license_type(License::Gpl30Only)
        .website(env!("CARGO_PKG_HOMEPAGE"))
        .issue_url(ISSUE_URL)
        .support_url(ISSUE_URL)
        .build();
    about.add_link("Repository", env!("CARGO_PKG_REPOSITORY"));
    about
}

#[cfg(not(feature = "flatpak"))]
fn about_comments(project: &str) -> String {
    let comments = option_env!("CARGO_PKG_DESCRIPTION").unwrap_or("");
    let settings = Preferences::new();
    let backend_details = if settings.uses_integrated_backend() {
        format!("backend: ripasso {RIPASSO_VERSION}\nsequoia-openpgp {SEQUOIA_OPENPGP_VERSION}")
    } else {
        get_pass_version(&settings).map_or_else(
            || "backend: host command".to_string(),
            |version| format!("backend: host command\n{version}"),
        )
    };

    if comments.is_empty() {
        backend_details
    } else {
        format!("{project}: {comments}\n\n{backend_details}")
    }
}

#[cfg(feature = "flatpak")]
fn about_comments(_project: &str) -> String {
    option_env!("CARGO_PKG_DESCRIPTION")
        .unwrap_or("")
        .to_string()
}

#[cfg(not(feature = "flatpak"))]
fn get_pass_version(settings: &Preferences) -> Option<String> {
    let mut cmd = settings.command();
    cmd.arg("--version");
    let output =
        run_command_output(&mut cmd, "Read pass version", CommandLogOptions::DEFAULT).ok()?;
    if !output.status.success() {
        return None;
    }
    let stdout = String::from_utf8_lossy(&output.stdout);
    let lines: Vec<String> = stdout
        .lines()
        .map(str::trim)
        .map(|line| line.trim_matches('='))
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .map(|s| s.to_string())
        .collect();
    if lines.is_empty() {
        None
    } else {
        Some(lines.join("\n"))
    }
}
