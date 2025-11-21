mod item;
mod window;

use adw::gio::SimpleAction;
use adw::prelude::*;
use adw::Application;
use gtk4::{gio, glib};
use std::process::Command;

const APP_ID: &str = "dev.noobping.passadw";

fn main() -> glib::ExitCode {
    // Make the compiled GResource available at runtime.
    gio::resources_register_include!("resources.gresource")
        .expect("Failed to register resources.gresource");

    // Initialize libadwaita
    adw::init().expect("Failed to initialize libadwaita");

    // Create the application
    let app = Application::builder().application_id(APP_ID).build();

    // keyboard shortcuts
    app.set_accels_for_action("app.about", &["F1"]);

    // When the app is activated, create and show the main window
    app.connect_activate(|app| {
        let win = window::create_main_window(app);
        win.window.present();

        let about_action = SimpleAction::new("about", None);
        about_action.connect_activate(move |_, _| {
            let authors: Vec<_> = env!("CARGO_PKG_AUTHORS").split(':').collect();
            let comments = option_env!("CARGO_PKG_DESCRIPTION").unwrap_or("");
            let pass_version = get_pass_version().unwrap_or_else(|| "unknown".to_string());
            let full_comments = if comments.is_empty() {
                format!("pass: {pass_version}")
            } else {
                format!("{comments}\npass: {pass_version}")
            };
            let about = adw::AboutDialog::builder()
                .application_name(env!("CARGO_PKG_NAME"))
                .application_icon("passadw")
                .version(env!("CARGO_PKG_VERSION"))
                .developers(&authors[..])
                .comments(full_comments)
                .build();
            about.present(Some(&win.window));
        });
        app.add_action(&about_action);
    });

    app.run()
}

fn get_pass_version() -> Option<String> {
    let output = Command::new("pass").arg("--version").output().ok()?; // command failed to spawn

    if !output.status.success() {
        return None;
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    // Usually first line is enough, trim to be safe
    let line = stdout.lines().next().unwrap_or("").trim();
    if line.is_empty() {
        None
    } else {
        Some(line.to_string())
    }
}
