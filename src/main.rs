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
            let about = adw::AboutDialog::builder()
                .application_name(env!("CARGO_PKG_NAME"))
                .application_icon("passadw")
                .version(env!("CARGO_PKG_VERSION"))
                .developers(&authors[..])
                .comments(comments)
                .build();
            about.present(Some(&win.window));
        });
        app.add_action(&about_action);
    });

    app.run()
}
