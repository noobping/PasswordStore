mod window;

use adw::prelude::*;
use adw::Application;
use adw::gio::SimpleAction;
use gtk4::{gio, glib};

const APP_ID: &str = "dev.noobping.passadw";

fn main() -> glib::ExitCode {
    // Make the compiled GResource available at runtime.
    gio::resources_register_include!("resources.gresource")
        .expect("Failed to register resources.gresource");

    // Initialize libadwaita
    adw::init().expect("Failed to initialize libadwaita");

    // Create the application
    let app = Application::builder()
        .application_id(APP_ID)
        .build();

    // app.about
    let about_action = SimpleAction::new("about", None);

    let app_for_about = app.clone();
    about_action.connect_activate(move |_, _| {
        if let Some(win) = app_for_about.active_window() {
            let about = adw::AboutWindow::builder()
                .transient_for(&win)
                .application_name("Password Store")
                .application_icon("passadw")
                .version("0.1.0")
                .developer_name("noobping")
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
