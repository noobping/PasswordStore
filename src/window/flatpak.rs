use super::build::widgets::WindowWidgets;
use crate::support::runtime::git_network_operations_available;
use adw::gio::Menu;

pub(crate) fn configure_flatpak_window(widgets: &WindowWidgets) {
    let menu = Menu::new();
    menu.append(Some("_Find item"), Some("win.toggle-find"));
    if git_network_operations_available() {
        menu.append(Some("_Synchronize with remote"), Some("win.synchronize"));
    }
    menu.append(Some("_Preferences"), Some("win.open-preferences"));
    menu.append(Some("_Logs"), Some("win.open-log"));
    menu.append(Some("_Keyboard Shortcuts"), Some("app.shortcuts"));
    menu.append(Some("_About Keycord"), Some("app.about"));
    widgets.primary_menu_button.set_menu_model(Some(&menu));
}
