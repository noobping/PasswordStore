use adw::gio::Menu;
use adw::gtk::{Builder, MenuButton};

pub(crate) fn configure_flatpak_window(builder: &Builder) {
    let primary_menu_button: MenuButton = builder
        .object("primary_menu_button")
        .expect("Failed to get primary menu button");

    let menu = Menu::new();
    menu.append(Some("_Find item"), Some("win.toggle-find"));
    menu.append(Some("_Preferences"), Some("win.open-preferences"));
    menu.append(Some("_About PasswordStore"), Some("app.about"));
    primary_menu_button.set_menu_model(Some(&menu));
}
