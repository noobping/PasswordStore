mod item;
mod window;

use adw::gio::SimpleAction;
use adw::prelude::*;
use adw::Application;
use gtk4::{gio, glib};
use std::process::Command;

#[allow(unused_imports)]
use gtk4::prelude::*; // Required for icons in a App Image

const APP_ID: &str = "dev.noobping.passwordstore";

fn main() -> glib::ExitCode {
    // Make the compiled GResource available at runtime.
    gio::resources_register_include!("resources.gresource").expect("Failed to register resources");

    // Initialize libadwaita
    adw::init().expect("Failed to initialize libadwaita");

    // Create the application
    let app = Application::builder()
        .application_id(APP_ID)
        .flags(gio::ApplicationFlags::HANDLES_OPEN)
        .build();

    // keyboard shortcuts
    app.set_accels_for_action("app.about", &["F1"]);

    // When the desktop/AppImage asks us to "open" something, just activate the app
    {
        app.connect_open(|app, _files, _hint| {
            app.activate();
        });
    }

    // When the app is activated, create and show the main window
    app.connect_activate(|app| {
        let win = window::create_main_window(app);
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
                .application_icon(project)
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
    let output = Command::new("pass").arg("--version").output().ok()?; // failed to spawn? -> None

    if !output.status.success() {
        return None;
    }

    let stdout = String::from_utf8_lossy(&output.stdout);

    // Collect cleaned, non-empty lines
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
