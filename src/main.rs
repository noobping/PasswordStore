mod window;

use adw::prelude::*;
use adw::Application;
use gtk4::{gio, glib};

const APP_ID: &str = "dev.noobping.passadw";

fn main() -> glib::ExitCode {
    // Make the compiled GResource available at runtime.
    // Name must match the one in build.rs.
    gio::resources_register_include!("resources.gresource")
        .expect("Failed to register resources.gresource");

    // Initialize libadwaita
    adw::init().expect("Failed to initialize libadwaita");

    // Create the application
    let app = Application::builder()
        .application_id(APP_ID)
        .build();

    // When the app is activated, create and show the main window
    app.connect_activate(|app| {
        let win = window::create_main_window(app);
        win.present();
    });

    app.run()
}
