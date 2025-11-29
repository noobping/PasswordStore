#[cfg(feature = "setup")]
mod setup;

mod config;
mod item;
mod methods;
mod preferences;
mod window;

use crate::config::{APP_ID, RESOURCE_ID};
use crate::methods::non_null_to_string_option;
use crate::preferences::Preferences;

use adw::gio::SimpleAction;
use adw::prelude::*;
use adw::Application;
use gtk4::{
    gdk::Display,
    gio::{resources_register_include, ApplicationFlags},
    glib::ExitCode,
    IconTheme,
};
use std::ffi::OsString;
use std::result::Result::Ok;

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
            "has dev.noobping.passwordstore-symbolic? {}",
            theme.has_icon("dev.noobping.passwordstore-symbolic")
        );
    }

    // Create the application
    let app = Application::builder()
        .application_id(APP_ID)
        .flags(ApplicationFlags::HANDLES_OPEN | ApplicationFlags::HANDLES_COMMAND_LINE)
        .build();

    // keyboard shortcuts
    app.set_accels_for_action("app.about", &["F1"]);

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
                    unsafe { app.set_data("query", query) };
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

        let about_action = SimpleAction::new("about", None);
        about_action.connect_activate(move |_, _| {
            let project = env!("CARGO_PKG_NAME");
            let authors: Vec<_> = env!("CARGO_PKG_AUTHORS").split(':').collect();
            let comments = option_env!("CARGO_PKG_DESCRIPTION").unwrap_or("");
            let pass_version =
                get_pass_version().unwrap_or_else(|| "pass version unknown".to_string());
            let full_comments = if comments.is_empty() {
                format!("pass: {pass_version}")
            } else {
                format!("{project}: {comments}\n\n{pass_version}")
            };
            let about = adw::AboutDialog::builder()
                .application_name(project)
                .application_icon(APP_ID)
                .version(env!("CARGO_PKG_VERSION"))
                .developers(&authors[..])
                .comments(full_comments)
                .build();
            about.present(Some(&win));
        });
        app.add_action(&about_action);
    });

    app.run()
}

fn get_pass_version() -> Option<String> {
    let settings = Preferences::new();
    let output = settings.command().arg("--version").output().ok()?;
    if !output.status.success() {
        return None;
    }
    let stdout = String::from_utf8_lossy(&output.stdout);
    let lines: Vec<String> = stdout
        .lines()
        .map(str::trim) // trim whitespace
        .map(|line| line.trim_matches('=')) // remove leading/trailing '='
        .map(str::trim) // trim again after removing '='
        .filter(|line| !line.is_empty()) // skip borders/empty lines
        .map(|s| s.to_string())
        .collect();
    if lines.is_empty() {
        None
    } else {
        Some(lines.join("\n"))
    }
}
