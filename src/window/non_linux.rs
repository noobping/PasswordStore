use super::build::widgets::WindowWidgets;
use adw::gio::Menu;

pub(crate) fn configure_non_linux_window(widgets: &WindowWidgets) {
    let menu = Menu::new();
    menu.append(Some("_Find item"), Some("win.toggle-find"));
    menu.append(Some("_Preferences"), Some("win.open-preferences"));
    menu.append(Some("_Keyboard Shortcuts"), Some("app.shortcuts"));
    menu.append(Some("_About Keycord"), Some("app.about"));
    widgets.primary_menu_button.set_menu_model(Some(&menu));
}
