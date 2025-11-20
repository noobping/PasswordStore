use adw::ApplicationWindow;
use gtk4::prelude::*;
use gtk4::{Application, Builder};

const UI_SRC: &str = include_str!("../data/window.ui");

pub fn build_ui(app: &Application) {
    let builder = Builder::from_string(UI_SRC);

    let window: ApplicationWindow = builder
        .object("main_window")
        .expect("Failed to get main_window from UI");
    window.set_application(Some(app));

    // ...

    window.show();
}
