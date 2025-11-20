use gtk4::prelude::*;
use gtk4::{gio, Application};

mod window;
use crate::window::build_ui;

const APP_ID: &str = "dev.noobping.passadw";

fn main() -> anyhow::Result<()> {
    // Register compiled GResources (from build.rs)
    gio::resources_register_include!("resources.gresource")
        .expect("Failed to register resources");

    // If you want to *force* Wayland:
    // std::env::set_var("GDK_BACKEND", "wayland");

    let app = Application::builder()
        .application_id(APP_ID)
        // We handle command line ourselves so GLib doesn't complain
        .flags(gio::ApplicationFlags::HANDLES_COMMAND_LINE)
        .build();

    // Ignore CLI args like "gui" and just activate
    app.connect_command_line(|app, _cmd| {
        app.activate();
        0.into()
    });

    app.connect_activate(build_ui);

    if let Ok(backend) = std::env::var("GDK_BACKEND") {
        eprintln!("GDK_BACKEND = {backend}");
    }

    app.run();
    Ok(())
}

// fn main() -> anyhow::Result<()> {
//     // Register compiled GResources (from build.rs)
//     gio::resources_register_include!("resources.gresource")
//         .expect("Failed to register resources");

//     // Initialize libadwaita
//     adw::init().expect("Failed to init libadwaita");

//     let app = Application::builder()
//         .application_id(APP_ID)
//         .build();

//     app.connect_activate(|app| {
//         // Create and show the main window from the template in window.ui
//         let win = window::PasswordstoreWindow::new(app);
//         win.present();
//     });

//     // Run the application
//     let _exit_code = app.run();
//     Ok(())
// }
