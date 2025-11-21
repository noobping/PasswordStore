mod item;
mod window;

use adw::gio::SimpleAction;
use adw::prelude::*;
use adw::Application;
use gtk4::{gio, glib};

const APP_ID: &str = "dev.noobping.passadw";

fn main() -> glib::ExitCode {
    // Make the compiled GResource available at runtime.
    gio::resources_register_include!("resources.gresource")
        .expect("Failed to register resources.gresource");

    // Initialize libadwaita
    adw::init().expect("Failed to initialize libadwaita");

    // Create the application
    let app = Application::builder().application_id(APP_ID).build();

    // app.about
    let about_action = SimpleAction::new("about", None);

    let app_for_about = app.clone();
    about_action.connect_activate(move |_, _| {
        if let Some(win) = app_for_about.active_window() {
            let authors_raw = env!("CARGO_PKG_AUTHORS");
            let authors: Vec<&str> = authors_raw
                .split(':')
                .map(|s| s.trim())
                .filter(|s| !s.is_empty())
                .collect();
            let about = adw::AboutWindow::builder()
                .transient_for(&win)
                .application_name(env!("CARGO_PKG_NAME"))
                .application_icon("passadw")
                .version(env!("CARGO_PKG_VERSION"))
                .developers(&authors[..])
                .comments(option_env!("CARGO_PKG_DESCRIPTION").unwrap_or(""))
                .build();
            about.present();
        }
    });
    app.add_action(&about_action);

    // keyboard shortcuts
    app.set_accels_for_action("app.about", &["F1"]);

    // When the app is activated, create and show the main window
    app.connect_activate(|app| {
        let win = window::create_main_window(app);
        win.window.present();
    });

    app.run()
}
