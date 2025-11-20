use adw::prelude::*;
use adw::Application;
use adw::ApplicationWindow;
use gtk4::Builder;

const UI_RESOURCE: &str = "/dev/noobping/passadw/window.ui";

pub fn create_main_window(app: &Application) -> ApplicationWindow {
    // The resources are registered in main.rs
    let builder = Builder::from_resource(UI_RESOURCE);

    // `main_window` id comes from window.ui
    let window: ApplicationWindow = builder
        .object("main_window")
        .expect("Failed to get main_window from UI");

    window.set_application(Some(app));

    // ...
    
    window
}
