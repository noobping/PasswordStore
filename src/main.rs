#![cfg_attr(
    all(not(debug_assertions), target_os = "windows"),
    windows_subsystem = "windows"
)]

#[cfg(all(target_os = "linux", feature = "setup"))]
mod setup;

mod backend;
mod clipboard;
mod i18n;
#[cfg(target_os = "linux")]
mod logging;
#[cfg(not(target_os = "linux"))]
#[path = "logging/non_linux.rs"]
mod logging;
mod password;
mod preferences;
mod private_key;
#[cfg(target_os = "linux")]
mod search_provider;
mod store;
mod support;
mod window;

use crate::i18n::gettext;
use crate::logging::{run_command_output, CommandLogOptions};
use crate::password::model::OpenPassFile;
use crate::preferences::Preferences;
use crate::support::object_data::{set_cloned_data, set_string_data, take_data, take_string_data};
use crate::window::navigation::APP_WINDOW_TITLE;

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

const APP_ID: &str = env!("APP_ID");
const RESOURCE_ID: &str = env!("RESOURCE_ID");
const ISSUE_URL: &str = concat!(env!("CARGO_PKG_REPOSITORY"), "/issues");
const RIPASSO_VERSION: &str = env!("RIPASSO_VERSION");
const SEQUOIA_OPENPGP_VERSION: &str = env!("SEQUOIA_OPENPGP_VERSION");
const SHORTCUTS_UI: &str = include_str!("../data/shortcuts.ui");

fn main() -> ExitCode {
    #[cfg(target_os = "linux")]
    {
        let args = std::env::args_os().collect::<Vec<_>>();
        if search_provider::is_search_provider_command(&args) {
            return search_provider::run();
        }
    }

    i18n::init();
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
            if let Some(pass_file) = command_line_pass_file(&args) {
                set_cloned_data(app, "open-pass-file", pass_file);
            } else if let Some(query) = command_line_query(&args) {
                set_string_data(app, "query", query);
            }
            app.activate(); // continue normal startup path

            0.into()
        });
    }

    // When the app is activated, create and show the main window
    app.connect_activate(|app| {
        let query = take_string_data(app, "query");
        let pass_file = take_data(app, "open-pass-file");
        let win = window::create_main_window(app, query, pass_file);
        win.present();
    });

    app.run()
}

fn command_line_pass_file(args: &[OsString]) -> Option<OpenPassFile> {
    if !args.get(1).is_some_and(|arg| arg == "--open-entry") {
        return None;
    }

    let store_root = args.get(2)?.to_string_lossy().into_owned();
    let label = args.get(3)?.to_string_lossy().into_owned();
    if store_root.is_empty() || label.is_empty() {
        return None;
    }

    Some(OpenPassFile::from_label(store_root, label))
}

fn command_line_query(args: &[OsString]) -> Option<String> {
    if args.len() <= 1 || args.get(1).is_some_and(|arg| arg == "--open-entry") {
        return None;
    }

    args[1..]
        .join(&OsString::from(" "))
        .into_string()
        .ok()
        .filter(|query| !query.is_empty())
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
    let application_name = gettext(APP_WINDOW_TITLE);
    let authors: Vec<_> = env!("CARGO_PKG_AUTHORS").split(':').collect();
    let developer_name = authors
        .first()
        .copied()
        .unwrap_or(application_name.as_str());
    let about = adw::AboutDialog::builder()
        .application_name(&application_name)
        .application_icon(APP_ID)
        .version(env!("CARGO_PKG_VERSION"))
        .developer_name(developer_name)
        .developers(&authors[..])
        .comments(about_comments(&application_name))
        .translator_credits(gettext("Translated by Nick."))
        .license_type(License::Gpl30Only)
        .website(env!("CARGO_PKG_HOMEPAGE"))
        .issue_url(ISSUE_URL)
        .support_url(ISSUE_URL)
        .build();
    about.add_link(&gettext("Repository"), env!("CARGO_PKG_REPOSITORY"));
    about
}

fn about_comments(project: &str) -> String {
    let comments = gettext(option_env!("CARGO_PKG_DESCRIPTION").unwrap_or(""));
    let settings = Preferences::new();
    let backend_details = if settings.uses_integrated_backend() {
        format!(
            "{} {RIPASSO_VERSION}\n{} {SEQUOIA_OPENPGP_VERSION}",
            gettext("backend: ripasso"),
            gettext("sequoia-openpgp")
        )
    } else {
        get_pass_version(&settings).map_or_else(
            || gettext("backend: host"),
            |version| format!("{}\n{version}", gettext("backend: host")),
        )
    };

    if comments.is_empty() {
        backend_details
    } else {
        format!("{project}: {comments}\n\n{backend_details}")
    }
}

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
        .map(str::to_string)
        .collect();
    if lines.is_empty() {
        None
    } else {
        Some(lines.join("\n"))
    }
}

#[cfg(test)]
mod tests {
    use super::{command_line_pass_file, command_line_query};
    use std::ffi::OsString;

    #[test]
    fn open_entry_command_line_is_parsed() {
        let args = vec![
            OsString::from("keycord"),
            OsString::from("--open-entry"),
            OsString::from("/tmp/store"),
            OsString::from("work/alice/github"),
        ];

        let pass_file = command_line_pass_file(&args).expect("expected pass file");
        assert_eq!(pass_file.store_path(), "/tmp/store");
        assert_eq!(pass_file.label(), "work/alice/github".to_string());
        assert_eq!(command_line_query(&args), None);
    }

    #[test]
    fn free_form_arguments_become_a_query() {
        let args = vec![
            OsString::from("keycord"),
            OsString::from("find"),
            OsString::from("otp"),
            OsString::from("and"),
            OsString::from("user"),
            OsString::from("alice"),
        ];

        assert_eq!(
            command_line_query(&args),
            Some("find otp and user alice".to_string())
        );
        assert!(command_line_pass_file(&args).is_none());
    }
}
